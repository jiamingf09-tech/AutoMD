use crate::models::*;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
enum LocalGpuKind {
    Nvidia,
    Amd,
    Apple,
    Intel,
    Other,
    None,
}

#[derive(Debug, Clone)]
struct LocalGpuInfo {
    kind: LocalGpuKind,
    label: String,
}

pub fn diagnostics() -> RuntimeDiagnostics {
    let gpu_info = detect_local_gpu();
    let tools: Vec<ToolDiagnostic> = [
        ("conda", "Conda", "conda"),
        ("mamba", "Mamba", "mamba"),
        ("docker", "Docker", "docker"),
        ("podman", "Podman", "podman"),
        ("apptainer", "Apptainer", "apptainer"),
        ("ssh", "SSH", "ssh"),
        ("rsync", "rsync", "rsync"),
        ("sbatch", "SLURM sbatch", "sbatch"),
        ("qsub", "PBS qsub", "qsub"),
        ("bsub", "LSF bsub", "bsub"),
        ("mpirun", "MPI", "mpirun"),
        ("plumed", "PLUMED", "plumed"),
        ("nvidia-smi", "CUDA / NVIDIA", "nvidia-smi"),
        ("rocminfo", "ROCm", "rocminfo"),
    ]
    .into_iter()
    .map(|(id, label, command)| diagnostic(id, label, command, &gpu_info))
    .collect();

    RuntimeDiagnostics {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        gpu: gpu_availability(&tools, &gpu_info),
        tools,
    }
}

pub fn remote_profile_templates() -> Vec<RemoteProfile> {
    vec![
        RemoteProfile {
            id: "slurm-gpu-template".to_string(),
            name: "SLURM GPU cluster".to_string(),
            host: "login.cluster.example".to_string(),
            scheduler: ExecutionMode::Slurm,
            workdir: "/scratch/$USER/automd".to_string(),
            module_load: vec![
                "module load gcc openmpi cuda".to_string(),
                "module load gromacs plumed".to_string(),
            ],
            default_queue: Some("gpu".to_string()),
        },
        RemoteProfile {
            id: "ssh-workstation-template".to_string(),
            name: "SSH workstation".to_string(),
            host: "workstation.example".to_string(),
            scheduler: ExecutionMode::Ssh,
            workdir: "/data/automd".to_string(),
            module_load: vec!["source ~/.bashrc".to_string()],
            default_queue: None,
        },
    ]
}

fn diagnostic(id: &str, label: &str, command: &str, gpu_info: &LocalGpuInfo) -> ToolDiagnostic {
    if id == "nvidia-smi" {
        return gpu_tool_diagnostic(id, label, command, gpu_info, LocalGpuKind::Nvidia);
    }
    if id == "rocminfo" {
        return gpu_tool_diagnostic(id, label, command, gpu_info, LocalGpuKind::Amd);
    }

    match crate::sysenv::resolve_command(command) {
        Some(path) => ToolDiagnostic {
            id: id.to_string(),
            label: label.to_string(),
            command: command.to_string(),
            status: DetectionStatus::Ready,
            detail: path.display().to_string(),
        },
        None => ToolDiagnostic {
            id: id.to_string(),
            label: label.to_string(),
            command: command.to_string(),
            status: DetectionStatus::MissingInstall,
            detail: "可自动查找、手动选择或一键安装。".to_string(),
        },
    }
}

fn gpu_tool_diagnostic(
    id: &str,
    label: &str,
    command: &str,
    gpu_info: &LocalGpuInfo,
    target_gpu: LocalGpuKind,
) -> ToolDiagnostic {
    if let Some(path) = crate::sysenv::resolve_command(command) {
        return ToolDiagnostic {
            id: id.to_string(),
            label: label.to_string(),
            command: command.to_string(),
            status: DetectionStatus::Ready,
            detail: path.display().to_string(),
        };
    }

    if is_gpu_tool_relevant(gpu_info, &target_gpu) {
        return ToolDiagnostic {
            id: id.to_string(),
            label: label.to_string(),
            command: command.to_string(),
            status: DetectionStatus::MissingInstall,
            detail: gpu_missing_detail(&target_gpu, gpu_info),
        };
    }

    ToolDiagnostic {
        id: id.to_string(),
        label: label.to_string(),
        command: command.to_string(),
        status: DetectionStatus::NotApplicable,
        detail: gpu_not_applicable_detail(&target_gpu, gpu_info),
    }
}

