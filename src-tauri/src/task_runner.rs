use crate::artifacts;
use crate::engine_adapters;
use crate::models::*;
use crate::runtime;
use chrono::Utc;
use serde_json::to_string_pretty;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use thiserror::Error;
use uuid::Uuid;

const LOG_TAIL_LIMIT: usize = 200;

#[derive(Clone)]
pub struct TaskManager {
    tasks: Arc<Mutex<HashMap<Uuid, Arc<Mutex<ManagedTask>>>>>,
    workspace_root: PathBuf,
}

struct ManagedTask {
    snapshot: LocalTaskSnapshot,
    cancel_requested: bool,
    child: Option<Child>,
    plan: SimulationPlan,
    project_root: PathBuf,
}

#[derive(Debug, Error)]
pub enum TaskRunnerError {
    #[error("local task not found: {0}")]
    TaskNotFound(Uuid),
    #[error("real local runs require a project path")]
    MissingProjectPath,
    #[error("engine run package error: {0}")]
    EnginePackage(String),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

struct ProcessSpec {
    program: String,
    args: Vec<String>,
    cwd: PathBuf,
    display: String,
}

impl TaskManager {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
            workspace_root,
        }
    }

    pub fn start(&self, request: StartLocalRunRequest) -> Result<LocalTaskSnapshot, TaskRunnerError> {
        let task_id = Uuid::new_v4();
        let plan = request.plan.clone();
        let project_root = self.resolve_project_root(&request)?;
        fs::create_dir_all(&project_root)?;

        let write_package = request.write_package || !matches!(request.mode, LocalRunMode::DryRun);
        let package = engine_adapters::prepare_run_package(EngineRunRequest {
            plan: request.plan.clone(),
            project_path: Some(project_root.display().to_string()),
            write_to_disk: write_package,
        })
        .map_err(|error| TaskRunnerError::EnginePackage(error.to_string()))?;

        let run_directory = package.run_directory.clone();
        let run_root = safe_join(&project_root, &run_directory);
        fs::create_dir_all(&run_root)?;

        let mut snapshot = LocalTaskSnapshot {
            id: task_id,
            plan_id: plan.id,
            engine_id: plan.engine_id.clone(),
            mode: request.mode.clone(),
            status: TaskStatus::Queued,
            run_directory,
            command: String::new(),
            progress_percent: 0.0,
            ns_per_day: None,
            current_step: None,
            log_tail: vec![
                format!("Prepared {} run package.", plan.engine_id),
                format!("Working directory: {}", project_root.display()),
            ],
            error_message: None,
            exit_code: None,
            artifacts: Vec::new(),
            report_path: None,
            failure_analysis: None,
            resume_plan: None,
            started_at: Utc::now(),
            finished_at: None,
        };

        if matches!(request.mode, LocalRunMode::DryRun) {
            snapshot.status = TaskStatus::Completed;
            snapshot.progress_percent = 100.0;
            snapshot.command = "dry-run package generation only".to_string();
            snapshot.finished_at = Some(Utc::now());
            append_log(&mut snapshot, "Dry run completed without launching a process.".to_string());
            write_run_manifest(&snapshot, &plan, &project_root)?;
            let mut managed = ManagedTask {
                snapshot: snapshot.clone(),
                cancel_requested: false,
                child: None,
                plan,
                project_root,
            };
            attach_artifacts_and_report(&mut managed);
            snapshot = managed.snapshot.clone();
            let record = Arc::new(Mutex::new(managed));
            self.tasks.lock().expect("task map lock").insert(task_id, record);
            return Ok(snapshot);
        }

        let command = process_spec_for(&request.mode, &plan, &project_root, &snapshot.run_directory, &self.workspace_root)?;
        snapshot.command = command.display.clone();
        write_run_manifest(&snapshot, &plan, &project_root)?;

        let record = Arc::new(Mutex::new(ManagedTask {
            snapshot: snapshot.clone(),
            cancel_requested: false,
            child: None,
            plan: plan.clone(),
            project_root: project_root.clone(),
        }));
        self.tasks.lock().expect("task map lock").insert(task_id, Arc::clone(&record));

        thread::spawn(move || run_process(record, command, plan.engine_id));

        Ok(snapshot)
    }

    pub fn snapshot(&self, task_id: Uuid) -> Result<LocalTaskSnapshot, TaskRunnerError> {
        let task = {
            let tasks = self.tasks.lock().expect("task map lock");
            tasks
                .get(&task_id)
                .cloned()
                .ok_or(TaskRunnerError::TaskNotFound(task_id))?
        };
        let snapshot = task.lock().expect("task lock").snapshot.clone();
        Ok(snapshot)
    }

    pub fn list(&self) -> Vec<LocalTaskSnapshot> {
        self.tasks
            .lock()
            .expect("task map lock")
            .values()
            .map(|task| task.lock().expect("task lock").snapshot.clone())
            .collect()
    }

    pub fn cancel(&self, task_id: Uuid) -> Result<LocalTaskSnapshot, TaskRunnerError> {
        let task = {
            let tasks = self.tasks.lock().expect("task map lock");
            tasks
                .get(&task_id)
                .cloned()
                .ok_or(TaskRunnerError::TaskNotFound(task_id))?
        };

        let mut task = task.lock().expect("task lock");
        task.cancel_requested = true;
        if let Some(child) = task.child.as_mut() {
            let _ = child.kill();
        }
        task.snapshot.status = TaskStatus::Cancelled;
        task.snapshot.finished_at = Some(Utc::now());
        append_log(&mut task.snapshot, "Cancellation requested.".to_string());
        attach_resume_plan(&mut task);
        Ok(task.snapshot.clone())
    }

    fn resolve_project_root(&self, request: &StartLocalRunRequest) -> Result<PathBuf, TaskRunnerError> {
        match (&request.project_path, &request.mode) {
            (Some(path), _) => Ok(PathBuf::from(path)),
            (None, LocalRunMode::Real) => Err(TaskRunnerError::MissingProjectPath),
            (None, _) => Ok(std::env::temp_dir().join("automd").join(request.plan.id.to_string())),
        }
    }
}

