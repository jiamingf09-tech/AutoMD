use crate::models::*;
use serde_json::to_string_pretty;
use std::fs;
use std::path::{Component, Path, PathBuf};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

pub fn remote_execution_package(request: RemoteExecutionRequest) -> RemoteExecutionPackage {
    let plan = request.plan;
    let profile = request.profile;
    let run_directory = engine_run_directory(&plan);
    let remote_workdir = format!("{}/{}-{}", profile.workdir, sanitize_job_name(&plan.name), plan.id.simple());
    let scheduler_script = scheduler_script(&plan, &profile, &run_directory, &remote_workdir);
    let scheduler_filename = match &profile.scheduler {
        ExecutionMode::Slurm => "remote/submit.slurm",
        ExecutionMode::Pbs => "remote/submit.pbs",
        ExecutionMode::Lsf => "remote/submit.lsf",
        _ => "remote/run-ssh.sh",
    }
    .to_string();
    let local_project = request
        .local_project_path
        .clone()
        .unwrap_or_else(|| ".".to_string());
    let remote_target = format!("{}:{}", profile.host, shell_quote(&remote_workdir));
    let submit_command = submit_command(&profile, &remote_workdir, &scheduler_filename);
    let mut warnings = remote_warnings(&plan, &profile);
    if !request.include_submit {
        warnings.push("submit 命令仅作为预览生成；GUI 不会自动连接远程主机。".to_string());
    }

    RemoteExecutionPackage {
        engine_id: plan.engine_id.clone(),
        scheduler: profile.scheduler.clone(),
        profile_id: profile.id.clone(),
        remote_workdir: remote_workdir.clone(),
        run_directory: run_directory.clone(),
        files: vec![
            GeneratedFile {
                path: scheduler_filename.clone(),
                language: scheduler_language(&profile.scheduler).to_string(),
                contents: scheduler_script,
            },
            GeneratedFile {
                path: "remote/sync-up.sh".to_string(),
                language: "bash".to_string(),
                contents: format!(
                    r#"#!/usr/bin/env bash
set -euo pipefail

ssh {host} {mkdir}
rsync -az --delete --partial --append-verify \
  --exclude 'src-tauri/target' \
  --exclude 'node_modules' \
  {local}/ {target}/
"#,
                    host = shell_quote(&profile.host),
                    mkdir = shell_quote(&format!("mkdir -p {}", shell_quote(&remote_workdir))),
                    local = shell_quote(&local_project),
                    target = remote_target,
                ),
            },
            GeneratedFile {
                path: "remote/sync-down.sh".to_string(),
                language: "bash".to_string(),
                contents: format!(
                    r#"#!/usr/bin/env bash
set -euo pipefail

mkdir -p {local}
rsync -az --partial --append-verify \
  {target}/runs/ {local}/runs/
rsync -az --partial --append-verify \
  {target}/checkpoints/ {local}/checkpoints/
rsync -az --partial --append-verify \
  {target}/trajectories/ {local}/trajectories/
rsync -az --partial --append-verify \
  {target}/analysis/ {local}/analysis/
rsync -az --partial --append-verify \
  {target}/reports/ {local}/reports/
"#,
                    local = shell_quote(&local_project),
                    target = remote_target,
                ),
            },
        ],
        commands: vec![
            RemoteCommand {
                id: "sync-up".to_string(),
                label: "同步到远程".to_string(),
                command: format!(
                    "ssh {host} {mkdir} && rsync -az --delete --partial --append-verify {local}/ {target}/",
                    host = shell_quote(&profile.host),
                    mkdir = shell_quote(&format!("mkdir -p {}", shell_quote(&remote_workdir))),
                    local = shell_quote(&local_project),
                    target = remote_target,
                ),
                description: "创建远程工作目录并同步项目输入、生成文件和运行脚本。".to_string(),
            },
            RemoteCommand {
                id: "submit".to_string(),
                label: "提交任务".to_string(),
                command: submit_command,
                description: "在远程工作目录中调用调度器或纯 SSH 后台运行脚本。".to_string(),
            },
            RemoteCommand {
                id: "status".to_string(),
                label: "查询状态".to_string(),
                command: status_command(&profile),
                description: "提交后将 <job-id> 替换为调度器返回的任务 ID。".to_string(),
            },
            RemoteCommand {
                id: "cancel".to_string(),
                label: "取消任务".to_string(),
                command: cancel_command(&profile),
                description: "取消远程调度任务；纯 SSH 模式需要使用远程 PID。".to_string(),
            },
            RemoteCommand {
                id: "tail-log".to_string(),
                label: "读取远程日志".to_string(),
                command: tail_log_command(&profile, &remote_workdir),
                description: "读取 logs、runs、analysis 中的近期文本日志，供 GUI 解析进度和失败信息。".to_string(),
            },
            RemoteCommand {
                id: "sync-down".to_string(),
                label: "回收结果".to_string(),
                command: format!(
                    "rsync -az --partial --append-verify {target}/runs/ {local}/runs/ && rsync -az --partial --append-verify {target}/analysis/ {local}/analysis/",
                    target = remote_target,
                    local = shell_quote(&local_project),
                ),
                description: "回收 runs、analysis、reports、checkpoints 和 trajectories；脚本版本包含完整目录列表。".to_string(),
            },
        ],
        warnings,
    }
}