fn is_gpu_tool_relevant(gpu_info: &LocalGpuInfo, target_gpu: &LocalGpuKind) -> bool {
    match target_gpu {
        LocalGpuKind::Nvidia => {
            gpu_info.kind == LocalGpuKind::Nvidia && std::env::consts::OS != "macos"
        }
        LocalGpuKind::Amd => gpu_info.kind == LocalGpuKind::Amd && std::env::consts::OS == "linux",
        _ => false,
    }
}

fn gpu_missing_detail(target_gpu: &LocalGpuKind, gpu_info: &LocalGpuInfo) -> String {
    match target_gpu {
        LocalGpuKind::Nvidia => format!(
            "检测到 {}，但未找到 nvidia-smi。可自动查找、手动选择，或自动安装 CUDA/NVIDIA 驱动工具链。",
            gpu_info.label
        ),
        LocalGpuKind::Amd => format!(
            "检测到 {}，但未找到 rocminfo。可自动查找、手动选择，或自动安装 ROCm/HIP 工具链。",
            gpu_info.label
        ),
        _ => "当前 GPU 后端无需安装此工具。".to_string(),
    }
}

fn gpu_not_applicable_detail(target_gpu: &LocalGpuKind, gpu_info: &LocalGpuInfo) -> String {
    match target_gpu {
        LocalGpuKind::Nvidia => format!(
            "当前检测到 {}，本机无需安装 CUDA/NVIDIA 工具。",
            gpu_info.label
        ),
        LocalGpuKind::Amd => format!("当前检测到 {}，本机无需安装 ROCm 工具。", gpu_info.label),
        _ => "当前 GPU 后端不适用此工具。".to_string(),
    }
}

fn gpu_availability(tools: &[ToolDiagnostic], gpu_info: &LocalGpuInfo) -> GpuAvailability {
    let checked_at = chrono::Utc::now();
    if let Some(tool) = ready_tool(tools, "nvidia-smi") {
        return GpuAvailability {
            available: true,
            mode: "gpu".to_string(),
            backend: Some(GpuBackend::Cuda),
            label: "GPU 可用：CUDA".to_string(),
            reason: "检测到 NVIDIA/CUDA 工具 nvidia-smi。".to_string(),
            detail: format!(
                "将优先允许 CUDA/GPU 配置；实际运行仍按所选引擎后端确认。路径：{}",
                tool.detail
            ),
            checked_at,
        };
    }
    if let Some(tool) = ready_tool(tools, "rocminfo") {
        return GpuAvailability {
            available: true,
            mode: "gpu".to_string(),
            backend: Some(GpuBackend::Rocm),
            label: "GPU 可用：ROCm".to_string(),
            reason: "检测到 AMD/ROCm 工具 rocminfo。".to_string(),
            detail: format!(
                "将优先允许 ROCm/GPU 配置；实际运行仍按所选引擎后端确认。路径：{}",
                tool.detail
            ),
            checked_at,
        };
    }
    if std::env::consts::OS == "macos" {
        return GpuAvailability {
            available: true,
            mode: "gpu".to_string(),
            backend: Some(GpuBackend::Metal),
            label: "GPU 可用：Metal".to_string(),
            reason: format!(
                "macOS 环境检测到 {}，可使用 Metal GPU 能力。",
                gpu_info.label
            ),
            detail: "是否能用于分子动力学运行取决于所选引擎；不支持 Metal 的引擎会自动提示 CPU 或远程/HPC 路线。".to_string(),
            checked_at,
        };
    }
    if gpu_info.kind == LocalGpuKind::Nvidia {
        return GpuAvailability {
            available: false,
            mode: "cpuFallback".to_string(),
            backend: Some(GpuBackend::Cuda),
            label: "GPU 不可用：CUDA 未配置".to_string(),
            reason: format!("检测到 {}，但未找到 nvidia-smi/CUDA runtime。", gpu_info.label),
            detail: "本地运行将按 CPU 模式准备；需要 NVIDIA GPU 时请安装驱动/CUDA，或使用已配置 GPU 的远程/HPC 节点。".to_string(),
            checked_at,
        };
    }
    if gpu_info.kind == LocalGpuKind::Amd {
        return GpuAvailability {
            available: false,
            mode: "cpuFallback".to_string(),
            backend: Some(GpuBackend::Rocm),
            label: "GPU 不可用：ROCm 未配置".to_string(),
            reason: format!("检测到 {}，但未找到 rocminfo/ROCm runtime。", gpu_info.label),
            detail: "本地运行将按 CPU 模式准备；需要 AMD GPU 时请在支持 ROCm 的 Linux 环境安装 ROCm/HIP，或使用远程/HPC 节点。".to_string(),
            checked_at,
        };
    }

    GpuAvailability {
        available: false,
        mode: "cpuFallback".to_string(),
        backend: None,
        label: "GPU 不可用：CPU 模式".to_string(),
        reason: format!(
            "检测到 {}，当前机器未暴露可用分子动力学 GPU 后端。",
            gpu_info.label
        ),
        detail: "本地运行将按 CPU 模式准备；需要 GPU 时请使用匹配显卡的驱动/工具链、容器 GPU runtime，或提交到远程/HPC GPU 节点。".to_string(),
        checked_at,
    }
}