fn process_spec_for(
    mode: &LocalRunMode,
    plan: &SimulationPlan,
    project_root: &Path,
    run_directory: &str,
    workspace_root: &Path,
) -> Result<ProcessSpec, TaskRunnerError> {
    match mode {
        LocalRunMode::DryRun => unreachable!("dry run never spawns a process"),
        LocalRunMode::Mock => {
            let plan_path = project_root
                .join("generated")
                .join(engine_generated_slug(&plan.engine_id))
                .join("automd-plan.json");
            if !plan_path.exists() {
                if let Some(parent) = plan_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&plan_path, to_string_pretty(plan)?)?;
            }
            let script = workspace_root.join("scripts/automd_mock_engine.py");
            let out = safe_join(project_root, run_directory);
            Ok(ProcessSpec {
                program: "python3".to_string(),
                args: vec![
                    script.display().to_string(),
                    "--plan".to_string(),
                    plan_path.display().to_string(),
                    "--out".to_string(),
                    out.display().to_string(),
                    "--sleep".to_string(),
                    "0.05".to_string(),
                ],
                cwd: project_root.to_path_buf(),
                display: format!(
                    "python3 {} --plan {} --out {} --sleep 0.05",
                    script.display(),
                    plan_path.display(),
                    out.display()
                ),
            })
        }
        LocalRunMode::Real => {
            let script = safe_join(project_root, run_directory).join(engine_run_script_name(&plan.engine_id));
            Ok(ProcessSpec {
                program: shell_program(),
                args: shell_args(&script),
                cwd: project_root.to_path_buf(),
                display: format!("{} {}", shell_program(), script.display()),
            })
        }
    }
}

fn engine_generated_slug(engine_id: &str) -> &str {
    match engine_id {
        "openmm" => "openmm",
        "ambertools" => "ambertools",
        "namd" => "namd",
        "lammps" => "lammps",
        "cp2k" => "cp2k",
        "genesis" => "genesis",
        "hoomd" => "hoomd",
        "dl_poly" => "dl_poly",
        "tinker" => "tinker",
        "amber_pmemd" => "amber_pmemd",
        "charmm" => "charmm",
        "desmond" => "desmond",
        "acemd" => "acemd",
        _ => "gromacs",
    }
}

fn engine_run_script_name(engine_id: &str) -> &str {
    match engine_id {
        "openmm" => "run-openmm.sh",
        "ambertools" => "run-ambertools.sh",
        "namd" => "run-namd.sh",
        "lammps" => "run-lammps.sh",
        "cp2k" => "run-cp2k.sh",
        "genesis" => "run-genesis.sh",
        "hoomd" => "run-hoomd.sh",
        "dl_poly" => "run-dl-poly.sh",
        "tinker" => "run-tinker.sh",
        "amber_pmemd" => "run-amber-pmemd.sh",
        "charmm" => "run-charmm.sh",
        "desmond" => "run-desmond.sh",
        "acemd" => "run-acemd.sh",
        _ => "run-gromacs.sh",
    }
}

