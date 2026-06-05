use crate::models::*;
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PluginRegistryError {
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("plugin not found: {0}")]
    NotFound(String),
    #[error("plugin already exists: {0}")]
    Conflict(String),
    #[error("built-in plugins cannot be modified: {0}")]
    BuiltIn(String),
    #[error("invalid plugin: {0}")]
    Invalid(String),
    #[error("plugin execution failed: {0}")]
    Execution(String),
}

pub fn registry_snapshot(plugin_root: &Path, states: &[PluginStateRecord]) -> Result<PluginRegistrySnapshot, PluginRegistryError> {
    fs::create_dir_all(plugin_root)?;
    let state_map = states
        .iter()
        .map(|state| (state.plugin_id.clone(), state.clone()))
        .collect::<HashMap<_, _>>();
    let mut manifests = builtin_manifests();
    let mut warnings = Vec::new();

    for manifest_path in discover_manifest_paths(plugin_root)? {
        match load_user_manifest(plugin_root, &manifest_path, &state_map) {
            Ok((manifest, warning)) => {
                if let Some(warning) = warning {
                    warnings.push(format!("{}: {warning}", manifest.id));
                }
                manifests.push(manifest);
            }
            Err(error) => warnings.push(format!("Could not parse plugin manifest: {} ({error})", manifest_path.display())),
        }
    }

    for manifest in &mut manifests {
        if let Some(state) = state_map.get(&manifest.id) {
            manifest.enabled = state.enabled;
            manifest.config = state.config.clone();
        } else if matches!(manifest.origin, PluginOrigin::BuiltIn) {
            manifest.enabled = true;
        }
        hydrate_default_action(manifest);
    }

    manifests.sort_by(|left, right| {
        left.kind_string()
            .cmp(right.kind_string())
            .then_with(|| left.origin_string().cmp(right.origin_string()))
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(PluginRegistrySnapshot {
        plugin_root: plugin_root.display().to_string(),
        manifests,
        warnings,
    })
}

pub fn import_plugin(plugin_root: &Path, request: &PluginImportRequest, states: &[PluginStateRecord]) -> Result<String, PluginRegistryError> {
    fs::create_dir_all(plugin_root)?;
    let source = PathBuf::from(request.source_path.trim());
    if !source.exists() {
        return Err(PluginRegistryError::Invalid(format!("source path does not exist: {}", source.display())));
    }
    let manifest_path = if source.is_dir() {
        discover_manifest_paths(&source)?
            .into_iter()
            .next()
            .ok_or_else(|| PluginRegistryError::Invalid("directory does not contain a *.automd-plugin.json manifest".to_string()))?
    } else {
        source.clone()
    };
    let manifest = parse_manifest_file(&manifest_path)?;
    ensure_user_plugin_id(&manifest.id)?;
    ensure_not_builtin(&manifest.id)?;

    let destination = plugin_root.join(safe_plugin_id(&manifest.id));
    if destination.exists() {
        if !request.overwrite {
            return Err(PluginRegistryError::Conflict(manifest.id));
        }
        let existing_snapshot = registry_snapshot(plugin_root, states)?;
        let existing = existing_snapshot
            .manifests
            .iter()
            .find(|item| item.id == manifest.id)
            .ok_or_else(|| PluginRegistryError::Conflict(manifest.id.clone()))?;
        if matches!(existing.origin, PluginOrigin::BuiltIn) {
            return Err(PluginRegistryError::BuiltIn(manifest.id));
        }
        fs::remove_dir_all(&destination)?;
    }
    fs::create_dir_all(&destination)?;

    if source.is_dir() {
        copy_dir_all(&source, &destination)?;
    } else {
        let target_manifest = destination.join(
            manifest_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("plugin.automd-plugin.json"),
        );
        fs::copy(&manifest_path, target_manifest)?;
        copy_entrypoint_sibling(&manifest, &manifest_path, &destination)?;
    }
    Ok(manifest.id)
}

pub fn create_plugin_template(plugin_root: &Path, request: &PluginTemplateRequest) -> Result<String, PluginRegistryError> {
    fs::create_dir_all(plugin_root)?;
    let plugin_id = safe_plugin_id(if request.id.trim().is_empty() { &request.name } else { &request.id });
    ensure_user_plugin_id(&plugin_id)?;
    ensure_not_builtin(&plugin_id)?;
    let destination = plugin_root.join(&plugin_id);
    if destination.exists() {
        return Err(PluginRegistryError::Conflict(plugin_id));
    }
    fs::create_dir_all(&destination)?;

    let language = request.language.trim().to_ascii_lowercase();
    let (entrypoint, command, args, contents) = template_entrypoint(&language);
    fs::write(destination.join(&entrypoint), contents)?;
    fs::write(
        destination.join("README.md"),
        format!(
            "# {}\n\n{}\n\n这个插件由 AutoMD 快速创建。修改 `{}` 后回到软件刷新插件列表。\n",
            request.name,
            request.description.as_deref().unwrap_or("AutoMD user plugin."),
            entrypoint
        ),
    )?;
    let manifest = json!({
        "id": plugin_id,
        "name": request.name.trim(),
        "version": "0.1.0",
        "kind": request.kind,
        "entrypoint": entrypoint,
        "description": request.description,
        "engineId": request.target,
        "capabilities": ["user-created", "run"],
        "integrationTargets": integration_targets_for_kind(&request.kind),
        "supportedPlatforms": ["windows", "macos", "linux"],
        "licensePolicy": "userManaged",
        "warnings": ["用户创建插件；运行前请检查入口脚本和写入目录。"],
        "permissions": ["projectRead", "sandboxWrite"],
        "actions": [{
            "id": "default",
            "label": "运行默认动作",
            "description": "使用插件入口脚本处理当前 AutoMD 上下文。",
            "command": command,
            "args": args
        }],
        "defaultConfig": {}
    });
    fs::write(destination.join(format!("{plugin_id}.automd-plugin.json")), serde_json::to_string_pretty(&manifest)?)?;
    Ok(plugin_id)
}

pub fn delete_user_plugin(plugin_root: &Path, plugin_id: &str, states: &[PluginStateRecord]) -> Result<(), PluginRegistryError> {
    ensure_not_builtin(plugin_id)?;
    let snapshot = registry_snapshot(plugin_root, states)?;
    let manifest = snapshot
        .manifests
        .into_iter()
        .find(|manifest| manifest.id == plugin_id)
        .ok_or_else(|| PluginRegistryError::NotFound(plugin_id.to_string()))?;
    if matches!(manifest.origin, PluginOrigin::BuiltIn) {
        return Err(PluginRegistryError::BuiltIn(plugin_id.to_string()));
    }
    let install_path = manifest
        .install_path
        .or_else(|| manifest.source_path.as_ref().and_then(|path| Path::new(path).parent().map(|parent| parent.display().to_string())))
        .ok_or_else(|| PluginRegistryError::Invalid("plugin has no install path".to_string()))?;
    let install_path = PathBuf::from(install_path);
    if install_path.exists() {
        fs::remove_dir_all(install_path)?;
    }
    Ok(())
}

pub fn execute_plugin_action(
    plugin_root: &Path,
    manifest: &PluginManifest,
    request: &PluginRunRequest,
    run_id: &str,
) -> Result<(String, String, Option<serde_json::Value>, Vec<String>), PluginRegistryError> {
    if matches!(manifest.origin, PluginOrigin::BuiltIn) {
        return Err(PluginRegistryError::BuiltIn(manifest.id.clone()));
    }
    if !manifest.enabled {
        return Err(PluginRegistryError::Invalid("plugin is disabled".to_string()));
    }
    if matches!(request.mode, PluginRunMode::Direct) && !request.confirmed_direct {
        return Err(PluginRegistryError::Invalid("direct plugin execution requires second confirmation".to_string()));
    }
    let action = manifest
        .actions
        .iter()
        .find(|action| action.id == request.action_id)
        .or_else(|| manifest.actions.first())
        .ok_or_else(|| PluginRegistryError::Invalid("plugin action is missing".to_string()))?;
    let install_dir = manifest_install_dir(manifest)?;
    let sandbox_dir = plugin_root.join(".runs").join(safe_plugin_id(&manifest.id)).join(run_id);
    fs::create_dir_all(&sandbox_dir)?;

    let (command, args, cwd) = command_spec(manifest, action, &install_dir, &sandbox_dir, &request.mode)?;
    let mut child = Command::new(&command);
    child.args(&args).current_dir(&cwd).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    if matches!(request.mode, PluginRunMode::Sandbox) {
        child.env_clear();
        if let Ok(path) = std::env::var("PATH") {
            child.env("PATH", path);
        }
        child.env("AUTOMD_PLUGIN_SANDBOX", "1");
        child.env("AUTOMD_PLUGIN_ID", &manifest.id);
    }
    let mut process = child
        .spawn()
        .map_err(|error| PluginRegistryError::Execution(format!("{}: {error}", command.display())))?;
    if let Some(stdin) = process.stdin.as_mut() {
        stdin.write_all(serde_json::to_string_pretty(&request.context)?.as_bytes())?;
    }
    let output = process.wait_with_output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let parsed = stdout
        .trim()
        .is_empty()
        .then_some(None)
        .unwrap_or_else(|| serde_json::from_str::<serde_json::Value>(&stdout).ok());
    if matches!(request.mode, PluginRunMode::Sandbox) {
        if let Some(value) = &parsed {
            validate_plugin_output_paths(value, &sandbox_dir, &request.context)?;
        }
    }
    if !output.status.success() {
        return Err(PluginRegistryError::Execution(if stderr.trim().is_empty() {
            format!("plugin exited with status {}", output.status)
        } else {
            stderr.clone()
        }));
    }
    let mut warnings = Vec::new();
    if matches!(request.mode, PluginRunMode::Direct) {
        warnings.push("插件已在直接运行模式执行；请检查输出和写入目录。".to_string());
    }
    Ok((stdout, stderr, parsed, warnings))
}

fn load_user_manifest(
    plugin_root: &Path,
    manifest_path: &Path,
    state_map: &HashMap<String, PluginStateRecord>,
) -> Result<(PluginManifest, Option<String>), PluginRegistryError> {
    let mut manifest = parse_manifest_file(manifest_path)?;
    manifest.origin = PluginOrigin::User;
    manifest.source_path = Some(manifest_path.display().to_string());
    manifest.install_path = manifest_path.parent().map(|parent| parent.display().to_string());
    manifest.enabled = state_map.get(&manifest.id).map(|state| state.enabled).unwrap_or(true);
    manifest.config = state_map
        .get(&manifest.id)
        .map(|state| state.config.clone())
        .unwrap_or_else(|| manifest.default_config.clone());
    let warning = validate_manifest(plugin_root, manifest_path, &mut manifest);
    Ok((manifest, warning))
}

fn parse_manifest_file(path: &Path) -> Result<PluginManifest, PluginRegistryError> {
    let contents = fs::read_to_string(path)?;
    Ok(serde_json::from_str::<PluginManifest>(&contents)?)
}

fn validate_manifest(plugin_root: &Path, manifest_path: &Path, manifest: &mut PluginManifest) -> Option<String> {
    let warning = if manifest.id.trim().is_empty() {
        Some("id is required".to_string())
    } else if manifest.name.trim().is_empty() {
        Some("name is required".to_string())
    } else if manifest.version.trim().is_empty() {
        Some("version is required".to_string())
    } else if manifest.entrypoint.trim().is_empty() {
        Some("entrypoint is required".to_string())
    } else if matches!(manifest.kind, PluginKind::EngineAdapter) && manifest.engine_id.as_deref().unwrap_or("").is_empty() {
        Some("engineId is required for engineAdapter plugins".to_string())
    } else if manifest.entrypoint.contains("..") || Path::new(&manifest.entrypoint).is_absolute() {
        Some("entrypoint should be relative to the plugin directory; direct run still requires confirmation".to_string())
    } else {
        let install_dir = manifest_path.parent().unwrap_or(plugin_root);
        let entry = install_dir.join(&manifest.entrypoint);
        (!entry.exists()).then(|| format!("entrypoint not found: {}", entry.display()))
    };
    if let Some(warning) = &warning {
        manifest.validation_status = PluginValidationStatus::Warning;
        if !manifest.warnings.iter().any(|item| item == warning) {
            manifest.warnings.push(warning.clone());
        }
    } else {
        manifest.validation_status = PluginValidationStatus::Valid;
    }
    warning
}

fn discover_manifest_paths(plugin_root: &Path) -> Result<Vec<PathBuf>, PluginRegistryError> {
    let mut paths = Vec::new();
    if !plugin_root.exists() {
        return Ok(paths);
    }
    visit(plugin_root, &mut paths)?;
    paths.sort();
    Ok(paths)
}

fn visit(current: &Path, paths: &mut Vec<PathBuf>) -> Result<(), PluginRegistryError> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|name| name.to_str()) == Some(".runs") {
                continue;
            }
            visit(&path, paths)?;
        } else if path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|name| name.ends_with(".automd-plugin.json"))
        {
            paths.push(path);
        }
    }
    Ok(())
}

