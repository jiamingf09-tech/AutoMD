use crate::models::*;
use std::fs;
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
    let mut tools: Vec<ToolDiagnostic> = [
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
    apply_contextual_tool_statuses(&mut tools);

    RuntimeDiagnostics {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        gpu: gpu_availability(&tools, &gpu_info),
        hardware: hardware_diagnostics(&gpu_info),
        tools,
    }
}

fn hardware_diagnostics(gpu_info: &LocalGpuInfo) -> HardwareDiagnostics {
    HardwareDiagnostics {
        cpu: cpu_hardware(),
        memory: memory_hardware(),
        gpus: gpu_devices(gpu_info),
        disks: disk_volumes(),
    }
}

fn cpu_hardware() -> CpuHardware {
    let logical_cores = std::thread::available_parallelism()
        .map(|value| value.get().min(u16::MAX as usize) as u16)
        .unwrap_or(1);
    let physical_cores = match std::env::consts::OS {
        "macos" => command_stdout("sysctl", &["-n", "hw.physicalcpu"])
            .trim()
            .parse::<u16>()
            .ok(),
        "linux" => parse_lscpu_value("Core(s) per socket")
            .and_then(|cores| {
                parse_lscpu_value("Socket(s)").map(|sockets| cores.saturating_mul(sockets))
            }),
        _ => None,
    };
    let brand = match std::env::consts::OS {
        "macos" => command_stdout("sysctl", &["-n", "machdep.cpu.brand_string"]),
        "linux" => fs::read_to_string("/proc/cpuinfo")
            .ok()
            .and_then(|text| {
                text.lines()
                    .find_map(|line| line.strip_prefix("model name").and_then(|value| value.split_once(':').map(|(_, right)| right.trim().to_string())))
            })
            .unwrap_or_default(),
        "windows" => command_stdout(
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                "(Get-CimInstance Win32_Processor | Select-Object -First 1 -ExpandProperty Name)",
            ],
        ),
        _ => String::new(),
    };

    CpuHardware {
        brand: brand.trim().to_string(),
        architecture: std::env::consts::ARCH.to_string(),
        logical_cores,
        physical_cores,
    }
}

fn parse_lscpu_value(key: &str) -> Option<u16> {
    let output = command_stdout("lscpu", &[]);
    output.lines().find_map(|line| {
        let (left, right) = line.split_once(':')?;
        (left.trim() == key)
            .then(|| right.trim().parse::<u16>().ok())
            .flatten()
    })
}

fn memory_hardware() -> MemoryHardware {
    match std::env::consts::OS {
        "macos" => {
            let total = command_stdout("sysctl", &["-n", "hw.memsize"])
                .trim()
                .parse::<u64>()
                .ok();
            MemoryHardware {
                total_bytes: total,
                available_bytes: None,
                detail: "macOS 统一内存；可用内存由系统动态压缩和调度。".to_string(),
            }
        }
        "linux" => {
            let meminfo = fs::read_to_string("/proc/meminfo").unwrap_or_default();
            let total = parse_meminfo_kb(&meminfo, "MemTotal").map(|kb| kb.saturating_mul(1024));
            let available = parse_meminfo_kb(&meminfo, "MemAvailable").map(|kb| kb.saturating_mul(1024));
            MemoryHardware {
                total_bytes: total,
                available_bytes: available,
                detail: "来自 /proc/meminfo。".to_string(),
            }
        }
        "windows" => {
            let total = command_stdout(
                "powershell",
                &[
                    "-NoProfile",
                    "-Command",
                    "(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory",
                ],
            )
            .trim()
            .parse::<u64>()
            .ok();
            let available = command_stdout(
                "powershell",
                &[
                    "-NoProfile",
                    "-Command",
                    "(Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory * 1024",
                ],
            )
            .trim()
            .parse::<u64>()
            .ok();
            MemoryHardware {
                total_bytes: total,
                available_bytes: available,
                detail: "来自 Windows CIM。".to_string(),
            }
        }
        _ => MemoryHardware {
            total_bytes: None,
            available_bytes: None,
            detail: "当前平台未提供内存检测。".to_string(),
        },
    }
}

