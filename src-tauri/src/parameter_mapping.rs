use crate::models::*;
use chrono::Utc;

pub fn map_parameters(request: ParameterMappingRequest) -> ParameterMappingReport {
    let engine_id = request
        .engine_id
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| request.plan.engine_id.clone());
    let plan = request.plan;
    let mut mapper = ParameterMapper::new(engine_id.clone(), plan.id);

    for stage in plan.stages.iter().filter(|stage| stage.enabled) {
        match engine_id.as_str() {
            "gromacs" => map_gromacs_stage(&mut mapper, &plan, stage),
            "openmm" => map_openmm_stage(&mut mapper, &plan, stage),
            "ambertools" => map_amber_stage(&mut mapper, &plan, stage, "ambertools", "generated/ambertools"),
            "amber_pmemd" => map_amber_stage(&mut mapper, &plan, stage, "amber_pmemd", "generated/amber_pmemd"),
            "namd" => map_namd_stage(&mut mapper, &plan, stage),
            "lammps" | "cp2k" | "genesis" | "hoomd" | "dl_poly" | "tinker" | "charmm" | "desmond"
            | "acemd" => map_preview_stage(&mut mapper, &plan, stage, &engine_id),
            other => mapper.warn(format!(
                "Unknown engine '{other}'; parameter mappings are marked for manual review."
            )),
        }
    }

    if plan.stages.iter().any(|stage| !stage.enabled) {
        mapper.warn("Disabled stages are omitted from this mapping report.".to_string());
    }

    mapper.finish()
}

struct ParameterMapper {
    engine_id: String,
    plan_id: uuid::Uuid,
    items: Vec<ParameterMappingItem>,
    warnings: Vec<String>,
}

struct MappingSpec<'a> {
    normalized_key: &'a str,
    normalized_value: String,
    engine_key: &'a str,
    engine_value: String,
    target_file: &'a str,
    status: ParameterMappingStatus,
    notes: Vec<String>,
}

impl ParameterMapper {
    fn new(engine_id: String, plan_id: uuid::Uuid) -> Self {
        Self {
            engine_id,
            plan_id,
            items: Vec::new(),
            warnings: Vec::new(),
        }
    }

    fn add(&mut self, stage: &SimulationStage, spec: MappingSpec<'_>) {
        self.items.push(ParameterMappingItem {
            stage_id: stage.id.clone(),
            stage_label: stage.label.clone(),
            normalized_key: spec.normalized_key.to_string(),
            normalized_value: spec.normalized_value,
            engine_key: spec.engine_key.to_string(),
            engine_value: spec.engine_value,
            target_file: spec.target_file.to_string(),
            status: spec.status,
            notes: spec.notes,
        });
    }

    fn warn(&mut self, warning: String) {
        if !self.warnings.contains(&warning) {
            self.warnings.push(warning);
        }
    }

    fn finish(self) -> ParameterMappingReport {
        ParameterMappingReport {
            engine_id: self.engine_id,
            plan_id: self.plan_id,
            items: self.items,
            warnings: self.warnings,
            generated_at: Utc::now(),
        }
    }
}