fn ready_tool<'a>(tools: &'a [ToolDiagnostic], id: &str) -> Option<&'a ToolDiagnostic> {
    tools.iter().find(|tool| tool.id == id && matches!(tool.status, DetectionStatus::Ready))
}

fn detect_local_gpu() -> LocalGpuInfo {
    if which::which("nvidia-smi").is_ok() {
        return LocalGpuInfo {
            kind: LocalGpuKind::Nvidia,
            label: "NVIDIA GPU".to_string(),
        };
    }
    if which::which("rocminfo").is_ok() {
        return LocalGpuInfo {
            kind: LocalGpuKind::Amd,
            label: "AMD GPU".to_string(),
        };
    }

    let summary = gpu_hardware_summary();
    classify_gpu_summary(&summary)
}

fn gpu_hardware_summary() -> String {
    match std::env::consts::OS {
        "macos" => command_stdout("system_profiler", &["SPDisplaysDataType", "-detailLevel", "mini"]),
        "linux" => command_stdout("lspci", &[]),
        "windows" => command_stdout(
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                "Get-CimInstance Win32_VideoController | Select-Object -ExpandProperty Name",
            ],
        ),
        _ => String::new(),
    }
}

fn command_stdout(command: &str, args: &[&str]) -> String {
    Command::new(command)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).to_string())
        .unwrap_or_default()
}

fn classify_gpu_summary(summary: &str) -> LocalGpuInfo {
    let normalized = summary.to_lowercase();
    if normalized.contains("nvidia")
        || normalized.contains("geforce")
        || normalized.contains("quadro")
        || normalized.contains("tesla")
    {
        return LocalGpuInfo {
            kind: LocalGpuKind::Nvidia,
            label: "NVIDIA GPU".to_string(),
        };
    }
    if normalized.contains("amd")
        || normalized.contains("radeon")
        || normalized.contains("advanced micro devices")
    {
        return LocalGpuInfo {
            kind: LocalGpuKind::Amd,
            label: "AMD GPU".to_string(),
        };
    }
    if normalized.contains("apple") || normalized.contains("metal") {
        return LocalGpuInfo {
            kind: LocalGpuKind::Apple,
            label: "Apple/Metal GPU".to_string(),
        };
    }
    if normalized.contains("intel") {
        return LocalGpuInfo {
            kind: LocalGpuKind::Intel,
            label: "Intel GPU".to_string(),
        };
    }
    if normalized.trim().is_empty() {
        return LocalGpuInfo {
            kind: LocalGpuKind::None,
            label: "未检测到独立 GPU".to_string(),
        };
    }
    LocalGpuInfo {
        kind: LocalGpuKind::Other,
        label: "非 CUDA/ROCm GPU".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_summary_classifier_identifies_common_vendors() {
        assert_eq!(
            classify_gpu_summary("NVIDIA GeForce RTX 4090").kind,
            LocalGpuKind::Nvidia
        );
        assert_eq!(classify_gpu_summary("AMD Radeon Pro").kind, LocalGpuKind::Amd);
        assert_eq!(
            classify_gpu_summary("Chipset Model: Apple M4").kind,
            LocalGpuKind::Apple
        );
        assert_eq!(
            classify_gpu_summary("Intel Iris Plus Graphics").kind,
            LocalGpuKind::Intel
        );
    }

    #[test]
    fn unrelated_gpu_tools_are_marked_not_applicable() {
        let gpu_info = LocalGpuInfo {
            kind: LocalGpuKind::Apple,
            label: "Apple/Metal GPU".to_string(),
        };

        let cuda = gpu_tool_diagnostic(
            "nvidia-smi",
            "CUDA / NVIDIA",
            "automd-definitely-missing-nvidia-smi",
            &gpu_info,
            LocalGpuKind::Nvidia,
        );
        let rocm = gpu_tool_diagnostic(
            "rocminfo",
            "ROCm",
            "automd-definitely-missing-rocminfo",
            &gpu_info,
            LocalGpuKind::Amd,
        );

        assert_eq!(cuda.status, DetectionStatus::NotApplicable);
        assert_eq!(rocm.status, DetectionStatus::NotApplicable);
    }
}