fn builtin_manifests() -> Vec<PluginManifest> {
    vec![
        builtin(
            "automd-core-engines",
            "AutoMD Core Engine Adapters",
            PluginKind::EngineAdapter,
            "builtin://engine_adapters",
            Some("gromacs/openmm/ambertools/namd"),
            vec!["prepare", "run", "parse_progress", "classify_failure", "resume"],
        ),
        builtin(
            "automd-core-analysis",
            "AutoMD Core Analysis Parsers",
            PluginKind::AnalysisModule,
            "builtin://analysis",
            None,
            vec!["xvg", "csv", "chart_series"],
        ),
        builtin(
            "automd-core-schedulers",
            "AutoMD Core Remote Schedulers",
            PluginKind::RemoteScheduler,
            "builtin://recipes/remote",
            None,
            vec!["ssh", "slurm", "pbs", "lsf", "rsync"],
        ),
        builtin(
            "automd-core-build-recipes",
            "AutoMD Core Build Recipes",
            PluginKind::BuildRecipe,
            "builtin://recipes/build",
            None,
            vec!["container", "source_build", "plumed", "mpi", "gpu"],
        ),
        builtin(
            "automd-core-report",
            "AutoMD Core Report Templates",
            PluginKind::ReportTemplate,
            "builtin://artifacts/report",
            None,
            vec!["markdown", "html", "pdf", "reproducibility_bundle"],
        ),
    ]
}