fn map_gromacs_stage(mapper: &mut ParameterMapper, plan: &SimulationPlan, stage: &SimulationStage) {
    match stage.id.as_str() {
        "em" => {
            if let Some(value) = parameter(stage, "maxSteps") {
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: "maxSteps",
                        normalized_value: value.to_string(),
                        engine_key: "nsteps",
                        engine_value: value.to_string(),
                        target_file: "generated/gromacs/em.mdp",
                        status: ParameterMappingStatus::Mapped,
                        notes: vec!["Energy minimization step limit.".to_string()],
                    },
                );
            }
            if let Some(value) = parameter(stage, "emtol") {
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: "emtol",
                        normalized_value: value.to_string(),
                        engine_key: "emtol",
                        engine_value: value.to_string(),
                        target_file: "generated/gromacs/em.mdp",
                        status: ParameterMappingStatus::Mapped,
                        notes: vec!["Maximum force convergence threshold.".to_string()],
                    },
                );
            }
            if let Some(value) = parameter(stage, "integrator") {
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: "integrator",
                        normalized_value: value.to_string(),
                        engine_key: "integrator",
                        engine_value: if value.contains("steep") {
                            "steep".to_string()
                        } else {
                            value.to_string()
                        },
                        target_file: "generated/gromacs/em.mdp",
                        status: if value.contains("steep") {
                            ParameterMappingStatus::Mapped
                        } else {
                            ParameterMappingStatus::ManualReview
                        },
                        notes: vec!["GROMACS uses short integrator identifiers in MDP files.".to_string()],
                    },
                );
            }
        }
        "nvt" | "npt" => {
            let target_file = if stage.id == "nvt" {
                "generated/gromacs/nvt.mdp"
            } else {
                "generated/gromacs/npt.mdp"
            };
            let timestep_fs = production_timestep_fs(plan, mapper, 2.0);
            if let Some(value) = parameter(stage, "durationPs") {
                map_steps_from_duration_ps(
                    mapper,
                    stage,
                    "durationPs",
                    value,
                    timestep_fs,
                    "nsteps",
                    target_file,
                    vec!["Uses production timestepFs for equilibration MDP generation.".to_string()],
                );
            }
            if let Some(value) = parameter(stage, "temperatureK") {
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: "temperatureK",
                        normalized_value: format!("{value} K"),
                        engine_key: if stage.id == "nvt" {
                            "ref_t / gen_temp"
                        } else {
                            "ref_t"
                        },
                        engine_value: if stage.id == "nvt" {
                            format!("{value} {value}; gen_temp={value}")
                        } else {
                            format!("{value} {value}")
                        },
                        target_file,
                        status: ParameterMappingStatus::Mapped,
                        notes: vec!["Applied to both default temperature-coupling groups.".to_string()],
                    },
                );
            }
            if stage.id == "npt" {
                if let Some(value) = parameter(stage, "pressureBar") {
                    mapper.add(
                        stage,
                        MappingSpec {
                            normalized_key: "pressureBar",
                            normalized_value: format!("{value} bar"),
                            engine_key: "ref_p",
                            engine_value: value.to_string(),
                            target_file,
                            status: ParameterMappingStatus::Mapped,
                            notes: vec!["C-rescale isotropic pressure target.".to_string()],
                        },
                    );
                    if value.trim() != "1.0" && value.trim() != "1" {
                        mapper.warn(
                            "GROMACS production template currently keeps ref_p=1.0; edit generated/gromacs/md.mdp for non-1 bar production runs."
                                .to_string(),
                        );
                    }
                }
            }
            if let Some(value) = parameter(stage, "velocitySeed") {
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: "velocitySeed",
                        normalized_value: value.to_string(),
                        engine_key: "gen_seed",
                        engine_value: value.to_string(),
                        target_file,
                        status: ParameterMappingStatus::Mapped,
                        notes: vec!["Used when velocities are generated for equilibration.".to_string()],
                    },
                );
            }
            if let Some(value) = parameter(stage, "restraints") {
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: "restraints",
                        normalized_value: value.to_string(),
                        engine_key: "define",
                        engine_value: "-DPOSRES".to_string(),
                        target_file,
                        status: ParameterMappingStatus::Approximated,
                        notes: vec!["Current template maps restrained equilibration to the standard GROMACS POSRES define.".to_string()],
                    },
                );
            }
        }
        "production" => {
            let target_file = "generated/gromacs/md.mdp";
            let timestep_fs = production_timestep_fs(plan, mapper, 2.0);
            if let Some(value) = parameter(stage, "durationNs") {
                map_steps_from_duration_ns(mapper, stage, value, timestep_fs, "nsteps", target_file, Vec::new());
            }
            if let Some(value) = parameter(stage, "timestepFs") {
                map_timestep_ps(mapper, stage, value, "dt", target_file);
            }
            if let Some(value) = parameter(stage, "checkpointEveryPs") {
                map_steps_from_duration_ps(
                    mapper,
                    stage,
                    "checkpointEveryPs",
                    value,
                    timestep_fs,
                    "nstcheckpoint",
                    target_file,
                    vec!["Checkpoint interval is converted from ps to MD steps.".to_string()],
                );
            }
            if let Some(value) = parameter(stage, "randomSeed") {
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: "randomSeed",
                        normalized_value: value.to_string(),
                        engine_key: "gen_seed",
                        engine_value: value.to_string(),
                        target_file: "generated/gromacs/nvt.mdp",
                        status: ParameterMappingStatus::Approximated,
                        notes: vec![
                            "GROMACS production MDP does not generate velocities; AutoMD uses production.randomSeed as an equilibration seed fallback."
                                .to_string(),
                        ],
                    },
                );
            }
        }
        "prepare" => map_prepare_manual(mapper, stage, "generated/gromacs/README.md"),
        "analysis" => map_analysis_manual(mapper, stage, "generated/gromacs/automd-plan.json"),
        _ => map_stage_manual(mapper, stage, "generated/gromacs/automd-plan.json"),
    }
}