fn parse_meminfo_kb(text: &str, key: &str) -> Option<u64> {
    text.lines().find_map(|line| {
        let (left, right) = line.split_once(':')?;
        if left.trim() != key {
            return None;
        }
        right.split_whitespace().next()?.parse::<u64>().ok()
    })
}

fn gpu_devices(gpu_info: &LocalGpuInfo) -> Vec<GpuDevice> {
    let mut devices = match std::env::consts::OS {
        "macos" => macos_gpu_devices(),
        "linux" => linux_gpu_devices(),
        "windows" => windows_gpu_devices(),
        _ => Vec::new(),
    };
    if devices.is_empty() && gpu_info.kind != LocalGpuKind::None {
        devices.push(GpuDevice {
            id: "gpu0".to_string(),
            name: gpu_info.label.clone(),
            vendor: gpu_vendor(&gpu_info.label),
            backend: backend_for_gpu_kind(&gpu_info.kind),
            memory_bytes: None,
            detail: "检测到 GPU 类型，但系统未提供完整设备列表。".to_string(),
        });
    }
    devices
}

fn macos_gpu_devices() -> Vec<GpuDevice> {
    let output = command_stdout("system_profiler", &["SPDisplaysDataType", "-detailLevel", "mini"]);
    let mut devices = Vec::new();
    let mut current_name: Option<String> = None;
    let mut vendor = String::new();
    let mut memory: Option<u64> = None;
    let mut detail_parts: Vec<String> = Vec::new();

    let flush = |devices: &mut Vec<GpuDevice>, current_name: &mut Option<String>, vendor: &mut String, memory: &mut Option<u64>, detail_parts: &mut Vec<String>| {
        if let Some(name) = current_name.take() {
            let kind = classify_gpu_summary(&format!("{name} {vendor}"));
            devices.push(GpuDevice {
                id: format!("gpu{}", devices.len()),
                name,
                vendor: if vendor.is_empty() { gpu_vendor(&kind.label) } else { vendor.clone() },
                backend: backend_for_gpu_kind(&kind.kind),
                memory_bytes: *memory,
                detail: if detail_parts.is_empty() {
                    "macOS display device".to_string()
                } else {
                    detail_parts.join("; ")
                },
            });
        }
        *vendor = String::new();
        *memory = None;
        detail_parts.clear();
    };

    for raw_line in output.lines() {
        let line = raw_line.trim();
        if let Some(value) = line.strip_prefix("Chipset Model:") {
            flush(&mut devices, &mut current_name, &mut vendor, &mut memory, &mut detail_parts);
            current_name = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("Vendor:") {
            vendor = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("VRAM (Total):").or_else(|| line.strip_prefix("VRAM:")) {
            memory = parse_memory_size(value.trim());
            detail_parts.push(format!("VRAM {}", value.trim()));
        } else if let Some(value) = line.strip_prefix("Total Number of Cores:") {
            detail_parts.push(format!("GPU cores {}", value.trim()));
        } else if let Some(value) = line.strip_prefix("Metal Support:") {
            detail_parts.push(format!("Metal {}", value.trim()));
        }
    }
    flush(&mut devices, &mut current_name, &mut vendor, &mut memory, &mut detail_parts);
    devices
}

fn linux_gpu_devices() -> Vec<GpuDevice> {
    let nvidia = command_stdout(
        "nvidia-smi",
        &["--query-gpu=index,name,memory.total", "--format=csv,noheader,nounits"],
    );
    let mut devices: Vec<GpuDevice> = nvidia
        .lines()
        .filter_map(|line| {
            let parts = line.split(',').map(str::trim).collect::<Vec<_>>();
            if parts.len() < 3 {
                return None;
            }
            let memory_bytes = parts[2]
                .parse::<u64>()
                .ok()
                .map(|mib| mib.saturating_mul(1024 * 1024));
            Some(GpuDevice {
                id: format!("gpu{}", parts[0]),
                name: parts[1].to_string(),
                vendor: "NVIDIA".to_string(),
                backend: Some(GpuBackend::Cuda),
                memory_bytes,
                detail: "来自 nvidia-smi。".to_string(),
            })
        })
        .collect();
    if !devices.is_empty() {
        return devices;
    }

    let lspci = command_stdout("lspci", &[]);
    devices = lspci
        .lines()
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            lower.contains("vga") || lower.contains("3d controller") || lower.contains("display controller")
        })
        .enumerate()
        .map(|(index, line)| {
            let kind = classify_gpu_summary(line);
            GpuDevice {
                id: format!("gpu{index}"),
                name: line.to_string(),
                vendor: gpu_vendor(line),
                backend: backend_for_gpu_kind(&kind.kind),
                memory_bytes: None,
                detail: "来自 lspci；显存未由 lspci 提供。".to_string(),
            }
        })
        .collect();
    devices
}

