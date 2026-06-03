use crate::models::*;
use crate::recipes;
use chrono::Utc;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use thiserror::Error;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const DEFAULT_TIMEOUT_SECONDS: u64 = 600;
const MAX_CAPTURE_BYTES: usize = 512 * 1024;

#[derive(Debug, Error)]
pub enum BuildRunnerError {
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("recipe export error: {0}")]
    RecipeExport(String),
}

pub fn run_build_workflow(request: BuildWorkflowRequest) -> Result<BuildWorkflowResult, BuildRunnerError> {
    let started_at = Utc::now();
    let started_instant = Instant::now();
    let engine_id = request.build_options.engine_id.clone();
    let preview = preview_export(&request)?;
    let command = format!("bash {}/build-{}.sh", shell_quote(&preview.directory), shell_quote(&engine_id));
    let mut warnings = preview.warnings.clone();
    let mut files_written = Vec::new();
    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut exit_code = None;
    let mut status = TaskStatus::Completed;
    let mut failure_analysis = None;
    let mut log_path = None;

    if matches!(request.mode, BuildWorkflowMode::DryRun) {
        warnings.push("Dry run only; no build files were written and no compiler process was started.".to_string());
    } else {
        let exported = recipes::export_recipe_package(RecipeExportRequest {
            project_path: request.project_path.clone(),
            build_options: request.build_options.clone(),
            include_container: request.include_container,
            include_build_script: request.include_build_script,
        })
        .map_err(BuildRunnerError::RecipeExport)?;
        files_written = exported.files.iter().map(|file| file.path.clone()).collect();
        warnings.extend(exported.warnings);

        if matches!(request.mode, BuildWorkflowMode::WriteFiles) {
            warnings.push("Build recipe files were written; command execution was skipped.".to_string());
        } else if request.include_build_script {
            let project_root = PathBuf::from(&request.project_path);
            let timeout = Duration::from_secs(request.timeout_seconds.unwrap_or(DEFAULT_TIMEOUT_SECONDS).max(1));
            let execution = execute_build_script(&project_root, &exported.directory, &engine_id, timeout)?;
            stdout = execution.stdout;
            stderr = execution.stderr;
            exit_code = execution.exit_code;
            status = execution.status;
            log_path = Some(execution.log_path);
            warnings.extend(execution.warnings);
            if status == TaskStatus::Failed {
                failure_analysis = Some(classify_build_failure(&engine_id, &stdout, &stderr, exit_code));
            }
        } else {
            warnings.push("No build script was requested, so execute mode only wrote recipe/container files.".to_string());
        }
    }

    Ok(BuildWorkflowResult {
        engine_id,
        directory: preview.directory,
        command,
        mode: request.mode,
        files_written,
        status,
        exit_code,
        stdout,
        stderr,
        log_path,
        failure_analysis,
        started_at,
        finished_at: Some(Utc::now()),
        duration_ms: Some(started_instant.elapsed().as_millis()),
        warnings,
    })
}

fn preview_export(request: &BuildWorkflowRequest) -> Result<RecipeExportResult, BuildRunnerError> {
    let engine_id = request.build_options.engine_id.clone();
    let directory = format!("build-recipes/{}", sanitize_path_component(&engine_id));
    let mut files = Vec::new();
    let mut warnings = Vec::new();

    if request.include_container {
        let container = recipes::container_recipe(&engine_id);
        warnings.extend(container.notes);
        files.extend(container.files.into_iter().map(|file| GeneratedFile {
            path: format!("{directory}/{}", file.path),
            language: file.language,
            contents: file.contents,
        }));
    }

    if request.include_build_script {
        let build = recipes::build_recipe(request.build_options.clone());
        warnings.extend(build.warnings.clone());
        files.push(GeneratedFile {
            path: format!("{directory}/build-{engine_id}.sh"),
            language: "bash".to_string(),
            contents: build.script.clone(),
        });
        files.push(GeneratedFile {
            path: format!("{directory}/automd-build-recipe.json"),
            language: "json".to_string(),
            contents: serde_json::to_string_pretty(&build).unwrap_or_else(|_| "{}".to_string()),
        });
    }

    Ok(RecipeExportResult {
        engine_id,
        directory,
        files,
        warnings,
    })
}

struct BuildExecution {
    status: TaskStatus,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    log_path: String,
    warnings: Vec<String>,
}