fn map_openmm_stage(mapper: &mut ParameterMapper, plan: &SimulationPlan, stage: &SimulationStage) {
    let target_file = "generated/openmm/run_openmm.py";
    match stage.id.as_str() {
        "em" => {
            for (key, value) in &stage.parameters {
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: key,
                        normalized_value: value.clone(),
                        engine_key: "simulation.minimizeEnergy",
                        engine_value: "default OpenMM minimizer settings".to_string(),
                        target_file,
                        status: ParameterMappingStatus::ManualReview,
                        notes: vec![
                            "Current OpenMM runner calls minimizeEnergy() without exposing maxIterations or tolerance."
                                .to_string(),
                        ],
                    },
                );
            }
        }
        "nvt" => {
            if let Some(value) = parameter(stage, "temperatureK") {
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: "temperatureK",
                        normalized_value: format!("{value} K"),
                        engine_key: "temperature / LangevinMiddleIntegrator / setVelocitiesToTemperature",
                        engine_value: value.to_string(),
                        target_file,
                        status: ParameterMappingStatus::Mapped,
                        notes: vec!["OpenMM receives temperature in kelvin units.".to_string()],
                    },
                );
            }
            if let Some(value) = parameter(stage, "velocitySeed") {
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: "velocitySeed",
                        normalized_value: value.to_string(),
                        engine_key: "random_seed fallback",
                        engine_value: value.to_string(),
                        target_file,
                        status: ParameterMappingStatus::Mapped,
                        notes: vec![
                            "Used when production.randomSeed is absent; applies to integrator seed and initial velocities."
                                .to_string(),
                        ],
                    },
                );
            }
            if let Some(value) = parameter(stage, "durationPs") {
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: "durationPs",
                        normalized_value: format!("{value} ps"),
                        engine_key: "equilibration loop",
                        engine_value: "not emitted".to_string(),
                        target_file,
                        status: ParameterMappingStatus::Unsupported,
                        notes: vec!["Current OpenMM runner performs minimization then production only.".to_string()],
                    },
                );
            }
            if let Some(value) = parameter(stage, "restraints") {
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: "restraints",
                        normalized_value: value.to_string(),
                        engine_key: "restraint force",
                        engine_value: "not emitted".to_string(),
                        target_file,
                        status: ParameterMappingStatus::ManualReview,
                        notes: vec!["Add a custom force or use the native editor for restrained equilibration.".to_string()],
                    },
                );
            }
        }
        "npt" => {
            if let Some(value) = parameter(stage, "temperatureK") {
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: "temperatureK",
                        normalized_value: format!("{value} K"),
                        engine_key: "temperature fallback",
                        engine_value: value.to_string(),
                        target_file,
                        status: ParameterMappingStatus::Approximated,
                        notes: vec![
                            "Used only when NVT temperature is absent; current OpenMM runner has no explicit NPT phase."
                                .to_string(),
                        ],
                    },
                );
            }
            if let Some(value) = parameter(stage, "pressureBar") {
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: "pressureBar",
                        normalized_value: format!("{value} bar"),
                        engine_key: "MonteCarloBarostat",
                        engine_value: "not emitted".to_string(),
                        target_file,
                        status: ParameterMappingStatus::Unsupported,
                        notes: vec!["Current OpenMM runner does not add a barostat; edit the Python runner for NPT.".to_string()],
                    },
                );
            }
            if let Some(value) = parameter(stage, "durationPs") {
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: "durationPs",
                        normalized_value: format!("{value} ps"),
                        engine_key: "equilibration loop",
                        engine_value: "not emitted".to_string(),
                        target_file,
                        status: ParameterMappingStatus::Unsupported,
                        notes: vec!["Current OpenMM runner performs minimization then production only.".to_string()],
                    },
                );
            }
        }
        "production" => {
            let timestep_fs = production_timestep_fs(plan, mapper, 2.0);
            if let Some(value) = parameter(stage, "durationNs") {
                map_steps_from_duration_ns(
                    mapper,
                    stage,
                    value,
                    timestep_fs,
                    "total_steps",
                    target_file,
                    vec!["Computed in generated Python as duration_ns * 1_000_000 / timestep_fs.".to_string()],
                );
            }
            if let Some(value) = parameter(stage, "timestepFs") {
                map_timestep_ps(mapper, stage, value, "LangevinMiddleIntegrator step size", target_file);
            }
            if let Some(value) = parameter(stage, "checkpointEveryPs") {
                map_steps_from_duration_ps(
                    mapper,
                    stage,
                    "checkpointEveryPs",
                    value,
                    timestep_fs,
                    "report_interval",
                    target_file,
                    vec!["Shared by StateDataReporter, DCDReporter, and CheckpointReporter.".to_string()],
                );
            }
            if let Some(value) = parameter(stage, "randomSeed") {
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: "randomSeed",
                        normalized_value: value.to_string(),
                        engine_key: "random_seed",
                        engine_value: value.to_string(),
                        target_file,
                        status: ParameterMappingStatus::Mapped,
                        notes: vec![
                            "Passed to integrator.setRandomNumberSeed and setVelocitiesToTemperature when positive."
                                .to_string(),
                        ],
                    },
                );
            }
        }
        "prepare" => map_prepare_manual(mapper, stage, "generated/openmm/README.md"),
        "analysis" => map_analysis_manual(mapper, stage, "generated/openmm/automd-plan.json"),
        _ => map_stage_manual(mapper, stage, target_file),
    }
}