fn builtin(
    id: &str,
    name: &str,
    kind: PluginKind,
    entrypoint: &str,
    engine_id: Option<&str>,
    capabilities: Vec<&str>,
) -> PluginManifest {
    PluginManifest {
        id: id.to_string(),
        name: name.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        kind: kind.clone(),
        entrypoint: entrypoint.to_string(),
        description: Some("AutoMD 内置能力，随软件分发。".to_string()),
        author: Some("AutoMD".to_string()),
        homepage: None,
        engine_id: engine_id.map(str::to_string),
        capabilities: capabilities.into_iter().map(str::to_string).collect(),
        license_policy: None,
        warnings: Vec::new(),
        source_path: None,
        supported_platforms: vec!["windows".to_string(), "macos".to_string(), "linux".to_string()],
        integration_targets: integration_targets_for_kind(&kind),
        actions: Vec::new(),
        config_schema: serde_json::Value::Null,
        default_config: serde_json::Value::Null,
        permissions: Vec::new(),
        origin: PluginOrigin::BuiltIn,
        enabled: true,
        install_path: None,
        validation_status: PluginValidationStatus::Valid,
        config: serde_json::Value::Null,
    }
}

fn hydrate_default_action(manifest: &mut PluginManifest) {
    if matches!(manifest.origin, PluginOrigin::BuiltIn) || !manifest.actions.is_empty() {
        return;
    }
    manifest.actions.push(PluginAction {
        id: "default".to_string(),
        label: "运行默认动作".to_string(),
        description: Some("使用插件 entrypoint 处理当前 AutoMD 上下文。".to_string()),
        command: None,
        args: Vec::new(),
        timeout_seconds: None,
    });
}

