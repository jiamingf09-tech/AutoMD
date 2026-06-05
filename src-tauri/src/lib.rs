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
mod remote_helper;
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

fn engine_python_module(engine_id: &str) -> Option<&'static str> {
    match engine_id {
        "openmm" => Some("openmm"),
        "hoomd" => Some("hoomd"),
        _ => None,
    }
}

fn miniforge_prefix(engines_root: &Path) -> PathBuf {
    engines_root.join("_tools").join("miniforge3")
}

fn path_has_whitespace(path: &Path) -> bool {
    path.as_os_str().to_string_lossy().chars().any(char::is_whitespace)
}

fn home_automd_dir() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("HOME") {
        return Some(PathBuf::from(home).join(".automd"));
    }
    if let Ok(profile) = std::env::var("USERPROFILE") {
        return Some(PathBuf::from(profile).join(".automd"));
    }
    None
}

fn managed_engines_root(app_dir: &Path) -> PathBuf {
    // Miniforge refuses prefixes containing spaces. macOS app data lives under
    // "Application Support", so keep engine/tool environments in a no-space
    // user-managed directory while projects/plugins stay in the normal app data.
    if path_has_whitespace(app_dir) {
        if let Some(home_dir) = home_automd_dir().filter(|path| !path_has_whitespace(path)) {
            return home_dir.join("engines");
        }
    }
    app_dir.join("engines")
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

fn python_binary(prefix: &Path) -> PathBuf {
    if cfg!(target_os = "windows") {
        prefix.join("Scripts").join("python.exe")
    } else {
        prefix.join("bin").join("python")
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
    if engine_python_module(engine_id).is_some() {
        let python = python_binary(prefix);
        return python.is_file().then_some(python);
    }

    let bin = prefix.join("bin");
    let candidates: Vec<String> = engine_registry::detect_engine_by_id(engine_id)
        .map(|capability| capability.executable_names)
        .unwrap_or_default();
    candidates
        .into_iter()
        .map(|name| bin.join(name))
        .find(|path| path.exists())
}

fn detect_python_module_version(python: &Path, module: &str) -> Option<String> {
    let script = format!(
        r#"import importlib.util, importlib.metadata as m
name = {module:?}
if importlib.util.find_spec(name) is None:
    raise SystemExit(2)
try:
    print(m.version(name))
except Exception:
    print("installed")
"#
    );
    let output = Command::new(python).args(["-c", &script]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn detect_installed_version(binary: &Path) -> Option<String> {
    let output = Command::new(binary).arg("--version").output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .next()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
}

fn detect_installed_engine_version(binary: &Path, engine_id: &str) -> Option<String> {
    if let Some(module) = engine_python_module(engine_id) {
        return detect_python_module_version(binary, module);
    }
    detect_installed_version(binary)
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
            apply_installation_records(&mut capabilities, &records, "local", Some(current_native_platform()));
        }
    }
    capabilities
}

#[tauri::command]
fn list_engine_targets(state: tauri::State<'_, AppState>) -> Result<Vec<EngineTarget>, String> {
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    let profiles = merge_remote_profile_templates(db.list_remote_profiles().map_err(|error| error.to_string())?);
    let helper_statuses = db
        .list_remote_helper_statuses()
        .map_err(|error| error.to_string())?;
    let mut targets = vec![local_engine_target()];
    targets.extend(
        profiles
            .iter()
            .map(|profile| engine_target_from_profile(profile, helper_status_for_profile(profile, &helper_statuses))),
    );
    Ok(targets)
}

#[tauri::command]
fn list_engine_capabilities_for_target(
    target_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<EngineCapability>, String> {
    let (target_kind, profile_id) = split_target_id(&target_id);
    let mut capabilities = engine_registry::detect_all();
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    let records = db.list_engine_installations().map_err(|error| error.to_string())?;
    match target_kind {
        EngineTargetKind::Local => {
            apply_installation_records(&mut capabilities, &records, "local", Some(current_native_platform()));
        }
        EngineTargetKind::Remote => {
            let profiles = merge_remote_profile_templates(db.list_remote_profiles().map_err(|error| error.to_string())?);
            let profile = profiles
                .iter()
                .find(|profile| profile.id == profile_id)
                .ok_or_else(|| format!("未找到远程 profile：{profile_id}"))?;
            let helper_status = helper_status_for_profile(
                profile,
                &db.list_remote_helper_statuses().map_err(|error| error.to_string())?,
            );
            if !matches!(&helper_status.status, RemoteHelperState::Ready | RemoteHelperState::Outdated) {
                for capability in &mut capabilities {
                    capability.detection = DetectionState {
                        status: DetectionStatus::MissingInstall,
                        path: None,
                        version: None,
                        message: format!("{} 的 AutoMD 远程 helper 未就绪：{}。", profile.name, profile.host),
                    };
                }
                return Ok(capabilities);
            }
            apply_installation_records(
                &mut capabilities,
                &records,
                &format!("remote:{profile_id}"),
                helper_status.platform,
            );
        }
    }
    Ok(capabilities)
}

fn apply_installation_records(
    capabilities: &mut [EngineCapability],
    records: &[EngineInstallationRecord],
    target_id: &str,
    target_platform: Option<Platform>,
) {
    for capability in capabilities {
        if let Some(platform) = &target_platform {
            if !capability.platform_support.native.contains(platform) {
                capability.detection = DetectionState {
                    status: DetectionStatus::NotApplicable,
                    path: None,
                    version: None,
                    message: format!(
                        "{} 不支持目标平台 {}；支持平台：{}。",
                        capability.name,
                        platform_label(platform),
                        platform_list(&capability.platform_support.native)
                    ),
                };
                continue;
            }
        } else if matches!(
            capability.detection.status,
            DetectionStatus::NotApplicable | DetectionStatus::PlatformUnsupported
        ) {
            continue;
        }
        if target_id != "local"
            && matches!(&capability.detection.status, DetectionStatus::NotApplicable | DetectionStatus::PlatformUnsupported)
        {
            capability.detection = DetectionState {
                status: DetectionStatus::MissingInstall,
                path: None,
                version: None,
                message: "远程目标尚未扫描到该引擎。".to_string(),
            };
        }
        if let Some(record) = records
            .iter()
            .find(|record| record.target_id == target_id && record.engine_id == capability.id)
        {
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

fn current_native_platform() -> Platform {
    match std::env::consts::OS {
        "windows" => Platform::Windows,
        "macos" => Platform::Macos,
        _ => Platform::Linux,
    }
}

fn platform_label(platform: &Platform) -> &'static str {
    match platform {
        Platform::Windows => "windows",
        Platform::Macos => "macos",
        Platform::Linux => "linux",
        Platform::Wsl2 => "wsl2",
        Platform::RemoteLinux => "remoteLinux",
    }
}

fn platform_list(platforms: &[Platform]) -> String {
    platforms
        .iter()
        .map(platform_label)
        .collect::<Vec<_>>()
        .join(", ")
}

fn merge_remote_profile_templates(mut saved: Vec<RemoteProfile>) -> Vec<RemoteProfile> {
    for template in runtime::remote_profile_templates() {
        if !saved.iter().any(|profile| profile.id == template.id) {
            saved.push(template);
        }
    }
    saved
}

fn local_engine_target() -> EngineTarget {
    let platform = current_native_platform();
    EngineTarget {
        id: "local".to_string(),
        kind: EngineTargetKind::Local,
        profile_id: None,
        label: "本机".to_string(),
        detail: format!("{} · {}", platform_label(&platform), std::env::consts::ARCH),
        status: RemoteHelperState::Ready,
        platform: Some(platform),
        arch: Some(std::env::consts::ARCH.to_string()),
        hostname: None,
    }
}

fn helper_status_for_profile(
    profile: &RemoteProfile,
    statuses: &[RemoteHelperStatus],
) -> RemoteHelperStatus {
    statuses
        .iter()
        .find(|status| status.profile_id == profile.id)
        .cloned()
        .unwrap_or_else(|| RemoteHelperStatus {
            profile_id: profile.id.clone(),
            helper_version: None,
            status: RemoteHelperState::Missing,
            install_path: None,
            platform: None,
            arch: None,
            hostname: None,
            hardware_json: None,
            checked_at: chrono::Utc::now(),
            last_error: Some("远程 helper 未安装。".to_string()),
        })
}

fn engine_target_from_profile(profile: &RemoteProfile, helper: RemoteHelperStatus) -> EngineTarget {
    let status_text = match &helper.status {
        RemoteHelperState::Ready => helper
            .platform
            .as_ref()
            .map(platform_label)
            .unwrap_or("已检测"),
        RemoteHelperState::Missing => "未安装 helper",
        RemoteHelperState::Outdated => "helper 版本过旧",
        RemoteHelperState::Unreachable => "远程不可达",
        RemoteHelperState::PermissionDenied => "权限不足",
    };
    EngineTarget {
        id: format!("remote:{}", profile.id),
        kind: EngineTargetKind::Remote,
        profile_id: Some(profile.id.clone()),
        label: profile.name.clone(),
        detail: format!("{} · {}", profile.host, status_text),
        status: helper.status,
        platform: helper.platform,
        arch: helper.arch,
        hostname: helper.hostname,
    }
}

fn split_target_id(target_id: &str) -> (EngineTargetKind, String) {
    if let Some(profile_id) = target_id.strip_prefix("remote:") {
        (EngineTargetKind::Remote, profile_id.to_string())
    } else if target_id == "local" {
        (EngineTargetKind::Local, "local".to_string())
    } else {
        (EngineTargetKind::Remote, target_id.to_string())
    }
}

fn remote_profile_by_id(state: &tauri::State<'_, AppState>, profile_id: &str) -> Result<RemoteProfile, String> {
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    let profiles = merge_remote_profile_templates(db.list_remote_profiles().map_err(|error| error.to_string())?);
    profiles
        .into_iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| format!("未找到远程 profile：{profile_id}"))
}

fn remote_helper_status_by_profile(
    state: &tauri::State<'_, AppState>,
    profile_id: &str,
) -> Result<RemoteHelperStatus, String> {
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    let statuses = db
        .list_remote_helper_statuses()
        .map_err(|error| error.to_string())?;
    statuses
        .into_iter()
        .find(|status| status.profile_id == profile_id)
        .ok_or_else(|| "远程 helper 未安装，请先在远程页安装/检测 helper。".to_string())
}

#[tauri::command]
fn get_runtime_diagnostics() -> RuntimeDiagnostics {
    runtime::diagnostics()
}

#[tauri::command]
fn get_science_sidecar_diagnostics(state: tauri::State<'_, AppState>) -> ScienceSidecarDiagnostics {
    science_sidecar::diagnostics(Some(&state.engines_root))
}

#[tauri::command]
async fn install_science_sidecar(
    state: tauri::State<'_, AppState>,
) -> Result<ScienceSidecarDiagnostics, String> {
    let engines_root = state.engines_root.clone();
    // The science env is large; run the whole install on the blocking pool so the
    // UI never freezes.
    tauri::async_runtime::spawn_blocking(move || -> Result<ScienceSidecarDiagnostics, String> {
        std::fs::create_dir_all(&engines_root).map_err(|error| error.to_string())?;
        let manager = ensure_conda_manager(&engines_root)?;
        let prefix = engines_root.join("_tools").join("automd-science");
        let python = if cfg!(target_os = "windows") {
            prefix.join("Scripts").join("python.exe")
        } else {
            prefix.join("bin").join("python")
        };
        let mut command = Command::new(&manager);
        if python.is_file() {
            command.arg("install");
        } else {
            command.arg("create");
        }
        let output = command
            .arg("-y")
            .arg("-p")
            .arg(&prefix)
            .arg("-c")
            .arg("conda-forge")
            .args([
                "python=3.11",
                "openmm",
                "pdbfixer",
                "mdanalysis",
                "mdtraj",
                "rdkit",
                "openbabel",
                "ambertools",
                "numpy",
                "pandas",
            ])
            .output()
            .map_err(|error| format!("启动科学侧车安装器失败：{error}"))?;

        if !output.status.success() {
            return Err(format!("科学侧车环境安装失败：\n{}", command_tail(&output.stderr)));
        }

        Ok(science_sidecar::diagnostics(Some(&engines_root)))
    })
    .await
    .map_err(|error| format!("安装任务执行失败：{error}"))?
}

#[tauri::command]
fn inspect_science_tool(request: ScienceToolInspectRequest) -> Result<ScienceToolDiagnostic, String> {
    let path = PathBuf::from(&request.executable_path);
    if !path.is_file() {
        return Err(format!("路径不存在或不是可执行文件：{}", path.display()));
    }
    if let Some(import_name) = request.import_name.clone() {
        let script = format!(
            r#"import importlib.util
import importlib.metadata as metadata
name = {import_name:?}
spec = importlib.util.find_spec(name)
if spec is None:
    raise SystemExit(2)
try:
    print(metadata.version(name))
except Exception:
    print("installed")
"#
        );
        let output = Command::new(&path)
            .args(["-c", &script])
            .output()
            .map_err(|error| format!("启动 Python 失败：{error}"))?;
        if output.status.success() {
            return Ok(ScienceToolDiagnostic {
                id: request.id,
                label: request.label,
                import_name: request.import_name,
                command: request.command,
                status: DetectionStatus::Ready,
                version: String::from_utf8(output.stdout)
                    .ok()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty()),
                detail: format!("{} can import {}", path.display(), import_name),
            });
        }
        return Ok(ScienceToolDiagnostic {
            id: request.id,
            label: request.label,
            import_name: request.import_name,
            command: request.command,
            status: DetectionStatus::MissingInstall,
            version: None,
            detail: format!("{} cannot import {}", path.display(), import_name),
        });
    }

    Ok(ScienceToolDiagnostic {
        id: request.id,
        label: request.label,
        import_name: request.import_name,
        command: request.command,
        status: DetectionStatus::Ready,
        version: detect_installed_version(&path),
        detail: path.display().to_string(),
    })
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

#[tauri::command]
fn delete_engine_installation_for_target(
    target_id: String,
    engine_id: String,
    location: String,
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    db.delete_engine_installation_for_target(target_id, engine_id, location)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn check_remote_helper(
    profile_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<RemoteHelperStatus, String> {
    let profile = remote_profile_by_id(&state, &profile_id)?;
    let existing = remote_helper_status_by_profile(&state, &profile_id).ok();
    let checked = remote_helper::check_helper(&profile, existing.and_then(|status| status.install_path))
        .unwrap_or_else(|error| RemoteHelperStatus {
            profile_id: profile.id.clone(),
            helper_version: None,
            status: if error.to_string().to_ascii_lowercase().contains("permission") {
                RemoteHelperState::PermissionDenied
            } else {
                RemoteHelperState::Unreachable
            },
            install_path: Some(remote_helper::default_install_path(&profile)),
            platform: None,
            arch: None,
            hostname: None,
            hardware_json: None,
            checked_at: chrono::Utc::now(),
            last_error: Some(error.to_string()),
        });
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    db.save_remote_helper_status(checked)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn install_remote_helper(
    profile_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<RemoteHelperStatus, String> {
    let profile = remote_profile_by_id(&state, &profile_id)?;
    let installed = remote_helper::install_helper(&profile).unwrap_or_else(|error| RemoteHelperStatus {
        profile_id: profile.id.clone(),
        helper_version: None,
        status: if error.to_string().to_ascii_lowercase().contains("permission") {
            RemoteHelperState::PermissionDenied
        } else {
            RemoteHelperState::Unreachable
        },
        install_path: Some(remote_helper::default_install_path(&profile)),
        platform: None,
        arch: None,
        hostname: None,
        hardware_json: None,
        checked_at: chrono::Utc::now(),
        last_error: Some(error.to_string()),
    });
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    db.save_remote_helper_status(installed)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn scan_engines_on_target(
    target_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<EngineCapability>, String> {
    let (target_kind, profile_id) = split_target_id(&target_id);
    match target_kind {
        EngineTargetKind::Local => Ok(list_engine_capabilities(state)),
        EngineTargetKind::Remote => {
            let profile = remote_profile_by_id(&state, &profile_id)?;
            let helper_status = remote_helper_status_by_profile(&state, &profile_id)?;
            if !matches!(&helper_status.status, RemoteHelperState::Ready | RemoteHelperState::Outdated) {
                return Err("远程 helper 未就绪，请先在远程页安装或检测 helper。".to_string());
            }
            let install_path = helper_status
                .install_path
                .as_deref()
                .ok_or_else(|| "远程 helper 缺少安装路径。".to_string())?;
            let target_id = format!("remote:{profile_id}");
            let target_label = profile.name.clone();
            let mut found_records = Vec::new();
            for engine in engine_registry::detect_all() {
                if let Some(platform) = &helper_status.platform {
                    if !engine.platform_support.native.contains(platform) {
                        continue;
                    }
                }
                if let Some(probe) = remote_helper::scan_engine(&profile, install_path, &engine.executable_names)
                    .map_err(|error| error.to_string())?
                {
                    found_records.push(EngineInstallationRecord {
                        target_kind: EngineTargetKind::Remote,
                        target_id: target_id.clone(),
                        target_label: target_label.clone(),
                        engine_id: engine.id,
                        location: probe.location,
                        version: probe.version,
                        authorization_status: if engine.license.requires_user_license {
                            DetectionStatus::MissingLicense
                        } else {
                            DetectionStatus::Ready
                        },
                        platform: probe.platform.or_else(|| helper_status.platform.clone()),
                        arch: probe.arch.or_else(|| helper_status.arch.clone()),
                        checked_at: chrono::Utc::now(),
                    });
                }
            }
            {
                let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
                for record in found_records {
                    db.save_engine_installation(record)
                        .map_err(|error| error.to_string())?;
                }
            }
            list_engine_capabilities_for_target(target_id, state)
        }
    }
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
async fn install_engine(
    engine_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<EngineInstallationRecord, String> {
    if let Some(capability) = engine_registry::detect_engine_by_id(&engine_id) {
        if matches!(
            capability.detection.status,
            DetectionStatus::NotApplicable | DetectionStatus::PlatformUnsupported
        ) {
            return Err(capability.detection.message);
        }
    }
    let package = engine_conda_package(&engine_id).ok_or_else(|| {
        format!("{engine_id} 暂不支持一键安装（通常需要许可或手动编译），请在指引页查看安装方式。")
    })?;
    let engines_root = state.engines_root.clone();
    let id = engine_id.clone();
    // The download + conda create can take minutes. Run it on the blocking pool
    // so the UI thread is never blocked (the install stays fully async).
    let (location, version) =
        tauri::async_runtime::spawn_blocking(move || -> Result<(String, Option<String>), String> {
            std::fs::create_dir_all(&engines_root).map_err(|error| error.to_string())?;
            let manager = ensure_conda_manager(&engines_root)?;
            let prefix = engines_root.join(&id);
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
                return Err(format!("{id} 安装失败：\n{}", command_tail(&output.stderr)));
            }
            let binary = locate_installed_binary(&prefix, &id).ok_or_else(|| {
                if engine_python_module(&id).is_some() {
                    format!("{id} 安装完成，但未在 {} 找到 Python 环境。", prefix.display())
                } else {
                    format!("{id} 安装完成，但未在 {} 找到可执行文件。", prefix.join("bin").display())
                }
            })?;
            let version = detect_installed_engine_version(&binary, &id);
            if let Some(module) = engine_python_module(&id) {
                if version.is_none() {
                    return Err(format!(
                        "{id} 安装完成，但 {} 无法导入 Python 模块 {module}。",
                        binary.display()
                    ));
                }
            }
            Ok((binary.display().to_string(), version))
        })
        .await
        .map_err(|error| format!("安装任务执行失败：{error}"))??;

    let record = EngineInstallationRecord {
        target_kind: EngineTargetKind::Local,
        target_id: "local".to_string(),
        target_label: "本机".to_string(),
        engine_id,
        location,
        version,
        authorization_status: DetectionStatus::Ready,
        platform: Some(current_native_platform()),
        arch: Some(std::env::consts::ARCH.to_string()),
        checked_at: chrono::Utc::now(),
    };
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    db.save_engine_installation(record.clone())
        .map_err(|error| error.to_string())?;
    Ok(record)
}

fn source_build_capable(engine_id: &str) -> bool {
    matches!(engine_id, "gromacs" | "cp2k")
}

fn resolve_deploy_strategy(engine_id: &str, requested: EngineDeployStrategy) -> EngineDeployStrategy {
    match requested {
        EngineDeployStrategy::Auto => {
            if engine_conda_package(engine_id).is_some() {
                EngineDeployStrategy::Package
            } else if source_build_capable(engine_id) {
                EngineDeployStrategy::SourceBuild
            } else {
                EngineDeployStrategy::RecipeOnly
            }
        }
        other => other,
    }
}

fn locate_source_build_binary(engine_id: &str, options: &BuildRecipeOptions) -> Option<PathBuf> {
    let prefix = options
        .install_prefix
        .as_deref()
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|home| PathBuf::from(home).join(".local").join("automd").join(engine_id))
        })?;
    let bin_dir = prefix.join("bin");
    let capability = engine_registry::detect_engine_by_id(engine_id)?;
    for executable in capability.executable_names {
        let candidate = bin_dir.join(&executable);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn source_build_record(
    target_id: &str,
    target_label: String,
    engine_id: String,
    options: &BuildRecipeOptions,
) -> Option<EngineInstallationRecord> {
    let binary = locate_source_build_binary(&engine_id, options)?;
    Some(EngineInstallationRecord {
        target_kind: EngineTargetKind::Local,
        target_id: target_id.to_string(),
        target_label,
        engine_id: engine_id.clone(),
        location: binary.display().to_string(),
        version: detect_installed_engine_version(&binary, &engine_id),
        authorization_status: DetectionStatus::Ready,
        platform: Some(current_native_platform()),
        arch: Some(std::env::consts::ARCH.to_string()),
        checked_at: chrono::Utc::now(),
    })
}

fn run_local_build_for_deploy(request: &EngineDeployRequest) -> Result<BuildWorkflowResult, String> {
    let project_path = request
        .project_path
        .clone()
        .ok_or_else(|| "需要先创建项目，才能生成或执行构建 recipe。".to_string())?;
    build_runner::run_build_workflow(BuildWorkflowRequest {
        project_path,
        build_options: request.build_options.clone(),
        include_container: true,
        include_build_script: true,
        mode: request.mode.clone(),
        timeout_seconds: request.timeout_seconds,
    })
    .map_err(|error| error.to_string())
}

#[tauri::command]
async fn install_or_build_engine(
    request: EngineDeployRequest,
    state: tauri::State<'_, AppState>,
) -> Result<EngineDeployResult, String> {
    let strategy = resolve_deploy_strategy(&request.engine_id, request.strategy.clone());
    let (target_kind, profile_id) = split_target_id(&request.target_id);
    let mut warnings = Vec::new();

    match (target_kind, strategy.clone()) {
        (EngineTargetKind::Local, EngineDeployStrategy::Package) => {
            let record = install_engine(request.engine_id.clone(), state.clone()).await?;
            Ok(EngineDeployResult {
                target_id: "local".to_string(),
                engine_id: request.engine_id,
                strategy,
                mode: request.mode,
                record: Some(record),
                build_result: None,
                status: TaskStatus::Completed,
                stdout: "本机包管理安装完成。".to_string(),
                stderr: String::new(),
                warnings,
            })
        }
        (EngineTargetKind::Local, EngineDeployStrategy::SourceBuild) => {
            if !source_build_capable(&request.engine_id) {
                return Err(format!("{} 尚无完整源码构建 recipe，只能生成接入清单或手动登记。", request.engine_id));
            }
            let build_result = run_local_build_for_deploy(&request)?;
            let mut record = None;
            if matches!(&request.mode, BuildWorkflowMode::Execute) && build_result.status == TaskStatus::Completed {
                if let Some(found) = source_build_record("local", "本机".to_string(), request.engine_id.clone(), &request.build_options) {
                    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
                    record = Some(db.save_engine_installation(found).map_err(|error| error.to_string())?);
                } else {
                    warnings.push("构建完成，但未自动定位到可执行文件；请回到引擎页手动登记路径。".to_string());
                }
            }
            Ok(EngineDeployResult {
                target_id: "local".to_string(),
                engine_id: request.engine_id,
                strategy,
                mode: request.mode,
                record,
                status: build_result.status.clone(),
                stdout: build_result.stdout.clone(),
                stderr: build_result.stderr.clone(),
                build_result: Some(build_result),
                warnings,
            })
        }
        (EngineTargetKind::Local, EngineDeployStrategy::RecipeOnly) => {
            let build_result = run_local_build_for_deploy(&request)?;
            warnings.push("该引擎不能由 AutoMD 自动下载或编译；已生成 recipe/接入清单，请按授权和上游文档处理。".to_string());
            Ok(EngineDeployResult {
                target_id: "local".to_string(),
                engine_id: request.engine_id,
                strategy,
                mode: request.mode,
                record: None,
                status: build_result.status.clone(),
                stdout: build_result.stdout.clone(),
                stderr: build_result.stderr.clone(),
                build_result: Some(build_result),
                warnings,
            })
        }
        (EngineTargetKind::Remote, EngineDeployStrategy::Package) => {
            let package = engine_conda_package(&request.engine_id)
                .ok_or_else(|| format!("{} 不能通过包管理器自动部署。", request.engine_id))?;
            let profile = remote_profile_by_id(&state, &profile_id)?;
            let helper = remote_helper_status_by_profile(&state, &profile_id)?;
            if !matches!(&helper.status, RemoteHelperState::Ready | RemoteHelperState::Outdated) {
                return Err("远程 helper 未就绪，请先在远程页安装或检测 helper。".to_string());
            }
            let install_path = helper
                .install_path
                .as_deref()
                .ok_or_else(|| "远程 helper 缺少安装路径。".to_string())?;
            let capability = engine_registry::detect_engine_by_id(&request.engine_id)
                .ok_or_else(|| format!("未知引擎：{}", request.engine_id))?;
            if let Some(platform) = &helper.platform {
                if !capability.platform_support.native.contains(platform) {
                    return Err(format!("{} 不支持该远程平台 {}。", capability.name, platform_label(platform)));
                }
            }
            let probe = remote_helper::install_engine_with_helper(
                &profile,
                install_path,
                &request.engine_id,
                package,
                &capability.executable_names,
            )
            .map_err(|error| error.to_string())?;
            let record = EngineInstallationRecord {
                target_kind: EngineTargetKind::Remote,
                target_id: format!("remote:{profile_id}"),
                target_label: profile.name.clone(),
                engine_id: request.engine_id.clone(),
                location: probe.location,
                version: probe.version,
                authorization_status: if capability.license.requires_user_license {
                    DetectionStatus::MissingLicense
                } else {
                    DetectionStatus::Ready
                },
                platform: probe.platform.or(helper.platform),
                arch: probe.arch.or(helper.arch),
                checked_at: chrono::Utc::now(),
            };
            let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
            let saved = db.save_engine_installation(record).map_err(|error| error.to_string())?;
            Ok(EngineDeployResult {
                target_id: format!("remote:{profile_id}"),
                engine_id: request.engine_id,
                strategy,
                mode: request.mode,
                record: Some(saved),
                build_result: None,
                status: TaskStatus::Completed,
                stdout: "远程包管理部署完成。".to_string(),
                stderr: String::new(),
                warnings,
            })
        }
        (EngineTargetKind::Remote, EngineDeployStrategy::SourceBuild) => {
            if !source_build_capable(&request.engine_id) {
                return Err(format!("{} 尚无完整源码构建 recipe，只能生成接入清单或手动登记。", request.engine_id));
            }
            let profile = remote_profile_by_id(&state, &profile_id)?;
            let helper = remote_helper_status_by_profile(&state, &profile_id)?;
            if !matches!(&helper.status, RemoteHelperState::Ready | RemoteHelperState::Outdated) {
                return Err("远程 helper 未就绪，请先在远程页安装或检测 helper。".to_string());
            }
            let install_path = helper
                .install_path
                .as_deref()
                .ok_or_else(|| "远程 helper 缺少安装路径。".to_string())?;
            if !matches!(&request.mode, BuildWorkflowMode::Execute) {
                let build_result = run_local_build_for_deploy(&request)?;
                return Ok(EngineDeployResult {
                    target_id: format!("remote:{profile_id}"),
                    engine_id: request.engine_id,
                    strategy,
                    mode: request.mode,
                    record: None,
                    status: build_result.status.clone(),
                    stdout: build_result.stdout.clone(),
                    stderr: build_result.stderr.clone(),
                    build_result: Some(build_result),
                    warnings,
                });
            }
            let build = recipes::build_recipe(request.build_options.clone());
            let stdout = remote_helper::run_build_engine_with_helper(&profile, install_path, &request.engine_id, &build.script)
                .map_err(|error| error.to_string())?;
            let capability = engine_registry::detect_engine_by_id(&request.engine_id)
                .ok_or_else(|| format!("未知引擎：{}", request.engine_id))?;
            let record = remote_helper::scan_engine(&profile, install_path, &capability.executable_names)
                .map_err(|error| error.to_string())?
                .map(|probe| EngineInstallationRecord {
                    target_kind: EngineTargetKind::Remote,
                    target_id: format!("remote:{profile_id}"),
                    target_label: profile.name.clone(),
                    engine_id: request.engine_id.clone(),
                    location: probe.location,
                    version: probe.version,
                    authorization_status: DetectionStatus::Ready,
                    platform: probe.platform.or(helper.platform.clone()),
                    arch: probe.arch.or(helper.arch.clone()),
                    checked_at: chrono::Utc::now(),
                });
            let saved = if let Some(record) = record {
                let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
                Some(db.save_engine_installation(record).map_err(|error| error.to_string())?)
            } else {
                warnings.push("远程构建完成，但未自动扫描到可执行文件；请手动登记远程路径。".to_string());
                None
            };
            Ok(EngineDeployResult {
                target_id: format!("remote:{profile_id}"),
                engine_id: request.engine_id,
                strategy,
                mode: request.mode,
                record: saved,
                build_result: None,
                status: TaskStatus::Completed,
                stdout,
                stderr: String::new(),
                warnings,
            })
        }
        (_, EngineDeployStrategy::RecipeOnly) => {
            let build_result = run_local_build_for_deploy(&request)?;
            warnings.push("该引擎需要用户源码、许可证或目标平台工具链；AutoMD 只生成 recipe/接入清单。".to_string());
            Ok(EngineDeployResult {
                target_id: request.target_id,
                engine_id: request.engine_id,
                strategy,
                mode: request.mode,
                record: None,
                status: build_result.status.clone(),
                stdout: build_result.stdout.clone(),
                stderr: build_result.stderr.clone(),
                build_result: Some(build_result),
                warnings,
            })
        }
        (_, EngineDeployStrategy::Auto) => unreachable!("auto strategy is resolved before matching"),
    }
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
async fn install_tool(tool_id: String, state: tauri::State<'_, AppState>) -> Result<String, String> {
    let engines_root = state.engines_root.clone();
    // Blocking conda/download work runs off the UI thread (fully async).
    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        std::fs::create_dir_all(&engines_root).map_err(|error| error.to_string())?;

        if tool_id == "conda" {
            return install_miniforge(&engines_root).map(|path| path.display().to_string());
        }
        if tool_id == "mamba" {
            return install_internal_mamba(&engines_root).map(|path| path.display().to_string());
        }

        let (package, exe) = tool_conda_spec(&tool_id).ok_or_else(|| {
            format!("{tool_id} 暂不支持一键安装（通常由系统、GPU 驱动或集群提供）。")
        })?;
        let manager = ensure_conda_manager(&engines_root)?;
        let prefix = engines_root.join("_tools").join(&tool_id);

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
            return Err(format!("{tool_id} 安装失败：\n{}", command_tail(&output.stderr)));
        }

        let binary = prefix.join("bin").join(exe);
        if !binary.is_file() {
            return Err(format!("{tool_id} 安装完成，但未找到 {}。", binary.display()));
        }
        Ok(binary.display().to_string())
    })
    .await
    .map_err(|error| format!("安装任务执行失败：{error}"))?
}

#[tauri::command]
fn list_plugin_manifests(state: tauri::State<'_, AppState>) -> Result<PluginRegistrySnapshot, String> {
    plugin_registry_snapshot(&state)
}

fn plugin_registry_snapshot(state: &tauri::State<'_, AppState>) -> Result<PluginRegistrySnapshot, String> {
    let states = state
        .db
        .lock()
        .map_err(|error| error.to_string())?
        .list_plugin_states()
        .map_err(|error| error.to_string())?;
    plugins::registry_snapshot(&state.plugin_root, &states).map_err(|error| error.to_string())
}

#[tauri::command]
fn import_plugin(state: tauri::State<'_, AppState>, request: PluginImportRequest) -> Result<PluginRegistrySnapshot, String> {
    let states = state
        .db
        .lock()
        .map_err(|error| error.to_string())?
        .list_plugin_states()
        .map_err(|error| error.to_string())?;
    let plugin_id = plugins::import_plugin(&state.plugin_root, &request, &states).map_err(|error| error.to_string())?;
    state
        .db
        .lock()
        .map_err(|error| error.to_string())?
        .set_plugin_enabled(&plugin_id, true)
        .map_err(|error| error.to_string())?;
    plugin_registry_snapshot(&state)
}

#[tauri::command]
fn create_plugin_template(state: tauri::State<'_, AppState>, request: PluginTemplateRequest) -> Result<PluginRegistrySnapshot, String> {
    let plugin_id = plugins::create_plugin_template(&state.plugin_root, &request).map_err(|error| error.to_string())?;
    state
        .db
        .lock()
        .map_err(|error| error.to_string())?
        .set_plugin_enabled(&plugin_id, true)
        .map_err(|error| error.to_string())?;
    plugin_registry_snapshot(&state)
}

#[tauri::command]
fn set_plugin_enabled(state: tauri::State<'_, AppState>, plugin_id: String, enabled: bool) -> Result<PluginRegistrySnapshot, String> {
    let snapshot = plugin_registry_snapshot(&state)?;
    let manifest = snapshot
        .manifests
        .iter()
        .find(|manifest| manifest.id == plugin_id)
        .ok_or_else(|| format!("插件不存在：{plugin_id}"))?;
    if matches!(manifest.origin, PluginOrigin::BuiltIn) {
        return Err(format!("内置插件不能停用：{plugin_id}"));
    }
    state
        .db
        .lock()
        .map_err(|error| error.to_string())?
        .set_plugin_enabled(&plugin_id, enabled)
        .map_err(|error| error.to_string())?;
    plugin_registry_snapshot(&state)
}

#[tauri::command]
fn delete_plugin(state: tauri::State<'_, AppState>, plugin_id: String) -> Result<PluginRegistrySnapshot, String> {
    let states = state
        .db
        .lock()
        .map_err(|error| error.to_string())?
        .list_plugin_states()
        .map_err(|error| error.to_string())?;
    plugins::delete_user_plugin(&state.plugin_root, &plugin_id, &states).map_err(|error| error.to_string())?;
    state
        .db
        .lock()
        .map_err(|error| error.to_string())?
        .delete_plugin_state(&plugin_id)
        .map_err(|error| error.to_string())?;
    plugin_registry_snapshot(&state)
}

#[tauri::command]
fn save_plugin_config(state: tauri::State<'_, AppState>, request: PluginConfigRequest) -> Result<PluginRegistrySnapshot, String> {
    let snapshot = plugin_registry_snapshot(&state)?;
    let manifest = snapshot
        .manifests
        .iter()
        .find(|manifest| manifest.id == request.plugin_id)
        .ok_or_else(|| format!("插件不存在：{}", request.plugin_id))?;
    if matches!(manifest.origin, PluginOrigin::BuiltIn) {
        return Err(format!("内置插件不能修改配置：{}", request.plugin_id));
    }
    state
        .db
        .lock()
        .map_err(|error| error.to_string())?
        .save_plugin_config(&request.plugin_id, request.config)
        .map_err(|error| error.to_string())?;
    plugin_registry_snapshot(&state)
}

#[tauri::command]
fn run_plugin_action(state: tauri::State<'_, AppState>, request: PluginRunRequest) -> Result<PluginRunResult, String> {
    let snapshot = plugin_registry_snapshot(&state)?;
    let manifest = snapshot
        .manifests
        .iter()
        .find(|manifest| manifest.id == request.plugin_id)
        .cloned()
        .ok_or_else(|| format!("插件不存在：{}", request.plugin_id))?;
    let run_id = uuid::Uuid::new_v4();
    {
        state
            .db
            .lock()
            .map_err(|error| error.to_string())?
            .insert_plugin_run(run_id, &request.plugin_id, &request.action_id, request.mode.clone())
            .map_err(|error| error.to_string())?;
    }

    match plugins::execute_plugin_action(&state.plugin_root, &manifest, &request, &run_id.to_string()) {
        Ok((stdout, stderr, parsed_output, warnings)) => {
            let record = state
                .db
                .lock()
                .map_err(|error| error.to_string())?
                .finish_plugin_run(run_id, PluginRunStatus::Completed, &stdout, &stderr)
                .map_err(|error| error.to_string())?;
            Ok(PluginRunResult { record, stdout, stderr, parsed_output, warnings })
        }
        Err(error) => {
            let message = error.to_string();
            let record = state
                .db
                .lock()
                .map_err(|error| error.to_string())?
                .finish_plugin_run(run_id, PluginRunStatus::Failed, "", &message)
                .map_err(|error| error.to_string())?;
            Ok(PluginRunResult {
                record,
                stdout: String::new(),
                stderr: message,
                parsed_output: None,
                warnings: vec!["插件运行失败，已记录到运行历史。".to_string()],
            })
        }
    }
}

#[tauri::command]
fn open_plugin_folder(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    std::fs::create_dir_all(&state.plugin_root).map_err(|error| error.to_string())?;
    tauri_plugin_opener::open_path(&state.plugin_root, None::<&str>).map_err(|error| error.to_string())?;
    Ok(true)
}

#[tauri::command]
fn open_plugin_install_folder(state: tauri::State<'_, AppState>, plugin_id: String) -> Result<bool, String> {
    let snapshot = plugin_registry_snapshot(&state)?;
    let manifest = snapshot
        .manifests
        .iter()
        .find(|manifest| manifest.id == plugin_id)
        .ok_or_else(|| format!("插件不存在：{plugin_id}"))?;
    let target = manifest
        .install_path
        .as_deref()
        .or(manifest.source_path.as_deref())
        .ok_or_else(|| "内置插件没有可打开的安装目录。".to_string())?;
    let path = PathBuf::from(target);
    let folder = if path.is_file() {
        path.parent().map(PathBuf::from).unwrap_or(path)
    } else {
        path
    };
    if !folder.exists() {
        return Err(format!("插件目录不存在：{}", folder.display()));
    }
    tauri_plugin_opener::open_path(folder, None::<&str>).map_err(|error| error.to_string())?;
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

fn find_python_module_executable(
    module: &str,
    dirs: &[PathBuf],
    checked_locations: &mut Vec<String>,
) -> Option<(PathBuf, Option<String>)> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    for command in ["python3", "python"] {
        if let Some(path) = sysenv::resolve_command(command) {
            candidates.push(path);
        }
    }
    for dir in dirs {
        for command in ["python3", "python"] {
            for candidate in sysenv::executable_candidates(command) {
                candidates.push(dir.join(candidate));
            }
        }
    }

    let mut seen = std::collections::HashSet::new();
    for candidate in candidates {
        if !seen.insert(candidate.clone()) {
            continue;
        }
        checked_locations.push(candidate.display().to_string());
        if !candidate.is_file() {
            continue;
        }
        if let Some(version) = detect_python_module_version(&candidate, module) {
            return Some((candidate, Some(version)));
        }
    }
    None
}

#[tauri::command]
fn find_executable(request: ExecutableSearchRequest) -> Result<ExecutableSearchResult, String> {
    let mut checked_locations = Vec::new();
    // search_dirs() recovers the login-shell PATH + conda install dirs/envs so a
    // GUI launch (minimal PATH) still finds conda-installed tools.
    let mut dirs = sysenv::search_dirs();
    dirs.extend(request.extra_dirs.into_iter().map(PathBuf::from));

    for command in request.commands.iter().filter(|command| !command.trim().is_empty()) {
        if let Some(module) = command.strip_prefix("python module:").map(str::trim) {
            if let Some((path, version)) = find_python_module_executable(module, &dirs, &mut checked_locations) {
                let version_text = version
                    .map(|value| format!("，版本 {value}"))
                    .unwrap_or_default();
                return Ok(ExecutableSearchResult {
                    found: true,
                    command: Some(command.clone()),
                    path: Some(path.display().to_string()),
                    checked_locations,
                    message: format!("已在 {} 检测到 Python 模块 {module}{version_text}。", path.display()),
                });
            }
            continue;
        }

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

fn require_plan_structure(plan: &SimulationPlan) -> Result<(), String> {
    let has_structure = plan
        .system
        .source_path
        .as_deref()
        .map(str::trim)
        .is_some_and(|path| !path.is_empty());
    if has_structure {
        Ok(())
    } else {
        Err("未选中结构：请先在“项目”页导入并选中一个结构；AutoMD 不会在无结构时生成或发送分子动力学运行指令。".to_string())
    }
}

#[tauri::command]
fn map_engine_parameters(request: ParameterMappingRequest) -> ParameterMappingReport {
    parameter_mapping::map_parameters(request)
}

#[tauri::command]
fn create_mock_task(plan: SimulationPlan) -> Result<SimulationTask, String> {
    require_plan_structure(&plan)?;
    Ok(planner::mock_task(plan))
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
fn generate_slurm_script(plan: SimulationPlan) -> Result<String, String> {
    require_plan_structure(&plan)?;
    Ok(recipes::slurm_script(&plan))
}

#[tauri::command]
fn generate_remote_execution_package(request: RemoteExecutionRequest) -> Result<RemoteExecutionPackage, String> {
    require_plan_structure(&request.plan)?;
    Ok(recipes::remote_execution_package(request))
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
    require_plan_structure(&request.plan)?;
    engine_adapters::prepare_run_package(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn prepare_batch_experiment(request: BatchExperimentRequest) -> Result<BatchExperimentPackage, String> {
    require_plan_structure(&request.plan)?;
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
    require_plan_structure(&request.plan)?;
    science_sidecar::prepare_structure_package(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn prepare_trajectory_analysis_package(request: TrajectoryAnalysisRequest) -> Result<TrajectoryAnalysisPackage, String> {
    require_plan_structure(&request.plan)?;
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
    require_plan_structure(&request.plan)?;
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
            let engines_root = managed_engines_root(&app_dir);
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
            list_engine_targets,
            list_engine_capabilities_for_target,
            get_runtime_diagnostics,
            get_science_sidecar_diagnostics,
            install_science_sidecar,
            inspect_science_tool,
            list_remote_profile_templates,
            list_remote_profiles,
            save_remote_profile,
            delete_remote_profile,
            list_engine_installations,
            save_engine_installation,
            delete_engine_installation,
            delete_engine_installation_for_target,
            scan_engines_on_target,
            check_remote_helper,
            install_remote_helper,
            list_installable_engines,
            install_engine,
            install_or_build_engine,
            list_installable_tools,
            install_tool,
            list_plugin_manifests,
            import_plugin,
            create_plugin_template,
            set_plugin_enabled,
            delete_plugin,
            save_plugin_config,
            run_plugin_action,
            open_plugin_folder,
            open_plugin_install_folder,
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