fn map_amber_stage(
    mapper: &mut ParameterMapper,
    plan: &SimulationPlan,
    stage: &SimulationStage,
    engine_id: &str,
    generated_dir: &str,
) {
    match stage.id.as_str() {
        "em" => {
            if let Some(value) = parameter(stage, "maxSteps") {
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: "maxSteps",
                        normalized_value: value.to_string(),
                        engine_key: "maxcyc",
                        engine_value: "5000".to_string(),
                        target_file: "generated/ambertools/min.mdin",
                        status: ParameterMappingStatus::ManualReview,
                        notes: vec![
                            "Current AmberTools minimization template is fixed at maxcyc=5000; edit min.mdin for a custom value."
                                .to_string(),
                        ],
                    },
                );
            }
            if let Some(value) = parameter(stage, "emtol") {
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: "emtol",
                        normalized_value: value.to_string(),
                        engine_key: "drms",
                        engine_value: "not emitted".to_string(),
                        target_file: "generated/ambertools/min.mdin",
                        status: ParameterMappingStatus::Unsupported,
                        notes: vec!["Amber minimization tolerance is not exposed in the current template.".to_string()],
                    },
                );
            }
        }
        "nvt" => {
            if let Some(value) = parameter(stage, "temperatureK") {
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: "temperatureK",
                        normalized_value: format!("{value} K"),
                        engine_key: "temp0",
                        engine_value: value.to_string(),
                        target_file: "generated/ambertools/heat.mdin",
                        status: ParameterMappingStatus::Mapped,
                        notes: vec!["Heating target temperature.".to_string()],
                    },
                );
            }
            if let Some(value) = parameter(stage, "durationPs") {
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: "durationPs",
                        normalized_value: format!("{value} ps"),
                        engine_key: "nstlim",
                        engine_value: "50000".to_string(),
                        target_file: "generated/ambertools/heat.mdin",
                        status: ParameterMappingStatus::ManualReview,
                        notes: vec![
                            "Current heat.mdin uses a fixed 50,000-step heating phase; edit native file for a custom duration."
                                .to_string(),
                        ],
                    },
                );
            }
            if let Some(value) = parameter(stage, "velocitySeed") {
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: "velocitySeed",
                        normalized_value: value.to_string(),
                        engine_key: "ig fallback",
                        engine_value: value.to_string(),
                        target_file: &format!("{generated_dir}/prod.mdin"),
                        status: ParameterMappingStatus::Mapped,
                        notes: vec!["Used as production ig when production.randomSeed is absent.".to_string()],
                    },
                );
            }
        }
        "npt" => {
            if let Some(value) = parameter(stage, "temperatureK") {
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: "temperatureK",
                        normalized_value: format!("{value} K"),
                        engine_key: "temp0",
                        engine_value: value.to_string(),
                        target_file: "generated/ambertools/equil.mdin / prod.mdin",
                        status: ParameterMappingStatus::Mapped,
                        notes: vec!["Used by NPT equilibration and as production temperature fallback.".to_string()],
                    },
                );
            }
            if let Some(value) = parameter(stage, "pressureBar") {
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: "pressureBar",
                        normalized_value: format!("{value} bar"),
                        engine_key: "pres0",
                        engine_value: "1.0".to_string(),
                        target_file: "generated/ambertools/equil.mdin / prod.mdin",
                        status: if value.trim() == "1" || value.trim() == "1.0" {
                            ParameterMappingStatus::Mapped
                        } else {
                            ParameterMappingStatus::ManualReview
                        },
                        notes: vec![
                            "Current Amber templates emit pres0=1.0; edit native mdin files for a different pressure."
                                .to_string(),
                        ],
                    },
                );
            }
            if let Some(value) = parameter(stage, "durationPs") {
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: "durationPs",
                        normalized_value: format!("{value} ps"),
                        engine_key: "nstlim",
                        engine_value: "250000".to_string(),
                        target_file: "generated/ambertools/equil.mdin",
                        status: ParameterMappingStatus::ManualReview,
                        notes: vec![
                            "Current equil.mdin uses a fixed 250,000-step NPT phase; edit native file for a custom duration."
                                .to_string(),
                        ],
                    },
                );
            }
        }
        "production" => {
            let target_file = format!("{generated_dir}/prod.mdin");
            let timestep_fs = production_timestep_fs(plan, mapper, 2.0);
            if let Some(value) = parameter(stage, "durationNs") {
                map_steps_from_duration_ns(
                    mapper,
                    stage,
                    value,
                    timestep_fs,
                    "nstlim",
                    &target_file,
                    Vec::new(),
                );
            }
            if let Some(value) = parameter(stage, "timestepFs") {
                map_timestep_ps(mapper, stage, value, "dt", &target_file);
            }
            if let Some(value) = parameter(stage, "randomSeed") {
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: "randomSeed",
                        normalized_value: value.to_string(),
                        engine_key: "ig",
                        engine_value: value.to_string(),
                        target_file: &target_file,
                        status: ParameterMappingStatus::Mapped,
                        notes: vec!["Amber Langevin random seed; -1 keeps Amber's random behavior.".to_string()],
                    },
                );
            }
            if let Some(value) = parameter(stage, "checkpointEveryPs") {
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: "checkpointEveryPs",
                        normalized_value: format!("{value} ps"),
                        engine_key: "ntwr",
                        engine_value: "5000".to_string(),
                        target_file: &target_file,
                        status: ParameterMappingStatus::ManualReview,
                        notes: vec![
                            "Current prod.mdin uses fixed ntwr=5000; edit native file for checkpoint cadence."
                                .to_string(),
                        ],
                    },
                );
            }
        }
        "prepare" => map_prepare_manual(mapper, stage, "generated/ambertools/tleap.in"),
        "analysis" => map_analysis_manual(mapper, stage, "generated/ambertools/cpptraj.in"),
        _ => map_stage_manual(mapper, stage, &format!("generated/{engine_id}/automd-plan.json")),
    }
}

