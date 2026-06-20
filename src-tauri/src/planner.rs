use crate::engine_registry;
use crate::models::*;
use chrono::Utc;
use std::collections::BTreeMap;
use uuid::Uuid;

pub fn default_simulation_plan(request: PlanRequest) -> SimulationPlan {
    let biomolecular = matches!(
        request.domain,
        ProjectDomain::Biomolecular | ProjectDomain::Qmmm
    );

    SimulationPlan {
        id: Uuid::new_v4(),
        project_id: request.project_id,
        name: request.name,
        engine_id: request.engine_id,
        system: SystemSpec {
            source_kind: StructureSourceKind::Pdb,
            source_path: None,
            name: if biomolecular {
                "protein-ligand-system".to_string()
            } else {
                "materials-system".to_string()
            },
            molecule_count: None,
            has_ligand: biomolecular,
            has_membrane: false,
            notes: vec!["导入结构后将自动更新体系摘要。".to_string()],
        },
        force_field: ForceFieldSpec {
            protein: if biomolecular {
                "CHARMM36m".to_string()
            } else {
                "UFF / user-defined".to_string()
            },
            water_model: "TIP3P".to_string(),
            ligand: biomolecular.then(|| "GAFF2 or CGenFF".to_string()),
            ions: "Joung-Cheatham".to_string(),
        },
        solvent: SolventSpec {
            model: "explicit".to_string(),
            box_shape: "dodecahedron".to_string(),
            padding_nm: 1.0,
            ionic_strength_molar: 0.15,
            neutralize: true,
        },
        resources: ResourceSpec {
            execution_mode: ExecutionMode::LocalProcess,
            cpu_threads: 8,
            gpu_count: 1,
            mpi_ranks: 1,
            walltime_hours: 24.0,
            remote_profile_id: None,
            queue: None,
        },
        stages: default_stages(biomolecular),
        outputs: default_outputs(),
        analysis: default_analysis_modules(),
        created_at: Utc::now(),
    }
}

pub fn validate_plan(plan: &SimulationPlan) -> ValidationReport {
    let mut items = Vec::new();

    if engine_registry::detect_engine_by_id(&plan.engine_id).is_none() {
        items.push(ValidationItem {
            severity: ValidationSeverity::Error,
            field: "engineId".to_string(),
            message: format!("未知引擎：{}", plan.engine_id),
        });
    }

    if plan.stages.iter().filter(|stage| stage.enabled).count() == 0 {
        items.push(ValidationItem {
            severity: ValidationSeverity::Error,
            field: "stages".to_string(),
            message: "至少需要启用一个模拟阶段。".to_string(),
        });
    }

    if plan.solvent.padding_nm < 0.5 {
        items.push(ValidationItem {
            severity: ValidationSeverity::Warning,
            field: "solvent.paddingNm".to_string(),
            message: "水盒 padding 低于 0.5 nm，可能导致周期性边界伪影。".to_string(),
        });
    }

    if plan.resources.walltime_hours <= 0.0 {
        items.push(ValidationItem {
            severity: ValidationSeverity::Error,
            field: "resources.walltimeHours".to_string(),
            message: "运行时长必须大于 0。".to_string(),
        });
    }

    let status = if items
        .iter()
        .any(|item| item.severity == ValidationSeverity::Error)
    {
        ValidationStatus::Invalid
    } else if items
        .iter()
        .any(|item| item.severity == ValidationSeverity::Warning)
    {
        ValidationStatus::ValidWithWarnings
    } else {
        ValidationStatus::Valid
    };

    ValidationReport { status, items }
}

pub fn mock_task(plan: SimulationPlan) -> SimulationTask {
    SimulationTask {
        id: Uuid::new_v4(),
        plan_id: plan.id,
        engine_id: plan.engine_id,
        status: TaskStatus::Queued,
        current_stage: Some(SimulationStageKind::StructurePreparation),
        progress_percent: 0.0,
        ns_per_day: parse_gromacs_ns_per_day("Performance: 0.000 ns/day"),
        log_tail: vec![
            "AutoMD task queued.".to_string(),
            "Engine adapter selected; waiting for launch confirmation.".to_string(),
        ],
        created_at: Utc::now(),
    }
}

pub fn parse_gromacs_ns_per_day(line: &str) -> Option<f32> {
    let marker = "Performance:";
    let start = line.find(marker)? + marker.len();
    let rest = line[start..].trim();
    let value = rest.split_whitespace().next()?;
    value.parse::<f32>().ok()
}

fn stage(
    id: &str,
    kind: SimulationStageKind,
    label: &str,
    parameters: &[(&str, &str)],
    outputs: &[&str],
) -> SimulationStage {
    SimulationStage {
        id: id.to_string(),
        kind,
        label: label.to_string(),
        enabled: true,
        parameters: parameters
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect::<BTreeMap<_, _>>(),
        expected_outputs: outputs.iter().map(|value| value.to_string()).collect(),
    }
}

