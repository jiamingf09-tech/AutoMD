mod analysis;
mod artifacts;
mod batch;
mod build_runner;
mod engine_adapters;
mod engine_registry;
mod models;
mod parameter_mapping;
mod planner;
mod plugins;
mod project_files;
mod project_store;
mod recipes;
mod remote_monitor;
mod remote_runner;
mod runtime;
mod science_sidecar;
mod structure_import;
mod sysenv;
mod task_runner;
mod trajectory;

use crate::models::*;
use crate::project_store::ProjectDatabase;
use crate::task_runner::TaskManager;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use tauri::menu::{MenuBuilder, MenuItem, SubmenuBuilder};
use tauri::{Emitter, Manager};

struct AppState {
    db: Mutex<ProjectDatabase>,
    project_root: PathBuf,
    plugin_root: PathBuf,
    engines_root: PathBuf,
    task_manager: TaskManager,
}

/// conda-forge package name for engines that can be installed with one click.
/// Engines absent here are commercial/licensed or need manual builds.
fn engine_conda_package(engine_id: &str) -> Option<&'static str> {
    match engine_id {
        "gromacs" => Some("gromacs"),
        "openmm" => Some("openmm"),
        "ambertools" => Some("ambertools"),
        "lammps" => Some("lammps"),
        "cp2k" => Some("cp2k"),
        "hoomd" => Some("hoomd"),
        _ => None,
    }
}

fn miniforge_prefix(engines_root: &Path) -> PathBuf {
    engines_root.join("_tools").join("miniforge3")
}

fn conda_binary(prefix: &Path) -> PathBuf {
    if cfg!(target_os = "windows") {
        prefix.join("Scripts").join("conda.exe")
    } else {
        prefix.join("bin").join("conda")
    }
}

fn mamba_binary(prefix: &Path) -> PathBuf {
    if cfg!(target_os = "windows") {
        prefix.join("Scripts").join("mamba.exe")
    } else {
        prefix.join("bin").join("mamba")
    }
}

fn ensure_conda_manager(engines_root: &Path) -> Result<PathBuf, String> {
    if let Some(manager) = sysenv::resolve_conda_manager() {
        return Ok(manager);
    }
    install_miniforge(engines_root)
}

fn install_miniforge(engines_root: &Path) -> Result<PathBuf, String> {
    let prefix = miniforge_prefix(engines_root);
    let conda = conda_binary(&prefix);
    if conda.is_file() {
        return Ok(conda);
    }

    std::fs::create_dir_all(prefix.parent().unwrap_or(engines_root)).map_err(|error| error.to_string())?;
    let downloads = engines_root.join("_downloads");
    std::fs::create_dir_all(&downloads).map_err(|error| error.to_string())?;
    let (url, file_name) = miniforge_installer_url()?;
    let installer = downloads.join(file_name);
    if !installer.is_file() {
        download_file(&url, &installer)?;
    }

    let output = if cfg!(target_os = "windows") {
        Command::new(&installer)
            .arg("/InstallationType=JustMe")
            .arg("/AddToPath=0")
            .arg("/RegisterPython=0")
            .arg("/S")
            .arg(format!("/D={}", prefix.display()))
            .output()
    } else {
        Command::new("bash")
            .arg(&installer)
            .arg("-b")
            .arg("-p")
            .arg(&prefix)
            .output()
    }
    .map_err(|error| format!("启动 Miniforge 安装器失败：{error}"))?;

    if !output.status.success() {
        return Err(format!("Miniforge 安装失败：\n{}", command_tail(&output.stderr)));
    }
    if !conda.is_file() {
        return Err(format!("Miniforge 安装完成，但未找到 {}。", conda.display()));
    }
    Ok(conda)
}

fn install_internal_mamba(engines_root: &Path) -> Result<PathBuf, String> {
    let prefix = miniforge_prefix(engines_root);
    let conda = install_miniforge(engines_root)?;
    let mamba = mamba_binary(&prefix);
    if mamba.is_file() {
        return Ok(mamba);
    }

    let output = Command::new(&conda)
        .arg("install")
        .arg("-y")
        .arg("-n")
        .arg("base")
        .arg("-c")
        .arg("conda-forge")
        .arg("mamba")
        .output()
        .map_err(|error| format!("启动 mamba 安装器失败：{error}"))?;

    if !output.status.success() {
        return Err(format!("mamba 安装失败：\n{}", command_tail(&output.stderr)));
    }
    if !mamba.is_file() {
        return Err(format!("mamba 安装完成，但未找到 {}。", mamba.display()));
    }
    Ok(mamba)
}