fn map_namd_stage(mapper: &mut ParameterMapper, plan: &SimulationPlan, stage: &SimulationStage) {
    let target_file = "generated/namd/automd.conf";
    match stage.id.as_str() {
        "em" => {
            if let Some(value) = parameter(stage, "maxSteps") {
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: "maxSteps",
                        normalized_value: value.to_string(),
                        engine_key: "minimize",
                        engine_value: "5000".to_string(),
                        target_file,
                        status: ParameterMappingStatus::ManualReview,
                        notes: vec!["Current NAMD template emits minimize 5000.".to_string()],
                    },
                );
            }
        }
        "nvt" => {
            if let Some(value) = parameter(stage, "temperatureK") {
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: "temperatureK",
                        normalized_value: format!("{value} K"),
                        engine_key: "temperature / langevinTemp / reinitvels",
                        engine_value: value.to_string(),
                        target_file,
                        status: ParameterMappingStatus::Mapped,
                        notes: vec!["Applied to initial temperature and Langevin thermostat.".to_string()],
                    },
                );
            }
            if let Some(value) = parameter(stage, "durationPs") {
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: "durationPs",
                        normalized_value: format!("{value} ps"),
                        engine_key: "equilibration run",
                        engine_value: "not emitted".to_string(),
                        target_file,
                        status: ParameterMappingStatus::Unsupported,
                        notes: vec!["Current NAMD external template emits a single production run.".to_string()],
                    },
                );
            }
        }
        "npt" => {
            if let Some(value) = parameter(stage, "temperatureK") {
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: "temperatureK",
                        normalized_value: format!("{value} K"),
                        engine_key: "temperature fallback",
                        engine_value: value.to_string(),
                        target_file,
                        status: ParameterMappingStatus::Approximated,
                        notes: vec!["Used only if the template is extended to a separate NPT stage.".to_string()],
                    },
                );
            }
            if let Some(value) = parameter(stage, "pressureBar") {
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: "pressureBar",
                        normalized_value: format!("{value} bar"),
                        engine_key: "langevinPistonTarget",
                        engine_value: "not emitted".to_string(),
                        target_file,
                        status: ParameterMappingStatus::Unsupported,
                        notes: vec!["Current NAMD template has no Langevin piston/NPT block.".to_string()],
                    },
                );
            }
        }
        "production" => {
            let timestep_fs = production_timestep_fs(plan, mapper, 2.0);
            if let Some(value) = parameter(stage, "durationNs") {
                map_steps_from_duration_ns(
                    mapper,
                    stage,
                    value,
                    timestep_fs,
                    "numsteps / run",
                    target_file,
                    Vec::new(),
                );
            }
            if let Some(value) = parameter(stage, "timestepFs") {
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: "timestepFs",
                        normalized_value: format!("{value} fs"),
                        engine_key: "timestep",
                        engine_value: value.to_string(),
                        target_file,
                        status: ParameterMappingStatus::Mapped,
                        notes: vec!["NAMD timestep is expressed in femtoseconds.".to_string()],
                    },
                );
            }
            if let Some(value) = parameter(stage, "checkpointEveryPs") {
                map_steps_from_duration_ps(
                    mapper,
                    stage,
                    "checkpointEveryPs",
                    value,
                    timestep_fs,
                    "restartfreq",
                    target_file,
                    vec![
                        "Current NAMD template emits restartfreq=5000; use this computed value when editing the native file."
                            .to_string(),
                    ],
                );
                if let Some(last) = mapper.items.last_mut() {
                    last.status = ParameterMappingStatus::ManualReview;
                }
            }
        }
        "prepare" => map_prepare_manual(mapper, stage, target_file),
        "analysis" => map_analysis_manual(mapper, stage, "generated/namd/automd-plan.json"),
        _ => map_stage_manual(mapper, stage, target_file),
    }
}