fn command_spec(
    manifest: &PluginManifest,
    action: &PluginAction,
    install_dir: &Path,
    sandbox_dir: &Path,
    mode: &PluginRunMode,
) -> Result<(PathBuf, Vec<String>, PathBuf), PluginRegistryError> {
    let cwd = if matches!(mode, PluginRunMode::Sandbox) {
        sandbox_dir.to_path_buf()
    } else {
        install_dir.to_path_buf()
    };
    if let Some(command) = &action.command {
        let args = action
            .args
            .iter()
            .map(|arg| resolve_arg(arg, install_dir))
            .collect::<Vec<_>>();
        return Ok((PathBuf::from(command), args, cwd));
    }
    let entrypoint = resolve_entrypoint(manifest, install_dir, mode)?;
    let extension = entrypoint.extension().and_then(|value| value.to_str()).unwrap_or_default();
    let command = match extension {
        "py" => PathBuf::from("python3"),
        "js" | "mjs" | "cjs" => PathBuf::from("node"),
        "sh" => PathBuf::from("bash"),
        _ => entrypoint.clone(),
    };
    let args = if command == entrypoint { Vec::new() } else { vec![entrypoint.display().to_string()] };
    Ok((command, args, cwd))
}

fn resolve_arg(arg: &str, install_dir: &Path) -> String {
    if let Some(rest) = arg.strip_prefix("$PLUGIN_DIR/") {
        return install_dir.join(rest).display().to_string();
    }
    arg.to_string()
}