fn windows_gpu_devices() -> Vec<GpuDevice> {
    let output = command_stdout(
        "powershell",
        &[
            "-NoProfile",
            "-Command",
            "Get-CimInstance Win32_VideoController | ForEach-Object { \"$($_.Name)|$($_.AdapterRAM)\" }",
        ],
    );
    output
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let (name, memory) = line.split_once('|')?;
            let memory_bytes = memory.trim().parse::<u64>().ok().filter(|value| *value > 0);
            let kind = classify_gpu_summary(name);
            Some(GpuDevice {
                id: format!("gpu{index}"),
                name: name.trim().to_string(),
                vendor: gpu_vendor(name),
                backend: backend_for_gpu_kind(&kind.kind),
                memory_bytes,
                detail: "来自 Win32_VideoController。".to_string(),
            })
        })
        .collect()
}

fn backend_for_gpu_kind(kind: &LocalGpuKind) -> Option<GpuBackend> {
    match kind {
        LocalGpuKind::Nvidia => Some(GpuBackend::Cuda),
        LocalGpuKind::Amd => Some(GpuBackend::Rocm),
        LocalGpuKind::Apple => Some(GpuBackend::Metal),
        _ => None,
    }
}

fn gpu_vendor(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    if lower.contains("nvidia") || lower.contains("geforce") || lower.contains("quadro") {
        "NVIDIA".to_string()
    } else if lower.contains("amd") || lower.contains("radeon") || lower.contains("advanced micro devices") {
        "AMD".to_string()
    } else if lower.contains("apple") || lower.contains("metal") {
        "Apple".to_string()
    } else if lower.contains("intel") {
        "Intel".to_string()
    } else {
        "Unknown".to_string()
    }
}

fn parse_memory_size(value: &str) -> Option<u64> {
    let normalized = value.replace(',', ".");
    let number = normalized
        .split_whitespace()
        .next()
        .and_then(|part| part.parse::<f64>().ok())?;
    let lower = normalized.to_ascii_lowercase();
    let bytes = if lower.contains("tib") || lower.contains("tb") {
        number * 1024.0 * 1024.0 * 1024.0 * 1024.0
    } else if lower.contains("gib") || lower.contains("gb") {
        number * 1024.0 * 1024.0 * 1024.0
    } else if lower.contains("mib") || lower.contains("mb") {
        number * 1024.0 * 1024.0
    } else if lower.contains("kib") || lower.contains("kb") {
        number * 1024.0
    } else {
        number
    };
    (bytes > 0.0).then_some(bytes as u64)
}

fn disk_volumes() -> Vec<DiskVolume> {
    if cfg!(target_os = "windows") {
        return windows_disk_volumes();
    }
    unix_disk_volumes()
}

