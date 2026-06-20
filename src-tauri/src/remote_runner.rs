use crate::models::*;
use crate::remote_monitor;
use chrono::Utc;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use thiserror::Error;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const DEFAULT_TIMEOUT_SECONDS: u64 = 120;
const MAX_CAPTURE_BYTES: usize = 256 * 1024;

#[derive(Debug, Error)]
pub enum RemoteRunnerError {
    #[error("remote command not found in package: {0}")]
    UnknownStep(String),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
}

pub fn run_remote_workflow_step(
    request: RemoteWorkflowStepRequest,
) -> Result<RemoteWorkflowStepResult, RemoteRunnerError> {
    let started_at = Utc::now();
    let started_instant = Instant::now();
    let command = request
        .package
        .commands
        .iter()
        .find(|command| command.id == request.step_id)
        .cloned()
        .ok_or_else(|| RemoteRunnerError::UnknownStep(request.step_id.clone()))?;
    let command_text = materialize_command(&command.command, request.job_id.as_deref());
    let mut warnings = Vec::new();
    let mut files_written = Vec::new();
    let project_root = PathBuf::from(&request.project_path);

    if matches!(
        request.mode,
        RemoteWorkflowMode::WriteFiles | RemoteWorkflowMode::Execute
    ) {
        files_written = write_package_files(&project_root, &request.package)?;
    }

    let mut result = RemoteWorkflowStepResult {
        step_id: command.id.clone(),
        label: command.label.clone(),
        command: command_text.clone(),
        mode: request.mode.clone(),
        files_written,
        status: TaskStatus::Completed,
        exit_code: None,
        stdout: String::new(),
        stderr: String::new(),
        snapshot: None,
        started_at,
        finished_at: Some(Utc::now()),
        duration_ms: Some(started_instant.elapsed().as_millis()),
        warnings: Vec::new(),
    };

    match request.mode {
        RemoteWorkflowMode::DryRun => {
            warnings.push(
                "Dry run only; no files were written and no SSH/rsync command was executed."
                    .to_string(),
            );
        }
        RemoteWorkflowMode::WriteFiles => {
            warnings.push(
                "Remote package files were written; command execution was skipped.".to_string(),
            );
        }
        RemoteWorkflowMode::Execute => {
            let timeout = Duration::from_secs(
                request
                    .timeout_seconds
                    .unwrap_or(DEFAULT_TIMEOUT_SECONDS)
                    .max(1),
            );
            let execution =
                execute_shell_command(&command_text, &project_root, &command.id, timeout)?;
            result.exit_code = execution.exit_code;
            result.stdout = execution.stdout;
            result.stderr = execution.stderr;
            result.status = execution.status;
            result.finished_at = Some(Utc::now());
            result.duration_ms = Some(started_instant.elapsed().as_millis());
            warnings.extend(execution.warnings);
            result.snapshot = snapshot_for_step(
                &request.package,
                &command.id,
                &result.stdout,
                &result.stderr,
            );
        }
    }

    result.warnings = warnings;
    Ok(result)
}

fn materialize_command(command: &str, job_id: Option<&str>) -> String {
    if let Some(job_id) = job_id {
        command.replace("<job-id>", job_id).replace("<pid>", job_id)
    } else {
        command.to_string()
    }
}

fn write_package_files(
    root: &Path,
    package: &RemoteExecutionPackage,
) -> Result<Vec<String>, RemoteRunnerError> {
    let mut written = Vec::new();
    for file in &package.files {
        let destination = safe_join(root, &file.path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&destination, &file.contents)?;
        mark_executable_if_script(&destination, &file.language)?;
        written.push(file.path.clone());
    }
    Ok(written)
}

fn mark_executable_if_script(path: &Path, language: &str) -> Result<(), RemoteRunnerError> {
    #[cfg(unix)]
    {
        let executable_language = matches!(language, "bash" | "slurm" | "pbs" | "lsf");
        let executable_extension = path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|extension| matches!(extension, "sh" | "slurm" | "pbs" | "lsf"));
        if executable_language || executable_extension {
            let mut permissions = fs::metadata(path)?.permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(path, permissions)?;
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (path, language);
    }
    Ok(())
}

struct ShellExecution {
    status: TaskStatus,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    warnings: Vec<String>,
}

fn execute_shell_command(
    command: &str,
    cwd: &Path,
    step_id: &str,
    timeout: Duration,
) -> Result<ShellExecution, RemoteRunnerError> {
    let runner_dir = cwd.join("remote").join(".automd-runner");
    fs::create_dir_all(&runner_dir)?;
    let stamp = Utc::now().timestamp_millis();
    let safe_step = step_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    let stdout_path = runner_dir.join(format!("{stamp}-{safe_step}.stdout"));
    let stderr_path = runner_dir.join(format!("{stamp}-{safe_step}.stderr"));
    let stdout_file = File::create(&stdout_path)?;
    let stderr_file = File::create(&stderr_path)?;

    let mut child = shell_command(command)
        .current_dir(cwd)
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file))
        .spawn()?;

    let start = Instant::now();
    let mut warnings = Vec::new();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() >= timeout {
            child.kill()?;
            let status = child.wait()?;
            warnings.push(format!(
                "Remote workflow step timed out after {} seconds and the local command process was killed.",
                timeout.as_secs()
            ));
            break status;
        }
        thread::sleep(Duration::from_millis(100));
    };

    let stdout = read_bounded_text(&stdout_path)?;
    let stderr = read_bounded_text(&stderr_path)?;
    let task_status = if status.success() {
        TaskStatus::Completed
    } else {
        TaskStatus::Failed
    };

    Ok(ShellExecution {
        status: task_status,
        exit_code: status.code(),
        stdout,
        stderr,
        warnings,
    })
}

