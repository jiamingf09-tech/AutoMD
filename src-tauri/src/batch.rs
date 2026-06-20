use crate::engine_adapters::{self, EngineAdapterError};
use crate::models::*;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::to_string_pretty;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

const MAX_REPLICATES: u32 = 64;

#[derive(Debug, Error)]
pub enum BatchExperimentError {
    #[error("replicate count must be between 1 and {MAX_REPLICATES}")]
    InvalidReplicateCount,
    #[error("seed range overflows u64")]
    SeedOverflow,
    #[error("project path is required when write_to_disk is true")]
    MissingProjectPath,
    #[error("engine adapter error: {0}")]
    EngineAdapter(#[from] EngineAdapterError),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchManifest {
    engine_id: String,
    source_plan_id: Uuid,
    generated_directory: String,
    replicate_count: usize,
    replicas: Vec<BatchReplicaPlan>,
    commands: Vec<EngineCommand>,
    warnings: Vec<String>,
    generated_at: DateTime<Utc>,
}

pub fn prepare_batch_experiment(
    request: BatchExperimentRequest,
) -> Result<BatchExperimentPackage, BatchExperimentError> {
    if request.replicate_count == 0 || request.replicate_count > MAX_REPLICATES {
        return Err(BatchExperimentError::InvalidReplicateCount);
    }
    if request.write_to_disk && request.project_path.is_none() {
        return Err(BatchExperimentError::MissingProjectPath);
    }

    let source_plan_id = request.plan.id;
    let engine_id = request.plan.engine_id.clone();
    let generated_directory = "generated/batch".to_string();
    let mut files = Vec::new();
    let mut commands = Vec::new();
    let mut warnings = Vec::new();
    let mut replicas = Vec::new();
    let mut replica_scripts = Vec::new();

    for offset in 0..request.replicate_count {
        let replica_index = offset + 1;
        let seed = request
            .seed_start
            .checked_add(u64::from(offset))
            .ok_or(BatchExperimentError::SeedOverflow)?;
        let mut replica_plan = request.plan.clone();
        replica_plan.id = Uuid::new_v4();
        replica_plan.name = format!("{} replica {:02}", request.plan.name, replica_index);
        replica_plan.created_at = Utc::now();
        inject_replica_seed(&mut replica_plan, seed);

        let package = engine_adapters::prepare_run_package(EngineRunRequest {
            plan: replica_plan.clone(),
            project_path: None,
            write_to_disk: false,
        })?;
        let package = namespace_replica_package(package, replica_index);
        let run_directory = package.run_directory.clone();
        let run_script = find_run_script(&package).unwrap_or_else(|| {
            package
                .commands
                .first()
                .map(|command| command.command.clone())
                .unwrap_or_else(|| "true".to_string())
        });
        let expected_outputs = unique_outputs(&package.commands);
        let replica_command = EngineCommand {
            stage_id: format!("batch-replica-{replica_index:02}"),
            label: format!("运行 replica {replica_index:02} (seed {seed})"),
            command: if run_script.ends_with(".sh") {
                format!("bash \"{run_script}\"")
            } else {
                run_script.clone()
            },
            working_directory: ".".to_string(),
            expected_outputs,
        };

        warnings.extend(
            package
                .warnings
                .iter()
                .map(|warning| format!("replica {replica_index:02}: {warning}")),
        );
        files.extend(package.files);
        commands.push(replica_command.clone());
        replica_scripts.push((
            replica_index,
            seed,
            run_directory.clone(),
            replica_command.command.clone(),
        ));
        replicas.push(BatchReplicaPlan {
            replica_index,
            seed,
            plan: replica_plan,
            run_directory,
        });
    }

    let batch_command = EngineCommand {
        stage_id: "batch-run".to_string(),
        label: format!("顺序运行 {} 个 replica", replicas.len()),
        command: "bash generated/batch/run-batch.sh".to_string(),
        working_directory: ".".to_string(),
        expected_outputs: replica_scripts
            .iter()
            .map(|(index, _, run_directory, _)| {
                format!("{run_directory}/batch-replica-{index:02}.log")
            })
            .collect(),
    };
    let mut all_commands = vec![batch_command.clone()];
    all_commands.extend(commands);
    commands = all_commands;

    for replica in &replicas {
        files.push(EngineRunFile {
            path: format!(
                "{generated_directory}/replica-{:02}/automd-plan.json",
                replica.replica_index
            ),
            language: "json".to_string(),
            contents: to_string_pretty(&replica.plan)?,
            written: false,
        });
    }

    let manifest = BatchManifest {
        engine_id: engine_id.clone(),
        source_plan_id,
        generated_directory: generated_directory.clone(),
        replicate_count: replicas.len(),
        replicas: replicas.clone(),
        commands: commands.clone(),
        warnings: warnings.clone(),
        generated_at: Utc::now(),
    };
    files.push(EngineRunFile {
        path: format!("{generated_directory}/automd-batch.json"),
        language: "json".to_string(),
        contents: to_string_pretty(&manifest)?,
        written: false,
    });
    files.push(EngineRunFile {
        path: format!("{generated_directory}/run-batch.sh"),
        language: "bash".to_string(),
        contents: batch_run_script(&request.plan, &replica_scripts),
        written: false,
    });

    if request.write_to_disk {
        let project_path = request
            .project_path
            .as_deref()
            .ok_or(BatchExperimentError::MissingProjectPath)?;
        write_files(project_path, &mut files)?;
    }

    Ok(BatchExperimentPackage {
        engine_id,
        plan_id: source_plan_id,
        generated_directory,
        replicas,
        files,
        commands,
        warnings,
        writable: request.project_path.is_some(),
    })
}

fn inject_replica_seed(plan: &mut SimulationPlan, seed: u64) {
    for stage in &mut plan.stages {
        match stage.kind {
            SimulationStageKind::NvtEquilibration => {
                stage
                    .parameters
                    .insert("velocitySeed".to_string(), seed.to_string());
            }
            SimulationStageKind::Production => {
                stage
                    .parameters
                    .insert("randomSeed".to_string(), seed.to_string());
            }
            _ => {}
        }
    }
}

fn namespace_replica_package(
    mut package: EngineRunPackage,
    replica_index: u32,
) -> EngineRunPackage {
    let generated_prefix = format!("generated/batch/replica-{replica_index:02}");
    let mut replacements = Vec::<(String, String)>::new();
    let mut directory_replacements = BTreeSet::<(String, String)>::new();

    for file in &mut package.files {
        if let Some(stripped) = file.path.strip_prefix("generated/") {
            let old_path = file.path.clone();
            let new_path = format!("{generated_prefix}/{stripped}");
            if let Some((old_dir, new_dir)) = generated_directory_pair(&old_path, &new_path) {
                directory_replacements.insert((old_dir, new_dir));
            }
            replacements.push((old_path, new_path.clone()));
            file.path = new_path;
        }
    }
    replacements.extend(directory_replacements);
    replacements.sort_by(|left, right| right.0.len().cmp(&left.0.len()));

    for file in &mut package.files {
        file.contents = apply_replacements(&file.contents, &replacements);
    }
    for command in &mut package.commands {
        command.command = apply_replacements(&command.command, &replacements);
        command.expected_outputs = command
            .expected_outputs
            .iter()
            .map(|output| apply_replacements(output, &replacements))
            .collect();
    }

    package
}

fn generated_directory_pair(old_path: &str, new_path: &str) -> Option<(String, String)> {
    let old_parts = old_path.split('/').collect::<Vec<_>>();
    let new_parts = new_path.split('/').collect::<Vec<_>>();
    if old_parts.len() < 2 || new_parts.len() < 5 {
        return None;
    }
    Some((
        format!("{}/{}", old_parts[0], old_parts[1]),
        format!(
            "{}/{}/{}/{}",
            new_parts[0], new_parts[1], new_parts[2], new_parts[3]
        ),
    ))
}

fn apply_replacements(value: &str, replacements: &[(String, String)]) -> String {
    replacements
        .iter()
        .fold(value.to_string(), |current, (old, new)| {
            current.replace(old, new)
        })
}

fn find_run_script(package: &EngineRunPackage) -> Option<String> {
    package
        .files
        .iter()
        .find(|file| file.path.starts_with(&package.run_directory) && file.path.ends_with(".sh"))
        .map(|file| file.path.clone())
}

fn unique_outputs(commands: &[EngineCommand]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    for command in commands {
        for output in &command.expected_outputs {
            seen.insert(output.clone());
        }
    }
    seen.into_iter().collect()
}

fn batch_run_script(plan: &SimulationPlan, replicas: &[(u32, u64, String, String)]) -> String {
    let body = replicas
        .iter()
        .map(|(index, seed, run_directory, command)| {
            let log_path = format!("{run_directory}/batch-replica-{index:02}.log");
            format!(
                r#"echo "[AutoMD] replica {index:02} seed {seed}"
mkdir -p "{run_directory}"
({command}) 2>&1 | tee "{log_path}"
"#
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"#!/usr/bin/env bash
set -euo pipefail

echo "AutoMD batch experiment: {name}"
echo "Source plan id: {plan_id}"
echo "Replicas: {replica_count}"

{body}

echo "[AutoMD] batch experiment completed"
"#,
        name = plan.name,
        plan_id = plan.id,
        replica_count = replicas.len()
    )
}

fn write_files(
    project_path: &str,
    files: &mut [EngineRunFile],
) -> Result<(), BatchExperimentError> {
    let root = PathBuf::from(project_path);
    for file in files {
        let destination = safe_join(&root, &file.path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(destination, &file.contents)?;
        file.written = true;
    }
    Ok(())
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
    use crate::planner;

    fn test_plan() -> SimulationPlan {
        planner::default_simulation_plan(PlanRequest {
            project_id: None,
            name: "batch-test".to_string(),
            engine_id: "gromacs".to_string(),
            domain: ProjectDomain::Biomolecular,
        })
    }

    #[test]
    fn batch_package_generates_unique_replica_plans_and_seeds() {
        let source = test_plan();
        let package = prepare_batch_experiment(BatchExperimentRequest {
            plan: source.clone(),
            project_path: None,
            replicate_count: 3,
            seed_start: 9000,
            write_to_disk: false,
        })
        .expect("batch package");

        assert_eq!(package.engine_id, "gromacs");
        assert_eq!(package.plan_id, source.id);
        assert_eq!(package.replicas.len(), 3);
        assert_eq!(package.replicas[0].seed, 9000);
        assert_eq!(package.replicas[2].seed, 9002);
        assert!(package
            .replicas
            .iter()
            .all(|replica| replica.plan.id != source.id
                && replica
                    .run_directory
                    .contains(&replica.plan.id.simple().to_string())));
        assert!(package
            .files
            .iter()
            .any(|file| file.path == "generated/batch/run-batch.sh"));
        assert!(package.files.iter().any(|file| {
            file.path == "generated/batch/replica-01/gromacs/nvt.mdp"
                && file.contents.contains("gen_seed")
        }));
        assert!(package
            .commands
            .iter()
            .any(|command| command.stage_id == "batch-run"));
    }

    #[test]
    fn batch_package_writes_namespaced_files() {
        let root = std::env::temp_dir().join(format!("automd-batch-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("temp root");

        let package = prepare_batch_experiment(BatchExperimentRequest {
            plan: test_plan(),
            project_path: Some(root.display().to_string()),
            replicate_count: 2,
            seed_start: 42,
            write_to_disk: true,
        })
        .expect("written batch");

        assert!(package.files.iter().all(|file| file.written));
        assert!(root.join("generated/batch/automd-batch.json").exists());
        assert!(root.join("generated/batch/run-batch.sh").exists());
        assert!(root
            .join("generated/batch/replica-01/gromacs/md.mdp")
            .exists());
        assert!(root
            .join("generated/batch/replica-02/gromacs/md.mdp")
            .exists());

        fs::remove_dir_all(root).expect("cleanup temp root");
    }

    #[test]
    fn batch_package_rejects_zero_replicas() {
        let error = prepare_batch_experiment(BatchExperimentRequest {
            plan: test_plan(),
            project_path: None,
            replicate_count: 0,
            seed_start: 1,
            write_to_disk: false,
        })
        .expect_err("invalid count");

        assert!(matches!(error, BatchExperimentError::InvalidReplicateCount));
    }
}