pub fn slurm_script(plan: &SimulationPlan) -> String {
    let job_name = sanitize_job_name(&plan.name);
    let gpu_line = if plan.resources.gpu_count > 0 {
        format!("#SBATCH --gres=gpu:{}\n", plan.resources.gpu_count)
    } else {
        String::new()
    };
    let queue_line = plan
        .resources
        .queue
        .as_ref()
        .map(|queue| format!("#SBATCH --partition={queue}\n"))
        .unwrap_or_default();
    let hours = plan.resources.walltime_hours.ceil() as u32;

    format!(
        r#"#!/usr/bin/env bash
#SBATCH --job-name={job_name}
#SBATCH --nodes=1
#SBATCH --ntasks={mpi_ranks}
#SBATCH --cpus-per-task={cpu_threads}
{gpu_line}{queue_line}#SBATCH --time={hours}:00:00
#SBATCH --output=logs/%x-%j.out
#SBATCH --error=logs/%x-%j.err

set -euo pipefail

mkdir -p logs
echo "AutoMD plan: {plan_name}"
echo "Engine: {engine_id}"
echo "Started at: $(date -Is)"

# AutoMD will replace this placeholder with the selected engine adapter command.
# For GROMACS this becomes: gmx grompp ... && gmx mdrun ...
# For OpenMM this becomes: python automd_openmm_runner.py ...
automd-run --plan automd-plan.json --engine {engine_id}

echo "Finished at: $(date -Is)"
"#,
        job_name = job_name,
        mpi_ranks = plan.resources.mpi_ranks.max(1),
        cpu_threads = plan.resources.cpu_threads.max(1),
        gpu_line = gpu_line,
        queue_line = queue_line,
        hours = hours.max(1),
        plan_name = plan.name,
        engine_id = plan.engine_id
    )
}

fn scheduler_script(
    plan: &SimulationPlan,
    profile: &RemoteProfile,
    run_directory: &str,
    remote_workdir: &str,
) -> String {
    match &profile.scheduler {
        ExecutionMode::Slurm => slurm_remote_script(plan, profile, run_directory, remote_workdir),
        ExecutionMode::Pbs => pbs_remote_script(plan, profile, run_directory, remote_workdir),
        ExecutionMode::Lsf => lsf_remote_script(plan, profile, run_directory, remote_workdir),
        _ => ssh_remote_script(plan, profile, run_directory, remote_workdir),
    }
}

fn slurm_remote_script(
    plan: &SimulationPlan,
    profile: &RemoteProfile,
    run_directory: &str,
    remote_workdir: &str,
) -> String {
    let job_name = sanitize_job_name(&plan.name);
    let queue_line = plan
        .resources
        .queue
        .as_ref()
        .or(profile.default_queue.as_ref())
        .map(|queue| format!("#SBATCH --partition={queue}\n"))
        .unwrap_or_default();
    let gpu_line = if plan.resources.gpu_count > 0 {
        format!("#SBATCH --gres=gpu:{}\n", plan.resources.gpu_count)
    } else {
        String::new()
    };
    format!(
        r#"#!/usr/bin/env bash
#SBATCH --job-name={job_name}
#SBATCH --nodes=1
#SBATCH --ntasks={mpi_ranks}
#SBATCH --cpus-per-task={cpu_threads}
{gpu_line}{queue_line}#SBATCH --time={walltime}:00:00
#SBATCH --output=logs/%x-%j.out
#SBATCH --error=logs/%x-%j.err

set -euo pipefail
cd {remote_workdir_quoted}
mkdir -p logs runs analysis reports checkpoints trajectories
{modules}
echo "AutoMD remote SLURM job started at $(date -Is)"
{engine_command}
echo "AutoMD remote SLURM job finished at $(date -Is)"
"#,
        job_name = job_name,
        mpi_ranks = plan.resources.mpi_ranks.max(1),
        cpu_threads = plan.resources.cpu_threads.max(1),
        gpu_line = gpu_line,
        queue_line = queue_line,
        walltime = plan.resources.walltime_hours.ceil().max(1.0) as u32,
        remote_workdir_quoted = shell_quote(remote_workdir),
        modules = module_block(profile),
        engine_command = engine_launch_command(plan, run_directory),
    )
}

