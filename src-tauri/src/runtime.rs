use crate::models::*;

pub fn diagnostics() -> RuntimeDiagnostics {
    RuntimeDiagnostics {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        tools: [
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
        .collect(),
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