fn map_preview_stage(mapper: &mut ParameterMapper, plan: &SimulationPlan, stage: &SimulationStage, engine_id: &str) {
    let target_file = preview_target_file(engine_id);
    match engine_id {
        "lammps" | "cp2k" | "genesis" => {
            if stage.id == "production" {
                let timestep_fs = production_timestep_fs(plan, mapper, if engine_id == "genesis" { 2.0 } else { 1.0 });
                if let Some(value) = parameter(stage, "durationNs") {
                    map_steps_from_duration_ns(mapper, stage, value, timestep_fs, "steps/run", target_file, Vec::new());
                    if let Some(last) = mapper.items.last_mut() {
                        last.status = ParameterMappingStatus::Approximated;
                        last.notes.push("Preview template still requires topology/force-field review before real execution.".to_string());
                    }
                }
                if let Some(value) = parameter(stage, "timestepFs") {
                    mapper.add(
                        stage,
                        MappingSpec {
                            normalized_key: "timestepFs",
                            normalized_value: format!("{value} fs"),
                            engine_key: "timestep",
                            engine_value: value.to_string(),
                            target_file,
                            status: ParameterMappingStatus::Approximated,
                            notes: vec!["Preview template mapping; validate native units before production use.".to_string()],
                        },
                    );
                }
            }
            if (stage.id == "nvt" || stage.id == "npt") && parameter(stage, "temperatureK").is_some() {
                let value = parameter(stage, "temperatureK").unwrap_or("300");
                mapper.add(
                    stage,
                    MappingSpec {
                        normalized_key: "temperatureK",
                        normalized_value: format!("{value} K"),
                        engine_key: "temperature",
                        engine_value: value.to_string(),
                        target_file,
                        status: ParameterMappingStatus::Approximated,
                        notes: vec!["Preview template thermostat target.".to_string()],
                    },
                );
            }
        }
        "amber_pmemd" => map_amber_stage(mapper, plan, stage, "amber_pmemd", "generated/amber_pmemd"),
        _ => map_stage_manual(mapper, stage, target_file),
    }
}

fn map_prepare_manual(mapper: &mut ParameterMapper, stage: &SimulationStage, target_file: &str) {
    for (key, value) in &stage.parameters {
        mapper.add(
            stage,
            MappingSpec {
                normalized_key: key,
                normalized_value: value.clone(),
                engine_key: "preparation workflow",
                engine_value: "sidecar / native setup".to_string(),
                target_file,
                status: ParameterMappingStatus::ManualReview,
                notes: vec!["Structure preparation spans external tools and generated setup scripts.".to_string()],
            },
        );
    }
}