fn pbs_remote_script(
    plan: &SimulationPlan,
    profile: &RemoteProfile,
    run_directory: &str,
    remote_workdir: &str,
) -> String {
    let queue_line = plan
        .resources
        .queue
        .as_ref()
        .or(profile.default_queue.as_ref())
        .map(|queue| format!("#PBS -q {queue}\n"))
        .unwrap_or_default();
    let gpu_resource = if plan.resources.gpu_count > 0 {
        format!(":ngpus={}", plan.resources.gpu_count)
    } else {
        String::new()
    };
    format!(
        r#"#!/usr/bin/env bash
#PBS -N {job_name}
#PBS -l select=1:ncpus={cpu_threads}:mpiprocs={mpi_ranks}{gpu_resource}
#PBS -l walltime={walltime}:00:00
{queue_line}#PBS -o logs/$PBS_JOBNAME-$PBS_JOBID.out
#PBS -e logs/$PBS_JOBNAME-$PBS_JOBID.err

set -euo pipefail
cd {remote_workdir_quoted}
mkdir -p logs runs analysis reports checkpoints trajectories
{modules}
echo "AutoMD remote PBS job started at $(date -Is)"
{engine_command}
echo "AutoMD remote PBS job finished at $(date -Is)"
"#,
        job_name = sanitize_job_name(&plan.name),
        cpu_threads = plan.resources.cpu_threads.max(1),
        mpi_ranks = plan.resources.mpi_ranks.max(1),
        gpu_resource = gpu_resource,
        walltime = walltime_hhmmss(plan.resources.walltime_hours),
        queue_line = queue_line,
        remote_workdir_quoted = shell_quote(remote_workdir),
        modules = module_block(profile),
        engine_command = engine_launch_command(plan, run_directory),
    )
}

fn lsf_remote_script(
    plan: &SimulationPlan,
    profile: &RemoteProfile,
    run_directory: &str,
    remote_workdir: &str,
) -> String {
    let queue_line = plan
        .resources
        .queue
        .as_ref()
        .or(profile.default_queue.as_ref())
        .map(|queue| format!("#BSUB -q {queue}\n"))
        .unwrap_or_default();
    let gpu_line = if plan.resources.gpu_count > 0 {
        format!("#BSUB -gpu \"num={}\"\n", plan.resources.gpu_count)
    } else {
        String::new()
    };
    format!(
        r#"#!/usr/bin/env bash
#BSUB -J {job_name}
#BSUB -n {ranks}
#BSUB -R "span[hosts=1]"
{gpu_line}{queue_line}#BSUB -W {walltime}
#BSUB -o logs/%J.out
#BSUB -e logs/%J.err

set -euo pipefail
cd {remote_workdir_quoted}
mkdir -p logs runs analysis reports checkpoints trajectories
{modules}
echo "AutoMD remote LSF job started at $(date -Is)"
{engine_command}
echo "AutoMD remote LSF job finished at $(date -Is)"
"#,
        job_name = sanitize_job_name(&plan.name),
        ranks = plan.resources.mpi_ranks.max(1) * plan.resources.cpu_threads.max(1),
        gpu_line = gpu_line,
        queue_line = queue_line,
        walltime = walltime_hhmm(plan.resources.walltime_hours),
        remote_workdir_quoted = shell_quote(remote_workdir),
        modules = module_block(profile),
        engine_command = engine_launch_command(plan, run_directory),
    )
}

fn ssh_remote_script(
    plan: &SimulationPlan,
    profile: &RemoteProfile,
    run_directory: &str,
    remote_workdir: &str,
) -> String {
    format!(
        r#"#!/usr/bin/env bash
set -euo pipefail
cd {remote_workdir_quoted}
mkdir -p logs runs analysis reports checkpoints trajectories
{modules}
echo "AutoMD remote SSH job started at $(date -Is)"
{engine_command}
echo "AutoMD remote SSH job finished at $(date -Is)"
"#,
        remote_workdir_quoted = shell_quote(remote_workdir),
        modules = module_block(profile),
        engine_command = engine_launch_command(plan, run_directory),
    )
}

fn submit_command(profile: &RemoteProfile, remote_workdir: &str, scheduler_filename: &str) -> String {
    let script = shell_quote(scheduler_filename);
    match &profile.scheduler {
        ExecutionMode::Slurm => format!(
            "ssh {host} {remote_cmd}",
            host = shell_quote(&profile.host),
            remote_cmd = shell_quote(&format!("cd {} && sbatch --parsable {script}", shell_quote(remote_workdir))),
        ),
        ExecutionMode::Pbs => format!(
            "ssh {host} {remote_cmd}",
            host = shell_quote(&profile.host),
            remote_cmd = shell_quote(&format!("cd {} && qsub {script}", shell_quote(remote_workdir))),
        ),
        ExecutionMode::Lsf => format!(
            "ssh {host} {remote_cmd}",
            host = shell_quote(&profile.host),
            remote_cmd = shell_quote(&format!("cd {} && bsub < {script}", shell_quote(remote_workdir))),
        ),
        _ => format!(
            "ssh {host} {remote_cmd}",
            host = shell_quote(&profile.host),
            remote_cmd = shell_quote(&format!(
                "cd {} && mkdir -p logs && (nohup bash {script} > logs/automd-ssh.out 2> logs/automd-ssh.err < /dev/null & echo $!)",
                shell_quote(remote_workdir)
            )),
        ),
    }
}