fn run_process(record: Arc<Mutex<ManagedTask>>, spec: ProcessSpec, engine_id: String) {
    {
        let mut task = record.lock().expect("task lock");
        task.snapshot.status = TaskStatus::Preparing;
        append_log(&mut task.snapshot, format!("Launching: {}", spec.display));
    }

    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .current_dir(&spec.cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            let mut task = record.lock().expect("task lock");
            task.snapshot.status = TaskStatus::Failed;
            task.snapshot.error_message = Some(error.to_string());
            task.snapshot.finished_at = Some(Utc::now());
            append_log(&mut task.snapshot, format!("Failed to spawn process: {error}"));
            let log_contents = task.snapshot.log_tail.join("\n");
            attach_failure_analysis(&mut task, log_contents, None);
            attach_artifacts(&mut task);
            return;
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    {
        let mut task = record.lock().expect("task lock");
        task.child = Some(child);
        task.snapshot.status = TaskStatus::Running;
        append_log(&mut task.snapshot, "Process started.".to_string());
    }

    let (tx, rx) = mpsc::channel::<String>();
    spawn_reader(stdout, "stdout", tx.clone());
    spawn_reader(stderr, "stderr", tx);

    loop {
        while let Ok(line) = rx.try_recv() {
            let mut task = record.lock().expect("task lock");
            apply_log_line(&mut task.snapshot, &engine_id, line);
        }

        let mut finished = None;
        {
            let mut task = record.lock().expect("task lock");
            if task.cancel_requested {
                if let Some(child) = task.child.as_mut() {
                    let _ = child.kill();
                }
                task.snapshot.status = TaskStatus::Cancelled;
                task.snapshot.finished_at = Some(Utc::now());
                append_log(&mut task.snapshot, "Process cancelled.".to_string());
                attach_artifacts(&mut task);
                break;
            }

            if let Some(child) = task.child.as_mut() {
                match child.try_wait() {
                    Ok(Some(status)) => finished = Some(status.code()),
                    Ok(None) => {}
                    Err(error) => {
                        task.snapshot.status = TaskStatus::Failed;
                        task.snapshot.error_message = Some(error.to_string());
                        task.snapshot.finished_at = Some(Utc::now());
                        append_log(&mut task.snapshot, format!("Failed while waiting for process: {error}"));
                        let log_contents = task.snapshot.log_tail.join("\n");
                        let exit_code = task.snapshot.exit_code;
                        attach_failure_analysis(&mut task, log_contents, exit_code);
                        attach_artifacts(&mut task);
                        break;
                    }
                }
            }
        }

        if let Some(exit_code) = finished {
            while let Ok(line) = rx.try_recv() {
                let mut task = record.lock().expect("task lock");
                apply_log_line(&mut task.snapshot, &engine_id, line);
            }
            let mut task = record.lock().expect("task lock");
            task.child = None;
            task.snapshot.exit_code = exit_code;
            task.snapshot.finished_at = Some(Utc::now());
            if exit_code == Some(0) && task.snapshot.error_message.is_none() {
                task.snapshot.status = TaskStatus::Completed;
                task.snapshot.progress_percent = 100.0;
                append_log(&mut task.snapshot, "Process completed successfully.".to_string());
                attach_artifacts_and_report(&mut task);
            } else if task.snapshot.status != TaskStatus::Cancelled {
                task.snapshot.status = TaskStatus::Failed;
                if task.snapshot.error_message.is_none() {
                    task.snapshot.error_message = Some(format!("Process exited with code {exit_code:?}"));
                }
                append_log(&mut task.snapshot, format!("Process failed with code {exit_code:?}."));
                let log_contents = task.snapshot.log_tail.join("\n");
                attach_failure_analysis(&mut task, log_contents, exit_code);
                attach_artifacts(&mut task);
            }
            break;
        }

        thread::sleep(Duration::from_millis(100));
    }
}

fn attach_artifacts_and_report(task: &mut ManagedTask) {
    match collect_task_artifacts(task) {
        Ok(index) => {
            task.snapshot.artifacts = index.artifacts.clone();
            attach_resume_plan(task);
            match artifacts::export_report(ReportExportRequest {
                project_path: task.project_root.display().to_string(),
                plan: task.plan.clone(),
                task: Some(task.snapshot.clone()),
                artifact_index: Some(index.clone()),
                format: ReportFormat::Markdown,
            }) {
                Ok(markdown) => {
                    task.snapshot.report_path = Some(markdown.path.clone());
                    append_log(&mut task.snapshot, format!("Report written: {}", markdown.path));
                }
                Err(error) => append_log(&mut task.snapshot, format!("Report export failed: {error}")),
            }
            match artifacts::export_report(ReportExportRequest {
                project_path: task.project_root.display().to_string(),
                plan: task.plan.clone(),
                task: Some(task.snapshot.clone()),
                artifact_index: Some(index),
                format: ReportFormat::Html,
            }) {
                Ok(html) => append_log(&mut task.snapshot, format!("HTML report written: {}", html.path)),
                Err(error) => append_log(&mut task.snapshot, format!("HTML report export failed: {error}")),
            }
        }
        Err(error) => append_log(&mut task.snapshot, format!("Artifact indexing failed: {error}")),
    }
}

fn attach_artifacts(task: &mut ManagedTask) {
    match collect_task_artifacts(task) {
        Ok(index) => {
            task.snapshot.artifacts = index.artifacts;
            let artifact_count = task.snapshot.artifacts.len();
            append_log(
                &mut task.snapshot,
                format!("Indexed {artifact_count} artifacts for diagnostics."),
            );
            attach_resume_plan(task);
        }
        Err(error) => append_log(&mut task.snapshot, format!("Artifact indexing failed: {error}")),
    }
}

fn attach_failure_analysis(task: &mut ManagedTask, log_contents: String, exit_code: Option<i32>) {
    match engine_adapters::classify_engine_failure(FailureAnalysisRequest {
        engine_id: task.snapshot.engine_id.clone(),
        log_contents,
        exit_code,
    }) {
        Ok(analysis) => {
            let label = failure_category_label(&analysis.category);
            let first_suggestion = analysis
                .suggestions
                .first()
                .map(|suggestion| format!(" Suggested next step: {}", suggestion.title))
                .unwrap_or_default();
            task.snapshot.error_message = Some(format!("{label}: {}", analysis.message));
            append_log(
                &mut task.snapshot,
                format!("Failure diagnosis: {label}.{first_suggestion}"),
            );
            task.snapshot.failure_analysis = Some(analysis);
        }
        Err(error) => append_log(&mut task.snapshot, format!("Failure diagnosis unavailable: {error}")),
    }
}

fn attach_resume_plan(task: &mut ManagedTask) {
    match engine_adapters::discover_resume_plan(ResumePlanRequest {
        project_path: task.project_root.display().to_string(),
        run_directory: task.snapshot.run_directory.clone(),
        engine_id: task.snapshot.engine_id.clone(),
    }) {
        Ok(resume_plan) => {
            let checkpoint_count = resume_plan.checkpoints.len();
            if let Some(command) = resume_plan.resume_command.clone() {
                append_log(
                    &mut task.snapshot,
                    format!("Resume checkpoint found ({checkpoint_count} total): {command}"),
                );
            }
            task.snapshot.resume_plan = Some(resume_plan);
        }
        Err(error) => append_log(&mut task.snapshot, format!("Checkpoint discovery unavailable: {error}")),
    }
}

fn collect_task_artifacts(task: &ManagedTask) -> Result<ArtifactIndex, artifacts::ArtifactError> {
    artifacts::collect_artifacts(ArtifactIndexRequest {
        project_path: task.project_root.display().to_string(),
        run_directory: Some(task.snapshot.run_directory.clone()),
    })
}

fn write_run_manifest(
    snapshot: &LocalTaskSnapshot,
    plan: &SimulationPlan,
    project_root: &Path,
) -> Result<(), TaskRunnerError> {
    let manifest = LocalRunManifest {
        task_id: snapshot.id,
        plan_id: snapshot.plan_id,
        engine_id: snapshot.engine_id.clone(),
        mode: snapshot.mode.clone(),
        command: snapshot.command.clone(),
        project_path: project_root.display().to_string(),
        run_directory: snapshot.run_directory.clone(),
        environment: environment_snapshot(project_root),
        plan: plan.clone(),
    };
    let path = safe_join(project_root, &snapshot.run_directory).join("automd-run-manifest.json");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, to_string_pretty(&manifest)?)?;
    Ok(())
}