fn resolve_entrypoint(manifest: &PluginManifest, install_dir: &Path, mode: &PluginRunMode) -> Result<PathBuf, PluginRegistryError> {
    let entry = PathBuf::from(&manifest.entrypoint);
    if entry.is_absolute() {
        if matches!(mode, PluginRunMode::Direct) {
            return Ok(entry);
        }
        return Err(PluginRegistryError::Invalid("sandbox entrypoint cannot be absolute".to_string()));
    }
    if path_has_parent(&entry) {
        if matches!(mode, PluginRunMode::Direct) {
            return Ok(install_dir.join(entry));
        }
        return Err(PluginRegistryError::Invalid("sandbox entrypoint cannot escape plugin directory".to_string()));
    }
    let resolved = install_dir.join(entry);
    if !resolved.exists() {
        return Err(PluginRegistryError::Invalid(format!("entrypoint not found: {}", resolved.display())));
    }
    Ok(resolved)
}

fn validate_plugin_output_paths(value: &serde_json::Value, sandbox_dir: &Path, context: &serde_json::Value) -> Result<(), PluginRegistryError> {
    let project_path = context
        .get("projectPath")
        .and_then(|value| value.as_str())
        .map(PathBuf::from);
    let Some(artifacts) = value.get("artifacts").and_then(|value| value.as_array()) else {
        return Ok(());
    };
    for artifact in artifacts {
        let Some(path) = artifact.get("path").and_then(|value| value.as_str()) else {
            continue;
        };
        let path = PathBuf::from(path);
        if path.is_absolute() {
            let inside_sandbox = path.starts_with(sandbox_dir);
            let inside_project = project_path.as_ref().is_some_and(|project| path.starts_with(project));
            if !inside_sandbox && !inside_project {
                return Err(PluginRegistryError::Invalid(format!("sandbox output path is outside allowed directories: {}", path.display())));
            }
        } else if path_has_parent(&path) {
            return Err(PluginRegistryError::Invalid(format!("relative output path escapes allowed directories: {}", path.display())));
        }
    }
    Ok(())
}