fn status_command(profile: &RemoteProfile) -> String {
    match &profile.scheduler {
        ExecutionMode::Slurm => format!("ssh {} {}", shell_quote(&profile.host), shell_quote("squeue -j <job-id>")),
        ExecutionMode::Pbs => format!("ssh {} {}", shell_quote(&profile.host), shell_quote("qstat <job-id>")),
        ExecutionMode::Lsf => format!("ssh {} {}", shell_quote(&profile.host), shell_quote("bjobs <job-id>")),
        _ => format!("ssh {} {}", shell_quote(&profile.host), shell_quote("ps -p <pid> -o pid,etime,cmd")),
    }
}

fn cancel_command(profile: &RemoteProfile) -> String {
    match &profile.scheduler {
        ExecutionMode::Slurm => format!("ssh {} {}", shell_quote(&profile.host), shell_quote("scancel <job-id>")),
        ExecutionMode::Pbs => format!("ssh {} {}", shell_quote(&profile.host), shell_quote("qdel <job-id>")),
        ExecutionMode::Lsf => format!("ssh {} {}", shell_quote(&profile.host), shell_quote("bkill <job-id>")),
        _ => format!("ssh {} {}", shell_quote(&profile.host), shell_quote("kill <pid>")),
    }
}

fn tail_log_command(profile: &RemoteProfile, remote_workdir: &str) -> String {
    let remote_cmd = format!(
        "cd {} && tail -n 200 logs/*.out logs/*.err runs/*/*.log analysis/*.log 2>/dev/null || true",
        shell_quote(remote_workdir)
    );
    format!(
        "ssh {} {}",
        shell_quote(&profile.host),
        shell_quote(&remote_cmd)
    )
}

fn scheduler_language(scheduler: &ExecutionMode) -> &'static str {
    match scheduler {
        ExecutionMode::Slurm => "slurm",
        ExecutionMode::Pbs => "pbs",
        ExecutionMode::Lsf => "lsf",
        _ => "bash",
    }
}

fn module_block(profile: &RemoteProfile) -> String {
    if profile.module_load.is_empty() {
        "# No module commands configured.".to_string()
    } else {
        profile.module_load.join("\n")
    }
}

fn engine_launch_command(plan: &SimulationPlan, run_directory: &str) -> String {
    match plan.engine_id.as_str() {
        "openmm" => format!("bash {}", shell_quote(&format!("{run_directory}/run-openmm.sh"))),
        "gromacs" => format!("bash {}", shell_quote(&format!("{run_directory}/run-gromacs.sh"))),
        "ambertools" => format!("bash {}", shell_quote(&format!("{run_directory}/run-ambertools.sh"))),
        "namd" => format!("bash {}", shell_quote(&format!("{run_directory}/run-namd.sh"))),
        "lammps" => format!("bash {}", shell_quote(&format!("{run_directory}/run-lammps.sh"))),
        "cp2k" => format!("bash {}", shell_quote(&format!("{run_directory}/run-cp2k.sh"))),
        "genesis" => format!("bash {}", shell_quote(&format!("{run_directory}/run-genesis.sh"))),
        "hoomd" => format!("bash {}", shell_quote(&format!("{run_directory}/run-hoomd.sh"))),
        "dl_poly" => format!("bash {}", shell_quote(&format!("{run_directory}/run-dl-poly.sh"))),
        "tinker" => format!("bash {}", shell_quote(&format!("{run_directory}/run-tinker.sh"))),
        "amber_pmemd" => format!("bash {}", shell_quote(&format!("{run_directory}/run-amber-pmemd.sh"))),
        "charmm" => format!("bash {}", shell_quote(&format!("{run_directory}/run-charmm.sh"))),
        "desmond" => format!("bash {}", shell_quote(&format!("{run_directory}/run-desmond.sh"))),
        "acemd" => format!("bash {}", shell_quote(&format!("{run_directory}/run-acemd.sh"))),
        other => format!("automd-run --plan generated/{other}/automd-plan.json --engine {other}"),
    }
}

fn engine_run_directory(plan: &SimulationPlan) -> String {
    match plan.engine_id.as_str() {
        "openmm" => format!("runs/openmm-{}", plan.id.simple()),
        "gromacs" => format!("runs/gromacs-{}", plan.id.simple()),
        other => format!("runs/{other}-{}", plan.id.simple()),
    }
}