fn miniforge_installer_url() -> Result<(String, String), String> {
    let platform = match std::env::consts::OS {
        "macos" => "MacOSX",
        "linux" => "Linux",
        "windows" => "Windows",
        other => return Err(format!("{other} 暂不支持自动安装 Miniforge。")),
    };
    let arch = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "arm64",
        (_, "x86_64") => "x86_64",
        ("linux", "aarch64") => "aarch64",
        (os, arch) => return Err(format!("{os}/{arch} 暂不支持自动安装 Miniforge。")),
    };
    let extension = if cfg!(target_os = "windows") { "exe" } else { "sh" };
    let file_name = format!("Miniforge3-{platform}-{arch}.{extension}");
    Ok((
        format!("https://github.com/conda-forge/miniforge/releases/latest/download/{file_name}"),
        file_name,
    ))
}

fn download_file(url: &str, destination: &Path) -> Result<(), String> {
    let output = if cfg!(target_os = "windows") {
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "$ProgressPreference='SilentlyContinue'; Invoke-WebRequest -UseBasicParsing -Uri '{}' -OutFile '{}'",
                    url.replace('\'', "''"),
                    destination.display().to_string().replace('\'', "''")
                ),
            ])
            .output()
    } else if let Some(curl) = sysenv::resolve_command("curl") {
        Command::new(curl)
            .arg("-L")
            .arg("--fail")
            .arg(url)
            .arg("-o")
            .arg(destination)
            .output()
    } else if let Some(wget) = sysenv::resolve_command("wget") {
        Command::new(wget)
            .arg("-O")
            .arg(destination)
            .arg(url)
            .output()
    } else {
        return Err("未找到 curl/wget，无法自动下载 Miniforge 安装器。".to_string());
    }
    .map_err(|error| format!("下载 Miniforge 失败：{error}"))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(format!("下载 Miniforge 失败：\n{}", command_tail(&output.stderr)))
    }
}

fn command_tail(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let tail = text
        .lines()
        .rev()
        .take(8)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    tail.trim().to_string()
}

fn locate_installed_binary(prefix: &Path, engine_id: &str) -> Option<PathBuf> {
    let bin = prefix.join("bin");
    let mut candidates: Vec<String> = engine_registry::detect_engine_by_id(engine_id)
        .map(|capability| capability.executable_names)
        .unwrap_or_default();
    if candidates.is_empty() {
        // Python-library engines (e.g. OpenMM) ship no CLI; point at the env python.
        candidates.push("python".to_string());
    }
    candidates
        .into_iter()
        .map(|name| bin.join(name))
        .find(|path| path.exists())
}

fn detect_installed_version(binary: &Path) -> Option<String> {
    let output = Command::new(binary).arg("--version").output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .next()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
}

#[tauri::command]
fn get_engine_capabilities() -> Vec<EngineCapability> {
    engine_registry::detect_all()
}

#[tauri::command]
fn list_engine_capabilities(state: tauri::State<'_, AppState>) -> Vec<EngineCapability> {
    let mut capabilities = engine_registry::detect_all();
    if let Ok(db) = state.db.lock() {
        if let Ok(records) = db.list_engine_installations() {
            apply_installation_records(&mut capabilities, &records);
        }
    }
    capabilities
}

fn apply_installation_records(capabilities: &mut [EngineCapability], records: &[EngineInstallationRecord]) {
    for capability in capabilities {
        if let Some(record) = records.iter().find(|record| record.engine_id == capability.id) {
            capability.detection = DetectionState {
                status: record.authorization_status.clone(),
                path: Some(record.location.clone()),
                version: record.version.clone(),
                message: match record.authorization_status {
                    DetectionStatus::Ready => "用户保存的引擎路径已标记可用。".to_string(),
                    DetectionStatus::MissingLicense => "用户保存的引擎路径仍需许可/授权确认。".to_string(),
                    DetectionStatus::MissingInstall => "用户保存的引擎路径标记为需要安装检查。".to_string(),
                    DetectionStatus::PlatformUnsupported => "用户保存的引擎路径标记为当前平台不支持。".to_string(),
                    DetectionStatus::RemoteRecommended => "用户保存的引擎配置建议通过远程环境运行。".to_string(),
                    DetectionStatus::NotApplicable => "用户保存的引擎配置标记为当前环境不适用。".to_string(),
                },
            };
        }
    }
}