fn map_analysis_manual(mapper: &mut ParameterMapper, stage: &SimulationStage, target_file: &str) {
    for (key, value) in &stage.parameters {
        mapper.add(
            stage,
            MappingSpec {
                normalized_key: key,
                normalized_value: value.clone(),
                engine_key: "analysis module parameter",
                engine_value: "analysis sidecar".to_string(),
                target_file,
                status: ParameterMappingStatus::ManualReview,
                notes: vec!["Detailed analysis parameters are emitted by the MDAnalysis/cpptraj analysis package.".to_string()],
            },
        );
    }
}

fn map_stage_manual(mapper: &mut ParameterMapper, stage: &SimulationStage, target_file: &str) {
    for (key, value) in &stage.parameters {
        mapper.add(
            stage,
            MappingSpec {
                normalized_key: key,
                normalized_value: value.clone(),
                engine_key: "native template",
                engine_value: "manual review".to_string(),
                target_file,
                status: ParameterMappingStatus::ManualReview,
                notes: vec!["This engine/stage is available through an editable native template.".to_string()],
            },
        );
    }
}

fn map_steps_from_duration_ns(
    mapper: &mut ParameterMapper,
    stage: &SimulationStage,
    duration_ns: &str,
    timestep_fs: f32,
    engine_key: &str,
    target_file: &str,
    mut notes: Vec<String>,
) {
    match parse_positive(duration_ns) {
        Some(duration_ns) => {
            let steps = nsteps_from_ps(duration_ns * 1000.0, timestep_fs);
            notes.push(format!("Computed from {duration_ns} ns and {timestep_fs} fs timestep."));
            mapper.add(
                stage,
                MappingSpec {
                    normalized_key: "durationNs",
                    normalized_value: format_number_with_unit(duration_ns, "ns"),
                    engine_key,
                    engine_value: steps.to_string(),
                    target_file,
                    status: ParameterMappingStatus::Mapped,
                    notes,
                },
            );
        }
        None => mapper.add(
            stage,
            MappingSpec {
                normalized_key: "durationNs",
                normalized_value: duration_ns.to_string(),
                engine_key,
                engine_value: "invalid duration".to_string(),
                target_file,
                status: ParameterMappingStatus::Unsupported,
                notes: vec!["Enter a positive numeric duration before generating native inputs.".to_string()],
            },
        ),
    }
}

fn map_steps_from_duration_ps(
    mapper: &mut ParameterMapper,
    stage: &SimulationStage,
    normalized_key: &str,
    duration_ps: &str,
    timestep_fs: f32,
    engine_key: &str,
    target_file: &str,
    mut notes: Vec<String>,
) {
    match parse_positive(duration_ps) {
        Some(duration_ps) => {
            let steps = nsteps_from_ps(duration_ps, timestep_fs);
            notes.push(format!("Computed from {duration_ps} ps and {timestep_fs} fs timestep."));
            mapper.add(
                stage,
                MappingSpec {
                    normalized_key,
                    normalized_value: format_number_with_unit(duration_ps, "ps"),
                    engine_key,
                    engine_value: steps.to_string(),
                    target_file,
                    status: ParameterMappingStatus::Mapped,
                    notes,
                },
            );
        }
        None => mapper.add(
            stage,
            MappingSpec {
                normalized_key,
                normalized_value: duration_ps.to_string(),
                engine_key,
                engine_value: "invalid interval".to_string(),
                target_file,
                status: ParameterMappingStatus::Unsupported,
                notes: vec!["Enter a positive numeric ps value before generating native inputs.".to_string()],
            },
        ),
    }
}

fn map_timestep_ps(
    mapper: &mut ParameterMapper,
    stage: &SimulationStage,
    timestep_fs: &str,
    engine_key: &str,
    target_file: &str,
) {
    match parse_positive(timestep_fs) {
        Some(value) => mapper.add(
            stage,
            MappingSpec {
                normalized_key: "timestepFs",
                normalized_value: format_number_with_unit(value, "fs"),
                engine_key,
                engine_value: format_number(value / 1000.0),
                target_file,
                status: ParameterMappingStatus::Mapped,
                notes: vec!["Converted from fs to ps for this native field.".to_string()],
            },
        ),
        None => mapper.add(
            stage,
            MappingSpec {
                normalized_key: "timestepFs",
                normalized_value: timestep_fs.to_string(),
                engine_key,
                engine_value: "invalid timestep".to_string(),
                target_file,
                status: ParameterMappingStatus::Unsupported,
                notes: vec!["Enter a positive numeric timestep before generating native inputs.".to_string()],
            },
        ),
    }
}