fn remote_warnings(plan: &SimulationPlan, profile: &RemoteProfile) -> Vec<String> {
    let mut warnings = Vec::new();
    if profile.host.contains(char::is_whitespace) {
        warnings.push("Remote host contains whitespace; review SSH target before running generated commands.".to_string());
    }
    if matches!(&profile.scheduler, ExecutionMode::Ssh) && plan.resources.walltime_hours > 24.0 {
        warnings.push("Pure SSH mode does not enforce walltime; long jobs should use SLURM/PBS/LSF when available.".to_string());
    }
    if plan.resources.gpu_count > 0 && matches!(&profile.scheduler, ExecutionMode::Pbs | ExecutionMode::Lsf) {
        warnings.push("GPU resource syntax varies by cluster; review ngpus/BSUB -gpu options with your site policy.".to_string());
    }
    if !matches!(
        plan.engine_id.as_str(),
        "gromacs"
            | "openmm"
            | "ambertools"
            | "namd"
            | "lammps"
            | "cp2k"
            | "genesis"
            | "hoomd"
            | "dl_poly"
            | "tinker"
            | "amber_pmemd"
            | "charmm"
            | "desmond"
            | "acemd"
    ) {
        warnings.push("This engine currently uses a generic remote launcher placeholder until its full adapter lands.".to_string());
    }
    warnings
}

fn walltime_hhmmss(hours: f32) -> String {
    let total_minutes = (hours.max(0.1) * 60.0).ceil() as u32;
    format!("{:02}:{:02}:00", total_minutes / 60, total_minutes % 60)
}

fn walltime_hhmm(hours: f32) -> String {
    let total_minutes = (hours.max(0.1) * 60.0).ceil() as u32;
    format!("{}:{:02}", total_minutes / 60, total_minutes % 60)
}

pub fn container_recipe(engine_id: &str) -> ContainerRecipe {
    match engine_id {
        "gromacs" => ContainerRecipe {
            engine_id: engine_id.to_string(),
            title: "GROMACS CPU/GPU container recipe".to_string(),
            files: vec![GeneratedFile {
                path: "containers/gromacs.Containerfile".to_string(),
                language: "dockerfile".to_string(),
                contents: r#"FROM ubuntu:24.04
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates gromacs openmpi-bin plumed \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /work
ENTRYPOINT ["gmx"]
"#
                .to_string(),
            }],
            notes: vec![
                "用于开源引擎快速验证；高性能 GPU 构建建议使用编译向导。".to_string(),
                "HPC 推荐转为 Apptainer/Singularity 镜像。".to_string(),
            ],
        },
        "openmm" => ContainerRecipe {
            engine_id: engine_id.to_string(),
            title: "OpenMM Python sidecar container recipe".to_string(),
            files: vec![GeneratedFile {
                path: "containers/openmm.Containerfile".to_string(),
                language: "dockerfile".to_string(),
                contents: r#"FROM mambaorg/micromamba:1.5.10
RUN micromamba install -y -n base -c conda-forge \
    python=3.11 openmm pdbfixer mdtraj mdanalysis rdkit openbabel \
 && micromamba clean --all --yes
WORKDIR /work
ENTRYPOINT ["python"]
"#
                .to_string(),
            }],
            notes: vec!["Python 科学侧车镜像，适合 OpenMM 与分析任务。".to_string()],
        },
        other => ContainerRecipe {
            engine_id: other.to_string(),
            title: format!("{other} generic container recipe"),
            files: vec![GeneratedFile {
                path: format!("containers/{other}.Containerfile"),
                language: "dockerfile".to_string(),
                contents: format!(
                    r#"FROM ubuntu:24.04
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates build-essential cmake git python3 \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /work
# Add {other} installation steps here, or point AutoMD to a user-provided binary.
"#
                ),
            }],
            notes: vec![
                "这是通用开源引擎模板；受限/商业引擎不能由 AutoMD 镜像分发。".to_string(),
            ],
        },
    }
}

pub fn build_recipe(options: BuildRecipeOptions) -> BuildRecipe {
    let engine_id = options.engine_id.clone();
    match engine_id.as_str() {
        "gromacs" => gromacs_build_recipe(options),
        "cp2k" => cp2k_build_recipe(options),
        other => generic_build_recipe(other, options),
    }
}

