use crate::models::*;

pub fn diagnostics() -> RuntimeDiagnostics {
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
    .map(|(id, label, command)| diagnostic(id, label, command))
    .collect();

    RuntimeDiagnostics {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        gpu: gpu_availability(&tools),
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

fn diagnostic(id: &str, label: &str, command: &str) -> ToolDiagnostic {
    match which::which(command) {
        Ok(path) => ToolDiagnostic {
            id: id.to_string(),
            label: label.to_string(),
            command: command.to_string(),
            status: DetectionStatus::Ready,
            detail: path.display().to_string(),
        },
        Err(_) => ToolDiagnostic {
            id: id.to_string(),
            label: label.to_string(),
            command: command.to_string(),
            status: DetectionStatus::MissingInstall,
            detail: format!("未在 PATH 中找到 {command}"),
        },
    }
}

fn gpu_availability(tools: &[ToolDiagnostic]) -> GpuAvailability {
    let checked_at = chrono::Utc::now();
    if let Some(tool) = ready_tool(tools, "nvidia-smi") {
        return GpuAvailability {
            available: true,
            mode: "gpu".to_string(),
            backend: Some(GpuBackend::Cuda),
            label: "GPU 可用：CUDA".to_string(),
            reason: "检测到 NVIDIA/CUDA 工具 nvidia-smi。".to_string(),
            detail: format!("将优先允许 CUDA/GPU 配置；实际运行仍按所选引擎后端确认。路径：{}", tool.detail),
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
            detail: format!("将优先允许 ROCm/GPU 配置；实际运行仍按所选引擎后端确认。路径：{}", tool.detail),
            checked_at,
        };
    }
    if std::env::consts::OS == "macos" {
        return GpuAvailability {
            available: true,
            mode: "gpu".to_string(),
            backend: Some(GpuBackend::Metal),
            label: "GPU 可用：Metal".to_string(),
            reason: "macOS 环境可使用 Metal/Apple GPU 能力。".to_string(),
            detail: "是否能用于分子动力学运行取决于所选引擎；不支持 Metal 的引擎会自动提示 CPU 或远程/HPC 路线。".to_string(),
            checked_at,
        };
    }

    GpuAvailability {
        available: false,
        mode: "cpuFallback".to_string(),
        backend: None,
        label: "GPU 不可用：CPU 模式".to_string(),
        reason: "未检测到 CUDA/ROCm GPU 工具，当前机器未暴露可用 GPU 后端。".to_string(),
        detail: "本地运行将按 CPU 模式准备；需要 GPU 时请安装驱动/工具链、使用容器 GPU runtime，或提交到远程/HPC GPU 节点。".to_string(),
        checked_at,
    }
}

fn ready_tool<'a>(tools: &'a [ToolDiagnostic], id: &str) -> Option<&'a ToolDiagnostic> {
    tools.iter().find(|tool| tool.id == id && matches!(tool.status, DetectionStatus::Ready))
}
