use crate::models::*;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PluginRegistryError {
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
}

pub fn registry_snapshot(plugin_root: &Path) -> Result<PluginRegistrySnapshot, PluginRegistryError> {
    fs::create_dir_all(plugin_root)?;
    let mut manifests = builtin_manifests();
    let mut warnings = Vec::new();

    for manifest_path in discover_manifest_paths(plugin_root)? {
        match fs::read_to_string(&manifest_path)
            .ok()
            .and_then(|contents| serde_json::from_str::<PluginManifest>(&contents).ok())
        {
            Some(mut manifest) => {
                manifest.source_path = Some(manifest_path.display().to_string());
                if let Some(warning) = validate_manifest(&manifest) {
                    warnings.push(format!("{}: {warning}", manifest.id));
                }
                manifests.push(manifest);
            }
            None => warnings.push(format!(
                "Could not parse plugin manifest: {}",
                manifest_path.display()
            )),
        }
    }

    manifests.sort_by(|left, right| left.kind_string().cmp(right.kind_string()).then_with(|| left.id.cmp(&right.id)));
    Ok(PluginRegistrySnapshot {
        plugin_root: plugin_root.display().to_string(),
        manifests,
        warnings,
    })
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

fn validate_manifest(manifest: &PluginManifest) -> Option<String> {
    if manifest.id.trim().is_empty() {
        return Some("id is required".to_string());
    }
    if manifest.name.trim().is_empty() {
        return Some("name is required".to_string());
    }
    if manifest.version.trim().is_empty() {
        return Some("version is required".to_string());
    }
    if manifest.entrypoint.trim().is_empty() {
        return Some("entrypoint is required".to_string());
    }
    if matches!(manifest.kind, PluginKind::EngineAdapter) && manifest.engine_id.as_deref().unwrap_or("").is_empty() {
        return Some("engineId is required for engineAdapter plugins".to_string());
    }
    None
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
        kind,
        entrypoint: entrypoint.to_string(),
        engine_id: engine_id.map(str::to_string),
        capabilities: capabilities.into_iter().map(str::to_string).collect(),
        license_policy: None,
        warnings: Vec::new(),
        source_path: None,
    }
}

trait PluginKindSort {
    fn kind_string(&self) -> &'static str;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn plugin_registry_loads_external_manifest() {
        let root = std::env::temp_dir().join(format!("automd-plugins-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("plugin root");
        let manifest = r#"{
          "id": "example-lammps-pack",
          "name": "Example LAMMPS Pack",
          "version": "0.1.0",
          "kind": "engineAdapter",
          "entrypoint": "plugins/example-lammps/run.js",
          "engineId": "lammps",
          "capabilities": ["prepare", "run"],
          "licensePolicy": "openSource",
          "warnings": []
        }"#;
        fs::write(root.join("example.automd-plugin.json"), manifest).expect("manifest write");

        let snapshot = registry_snapshot(&root).expect("registry snapshot");

        assert!(snapshot
            .manifests
            .iter()
            .any(|manifest| manifest.id == "example-lammps-pack" && manifest.source_path.is_some()));
        assert!(snapshot
            .manifests
            .iter()
            .any(|manifest| manifest.id == "automd-core-engines"));

        fs::remove_dir_all(root).expect("cleanup");
    }
}