#[tauri::command]
fn get_runtime_diagnostics() -> RuntimeDiagnostics {
    runtime::diagnostics()
}

#[tauri::command]
fn get_science_sidecar_diagnostics() -> ScienceSidecarDiagnostics {
    science_sidecar::diagnostics()
}

#[tauri::command]
fn list_remote_profile_templates() -> Vec<RemoteProfile> {
    runtime::remote_profile_templates()
}

#[tauri::command]
fn list_remote_profiles(state: tauri::State<'_, AppState>) -> Result<Vec<RemoteProfile>, String> {
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    let saved = db.list_remote_profiles().map_err(|error| error.to_string())?;
    let mut profiles = saved;
    for template in runtime::remote_profile_templates() {
        if !profiles.iter().any(|profile| profile.id == template.id) {
            profiles.push(template);
        }
    }
    Ok(profiles)
}

#[tauri::command]
fn save_remote_profile(profile: RemoteProfile, state: tauri::State<'_, AppState>) -> Result<RemoteProfile, String> {
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    db.save_remote_profile(profile).map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_remote_profile(id: String, state: tauri::State<'_, AppState>) -> Result<bool, String> {
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    db.delete_remote_profile(id).map_err(|error| error.to_string())
}

#[tauri::command]
fn list_engine_installations(state: tauri::State<'_, AppState>) -> Result<Vec<EngineInstallationRecord>, String> {
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    db.list_engine_installations().map_err(|error| error.to_string())
}

#[tauri::command]
fn save_engine_installation(
    record: EngineInstallationRecord,
    state: tauri::State<'_, AppState>,
) -> Result<EngineInstallationRecord, String> {
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    db.save_engine_installation(record).map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_engine_installation(
    engine_id: String,
    location: String,
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    db.delete_engine_installation(engine_id, location)
        .map_err(|error| error.to_string())
}

/// Engine ids that AutoMD can install with one click via conda-forge
/// (kept in sync with `engine_conda_package`).
#[tauri::command]
fn list_installable_engines() -> Vec<String> {
    ["gromacs", "openmm", "ambertools", "lammps", "cp2k", "hoomd"]
        .iter()
        .map(|id| id.to_string())
        .collect()
}

/// One-click install: create an isolated conda-forge environment for the engine
/// (no manual download or compilation), then record the resulting binary as a
/// ready installation. Requires micromamba/mamba/conda on PATH.
#[tauri::command]
fn install_engine(
    engine_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<EngineInstallationRecord, String> {
    let package = engine_conda_package(&engine_id).ok_or_else(|| {
        format!("{engine_id} 暂不支持一键安装（通常需要许可或手动编译），请在指引页查看安装方式。")
    })?;
    std::fs::create_dir_all(&state.engines_root).map_err(|error| error.to_string())?;
    let manager = ensure_conda_manager(&state.engines_root)?;
    let prefix = state.engines_root.join(&engine_id);

    let output = Command::new(&manager)
        .arg("create")
        .arg("-y")
        .arg("-p")
        .arg(&prefix)
        .arg("-c")
        .arg("conda-forge")
        .arg(package)
        .output()
        .map_err(|error| format!("启动安装器失败：{error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail = stderr
            .lines()
            .rev()
            .take(6)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        return Err(format!("{engine_id} 安装失败：\n{}", tail.trim()));
    }

    let binary = locate_installed_binary(&prefix, &engine_id).ok_or_else(|| {
        format!("{engine_id} 安装完成，但未在 {} 找到可执行文件。", prefix.join("bin").display())
    })?;

    let record = EngineInstallationRecord {
        engine_id: engine_id.clone(),
        location: binary.display().to_string(),
        version: detect_installed_version(&binary),
        authorization_status: DetectionStatus::Ready,
        checked_at: chrono::Utc::now(),
    };

    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    db.save_engine_installation(record.clone())
        .map_err(|error| error.to_string())?;
    Ok(record)
}

/// (conda-forge package, primary executable) for runtime tools that can be
/// installed with one click. GPU drivers, schedulers and system tools are
/// intentionally absent — they need the OS/vendor/cluster, not conda.
fn tool_conda_spec(tool_id: &str) -> Option<(&'static str, &'static str)> {
    match tool_id {
        "mpirun" => Some(("openmpi", "mpirun")),
        "plumed" => Some(("plumed", "plumed")),
        _ => None,
    }
}

#[tauri::command]
fn list_installable_tools() -> Vec<String> {
    ["conda", "mamba", "mpirun", "plumed"]
        .iter()
        .map(|id| id.to_string())
        .collect()
}

/// One-click install for a runtime tool via conda-forge. Returns the absolute
/// path of the installed binary (the UI marks the tool ready with it).
#[tauri::command]
fn install_tool(tool_id: String, state: tauri::State<'_, AppState>) -> Result<String, String> {
    std::fs::create_dir_all(&state.engines_root).map_err(|error| error.to_string())?;

    if tool_id == "conda" {
        return install_miniforge(&state.engines_root).map(|path| path.display().to_string());
    }
    if tool_id == "mamba" {
        return install_internal_mamba(&state.engines_root).map(|path| path.display().to_string());
    }

    let (package, exe) = tool_conda_spec(&tool_id).ok_or_else(|| {
        format!("{tool_id} 暂不支持一键安装（通常由系统、GPU 驱动或集群提供）。")
    })?;
    let manager = ensure_conda_manager(&state.engines_root)?;
    let prefix = state.engines_root.join("_tools").join(&tool_id);

    let output = Command::new(&manager)
        .arg("create")
        .arg("-y")
        .arg("-p")
        .arg(&prefix)
        .arg("-c")
        .arg("conda-forge")
        .arg(package)
        .output()
        .map_err(|error| format!("启动安装器失败：{error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail = stderr
            .lines()
            .rev()
            .take(6)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        return Err(format!("{tool_id} 安装失败：\n{}", tail.trim()));
    }

    let binary = prefix.join("bin").join(exe);
    if !binary.is_file() {
        return Err(format!("{tool_id} 安装完成，但未找到 {}。", binary.display()));
    }
    Ok(binary.display().to_string())
}

#[tauri::command]
fn list_plugin_manifests(state: tauri::State<'_, AppState>) -> Result<PluginRegistrySnapshot, String> {
    plugins::registry_snapshot(&state.plugin_root).map_err(|error| error.to_string())
}

#[tauri::command]
fn open_plugin_folder(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    std::fs::create_dir_all(&state.plugin_root).map_err(|error| error.to_string())?;
    tauri_plugin_opener::open_path(&state.plugin_root, None::<&str>).map_err(|error| error.to_string())?;
    Ok(true)
}

#[tauri::command]
fn open_path_in_system(path: String) -> Result<bool, String> {
    let target = PathBuf::from(path);
    if !target.exists() {
        return Err(format!("路径不存在：{}", target.display()));
    }
    tauri_plugin_opener::open_path(target, None::<&str>).map_err(|error| error.to_string())?;
    Ok(true)
}

#[tauri::command]
fn pick_file_in_system(request: FilePickRequest) -> Result<Option<String>, String> {
    let title = request.title.unwrap_or_else(|| "选择文件".to_string());
    pick_file_dialog(&title, &request.extensions)
}

#[tauri::command]
fn find_executable(request: ExecutableSearchRequest) -> Result<ExecutableSearchResult, String> {
    let mut checked_locations = Vec::new();
    // search_dirs() recovers the login-shell PATH + conda install dirs/envs so a
    // GUI launch (minimal PATH) still finds conda-installed tools.
    let mut dirs = sysenv::search_dirs();
    dirs.extend(request.extra_dirs.into_iter().map(PathBuf::from));

    for command in request.commands.iter().filter(|command| !command.trim().is_empty()) {
        if let Ok(path) = which::which(command) {
            return Ok(ExecutableSearchResult {
                found: true,
                command: Some(command.clone()),
                path: Some(path.display().to_string()),
                checked_locations,
                message: format!("已在 PATH 中找到 {command}。"),
            });
        }

        let command_path = PathBuf::from(command);
        if command_path.components().count() > 1 && command_path.is_file() {
            return Ok(ExecutableSearchResult {
                found: true,
                command: Some(command.clone()),
                path: Some(command_path.display().to_string()),
                checked_locations,
                message: "已找到用户提供的可执行文件路径。".to_string(),
            });
        }

        for dir in &dirs {
            for candidate in sysenv::executable_candidates(command) {
                let path = dir.join(&candidate);
                checked_locations.push(path.display().to_string());
                if path.is_file() {
                    return Ok(ExecutableSearchResult {
                        found: true,
                        command: Some(command.clone()),
                        path: Some(path.display().to_string()),
                        checked_locations,
                        message: format!("已在 {} 找到 {command}。", dir.display()),
                    });
                }
            }
        }
    }

    Ok(ExecutableSearchResult {
        found: false,
        command: None,
        path: None,
        checked_locations,
        message: "自动查找未发现可执行文件（已扫描 PATH、shell 环境与 conda 安装目录）。可手动选择，或使用一键安装。".to_string(),
    })
}

fn pick_file_dialog(title: &str, extensions: &[String]) -> Result<Option<String>, String> {
    if cfg!(target_os = "macos") {
        pick_file_macos(title, extensions)
    } else if cfg!(target_os = "windows") {
        pick_file_windows(title, extensions)
    } else {
        pick_file_linux(title)
    }
}

fn pick_file_macos(title: &str, extensions: &[String]) -> Result<Option<String>, String> {
    let escaped_title = escape_applescript(title);
    let extension_filter = if extensions.is_empty() {
        String::new()
    } else {
        let values = extensions
            .iter()
            .map(|extension| extension.trim_start_matches('.'))
            .filter(|extension| !extension.is_empty())
            .map(|extension| format!("\"{}\"", escape_applescript(extension)))
            .collect::<Vec<_>>();
        if values.is_empty() {
            String::new()
        } else {
            format!(" of type {{{}}}", values.join(", "))
        }
    };
    let script = format!("POSIX path of (choose file with prompt \"{escaped_title}\"{extension_filter})");
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return Ok((!path.is_empty()).then_some(path));
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.to_lowercase().contains("user canceled") || stderr.contains("-128") {
        return Ok(None);
    }
    Err(stderr.trim().to_string())
}

fn pick_file_windows(title: &str, extensions: &[String]) -> Result<Option<String>, String> {
    let filter = if extensions.is_empty() {
        "All files (*.*)|*.*".to_string()
    } else {
        let patterns = extensions
            .iter()
            .map(|extension| format!("*.{}", extension.trim_start_matches('.')))
            .collect::<Vec<_>>()
            .join(";");
        format!("Selected files ({patterns})|{patterns}|All files (*.*)|*.*")
    };
    let script = format!(
        "Add-Type -AssemblyName System.Windows.Forms; $d = New-Object System.Windows.Forms.OpenFileDialog; $d.Title = '{}'; $d.Filter = '{}'; if ($d.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {{ $d.FileName }}",
        title.replace('\'', "''"),
        filter.replace('\'', "''")
    );
    let output = Command::new("powershell")
        .args(["-NoProfile", "-STA", "-Command", &script])
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return Ok((!path.is_empty()).then_some(path));
    }
    Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
}

fn pick_file_linux(title: &str) -> Result<Option<String>, String> {
    for (command, args) in [
        ("zenity", vec!["--file-selection", "--title", title]),
        ("kdialog", vec!["--getopenfilename", "."]),
    ] {
        if which::which(command).is_err() {
            continue;
        }
        let output = Command::new(command)
            .args(args)
            .output()
            .map_err(|error| error.to_string())?;
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            return Ok((!path.is_empty()).then_some(path));
        }
    }
    Err("当前 Linux 环境未检测到 zenity/kdialog，无法打开图形文件选择器。".to_string())
}

fn escape_applescript(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[tauri::command]
fn list_projects(state: tauri::State<'_, AppState>) -> Result<Vec<ProjectSummary>, String> {
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    db.list_projects().map_err(|error| error.to_string())
}

#[tauri::command]
fn create_project(
    request: CreateProjectRequest,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectSummary, String> {
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    db.create_project(request, &state.project_root)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_project(id: String, state: tauri::State<'_, AppState>) -> Result<bool, String> {
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    db.delete_project(id).map_err(|error| error.to_string())
}

#[tauri::command]
fn generate_simulation_plan(request: PlanRequest) -> SimulationPlan {
    planner::default_simulation_plan(request)
}

#[tauri::command]
fn validate_simulation_plan(plan: SimulationPlan) -> ValidationReport {
    planner::validate_plan(&plan)
}

#[tauri::command]
fn map_engine_parameters(request: ParameterMappingRequest) -> ParameterMappingReport {
    parameter_mapping::map_parameters(request)
}

#[tauri::command]
fn create_mock_task(plan: SimulationPlan) -> SimulationTask {
    planner::mock_task(plan)
}

#[tauri::command]
fn import_structure(request: StructureImportRequest) -> Result<StructureImportResult, String> {
    structure_import::import_structure(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn list_imported_structures(project_path: String) -> Result<Vec<ImportedStructureEntry>, String> {
    structure_import::list_imported_structures(project_path).map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_imported_structure(request: DeleteImportedStructureRequest) -> Result<bool, String> {
    structure_import::delete_imported_structure(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn read_structure_file(request: StructureFileRequest) -> Result<StructureFilePayload, String> {
    structure_import::read_structure_file(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn generate_slurm_script(plan: SimulationPlan) -> String {
    recipes::slurm_script(&plan)
}

#[tauri::command]
fn generate_remote_execution_package(request: RemoteExecutionRequest) -> RemoteExecutionPackage {
    recipes::remote_execution_package(request)
}

#[tauri::command]
fn parse_remote_job_status(request: RemoteStatusParseRequest) -> RemoteJobSnapshot {
    remote_monitor::parse_remote_status(request)
}

#[tauri::command]
fn run_remote_workflow_step(request: RemoteWorkflowStepRequest) -> Result<RemoteWorkflowStepResult, String> {
    remote_runner::run_remote_workflow_step(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn generate_container_recipe(engine_id: String) -> ContainerRecipe {
    recipes::container_recipe(&engine_id)
}

#[tauri::command]
fn generate_build_recipe(options: BuildRecipeOptions) -> BuildRecipe {
    recipes::build_recipe(options)
}

#[tauri::command]
fn export_recipe_package(request: RecipeExportRequest) -> Result<RecipeExportResult, String> {
    recipes::export_recipe_package(request)
}

#[tauri::command]
fn run_build_workflow(request: BuildWorkflowRequest) -> Result<BuildWorkflowResult, String> {
    build_runner::run_build_workflow(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn prepare_engine_run_package(request: EngineRunRequest) -> Result<EngineRunPackage, String> {
    engine_adapters::prepare_run_package(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn prepare_batch_experiment(request: BatchExperimentRequest) -> Result<BatchExperimentPackage, String> {
    batch::prepare_batch_experiment(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn read_project_text_file(request: ProjectTextFileRequest) -> Result<ProjectTextFilePayload, String> {
    project_files::read_project_text_file(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn write_project_text_file(request: ProjectTextFileWriteRequest) -> Result<ProjectTextFilePayload, String> {
    project_files::write_project_text_file(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn prepare_structure_package(request: StructurePreparationRequest) -> Result<StructurePreparationPackage, String> {
    science_sidecar::prepare_structure_package(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn prepare_trajectory_analysis_package(request: TrajectoryAnalysisRequest) -> Result<TrajectoryAnalysisPackage, String> {
    science_sidecar::prepare_analysis_package(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn parse_engine_log(request: EngineLogParseRequest) -> Result<EngineLogReport, String> {
    engine_adapters::parse_engine_log(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn classify_engine_failure(request: FailureAnalysisRequest) -> Result<FailureAnalysis, String> {
    engine_adapters::classify_engine_failure(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn discover_resume_plan(request: ResumePlanRequest) -> Result<ResumePlan, String> {
    engine_adapters::discover_resume_plan(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn start_local_engine_run(
    request: StartLocalRunRequest,
    state: tauri::State<'_, AppState>,
) -> Result<LocalTaskSnapshot, String> {
    let project_id = request.plan.project_id;
    let snapshot = state
        .task_manager
        .start(request)
        .map_err(|error| error.to_string())?;
    persist_task_snapshot(&state, &snapshot, project_id)?;
    Ok(snapshot)
}

#[tauri::command]
fn get_local_task_snapshot(
    task_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<LocalTaskSnapshot, String> {
    let task_id = uuid::Uuid::parse_str(&task_id).map_err(|error| error.to_string())?;
    let snapshot = state
        .task_manager
        .snapshot(task_id)
        .map_err(|error| error.to_string())?;
    persist_task_snapshot(&state, &snapshot, None)?;
    Ok(snapshot)
}

#[tauri::command]
fn list_local_tasks(state: tauri::State<'_, AppState>) -> Vec<LocalTaskSnapshot> {
    state.task_manager.list()
}

#[tauri::command]
fn cancel_local_task(
    task_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<LocalTaskSnapshot, String> {
    let task_id = uuid::Uuid::parse_str(&task_id).map_err(|error| error.to_string())?;
    let snapshot = state
        .task_manager
        .cancel(task_id)
        .map_err(|error| error.to_string())?;
    persist_task_snapshot(&state, &snapshot, None)?;
    Ok(snapshot)
}

#[tauri::command]
fn list_task_records(project_id: Option<String>, state: tauri::State<'_, AppState>) -> Result<Vec<TaskRecord>, String> {
    let project_id = project_id
        .as_deref()
        .map(uuid::Uuid::parse_str)
        .transpose()
        .map_err(|error| error.to_string())?;
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    db.list_task_records(project_id).map_err(|error| error.to_string())
}

fn persist_task_snapshot(
    state: &tauri::State<'_, AppState>,
    snapshot: &LocalTaskSnapshot,
    project_id: Option<uuid::Uuid>,
) -> Result<(), String> {
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    db.upsert_task_snapshot(snapshot, project_id)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn collect_artifact_index(
    request: ArtifactIndexRequest,
    state: tauri::State<'_, AppState>,
) -> Result<ArtifactIndex, String> {
    let index = artifacts::collect_artifacts(request).map_err(|error| error.to_string())?;
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    db.upsert_artifact_index(&index)
        .map_err(|error| error.to_string())?;
    Ok(index)
}

#[tauri::command]
fn list_artifact_records(project_path: String, state: tauri::State<'_, AppState>) -> Result<Vec<ArtifactRecord>, String> {
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    db.list_artifact_records(project_path)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn export_simulation_report(request: ReportExportRequest) -> Result<ExportedReport, String> {
    artifacts::export_report(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn parse_analysis_results(
    request: AnalysisParseRequest,
    state: tauri::State<'_, AppState>,
) -> Result<AnalysisParseResult, String> {
    let result = analysis::parse_analysis_results(request).map_err(|error| error.to_string())?;
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    db.upsert_analysis_cache(&result)
        .map_err(|error| error.to_string())?;
    Ok(result)
}

#[tauri::command]
fn list_analysis_cache_records(
    project_path: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<AnalysisCacheRecord>, String> {
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    db.list_analysis_cache_records(project_path)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn index_trajectory_file(request: TrajectoryIndexRequest) -> Result<TrajectoryIndex, String> {
    trajectory::index_trajectory(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn read_trajectory_chunk(request: TrajectoryChunkRequest) -> Result<TrajectoryChunk, String> {
    trajectory::read_trajectory_chunk(request).map_err(|error| error.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(".automd"));
            std::fs::create_dir_all(&app_dir)?;
            let project_root = app_dir.join("projects");
            std::fs::create_dir_all(&project_root)?;
            let plugin_root = app_dir.join("plugins");
            std::fs::create_dir_all(&plugin_root)?;
            let engines_root = app_dir.join("engines");
            std::fs::create_dir_all(&engines_root)?;
            let db = ProjectDatabase::open(app_dir.join("automd.sqlite"))
                .map_err(|error| Box::<dyn std::error::Error>::from(error))?;
            let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let resource_dir = app.path().resource_dir().ok();
            let task_resource_root = resource_dir
                .filter(|path| path.join("scripts").join("automd_mock_engine.py").exists())
                .unwrap_or(current_dir);
            app.manage(AppState {
                db: Mutex::new(db),
                project_root,
                plugin_root,
                engines_root,
                task_manager: TaskManager::new(task_resource_root),
            });

            // Native macOS menu bar. Custom items emit a "menu-action" event that
            // the frontend handles (jump pages, open settings, toggle theme, …);
            // standard items (Edit copy/paste, About, Quit, fullscreen) are native.
            let handle = app.handle();
            let settings_item = MenuItem::with_id(handle, "settings", "设置…", true, Some("CmdOrCtrl+,"))?;
            let new_project_item = MenuItem::with_id(handle, "new-project", "新建项目", true, Some("CmdOrCtrl+N"))?;
            let open_folder_item = MenuItem::with_id(handle, "open-project-folder", "打开项目文件夹", true, None::<&str>)?;
            let toggle_theme_item = MenuItem::with_id(handle, "toggle-theme", "切换深色 / 浅色", true, Some("CmdOrCtrl+Shift+L"))?;
            let reload_item = MenuItem::with_id(handle, "reload", "重新加载", true, Some("CmdOrCtrl+R"))?;
            let guide_item = MenuItem::with_id(handle, "guide", "使用指引", true, None::<&str>)?;

            let app_menu = SubmenuBuilder::new(handle, "AutoMD")
                .about(None)
                .separator()
                .item(&settings_item)
                .separator()
                .services()
                .separator()
                .hide()
                .hide_others()
                .show_all()
                .separator()
                .quit()
                .build()?;
            let file_menu = SubmenuBuilder::new(handle, "文件")
                .item(&new_project_item)
                .item(&open_folder_item)
                .separator()
                .close_window()
                .build()?;
            let edit_menu = SubmenuBuilder::new(handle, "编辑")
                .undo()
                .redo()
                .separator()
                .cut()
                .copy()
                .paste()
                .select_all()
                .build()?;
            let view_menu = SubmenuBuilder::new(handle, "视图")
                .item(&toggle_theme_item)
                .separator()
                .item(&reload_item)
                .fullscreen()
                .build()?;
            let help_menu = SubmenuBuilder::new(handle, "帮助")
                .item(&guide_item)
                .build()?;
            let menu = MenuBuilder::new(handle)
                .items(&[&app_menu, &file_menu, &edit_menu, &view_menu, &help_menu])
                .build()?;
            app.set_menu(menu)?;
            app.on_menu_event(move |app, event| {
                let id = event.id().0.as_str();
                if id == "reload" {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.eval("window.location.reload()");
                    }
                    return;
                }
                let _ = app.emit("menu-action", id.to_string());
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_engine_capabilities,
            list_engine_capabilities,
            get_runtime_diagnostics,
            get_science_sidecar_diagnostics,
            list_remote_profile_templates,
            list_remote_profiles,
            save_remote_profile,
            delete_remote_profile,
            list_engine_installations,
            save_engine_installation,
            delete_engine_installation,
            list_installable_engines,
            install_engine,
            list_installable_tools,
            install_tool,
            list_plugin_manifests,
            open_plugin_folder,
            open_path_in_system,
            pick_file_in_system,
            find_executable,
            list_projects,
            create_project,
            delete_project,
            generate_simulation_plan,
            validate_simulation_plan,
            map_engine_parameters,
            create_mock_task,
            import_structure,
            list_imported_structures,
            delete_imported_structure,
            read_structure_file,
            generate_slurm_script,
            generate_remote_execution_package,
            parse_remote_job_status,
            run_remote_workflow_step,
            generate_container_recipe,
            generate_build_recipe,
            export_recipe_package,
            run_build_workflow,
            prepare_engine_run_package,
            prepare_batch_experiment,
            read_project_text_file,
            write_project_text_file,
            prepare_structure_package,
            prepare_trajectory_analysis_package,
            parse_engine_log,
            classify_engine_failure,
            discover_resume_plan,
            start_local_engine_run,
            get_local_task_snapshot,
            list_local_tasks,
            cancel_local_task,
            list_task_records,
            collect_artifact_index,
            list_artifact_records,
            export_simulation_report,
            parse_analysis_results,
            list_analysis_cache_records,
            index_trajectory_file,
            read_trajectory_chunk
        ])
        .run(tauri::generate_context!())
        .expect("error while running AutoMD");
}