fn environment_snapshot(project_root: &Path) -> RunEnvironmentSnapshot {
    let keys = [
        "PATH",
        "CONDA_PREFIX",
        "VIRTUAL_ENV",
        "OMP_NUM_THREADS",
        "CUDA_VISIBLE_DEVICES",
        "ROCR_VISIBLE_DEVICES",
        "HIP_VISIBLE_DEVICES",
        "GMX_GPU_DD_COMMS",
        "PLUMED_KERNEL",
        "LD_LIBRARY_PATH",
        "DYLD_LIBRARY_PATH",
    ];
    RunEnvironmentSnapshot {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        current_dir: project_root.display().to_string(),
        environment: keys
            .iter()
            .map(|key| EnvironmentVariableRecord {
                key: (*key).to_string(),
                value: std::env::var(key).ok(),
            })
            .collect(),
        tools: runtime::diagnostics().tools,
        generated_at: Utc::now(),
    }
}

fn spawn_reader(pipe: Option<impl std::io::Read + Send + 'static>, label: &'static str, tx: mpsc::Sender<String>) {
    if let Some(pipe) = pipe {
        thread::spawn(move || {
            let reader = BufReader::new(pipe);
            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        let _ = tx.send(format!("[{label}] {line}"));
                    }
                    Err(error) => {
                        let _ = tx.send(format!("[{label}] read error: {error}"));
                        break;
                    }
                }
            }
        });
    }
}