fn manifest_install_dir(manifest: &PluginManifest) -> Result<PathBuf, PluginRegistryError> {
    manifest
        .install_path
        .as_deref()
        .map(PathBuf::from)
        .or_else(|| manifest.source_path.as_ref().and_then(|source| Path::new(source).parent().map(PathBuf::from)))
        .ok_or_else(|| PluginRegistryError::Invalid("plugin installPath is missing".to_string()))
}

fn copy_entrypoint_sibling(manifest: &PluginManifest, manifest_path: &Path, destination: &Path) -> Result<(), PluginRegistryError> {
    let entrypoint = PathBuf::from(&manifest.entrypoint);
    if entrypoint.is_absolute() || path_has_parent(&entrypoint) {
        return Ok(());
    }
    let Some(parent) = manifest_path.parent() else {
        return Ok(());
    };
    let source = parent.join(&entrypoint);
    if source.exists() && source.is_file() {
        if let Some(target_parent) = destination.join(&entrypoint).parent() {
            fs::create_dir_all(target_parent)?;
        }
        fs::copy(source, destination.join(entrypoint))?;
    }
    Ok(())
}

fn copy_dir_all(source: &Path, destination: &Path) -> Result<(), PluginRegistryError> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        let target = destination.join(entry.file_name());
        if path.is_dir() {
            copy_dir_all(&path, &target)?;
        } else {
            fs::copy(path, target)?;
        }
    }
    Ok(())
}

fn ensure_user_plugin_id(plugin_id: &str) -> Result<(), PluginRegistryError> {
    let safe = safe_plugin_id(plugin_id);
    if safe != plugin_id {
        return Err(PluginRegistryError::Invalid(format!("plugin id must be lowercase ascii, numbers, hyphen or underscore: {safe}")));
    }
    Ok(())
}

fn ensure_not_builtin(plugin_id: &str) -> Result<(), PluginRegistryError> {
    if builtin_manifests().iter().any(|manifest| manifest.id == plugin_id) {
        return Err(PluginRegistryError::BuiltIn(plugin_id.to_string()));
    }
    Ok(())
}

fn path_has_parent(path: &Path) -> bool {
    path.components().any(|component| matches!(component, Component::ParentDir))
}

fn safe_plugin_id(value: &str) -> String {
    let mut id = String::new();
    for ch in value.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            id.push(ch.to_ascii_lowercase());
        } else if ch == '-' || ch == '_' || ch.is_whitespace() {
            if !id.ends_with('-') {
                id.push('-');
            }
        }
    }
    let trimmed = id.trim_matches('-');
    if trimmed.is_empty() { "automd-plugin".to_string() } else { trimmed.to_string() }
}

fn template_entrypoint(language: &str) -> (&'static str, &'static str, Vec<&'static str>, &'static str) {
    match language {
        "javascript" | "js" | "node" => (
            "entrypoint.js",
            "node",
            vec!["$PLUGIN_DIR/entrypoint.js"],
            "let data = '';\nprocess.stdin.on('data', chunk => data += chunk);\nprocess.stdin.on('end', () => {\n  const context = data ? JSON.parse(data) : {};\n  console.log(JSON.stringify({ artifacts: [], warnings: ['示例插件已运行'], logs: [`project=${context.projectPath || 'none'}`] }));\n});\n",
        ),
        "bash" | "sh" => (
            "entrypoint.sh",
            "bash",
            vec!["$PLUGIN_DIR/entrypoint.sh"],
            "#!/usr/bin/env bash\nset -euo pipefail\ncat >/dev/null\nprintf '{\"artifacts\":[],\"warnings\":[\"示例 Bash 插件已运行\"],\"logs\":[]}'\n",
        ),
        _ => (
            "entrypoint.py",
            "python3",
            vec!["$PLUGIN_DIR/entrypoint.py"],
            "import json, sys\ncontext = json.load(sys.stdin)\nprint(json.dumps({\"artifacts\": [], \"warnings\": [\"示例 Python 插件已运行\"], \"logs\": [f\"project={context.get('projectPath')}\"]}))\n",
        ),
    }
}