fn production_timestep_fs(plan: &SimulationPlan, mapper: &mut ParameterMapper, fallback: f32) -> f32 {
    match plan
        .stages
        .iter()
        .find(|stage| stage.id == "production")
        .and_then(|stage| parameter(stage, "timestepFs"))
    {
        Some(value) => parse_positive(value).unwrap_or_else(|| {
            mapper.warn(format!(
                "Invalid production timestepFs '{value}'; mapping report used fallback {fallback} fs."
            ));
            fallback
        }),
        None => {
            mapper.warn(format!(
                "production.timestepFs is absent; mapping report used fallback {fallback} fs."
            ));
            fallback
        }
    }
}

fn parameter<'a>(stage: &'a SimulationStage, key: &str) -> Option<&'a str> {
    stage.parameters.get(key).map(String::as_str)
}

fn parse_positive(value: &str) -> Option<f32> {
    let parsed = value.trim().parse::<f32>().ok()?;
    (parsed > 0.0 && parsed.is_finite()).then_some(parsed)
}

fn nsteps_from_ps(duration_ps: f32, timestep_fs: f32) -> u64 {
    ((duration_ps * 1000.0) / timestep_fs.max(0.001)).round().max(1.0) as u64
}

fn format_number(value: f32) -> String {
    let rounded = format!("{value:.4}");
    rounded.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn format_number_with_unit(value: f32, unit: &str) -> String {
    format!("{} {unit}", format_number(value))
}

fn preview_target_file(engine_id: &str) -> &'static str {
    match engine_id {
        "lammps" => "generated/lammps/in.automd",
        "cp2k" => "generated/cp2k/automd.inp",
        "genesis" => "generated/genesis/automd.inp",
        "hoomd" => "generated/hoomd/run_hoomd.py",
        "dl_poly" => "generated/dl_poly/CONTROL",
        "tinker" => "generated/tinker/automd.key",
        "charmm" => "generated/charmm/automd.inp",
        "desmond" => "generated/desmond/automd.cfg",
        "acemd" => "generated/acemd/input",
        _ => "generated/<engine>/automd-plan.json",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planner;

    fn demo_plan(engine_id: &str) -> SimulationPlan {
        planner::default_simulation_plan(PlanRequest {
            project_id: None,
            name: "mapping-test".to_string(),
            engine_id: engine_id.to_string(),
            domain: ProjectDomain::Biomolecular,
        })
    }

    #[test]
    fn gromacs_maps_production_duration_to_steps() {
        let plan = demo_plan("gromacs");
        let report = map_parameters(ParameterMappingRequest {
            plan,
            engine_id: None,
        });

        let nsteps = report
            .items
            .iter()
            .find(|item| item.stage_id == "production" && item.normalized_key == "durationNs")
            .expect("production duration mapping");
        assert_eq!(nsteps.engine_key, "nsteps");
        assert_eq!(nsteps.engine_value, "50000000");
        assert_eq!(nsteps.target_file, "generated/gromacs/md.mdp");

        let checkpoint = report
            .items
            .iter()
            .find(|item| item.stage_id == "production" && item.normalized_key == "checkpointEveryPs")
            .expect("checkpoint mapping");
        assert_eq!(checkpoint.engine_key, "nstcheckpoint");
        assert_eq!(checkpoint.engine_value, "50000");
    }

    #[test]
    fn openmm_maps_report_interval_from_checkpoint_ps() {
        let plan = demo_plan("openmm");
        let report = map_parameters(ParameterMappingRequest {
            plan,
            engine_id: None,
        });

        let duration = report
            .items
            .iter()
            .find(|item| item.stage_id == "production" && item.normalized_key == "durationNs")
            .expect("openmm duration mapping");
        assert_eq!(duration.engine_key, "total_steps");
        assert_eq!(duration.engine_value, "50000000");

        let interval = report
            .items
            .iter()
            .find(|item| item.stage_id == "production" && item.normalized_key == "checkpointEveryPs")
            .expect("openmm interval mapping");
        assert_eq!(interval.engine_key, "report_interval");
        assert_eq!(interval.engine_value, "50000");
        assert_eq!(interval.target_file, "generated/openmm/run_openmm.py");
    }
}