fn unix_disk_volumes() -> Vec<DiskVolume> {
    let output = command_stdout("df", &["-kP"]);
    output
        .lines()
        .skip(1)
        .enumerate()
        .filter_map(|(index, line)| {
            let parts = line.split_whitespace().collect::<Vec<_>>();
            if parts.len() < 6 {
                return None;
            }
            let total_bytes = parts[1].parse::<u64>().ok().map(|kb| kb.saturating_mul(1024));
            let available_bytes = parts[3].parse::<u64>().ok().map(|kb| kb.saturating_mul(1024));
            Some(DiskVolume {
                id: format!("disk{index}"),
                mount_point: parts[5..].join(" "),
                filesystem: parts[0].to_string(),
                total_bytes,
                available_bytes,
                detail: format!("{} used", parts.get(4).copied().unwrap_or("unknown")),
            })
        })
        .collect()
}

fn windows_disk_volumes() -> Vec<DiskVolume> {
    let output = command_stdout(
        "powershell",
        &[
            "-NoProfile",
            "-Command",
            "Get-CimInstance Win32_LogicalDisk -Filter \"DriveType=3\" | ForEach-Object { \"$($_.DeviceID)|$($_.FileSystem)|$($_.Size)|$($_.FreeSpace)\" }",
        ],
    );
    output
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let parts = line.split('|').collect::<Vec<_>>();
            if parts.len() < 4 {
                return None;
            }
            Some(DiskVolume {
                id: format!("disk{index}"),
                mount_point: parts[0].trim().to_string(),
                filesystem: parts[1].trim().to_string(),
                total_bytes: parts[2].trim().parse::<u64>().ok(),
                available_bytes: parts[3].trim().parse::<u64>().ok(),
                detail: "来自 Win32_LogicalDisk。".to_string(),
            })
        })
        .collect()
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
            detail: "可自动查找或手动选择；能由 AutoMD 安装的项目会显示一键安装。".to_string(),
        },
    }
}

fn apply_contextual_tool_statuses(tools: &mut [ToolDiagnostic]) {
    let ready = |id: &str, tools: &[ToolDiagnostic]| {
        tools
            .iter()
            .any(|tool| tool.id == id && matches!(tool.status, DetectionStatus::Ready))
    };

    let docker_ready = ready("docker", tools);
    let podman_ready = ready("podman", tools);
    if docker_ready {
        mark_missing_not_applicable(
            tools,
            "podman",
            "Docker 已可用，Podman 不是当前本机运行的必需项；需要 Podman 时仍可手动配置。",
        );
    }
    if podman_ready {
        mark_missing_not_applicable(
            tools,
            "docker",
            "Podman 已可用，Docker 不是当前本机运行的必需项；需要 Docker Desktop 时仍可手动配置。",
        );
    }

    if std::env::consts::OS != "linux" {
        mark_missing_not_applicable(
            tools,
            "apptainer",
            "Apptainer/Singularity 主要用于 Linux 或 HPC；当前桌面环境建议使用 Docker/Podman 或远程 profile。",
        );
    }

    for (id, scheduler) in [
        ("sbatch", "SLURM"),
        ("qsub", "PBS"),
        ("bsub", "LSF"),
    ] {
        mark_missing_not_applicable(
            tools,
            id,
            &format!("{scheduler} 命令通常由远程/HPC 登录节点提供；本机缺失不影响配置远程 profile。"),
        );
    }
}

fn mark_missing_not_applicable(tools: &mut [ToolDiagnostic], id: &str, detail: &str) {
    if let Some(tool) = tools
        .iter_mut()
        .find(|tool| tool.id == id && matches!(tool.status, DetectionStatus::MissingInstall))
    {
        tool.status = DetectionStatus::NotApplicable;
        tool.detail = detail.to_string();
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
            "检测到 {}，但未找到 nvidia-smi。可自动查找、手动选择，或查看 CUDA/NVIDIA 驱动安装方式。",
            gpu_info.label
        ),
        LocalGpuKind::Amd => format!(
            "检测到 {}，但未找到 rocminfo。可自动查找、手动选择，或查看 ROCm/HIP 安装方式。",
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