fn apply_log_line(snapshot: &mut LocalTaskSnapshot, engine_id: &str, line: String) {
    append_log(snapshot, line.clone());
    let raw_line = line
        .strip_prefix("[stdout] ")
        .or_else(|| line.strip_prefix("[stderr] "))
        .unwrap_or(&line)
        .to_string();

    if let Ok(report) = engine_adapters::parse_engine_log(EngineLogParseRequest {
        engine_id: engine_id.to_string(),
        log_contents: raw_line,
    }) {
        if let Some(value) = report.progress_percent {
            snapshot.progress_percent = value;
        }
        if let Some(value) = report.ns_per_day {
            snapshot.ns_per_day = Some(value);
        }
        if let Some(value) = report.current_step {
            snapshot.current_step = Some(value);
        }
        if let Some(value) = report.fatal_error {
            snapshot.error_message = Some(value);
        }
    }
}

fn append_log(snapshot: &mut LocalTaskSnapshot, line: String) {
    snapshot.log_tail.push(line);
    if snapshot.log_tail.len() > LOG_TAIL_LIMIT {
        let overflow = snapshot.log_tail.len() - LOG_TAIL_LIMIT;
        snapshot.log_tail.drain(0..overflow);
    }
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

fn shell_program() -> String {
    if cfg!(windows) {
        "wsl".to_string()
    } else {
        "bash".to_string()
    }
}

fn shell_args(script: &Path) -> Vec<String> {
    if cfg!(windows) {
        vec!["bash".to_string(), script.display().to_string()]
    } else {
        vec![script.display().to_string()]
    }
}

fn failure_category_label(category: &FailureCategory) -> &'static str {
    match category {
        FailureCategory::MissingExecutable => "Missing executable",
        FailureCategory::MissingInput => "Missing input",
        FailureCategory::MissingTopology => "Missing topology",
        FailureCategory::ParameterMismatch => "Parameter mismatch",
        FailureCategory::MissingForceField => "Missing force field",
        FailureCategory::LicenseRequired => "License required",
        FailureCategory::GpuUnavailable => "GPU unavailable",
        FailureCategory::MpiFailure => "MPI failure",
        FailureCategory::NumericalInstability => "Numerical instability",
        FailureCategory::DiskOrPermission => "Disk or permission issue",
        FailureCategory::SchedulerFailure => "Scheduler failure",
        FailureCategory::Unknown => "Unknown failure",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planner;

    fn manager() -> TaskManager {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        TaskManager::new(manifest_dir.parent().expect("workspace root").to_path_buf())
    }

    fn plan() -> SimulationPlan {
        planner::default_simulation_plan(PlanRequest {
            project_id: None,
            name: "local-run-test".to_string(),
            engine_id: "gromacs".to_string(),
            domain: ProjectDomain::Biomolecular,
        })
    }

    #[test]
    fn dry_run_prepares_package_without_process() {
        let manager = manager();
        let plan = plan();
        let project_root = std::env::temp_dir().join("automd").join(plan.id.to_string());
        let snapshot = manager
            .start(StartLocalRunRequest {
                plan,
                project_path: None,
                mode: LocalRunMode::DryRun,
                write_package: false,
            })
            .expect("dry run starts");

        assert_eq!(snapshot.status, TaskStatus::Completed);
        assert_eq!(snapshot.progress_percent, 100.0);
        assert_eq!(snapshot.command, "dry-run package generation only");
        assert!(project_root
            .join(&snapshot.run_directory)
            .join("automd-run-manifest.json")
            .exists());
        assert!(snapshot
            .artifacts
            .iter()
            .any(|artifact| artifact.path.ends_with("automd-run-manifest.json")));
    }

    #[test]
    fn log_tail_is_bounded() {
        let mut snapshot = LocalTaskSnapshot {
            id: Uuid::new_v4(),
            plan_id: Uuid::new_v4(),
            engine_id: "gromacs".to_string(),
            mode: LocalRunMode::Mock,
            status: TaskStatus::Running,
            run_directory: "runs/test".to_string(),
            command: "mock".to_string(),
            progress_percent: 0.0,
            ns_per_day: None,
            current_step: None,
            log_tail: Vec::new(),
            error_message: None,
            exit_code: None,
            artifacts: Vec::new(),
            report_path: None,
            failure_analysis: None,
            resume_plan: None,
            started_at: Utc::now(),
            finished_at: None,
        };

        for index in 0..(LOG_TAIL_LIMIT + 10) {
            append_log(&mut snapshot, format!("line {index}"));
        }

        assert_eq!(snapshot.log_tail.len(), LOG_TAIL_LIMIT);
        assert_eq!(snapshot.log_tail.first(), Some(&"line 10".to_string()));
    }

    #[test]
    fn mock_runner_reaches_completed_snapshot() {
        if which::which("python3").is_err() {
            return;
        }

        let manager = manager();
        let snapshot = manager
            .start(StartLocalRunRequest {
                plan: plan(),
                project_path: None,
                mode: LocalRunMode::Mock,
                write_package: true,
            })
            .expect("mock run starts");

        let mut final_snapshot = snapshot.clone();
        for _ in 0..50 {
            final_snapshot = manager.snapshot(snapshot.id).expect("snapshot");
            if matches!(
                final_snapshot.status,
                TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
            ) {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }

        assert_eq!(final_snapshot.status, TaskStatus::Completed);
        assert_eq!(final_snapshot.progress_percent, 100.0);
        assert!(final_snapshot.ns_per_day.is_some());
        assert!(final_snapshot.report_path.is_some());
        assert!(final_snapshot
            .artifacts
            .iter()
            .any(|artifact| artifact.kind == ArtifactKind::AnalysisTable));
        assert!(final_snapshot
            .artifacts
            .iter()
            .any(|artifact| artifact.kind == ArtifactKind::Report));
        assert!(final_snapshot
            .artifacts
            .iter()
            .any(|artifact| artifact.path.ends_with("automd-run-manifest.json")));
        assert!(final_snapshot
            .resume_plan
            .as_ref()
            .is_some_and(|resume_plan| !resume_plan.checkpoints.is_empty()));
        assert!(final_snapshot.log_tail.iter().any(|line| line.contains("Performance:")));
    }

    #[test]
    fn mock_process_uses_configured_resource_root() {
        let root = std::env::temp_dir().join(format!("automd-resource-root-{}", Uuid::new_v4()));
        let scripts = root.join("scripts");
        fs::create_dir_all(&scripts).expect("scripts dir");
        fs::write(scripts.join("automd_mock_engine.py"), "# mock\n").expect("mock script");

        let plan = plan();
        let project_root = std::env::temp_dir().join(format!("automd-project-{}", Uuid::new_v4()));
        let spec = process_spec_for(
            &LocalRunMode::Mock,
            &plan,
            &project_root,
            "runs/mock",
            &root,
        )
        .expect("mock process spec");

        let expected = scripts.join("automd_mock_engine.py").display().to_string();
        let normalize = |value: &str| value.replace('\\', "/");
        let expected = normalize(&expected);
        assert_eq!(normalize(&spec.args[0]), expected);
        assert!(normalize(&spec.display).contains(&expected));
    }
}