fn execute_build_script(
    project_root: &Path,
    directory: &str,
    engine_id: &str,
    timeout: Duration,
) -> Result<BuildExecution, BuildRunnerError> {
    let build_dir = safe_join(project_root, directory);
    fs::create_dir_all(build_dir.join("logs"))?;
    let stdout_path = build_dir.join("logs").join("build.stdout.log");
    let stderr_path = build_dir.join("logs").join("build.stderr.log");
    let combined_path = format!("{directory}/logs/build-combined.log");
    let stdout_file = File::create(&stdout_path)?;
    let stderr_file = File::create(&stderr_path)?;

    let script = format!("{directory}/build-{engine_id}.sh");
    let mut child = shell_command(&format!("bash {}", shell_quote(&script)))
        .current_dir(project_root)
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file))
        .spawn()?;

    let start = Instant::now();
    let mut warnings = Vec::new();
    let exit_status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() >= timeout {
            child.kill()?;
            let status = child.wait()?;
            warnings.push(format!(
                "Build command timed out after {} seconds and the local process was killed.",
                timeout.as_secs()
            ));
            break status;
        }
        thread::sleep(Duration::from_millis(100));
    };

    let stdout = read_bounded_text(&stdout_path)?;
    let stderr = read_bounded_text(&stderr_path)?;
    let combined_abs = safe_join(project_root, &combined_path);
    if let Some(parent) = combined_abs.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&combined_abs, format!("{stdout}\n{stderr}"))?;
    mark_executable_if_needed(&safe_join(project_root, &script))?;

    Ok(BuildExecution {
        status: if exit_status.success() {
            TaskStatus::Completed
        } else {
            TaskStatus::Failed
        },
        exit_code: exit_status.code(),
        stdout,
        stderr,
        log_path: combined_path,
        warnings,
    })
}

fn classify_build_failure(
    engine_id: &str,
    stdout: &str,
    stderr: &str,
    exit_code: Option<i32>,
) -> FailureAnalysis {
    let log = format!("{stdout}\n{stderr}");
    let lower = log.to_ascii_lowercase();
    let category = if contains_any(&lower, &["cmake: command not found", "command not found: cmake", "ninja: command not found", "make: command not found", "git: command not found", "curl: command not found", "compiler not found", "c++: command not found", "gcc: command not found", "g++: command not found"]) {
        FailureCategory::MissingExecutable
    } else if contains_any(&lower, &["permission denied", "read-only file system", "no space left on device", "disk quota exceeded"]) {
        FailureCategory::DiskOrPermission
    } else if contains_any(&lower, &["cuda", "cudart", "nvcc", "hip", "rocm", "opencl", "sycl"]) {
        FailureCategory::GpuUnavailable
    } else if contains_any(&lower, &["mpi", "mpicc", "mpicxx", "openmpi", "mpirun"]) {
        FailureCategory::MpiFailure
    } else if contains_any(&lower, &["plumed", "patch failed"]) {
        FailureCategory::ParameterMismatch
    } else if contains_any(&lower, &["could not resolve host", "failed to connect", "connection timed out", "ssl certificate", "http error", "not found"]) {
        FailureCategory::MissingInput
    } else {
        FailureCategory::Unknown
    };
    let message = if let Some(line) = log
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
    {
        format!("Build for {engine_id} failed with exit code {:?}: {line}", exit_code)
    } else {
        format!("Build for {engine_id} failed with exit code {:?}", exit_code)
    };
    FailureAnalysis {
        engine_id: engine_id.to_string(),
        category: category.clone(),
        severity: ValidationSeverity::Error,
        message,
        suggestions: build_failure_suggestions(engine_id, category),
    }
}