pub fn export_recipe_package(request: RecipeExportRequest) -> Result<RecipeExportResult, String> {
    let project_root = PathBuf::from(&request.project_path);
    if !project_root.exists() {
        return Err(format!("project path does not exist: {}", request.project_path));
    }

    let engine_id = request.build_options.engine_id.clone();
    let directory = format!("build-recipes/{}", sanitize_job_name(&engine_id));
    let mut warnings = Vec::new();
    let mut files = Vec::new();

    if request.include_container {
        let container = container_recipe(&engine_id);
        warnings.extend(container.notes);
        for file in container.files {
            files.push(GeneratedFile {
                path: format!("{directory}/{}", file.path),
                language: file.language,
                contents: file.contents,
            });
        }
    }

    if request.include_build_script {
        let build = build_recipe(request.build_options.clone());
        warnings.extend(build.warnings.clone());
        files.push(GeneratedFile {
            path: format!("{directory}/build-{engine_id}.sh"),
            language: "bash".to_string(),
            contents: build.script.clone(),
        });
        files.push(GeneratedFile {
            path: format!("{directory}/README.md"),
            language: "markdown".to_string(),
            contents: build_recipe_readme(&engine_id, &directory, &build),
        });
        files.push(GeneratedFile {
            path: format!("{directory}/automd-build-recipe.json"),
            language: "json".to_string(),
            contents: to_string_pretty(&build).map_err(|error| error.to_string())?,
        });
    } else {
        files.push(GeneratedFile {
            path: format!("{directory}/README.md"),
            language: "markdown".to_string(),
            contents: format!(
                "# AutoMD {engine_id} container recipe\n\nThis package contains container recipe files generated by AutoMD. Build and run images only in environments where the engine license and upstream distribution policy allow it.\n"
            ),
        });
    }

    if files.is_empty() {
        warnings.push("No recipe files were selected for export.".to_string());
    }

    for file in &files {
        let destination = safe_join(&project_root, &file.path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(&destination, &file.contents).map_err(|error| error.to_string())?;
        #[cfg(unix)]
        if file.path.ends_with(".sh") {
            let mut permissions = fs::metadata(&destination)
                .map_err(|error| error.to_string())?
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&destination, permissions).map_err(|error| error.to_string())?;
        }
    }

    Ok(RecipeExportResult {
        engine_id,
        directory,
        files,
        warnings,
    })
}

fn gromacs_build_recipe(options: BuildRecipeOptions) -> BuildRecipe {
    let prefix = options.install_prefix.unwrap_or_else(|| "$HOME/.local/automd/gromacs".to_string());
    let mpi = if options.enable_mpi { "-DGMX_MPI=ON" } else { "-DGMX_MPI=OFF" };
    let gpu = match (options.enable_gpu, options.gpu_backend) {
        (true, Some(GpuBackend::Cuda)) => "-DGMX_GPU=CUDA",
        (true, Some(GpuBackend::OpenCl)) => "-DGMX_GPU=OpenCL",
        (true, Some(GpuBackend::Sycl)) => "-DGMX_GPU=SYCL",
        (true, _) => "-DGMX_GPU=CUDA",
        (false, _) => "-DGMX_GPU=OFF",
    };
    let plumed = if options.enable_plumed {
        "\n# Build PLUMED separately and patch GROMACS before cmake when required by your target version."
    } else {
        ""
    };
    let script = format!(
        r#"#!/usr/bin/env bash
set -euo pipefail

version="${{GROMACS_VERSION:-2026.1}}"
prefix="{prefix}"

curl -L "https://ftp.gromacs.org/gromacs/gromacs-${{version}}.tar.gz" -o "gromacs-${{version}}.tar.gz"
tar xf "gromacs-${{version}}.tar.gz"
cmake -S "gromacs-${{version}}" -B "build-gromacs" \
  -DCMAKE_INSTALL_PREFIX="${{prefix}}" \
  -DGMX_BUILD_OWN_FFTW=ON \
  {mpi} \
  {gpu}
cmake --build "build-gromacs" --parallel
cmake --install "build-gromacs"
{plumed}
"#
    );

    BuildRecipe {
        engine_id: "gromacs".to_string(),
        title: "GROMACS source build recipe".to_string(),
        script,
        steps: vec![
            "下载 GROMACS 源码。".to_string(),
            "根据 MPI/GPU/PLUMED 选项生成 CMake 配置。".to_string(),
            "编译并安装到用户目录。".to_string(),
        ],
        warnings: vec!["GPU/MPI 组合强依赖驱动、编译器和目标平台；AutoMD 只生成脚本并记录日志。".to_string()],
    }
}