fn default_stages(biomolecular: bool) -> Vec<SimulationStage> {
    let production_ns = if biomolecular { "100" } else { "10" };
    vec![
        stage(
            "prepare",
            SimulationStageKind::StructurePreparation,
            "结构准备",
            &[
                ("repairMissingAtoms", "true"),
                ("addHydrogens", "true"),
                ("parameterizeLigands", "true"),
            ],
            &["prepared_structure", "topology"],
        ),
        stage(
            "em",
            SimulationStageKind::EnergyMinimization,
            "能量最小化",
            &[
                ("integrator", "steepest-descent"),
                ("maxSteps", "50000"),
                ("emtol", "1000"),
            ],
            &["minimized_structure", "energy_log"],
        ),
        stage(
            "nvt",
            SimulationStageKind::NvtEquilibration,
            "NVT 平衡",
            &[
                ("durationPs", "100"),
                ("temperatureK", "300"),
                ("restraints", "heavy-atoms"),
            ],
            &["nvt_checkpoint", "temperature_trace"],
        ),
        stage(
            "npt",
            SimulationStageKind::NptEquilibration,
            "NPT 平衡",
            &[
                ("durationPs", "1000"),
                ("pressureBar", "1.0"),
                ("temperatureK", "300"),
            ],
            &["npt_checkpoint", "pressure_trace", "density_trace"],
        ),
        stage(
            "production",
            SimulationStageKind::Production,
            "生产模拟",
            &[
                ("durationNs", production_ns),
                ("timestepFs", "2"),
                ("checkpointEveryPs", "100"),
            ],
            &["trajectory", "checkpoint", "energy"],
        ),
        stage(
            "analysis",
            SimulationStageKind::Analysis,
            "自动分析",
            &[("stride", "10"), ("generateReport", "true")],
            &["analysis_tables", "figures", "report"],
        ),
    ]
}

fn default_analysis_modules() -> Vec<AnalysisModule> {
    [
        AnalysisKind::Rmsd,
        AnalysisKind::Rmsf,
        AnalysisKind::RadiusOfGyration,
        AnalysisKind::HydrogenBonds,
        AnalysisKind::Distances,
        AnalysisKind::Angles,
        AnalysisKind::Dihedrals,
        AnalysisKind::EnergyTerms,
        AnalysisKind::Contacts,
    ]
    .into_iter()
    .map(|kind| AnalysisModule {
        kind,
        enabled: true,
        parameters: BTreeMap::new(),
    })
    .collect()
}

fn default_outputs() -> OutputSpec {
    OutputSpec {
        generated_inputs: vec![
            "generated/<engine>/automd-plan.json".to_string(),
            "generated/<engine>/*".to_string(),
        ],
        run_logs: vec!["runs/<engine-plan>/*.log".to_string()],
        checkpoints: vec![
            "runs/<engine-plan>/*.{cpt,chk,rst7,restart.*}".to_string(),
            "checkpoints/*".to_string(),
        ],
        trajectories: vec!["trajectories/*.{xtc,trr,dcd,nc,pdb,xyz,lammpstrj,dump,gsd}".to_string()],
        energy: vec![
            "runs/<engine-plan>/*.{edr,out,log}".to_string(),
            "analysis/openmm_state.csv".to_string(),
        ],
        analysis_tables: vec![
            "analysis/*.xvg".to_string(),
            "analysis/*.csv".to_string(),
            "analysis/*.json".to_string(),
        ],
        reports: vec![
            "reports/automd-report.md".to_string(),
            "reports/automd-report.html".to_string(),
            "reports/automd-report.pdf".to_string(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_plan_has_required_biomolecular_stages() {
        let plan = default_simulation_plan(PlanRequest {
            project_id: None,
            name: "test".to_string(),
            engine_id: "gromacs".to_string(),
            domain: ProjectDomain::Biomolecular,
        });
        assert!(plan
            .stages
            .iter()
            .any(|stage| stage.kind == SimulationStageKind::EnergyMinimization));
        assert!(plan
            .stages
            .iter()
            .any(|stage| stage.kind == SimulationStageKind::Production));
        assert_eq!(plan.force_field.water_model, "TIP3P");
        assert!(plan
            .outputs
            .trajectories
            .iter()
            .any(|path| path.contains("trajectories")));
    }

    #[test]
    fn parses_gromacs_performance_line() {
        let parsed = parse_gromacs_ns_per_day("Performance:        102.731        ns/day");
        assert_eq!(parsed, Some(102.731));
    }
}