fn build_failure_suggestions(engine_id: &str, category: FailureCategory) -> Vec<FailureSuggestion> {
    match category {
        FailureCategory::MissingExecutable => vec![FailureSuggestion {
            title: "Install the build toolchain".to_string(),
            detail: "Install CMake, make/ninja, git, curl, and a C/C++ compiler before rerunning the build recipe.".to_string(),
            action_label: "Check build tools".to_string(),
            command_hint: Some("cmake --version && git --version && c++ --version".to_string()),
        }],
        FailureCategory::DiskOrPermission => vec![FailureSuggestion {
            title: "Use a writable build directory".to_string(),
            detail: "Builds need several GB of temporary space and write access to the selected install prefix.".to_string(),
            action_label: "Change install prefix".to_string(),
            command_hint: Some("df -h . && touch build-recipes/.write-test".to_string()),
        }],
        FailureCategory::GpuUnavailable => vec![FailureSuggestion {
            title: "Match GPU backend to this machine".to_string(),
            detail: "CUDA/ROCm/OpenCL/SYCL builds require matching drivers, SDKs, and compiler versions. Retry CPU-only or build on the target HPC node.".to_string(),
            action_label: "Use CPU build".to_string(),
            command_hint: Some(format!("AutoMD build options: disable GPU for {engine_id}")),
        }],
        FailureCategory::MpiFailure => vec![FailureSuggestion {
            title: "Load or install MPI".to_string(),
            detail: "MPI builds require mpicc/mpicxx wrappers from OpenMPI, MPICH, Intel MPI, or the cluster module stack.".to_string(),
            action_label: "Check MPI".to_string(),
            command_hint: Some("which mpicc || which mpicxx".to_string()),
        }],
        FailureCategory::ParameterMismatch => vec![FailureSuggestion {
            title: "Review PLUMED or patch options".to_string(),
            detail: "PLUMED integration can be version-sensitive. Build PLUMED first and apply the upstream patch instructions for the selected engine version.".to_string(),
            action_label: "Review patch".to_string(),
            command_hint: Some("plumed --version".to_string()),
        }],
        FailureCategory::MissingInput => vec![FailureSuggestion {
            title: "Check network and source availability".to_string(),
            detail: "The recipe could not fetch or find source inputs. Use an allowed network path, pre-download sources, or run on a machine with internet access.".to_string(),
            action_label: "Check source".to_string(),
            command_hint: Some("curl -I https://github.com".to_string()),
        }],
        _ => vec![FailureSuggestion {
            title: "Inspect the captured build log".to_string(),
            detail: "The build failed outside known patterns. Read the first compiler error in build-combined.log and adjust the recipe or environment.".to_string(),
            action_label: "Open build log".to_string(),
            command_hint: Some(format!("less build-recipes/{engine_id}/logs/build-combined.log")),
        }],
    }
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

fn read_bounded_text(path: &Path) -> Result<String, BuildRunnerError> {
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

fn mark_executable_if_needed(path: &Path) -> Result<(), BuildRunnerError> {
    #[cfg(unix)]
    if path.exists() {
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

fn sanitize_path_component(value: &str) -> String {
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

fn safe_join(root: &Path, relative: &str) -> PathBuf {
    let mut destination = root.to_path_buf();
    for component in Path::new(relative).components() {
        if let std::path::Component::Normal(value) = component {
            destination.push(value);
        }
    }
    destination
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

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn request(engine_id: &str, mode: BuildWorkflowMode) -> (PathBuf, BuildWorkflowRequest) {
        let root = std::env::temp_dir().join(format!("automd-build-runner-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("temp root");
        (
            root.clone(),
            BuildWorkflowRequest {
                project_path: root.display().to_string(),
                build_options: BuildRecipeOptions {
                    engine_id: engine_id.to_string(),
                enable_mpi: false,
                enable_gpu: false,
                gpu_backend: None,
                enable_plumed: false,
                install_prefix: Some(root.join("install").display().to_string()),
            },
                include_container: false,
                include_build_script: true,
                mode,
                timeout_seconds: Some(10),
            },
        )
    }

    #[test]
    fn write_files_mode_exports_build_script_without_running() {
        let (root, request) = request("dummy_engine", BuildWorkflowMode::WriteFiles);
        let result = run_build_workflow(request).expect("write files");

        assert_eq!(result.status, TaskStatus::Completed);
        assert!(result.stdout.is_empty());
        assert!(root.join("build-recipes/dummy_engine/build-dummy_engine.sh").exists());

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn execute_mode_captures_generic_build_log() {
        let (root, request) = request("dummy_engine", BuildWorkflowMode::Execute);
        let result = run_build_workflow(request).expect("execute");

        assert_eq!(result.status, TaskStatus::Completed);
        assert!(result.stdout.contains("dummy_engine"));
        let log_path = result.log_path.expect("log path");
        assert!(root.join(log_path).exists());

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn build_failure_classifier_detects_missing_toolchain() {
        let analysis = classify_build_failure(
            "gromacs",
            "",
            "cmake: command not found",
            Some(127),
        );

        assert_eq!(analysis.category, FailureCategory::MissingExecutable);
        assert!(!analysis.suggestions.is_empty());
    }
}