fn cp2k_build_recipe(options: BuildRecipeOptions) -> BuildRecipe {
    let prefix = options.install_prefix.unwrap_or_else(|| "$HOME/.local/automd/cp2k".to_string());
    let script = format!(
        r#"#!/usr/bin/env bash
set -euo pipefail

prefix="{prefix}"
git clone --recursive https://github.com/cp2k/cp2k.git
cd cp2k/tools/toolchain
./install_cp2k_toolchain.sh --with-openmpi=install --with-libint=install --with-libxc=install
cd ../..
source tools/toolchain/install/setup
make -j ARCH=local VERSION=psmp
mkdir -p "${{prefix}}/bin"
cp exe/local/cp2k.psmp "${{prefix}}/bin/"
"#
    );

    BuildRecipe {
        engine_id: "cp2k".to_string(),
        title: "CP2K source build recipe".to_string(),
        script,
        steps: vec![
            "拉取 CP2K 源码和子模块。".to_string(),
            "使用官方 toolchain 准备 MPI/libint/libxc。".to_string(),
            "构建 psmp 可执行文件并复制到安装目录。".to_string(),
        ],
        warnings: vec![
            "CP2K 编译耗时较长，建议优先在 Linux/HPC 环境运行。".to_string(),
            "GPU 构建需要按目标集群工具链单独调整。".to_string(),
        ],
    }
}

fn generic_build_recipe(engine_id: &str, options: BuildRecipeOptions) -> BuildRecipe {
    let prefix = options.install_prefix.unwrap_or_else(|| format!("$HOME/.local/automd/{engine_id}"));
    BuildRecipe {
        engine_id: engine_id.to_string(),
        title: format!("{engine_id} generic build checklist"),
        script: format!(
            r#"#!/usr/bin/env bash
set -euo pipefail

prefix="{prefix}"
mkdir -p "${{prefix}}"
echo "Use this placeholder to compile {engine_id} after confirming its upstream build instructions and license."
"#
        ),
        steps: vec![
            "确认上游许可证和平台支持。".to_string(),
            "下载源码或配置用户已有源码目录。".to_string(),
            "检查 CMake/Make/MPI/GPU/PLUMED 依赖。".to_string(),
            "编译后在 AutoMD 引擎设置中登记可执行文件路径。".to_string(),
        ],
        warnings: vec!["受限/商业引擎不能通过 AutoMD 自动下载或分发。".to_string()],
    }
}

fn sanitize_job_name(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' { ch } else { '-' })
        .collect();
    let trimmed = sanitized.trim_matches('-');
    if trimmed.is_empty() {
        "automd".to_string()
    } else {
        trimmed.to_string()
    }
}

fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '/' | '.' | '_' | '-' | '$'))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn build_recipe_readme(engine_id: &str, directory: &str, build: &BuildRecipe) -> String {
    let steps = build
        .steps
        .iter()
        .map(|step| format!("- {step}"))
        .collect::<Vec<_>>()
        .join("\n");
    let warnings = build
        .warnings
        .iter()
        .map(|warning| format!("- {warning}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"# AutoMD {engine_id} build recipe

This folder was generated by AutoMD as a reproducible build checklist. Review the script before execution, especially for GPU, MPI, PLUMED, compiler, and license-sensitive engines.

## Command

```bash
bash {directory}/build-{engine_id}.sh 2>&1 | tee {directory}/build.log
```

## Steps

{steps}

## Warnings

{warnings}

After a successful build, register the executable or module path in AutoMD's engine settings so future runs can detect the installation.
"#
    )
}

fn safe_join(root: &Path, relative: &str) -> PathBuf {
    let mut destination = root.to_path_buf();
    for component in Path::new(relative).components() {
        if let Component::Normal(value) = component {
            destination.push(value);
        }
    }
    destination
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planner;

    #[test]
    fn slurm_script_mentions_engine_and_resources() {
        let plan = planner::default_simulation_plan(PlanRequest {
            project_id: None,
            name: "demo".to_string(),
            engine_id: "gromacs".to_string(),
            domain: ProjectDomain::Biomolecular,
        });
        let script = slurm_script(&plan);
        assert!(script.contains("#SBATCH --job-name=demo"));
        assert!(script.contains("--engine gromacs"));
    }

    #[test]
    fn remote_slurm_package_contains_sync_submit_and_run_script() {
        let plan = planner::default_simulation_plan(PlanRequest {
            project_id: None,
            name: "remote demo".to_string(),
            engine_id: "gromacs".to_string(),
            domain: ProjectDomain::Biomolecular,
        });
        let package = remote_execution_package(RemoteExecutionRequest {
            plan,
            profile: RemoteProfile {
                id: "slurm-test".to_string(),
                name: "SLURM test".to_string(),
                host: "login.example".to_string(),
                username: String::new(),
                port: 22,
                auth_method: RemoteAuthMethod::Agent,
                identity_file: None,
                scheduler: ExecutionMode::Slurm,
                workdir: "/scratch/$USER/automd".to_string(),
                module_load: vec!["module load gromacs".to_string()],
                default_queue: Some("gpu".to_string()),
            },
            local_project_path: Some("/tmp/AutoMD project".to_string()),
            include_submit: true,
        });

        assert_eq!(package.scheduler, ExecutionMode::Slurm);
        assert!(package.files.iter().any(|file| file.path == "remote/submit.slurm"));
        assert!(package
            .commands
            .iter()
            .any(|command| command.id == "submit" && command.command.contains("sbatch --parsable")));
        assert!(package
            .commands
            .iter()
            .any(|command| command.id == "sync-up" && command.command.contains("rsync -az")));
    }

    #[test]
    fn remote_ssh_submit_command_detaches_job() {
        let plan = planner::default_simulation_plan(PlanRequest {
            project_id: None,
            name: "ssh demo".to_string(),
            engine_id: "gromacs".to_string(),
            domain: ProjectDomain::Biomolecular,
        });
        let package = remote_execution_package(RemoteExecutionRequest {
            plan,
            profile: RemoteProfile {
                id: "ssh-test".to_string(),
                name: "SSH test".to_string(),
                host: "workstation.example".to_string(),
                username: String::new(),
                port: 22,
                auth_method: RemoteAuthMethod::Agent,
                identity_file: None,
                scheduler: ExecutionMode::Ssh,
                workdir: "/scratch/$USER/automd".to_string(),
                module_load: vec![],
                default_queue: None,
            },
            local_project_path: Some("/tmp/AutoMD project".to_string()),
            include_submit: true,
        });

        let submit = package
            .commands
            .iter()
            .find(|command| command.id == "submit")
            .expect("submit command");
        assert!(submit.command.contains("mkdir -p logs"));
        assert!(submit.command.contains("(nohup bash remote/run-ssh.sh"));
        assert!(submit.command.contains("< /dev/null & echo $!)"));
    }

    #[test]
    fn remote_scheduler_scripts_cover_pbs_and_lsf() {
        let mut plan = planner::default_simulation_plan(PlanRequest {
            project_id: None,
            name: "scheduler-demo".to_string(),
            engine_id: "openmm".to_string(),
            domain: ProjectDomain::Biomolecular,
        });
        plan.resources.gpu_count = 1;

        let pbs = remote_execution_package(RemoteExecutionRequest {
            plan: plan.clone(),
            profile: RemoteProfile {
                id: "pbs-test".to_string(),
                name: "PBS test".to_string(),
                host: "pbs.example".to_string(),
                username: String::new(),
                port: 22,
                auth_method: RemoteAuthMethod::Agent,
                identity_file: None,
                scheduler: ExecutionMode::Pbs,
                workdir: "/work/automd".to_string(),
                module_load: vec!["module load openmm".to_string()],
                default_queue: Some("batch".to_string()),
            },
            local_project_path: None,
            include_submit: true,
        });
        let pbs_script = &pbs.files.iter().find(|file| file.path == "remote/submit.pbs").expect("pbs script").contents;
        assert!(pbs_script.contains("#PBS -l select=1"));
        assert!(pbs_script.contains(":ngpus=1"));
        assert!(pbs.commands.iter().any(|command| command.command.contains("qsub")));

        let lsf = remote_execution_package(RemoteExecutionRequest {
            plan,
            profile: RemoteProfile {
                id: "lsf-test".to_string(),
                name: "LSF test".to_string(),
                host: "lsf.example".to_string(),
                username: String::new(),
                port: 22,
                auth_method: RemoteAuthMethod::Agent,
                identity_file: None,
                scheduler: ExecutionMode::Lsf,
                workdir: "/work/automd".to_string(),
                module_load: vec!["module load openmm".to_string()],
                default_queue: Some("normal".to_string()),
            },
            local_project_path: None,
            include_submit: true,
        });
        let lsf_script = &lsf.files.iter().find(|file| file.path == "remote/submit.lsf").expect("lsf script").contents;
        assert!(lsf_script.contains("#BSUB -gpu"));
        assert!(lsf.commands.iter().any(|command| command.command.contains("bsub <")));
    }

    #[test]
    fn export_recipe_package_writes_build_manifest_and_scripts() {
        let root = std::env::temp_dir().join(format!("automd-build-recipes-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("project root");

        let exported = export_recipe_package(RecipeExportRequest {
            project_path: root.display().to_string(),
            build_options: BuildRecipeOptions {
                engine_id: "gromacs".to_string(),
                enable_mpi: true,
                enable_gpu: false,
                gpu_backend: None,
                enable_plumed: true,
                install_prefix: None,
            },
            include_container: true,
            include_build_script: true,
        })
        .expect("export recipe package");

        assert_eq!(exported.directory, "build-recipes/gromacs");
        assert!(root.join("build-recipes/gromacs/build-gromacs.sh").exists());
        assert!(root.join("build-recipes/gromacs/automd-build-recipe.json").exists());
        assert!(root.join("build-recipes/gromacs/containers/gromacs.Containerfile").exists());
        assert!(exported.files.iter().any(|file| file.path.ends_with("README.md")));

        std::fs::remove_dir_all(root).expect("cleanup");
    }
}