fn shell_command(command: &str) -> Command {
    #[cfg(windows)]
    {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", command]);
        cmd
    }
    #[cfg(not(windows))]
    {
        let mut cmd = Command::new("sh");
        cmd.args(["-lc", command]);
        cmd
    }
}

fn read_bounded_text(path: &Path) -> Result<String, RemoteRunnerError> {
    let bytes = fs::read(path)?;
    if bytes.len() <= MAX_CAPTURE_BYTES {
        return Ok(String::from_utf8_lossy(&bytes).to_string());
    }
    let start = bytes.len().saturating_sub(MAX_CAPTURE_BYTES);
    Ok(format!(
        "[AutoMD truncated output to last {} bytes]\n{}",
        MAX_CAPTURE_BYTES,
        String::from_utf8_lossy(&bytes[start..])
    ))
}

fn snapshot_for_step(
    package: &RemoteExecutionPackage,
    step_id: &str,
    stdout: &str,
    stderr: &str,
) -> Option<RemoteJobSnapshot> {
    let output = join_output(stdout, stderr);
    if output.trim().is_empty() {
        return None;
    }
    let request = match step_id {
        "submit" => RemoteStatusParseRequest {
            engine_id: package.engine_id.clone(),
            scheduler: package.scheduler.clone(),
            submit_output: Some(output),
            status_output: None,
            log_output: None,
        },
        "status" => RemoteStatusParseRequest {
            engine_id: package.engine_id.clone(),
            scheduler: package.scheduler.clone(),
            submit_output: None,
            status_output: Some(output),
            log_output: None,
        },
        "tail-log" => RemoteStatusParseRequest {
            engine_id: package.engine_id.clone(),
            scheduler: package.scheduler.clone(),
            submit_output: None,
            status_output: None,
            log_output: Some(output),
        },
        _ => return None,
    };
    Some(remote_monitor::parse_remote_status(request))
}

fn join_output(stdout: &str, stderr: &str) -> String {
    match (stdout.trim().is_empty(), stderr.trim().is_empty()) {
        (false, false) => format!("{stdout}\n{stderr}"),
        (false, true) => stdout.to_string(),
        (true, false) => stderr.to_string(),
        (true, true) => String::new(),
    }
}

fn safe_join(root: &Path, relative: &str) -> PathBuf {
    let mut destination = root.to_path_buf();
    for component in Path::new(relative).components() {
        if let std::path::Component::Normal(value) = component {
            destination.push(value);
        }
    }
    destination
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn package_with_command(step_id: &str, command: &str) -> RemoteExecutionPackage {
        RemoteExecutionPackage {
            engine_id: "gromacs".to_string(),
            scheduler: ExecutionMode::Slurm,
            profile_id: "test".to_string(),
            remote_workdir: "/scratch/test".to_string(),
            run_directory: "runs/gromacs-test".to_string(),
            files: vec![GeneratedFile {
                path: "remote/submit.slurm".to_string(),
                language: "slurm".to_string(),
                contents: "#!/usr/bin/env bash\n#SBATCH --job-name=automd-test\n".to_string(),
            }],
            commands: vec![RemoteCommand {
                id: step_id.to_string(),
                label: "test command".to_string(),
                command: command.to_string(),
                description: "test".to_string(),
            }],
            warnings: Vec::new(),
        }
    }

    #[test]
    fn write_files_mode_writes_remote_package_without_execution() {
        let root = std::env::temp_dir().join(format!("automd-remote-write-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("temp root");

        let result = run_remote_workflow_step(RemoteWorkflowStepRequest {
            project_path: root.display().to_string(),
            package: package_with_command("submit", "echo should-not-run"),
            step_id: "submit".to_string(),
            mode: RemoteWorkflowMode::WriteFiles,
            job_id: None,
            timeout_seconds: None,
        })
        .expect("write files");

        assert_eq!(result.status, TaskStatus::Completed);
        assert!(result.stdout.is_empty());
        assert!(root.join("remote/submit.slurm").exists());

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn execute_mode_captures_submit_output_and_snapshot() {
        let root = std::env::temp_dir().join(format!("automd-remote-exec-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("temp root");

        let result = run_remote_workflow_step(RemoteWorkflowStepRequest {
            project_path: root.display().to_string(),
            package: package_with_command("submit", "echo 123456"),
            step_id: "submit".to_string(),
            mode: RemoteWorkflowMode::Execute,
            job_id: None,
            timeout_seconds: Some(5),
        })
        .expect("execute");

        assert_eq!(result.status, TaskStatus::Completed);
        assert!(result.stdout.contains("123456"));
        assert_eq!(
            result.snapshot.expect("snapshot").job_id.as_deref(),
            Some("123456")
        );

        fs::remove_dir_all(root).expect("cleanup");
    }
}