fn integration_targets_for_kind(kind: &PluginKind) -> Vec<String> {
    match kind {
        PluginKind::EngineAdapter => vec!["engines".to_string(), "run".to_string()],
        PluginKind::AnalysisModule => vec!["workflow".to_string(), "run".to_string()],
        PluginKind::RemoteScheduler => vec!["remote".to_string()],
        PluginKind::BuildRecipe => vec!["build".to_string()],
        PluginKind::ReportTemplate => vec!["report".to_string()],
    }
}

trait PluginKindSort {
    fn kind_string(&self) -> &'static str;
    fn origin_string(&self) -> &'static str;
}

impl PluginKindSort for PluginManifest {
    fn kind_string(&self) -> &'static str {
        match self.kind {
            PluginKind::EngineAdapter => "engineAdapter",
            PluginKind::AnalysisModule => "analysisModule",
            PluginKind::RemoteScheduler => "remoteScheduler",
            PluginKind::BuildRecipe => "buildRecipe",
            PluginKind::ReportTemplate => "reportTemplate",
        }
    }

    fn origin_string(&self) -> &'static str {
        match self.origin {
            PluginOrigin::BuiltIn => "builtIn",
            PluginOrigin::User => "user",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn temp_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("automd-{label}-{}", Uuid::new_v4()))
    }

    #[test]
    fn plugin_registry_loads_external_manifest_with_origin_and_state() {
        let root = temp_root("plugins");
        fs::create_dir_all(&root).expect("plugin root");
        let plugin_dir = root.join("example-lammps-pack");
        fs::create_dir_all(&plugin_dir).expect("plugin dir");
        fs::write(plugin_dir.join("run.py"), "print('{}')").expect("entrypoint write");
        let manifest = r#"{
          "id": "example-lammps-pack",
          "name": "Example LAMMPS Pack",
          "version": "0.1.0",
          "kind": "engineAdapter",
          "entrypoint": "run.py",
          "engineId": "lammps",
          "capabilities": ["prepare", "run"],
          "licensePolicy": "openSource",
          "warnings": []
        }"#;
        fs::write(plugin_dir.join("example.automd-plugin.json"), manifest).expect("manifest write");
        let states = vec![PluginStateRecord {
            plugin_id: "example-lammps-pack".to_string(),
            enabled: false,
            config: json!({"threads": 2}),
            installed_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            last_run_at: None,
            last_error: None,
        }];

        let snapshot = registry_snapshot(&root, &states).expect("registry snapshot");

        let user = snapshot.manifests.iter().find(|manifest| manifest.id == "example-lammps-pack").expect("user plugin");
        assert_eq!(user.origin, PluginOrigin::User);
        assert!(!user.enabled);
        assert_eq!(user.config["threads"], 2);
        assert!(snapshot.manifests.iter().any(|manifest| manifest.id == "automd-core-engines" && manifest.origin == PluginOrigin::BuiltIn));
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn import_plugin_rejects_duplicate_without_overwrite_and_builtins() {
        let root = temp_root("import");
        let source = temp_root("source");
        fs::create_dir_all(&source).expect("source");
        fs::write(source.join("entrypoint.py"), "print('{}')").expect("entrypoint");
        fs::write(
            source.join("demo.automd-plugin.json"),
            r#"{"id":"demo-plugin","name":"Demo","version":"0.1.0","kind":"analysisModule","entrypoint":"entrypoint.py","capabilities":["run"],"warnings":[]}"#,
        )
        .expect("manifest");
        let request = PluginImportRequest { source_path: source.display().to_string(), overwrite: false };
        assert_eq!(import_plugin(&root, &request, &[]).expect("first import"), "demo-plugin");
        assert!(matches!(import_plugin(&root, &request, &[]), Err(PluginRegistryError::Conflict(_))));

        let builtin_source = temp_root("builtin-source");
        fs::create_dir_all(&builtin_source).expect("builtin source");
        fs::write(
            builtin_source.join("bad.automd-plugin.json"),
            r#"{"id":"automd-core-engines","name":"Bad","version":"0.1.0","kind":"analysisModule","entrypoint":"entrypoint.py","capabilities":[],"warnings":[]}"#,
        )
        .expect("builtin manifest");
        let request = PluginImportRequest { source_path: builtin_source.display().to_string(), overwrite: true };
        assert!(matches!(import_plugin(&root, &request, &[]), Err(PluginRegistryError::BuiltIn(_))));
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(source);
        let _ = fs::remove_dir_all(builtin_source);
    }

    #[test]
    fn sandbox_runner_rejects_escaping_entrypoint_and_outputs() {
        let root = temp_root("runner");
        let plugin_dir = root.join("escape-plugin");
        fs::create_dir_all(&plugin_dir).expect("plugin dir");
        let manifest = PluginManifest {
            id: "escape-plugin".to_string(),
            name: "Escape".to_string(),
            version: "0.1.0".to_string(),
            kind: PluginKind::AnalysisModule,
            entrypoint: "../escape.py".to_string(),
            description: None,
            author: None,
            homepage: None,
            engine_id: None,
            capabilities: vec!["run".to_string()],
            license_policy: None,
            warnings: Vec::new(),
            source_path: Some(plugin_dir.join("escape.automd-plugin.json").display().to_string()),
            supported_platforms: vec!["linux".to_string()],
            integration_targets: vec!["workflow".to_string()],
            actions: vec![PluginAction {
                id: "default".to_string(),
                label: "Run".to_string(),
                description: None,
                command: None,
                args: Vec::new(),
                timeout_seconds: None,
            }],
            config_schema: serde_json::Value::Null,
            default_config: serde_json::Value::Null,
            permissions: Vec::new(),
            origin: PluginOrigin::User,
            enabled: true,
            install_path: Some(plugin_dir.display().to_string()),
            validation_status: PluginValidationStatus::Warning,
            config: serde_json::Value::Null,
        };
        let request = PluginRunRequest {
            plugin_id: manifest.id.clone(),
            action_id: "default".to_string(),
            mode: PluginRunMode::Sandbox,
            confirmed_direct: false,
            context: json!({"projectPath": root.join("project").display().to_string()}),
        };
        assert!(matches!(execute_plugin_action(&root, &manifest, &request, "run1"), Err(PluginRegistryError::Invalid(_))));

        let bad_output = json!({"artifacts":[{"path":"../outside.txt"}]});
        assert!(matches!(
            validate_plugin_output_paths(&bad_output, &root.join(".runs/plugin/run1"), &json!({})),
            Err(PluginRegistryError::Invalid(_))
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn direct_runner_requires_confirmation() {
        let root = temp_root("direct");
        let plugin_dir = root.join("direct-plugin");
        fs::create_dir_all(&plugin_dir).expect("plugin dir");
        fs::write(plugin_dir.join("entrypoint.py"), "print('{}')").expect("entrypoint");
        let mut manifest = builtin("direct-plugin", "Direct", PluginKind::AnalysisModule, "entrypoint.py", None, vec!["run"]);
        manifest.origin = PluginOrigin::User;
        manifest.install_path = Some(plugin_dir.display().to_string());
        manifest.actions = vec![PluginAction {
            id: "default".to_string(),
            label: "Run".to_string(),
            description: None,
            command: None,
            args: Vec::new(),
            timeout_seconds: None,
        }];
        let request = PluginRunRequest {
            plugin_id: manifest.id.clone(),
            action_id: "default".to_string(),
            mode: PluginRunMode::Direct,
            confirmed_direct: false,
            context: json!({}),
        };
        assert!(matches!(execute_plugin_action(&root, &manifest, &request, "run1"), Err(PluginRegistryError::Invalid(_))));
        let _ = fs::remove_dir_all(root);
    }
}
