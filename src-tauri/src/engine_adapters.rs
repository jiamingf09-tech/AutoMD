use crate::models::*;
use serde_json::to_string_pretty;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineAdapterError {
    #[error("engine adapter is not implemented yet: {0}")]
    UnsupportedEngine(String),
    #[error("project path is required when write_to_disk is true")]
    MissingProjectPath,
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub fn prepare_run_package(
    request: EngineRunRequest,
) -> Result<EngineRunPackage, EngineAdapterError> {
    match request.plan.engine_id.as_str() {
        "gromacs" => prepare_gromacs_run_package(request),
        "openmm" => prepare_openmm_run_package(request),
        "ambertools" => prepare_ambertools_run_package(request),
        "namd" => prepare_namd_run_package(request),
        "lammps" | "cp2k" | "genesis" | "hoomd" | "dl_poly" | "tinker" | "amber_pmemd"
        | "charmm" | "desmond" | "acemd" => prepare_preview_run_package(request),
        other => Err(EngineAdapterError::UnsupportedEngine(other.to_string())),
    }
}

pub fn parse_engine_log(
    request: EngineLogParseRequest,
) -> Result<EngineLogReport, EngineAdapterError> {
    match request.engine_id.as_str() {
        "gromacs" => Ok(parse_gromacs_log(&request.log_contents)),
        "openmm" => Ok(parse_openmm_log(&request.log_contents)),
        "ambertools" => Ok(parse_ambertools_log(&request.log_contents)),
        "namd" => Ok(parse_namd_log(&request.log_contents)),
        "lammps" | "cp2k" | "genesis" | "hoomd" | "dl_poly" | "tinker" | "amber_pmemd"
        | "charmm" | "desmond" | "acemd" => Ok(parse_generic_engine_log(
            &request.engine_id,
            &request.log_contents,
        )),
        other => Err(EngineAdapterError::UnsupportedEngine(other.to_string())),
    }
}

pub fn classify_engine_failure(
    request: FailureAnalysisRequest,
) -> Result<FailureAnalysis, EngineAdapterError> {
    match request.engine_id.as_str() {
        "gromacs" => Ok(classify_gromacs_failure(
            &request.log_contents,
            request.exit_code,
        )),
        "openmm" => Ok(classify_openmm_failure(
            &request.log_contents,
            request.exit_code,
        )),
        "ambertools" => Ok(classify_ambertools_failure(
            &request.log_contents,
            request.exit_code,
        )),
        "namd" => Ok(classify_namd_failure(
            &request.log_contents,
            request.exit_code,
        )),
        "lammps" | "cp2k" | "genesis" | "hoomd" | "dl_poly" | "tinker" | "amber_pmemd"
        | "charmm" | "desmond" | "acemd" => Ok(classify_generic_engine_failure(
            &request.engine_id,
            &request.log_contents,
            request.exit_code,
        )),
        other => Err(EngineAdapterError::UnsupportedEngine(other.to_string())),
    }
}

pub fn discover_resume_plan(request: ResumePlanRequest) -> Result<ResumePlan, EngineAdapterError> {
    match request.engine_id.as_str() {
        "gromacs" => discover_gromacs_resume_plan(request),
        "openmm" => discover_openmm_resume_plan(request),
        other => Err(EngineAdapterError::UnsupportedEngine(other.to_string())),
    }
}

fn prepare_gromacs_run_package(
    request: EngineRunRequest,
) -> Result<EngineRunPackage, EngineAdapterError> {
    let plan = request.plan;
    let run_slug = format!("gromacs-{}", plan.id.simple());
    let run_directory = format!("runs/{run_slug}");
    let mut warnings = Vec::new();

    if plan.system.source_path.is_none() {
        warnings.push("未设置输入结构路径；脚本使用 inputs/system.pdb 占位。".to_string());
    }
    if plan.system.has_ligand {
        warnings.push(
            "检测到配体体系；GROMACS 原生流程需要先由 CGenFF/GAFF2 等工具提供配体拓扑。"
                .to_string(),
        );
    }
    if plan.system.has_membrane {
        warnings.push(
            "膜体系需要外部构建或 CHARMM-GUI 导入；当前 GROMACS 模板只覆盖水溶液体系。".to_string(),
        );
    }

    let mut files = vec![
        gromacs_mdp_file(&plan, "ions", "generated/gromacs/ions.mdp"),
        gromacs_mdp_file(&plan, "em", "generated/gromacs/em.mdp"),
        gromacs_mdp_file(&plan, "nvt", "generated/gromacs/nvt.mdp"),
        gromacs_mdp_file(&plan, "npt", "generated/gromacs/npt.mdp"),
        gromacs_mdp_file(&plan, "production", "generated/gromacs/md.mdp"),
        EngineRunFile {
            path: "generated/gromacs/automd-plan.json".to_string(),
            language: "json".to_string(),
            contents: to_string_pretty(&plan)?,
            written: false,
        },
    ];

    let commands = gromacs_commands(&plan, &run_directory);
    files.push(EngineRunFile {
        path: format!("{run_directory}/run-gromacs.sh"),
        language: "bash".to_string(),
        contents: gromacs_run_script(&plan, &commands),
        written: false,
    });
    files.push(EngineRunFile {
        path: format!("{run_directory}/README.md"),
        language: "markdown".to_string(),
        contents: gromacs_run_readme(&plan, &warnings),
        written: false,
    });

    if request.write_to_disk {
        let project_path = request
            .project_path
            .as_deref()
            .ok_or(EngineAdapterError::MissingProjectPath)?;
        write_files(project_path, &mut files)?;
    }

    Ok(EngineRunPackage {
        engine_id: "gromacs".to_string(),
        plan_id: plan.id,
        run_directory,
        commands,
        files,
        warnings,
        writable: request.project_path.is_some(),
    })
}

fn prepare_openmm_run_package(
    request: EngineRunRequest,
) -> Result<EngineRunPackage, EngineAdapterError> {
    let plan = request.plan;
    let run_slug = format!("openmm-{}", plan.id.simple());
    let run_directory = format!("runs/{run_slug}");
    let mut warnings = Vec::new();

    if plan.system.source_path.is_none() {
        warnings.push("未设置输入结构路径；OpenMM 脚本使用 inputs/system.pdb 占位。".to_string());
    }
    if plan.system.has_ligand {
        warnings.push("OpenMM 首版模板不自动完成配体参数化；请先提供兼容的拓扑/力场 XML 或改用 AmberTools/GROMACS 准备链。".to_string());
    }
    if plan.system.has_membrane {
        warnings.push(
            "膜体系需要外部构建并确认力场 XML；当前 OpenMM 模板只覆盖普通显式溶剂体系。"
                .to_string(),
        );
    }

    let commands = openmm_commands(&plan, &run_directory);
    let mut files = vec![
        EngineRunFile {
            path: "generated/openmm/automd-plan.json".to_string(),
            language: "json".to_string(),
            contents: to_string_pretty(&plan)?,
            written: false,
        },
        EngineRunFile {
            path: "generated/openmm/run_openmm.py".to_string(),
            language: "python".to_string(),
            contents: openmm_runner_py(&plan, &run_directory),
            written: false,
        },
        EngineRunFile {
            path: format!("{run_directory}/run-openmm.sh"),
            language: "bash".to_string(),
            contents: openmm_run_script(&plan, &commands),
            written: false,
        },
        EngineRunFile {
            path: format!("{run_directory}/README.md"),
            language: "markdown".to_string(),
            contents: openmm_run_readme(&plan, &warnings),
            written: false,
        },
    ];

    if request.write_to_disk {
        let project_path = request
            .project_path
            .as_deref()
            .ok_or(EngineAdapterError::MissingProjectPath)?;
        write_files(project_path, &mut files)?;
    }

    Ok(EngineRunPackage {
        engine_id: "openmm".to_string(),
        plan_id: plan.id,
        run_directory,
        commands,
        files,
        warnings,
        writable: request.project_path.is_some(),
    })
}

fn prepare_ambertools_run_package(
    request: EngineRunRequest,
) -> Result<EngineRunPackage, EngineAdapterError> {
    let plan = request.plan;
    let run_slug = format!("ambertools-{}", plan.id.simple());
    let run_directory = format!("runs/{run_slug}");
    let mut warnings = Vec::new();

    if plan.system.source_path.is_none() {
        warnings.push(
            "未设置输入结构路径；AmberTools tleap 脚本使用 inputs/system.pdb 占位。".to_string(),
        );
    }
    if plan.system.has_ligand {
        warnings.push("配体体系需要 antechamber/parmchk2 或用户提供 mol2/frcmod；当前模板预留加载位置但不自动参数化。".to_string());
    }
    if plan.system.has_membrane {
        warnings.push("膜体系需要专门 lipid force field 和构建流程；当前 AmberTools 模板只覆盖普通显式溶剂体系。".to_string());
    }

    let commands = ambertools_commands(&plan, &run_directory);
    if plan.solvent.ionic_strength_molar > 0.0 {
        warnings.push(format!(
            "离子强度 {} M：在溶剂化/中和后按 n_water×C/55.5 估算 1:1 盐离子对数并二次 tleap 添加。",
            format_number(plan.solvent.ionic_strength_molar)
        ));
    }

    let mut files = vec![
        EngineRunFile {
            path: "generated/ambertools/automd-plan.json".to_string(),
            language: "json".to_string(),
            contents: to_string_pretty(&plan)?,
            written: false,
        },
        EngineRunFile {
            path: "generated/ambertools/tleap.in".to_string(),
            language: "amber".to_string(),
            contents: ambertools_tleap(&plan),
            written: false,
        },
        EngineRunFile {
            path: "generated/ambertools/add_salt.py".to_string(),
            language: "python".to_string(),
            contents: ambertools_add_salt_py(&plan),
            written: false,
        },
        EngineRunFile {
            path: "generated/ambertools/min.mdin".to_string(),
            language: "amber".to_string(),
            contents: ambertools_min_mdin(),
            written: false,
        },
        EngineRunFile {
            path: "generated/ambertools/heat.mdin".to_string(),
            language: "amber".to_string(),
            contents: ambertools_heat_mdin(&plan),
            written: false,
        },
        EngineRunFile {
            path: "generated/ambertools/equil.mdin".to_string(),
            language: "amber".to_string(),
            contents: ambertools_equil_mdin(&plan),
            written: false,
        },
        EngineRunFile {
            path: "generated/ambertools/prod.mdin".to_string(),
            language: "amber".to_string(),
            contents: ambertools_prod_mdin(&plan),
            written: false,
        },
        EngineRunFile {
            path: "generated/ambertools/cpptraj.in".to_string(),
            language: "amber".to_string(),
            contents: ambertools_cpptraj(&run_directory),
            written: false,
        },
        EngineRunFile {
            path: format!("{run_directory}/run-ambertools.sh"),
            language: "bash".to_string(),
            contents: ambertools_run_script(&plan, &commands),
            written: false,
        },
        EngineRunFile {
            path: format!("{run_directory}/README.md"),
            language: "markdown".to_string(),
            contents: ambertools_run_readme(&plan, &warnings),
            written: false,
        },
    ];

    if request.write_to_disk {
        let project_path = request
            .project_path
            .as_deref()
            .ok_or(EngineAdapterError::MissingProjectPath)?;
        write_files(project_path, &mut files)?;
    }

    Ok(EngineRunPackage {
        engine_id: "ambertools".to_string(),
        plan_id: plan.id,
        run_directory,
        commands,
        files,
        warnings,
        writable: request.project_path.is_some(),
    })
}

fn prepare_namd_run_package(
    request: EngineRunRequest,
) -> Result<EngineRunPackage, EngineAdapterError> {
    let plan = request.plan;
    let run_slug = format!("namd-{}", plan.id.simple());
    let run_directory = format!("runs/{run_slug}");
    let warnings = vec![
        "NAMD 是用户自带许可/安装的外部模块；AutoMD 不下载、不分发 NAMD 二进制文件。".to_string(),
        "当前模板需要用户提供 PSF/PDB 或从 CHARMM-GUI、VMD psfgen、AmberTools 等流程导入。"
            .to_string(),
    ];

    let commands = namd_commands(&run_directory, plan.resources.cpu_threads);
    let mut files = vec![
        EngineRunFile {
            path: "generated/namd/automd-plan.json".to_string(),
            language: "json".to_string(),
            contents: to_string_pretty(&plan)?,
            written: false,
        },
        EngineRunFile {
            path: "generated/namd/automd.conf".to_string(),
            language: "tcl".to_string(),
            contents: namd_conf(&plan, &run_directory),
            written: false,
        },
        EngineRunFile {
            path: format!("{run_directory}/run-namd.sh"),
            language: "bash".to_string(),
            contents: namd_run_script(&plan, &commands),
            written: false,
        },
        EngineRunFile {
            path: format!("{run_directory}/README.md"),
            language: "markdown".to_string(),
            contents: namd_run_readme(&plan, &warnings),
            written: false,
        },
    ];

    if request.write_to_disk {
        let project_path = request
            .project_path
            .as_deref()
            .ok_or(EngineAdapterError::MissingProjectPath)?;
        write_files(project_path, &mut files)?;
    }

    Ok(EngineRunPackage {
        engine_id: "namd".to_string(),
        plan_id: plan.id,
        run_directory,
        commands,
        files,
        warnings,
        writable: request.project_path.is_some(),
    })
}

struct PreviewRunSpec {
    display_name: String,
    generated_slug: String,
    run_script_name: String,
    files: Vec<EngineRunFile>,
    commands: Vec<EngineCommand>,
    warnings: Vec<String>,
    scope_note: String,
}

fn prepare_preview_run_package(
    request: EngineRunRequest,
) -> Result<EngineRunPackage, EngineAdapterError> {
    let plan = request.plan;
    let run_slug = format!("{}-{}", plan.engine_id.replace('_', "-"), plan.id.simple());
    let run_directory = format!("runs/{run_slug}");
    let mut spec = preview_run_spec(&plan, &run_directory)?;

    if plan.system.source_path.is_none() {
        spec.warnings.push(format!(
            "未设置输入结构路径；{} 模板使用 inputs/ 下的占位文件，真实运行前必须替换。",
            spec.display_name
        ));
    }
    if plan.system.has_ligand {
        spec.warnings.push(format!(
            "{} 预览模板不自动完成配体/非标准残基参数化；请先提供引擎原生参数文件。",
            spec.display_name
        ));
    }
    if matches!(
        plan.engine_id.as_str(),
        "amber_pmemd" | "charmm" | "desmond" | "acemd"
    ) {
        spec.warnings.push(format!(
            "{} 是用户自带授权的外部模块；AutoMD 不下载、不分发、不代管许可证。",
            spec.display_name
        ));
    }

    let mut files = vec![EngineRunFile {
        path: format!("generated/{}/automd-plan.json", spec.generated_slug),
        language: "json".to_string(),
        contents: to_string_pretty(&plan)?,
        written: false,
    }];
    files.append(&mut spec.files);
    files.push(EngineRunFile {
        path: format!("{run_directory}/{}", spec.run_script_name),
        language: "bash".to_string(),
        contents: preview_run_script(&plan, &spec),
        written: false,
    });
    files.push(EngineRunFile {
        path: format!("{run_directory}/README.md"),
        language: "markdown".to_string(),
        contents: preview_run_readme(&plan, &spec),
        written: false,
    });

    if request.write_to_disk {
        let project_path = request
            .project_path
            .as_deref()
            .ok_or(EngineAdapterError::MissingProjectPath)?;
        write_files(project_path, &mut files)?;
    }

    Ok(EngineRunPackage {
        engine_id: plan.engine_id,
        plan_id: plan.id,
        run_directory,
        commands: spec.commands,
        files,
        warnings: spec.warnings,
        writable: request.project_path.is_some(),
    })
}

fn preview_run_spec(
    plan: &SimulationPlan,
    run_directory: &str,
) -> Result<PreviewRunSpec, EngineAdapterError> {
    match plan.engine_id.as_str() {
        "lammps" => Ok(lammps_preview_spec(plan, run_directory)),
        "cp2k" => Ok(cp2k_preview_spec(plan, run_directory)),
        "genesis" => Ok(genesis_preview_spec(plan, run_directory)),
        "hoomd" => Ok(hoomd_preview_spec(plan, run_directory)),
        "dl_poly" => Ok(dl_poly_preview_spec(run_directory)),
        "tinker" => Ok(tinker_preview_spec(run_directory)),
        "amber_pmemd" => Ok(amber_pmemd_preview_spec(plan, run_directory)),
        "charmm" => Ok(charmm_preview_spec(run_directory)),
        "desmond" => Ok(desmond_preview_spec(run_directory)),
        "acemd" => Ok(acemd_preview_spec(run_directory)),
        other => Err(EngineAdapterError::UnsupportedEngine(other.to_string())),
    }
}

fn preview_file(path: &str, language: &str, contents: String) -> EngineRunFile {
    EngineRunFile {
        path: path.to_string(),
        language: language.to_string(),
        contents,
        written: false,
    }
}

fn preview_command(
    stage_id: &str,
    label: &str,
    command: String,
    expected_outputs: Vec<String>,
) -> EngineCommand {
    EngineCommand {
        stage_id: stage_id.to_string(),
        label: label.to_string(),
        command,
        working_directory: ".".to_string(),
        expected_outputs,
    }
}

fn lammps_preview_spec(plan: &SimulationPlan, run_directory: &str) -> PreviewRunSpec {
    PreviewRunSpec {
        display_name: "LAMMPS".to_string(),
        generated_slug: "lammps".to_string(),
        run_script_name: "run-lammps.sh".to_string(),
        files: vec![preview_file("generated/lammps/in.automd", "lammps", lammps_input(plan, run_directory))],
        commands: vec![
            preview_command(
                "lammps-env",
                "检测 LAMMPS 可执行文件",
                "command -v lmp >/dev/null 2>&1 || command -v lmp_serial >/dev/null 2>&1 || command -v lmp_mpi >/dev/null 2>&1".to_string(),
                Vec::new(),
            ),
            preview_command(
                "lammps-run",
                "运行 LAMMPS input 模板",
                format!(
                    "LAMMPS_BIN=\"${{LAMMPS_BIN:-$(command -v lmp || command -v lmp_serial || command -v lmp_mpi)}}\"; \"$LAMMPS_BIN\" -in generated/lammps/in.automd -log {run_directory}/lammps.log"
                ),
                vec![format!("{run_directory}/lammps.log"), format!("{run_directory}/dump.lammpstrj")],
            ),
        ],
        warnings: vec!["LAMMPS 模板保留 pair_style、read_data、force-field 系数的原生编辑入口。".to_string()],
        scope_note: "Materials/soft-matter preview template; users must provide a valid LAMMPS data file and force-field coefficients.".to_string(),
    }
}

fn cp2k_preview_spec(plan: &SimulationPlan, run_directory: &str) -> PreviewRunSpec {
    PreviewRunSpec {
        display_name: "CP2K".to_string(),
        generated_slug: "cp2k".to_string(),
        run_script_name: "run-cp2k.sh".to_string(),
        files: vec![preview_file("generated/cp2k/automd.inp", "cp2k", cp2k_input(plan, run_directory))],
        commands: vec![
            preview_command(
                "cp2k-env",
                "检测 CP2K 可执行文件",
                "command -v cp2k >/dev/null 2>&1 || command -v cp2k.psmp >/dev/null 2>&1 || command -v cp2k.popt >/dev/null 2>&1".to_string(),
                Vec::new(),
            ),
            preview_command(
                "cp2k-run",
                "运行 CP2K input 模板",
                format!(
                    "CP2K_BIN=\"${{CP2K_BIN:-$(command -v cp2k || command -v cp2k.psmp || command -v cp2k.popt)}}\"; \"$CP2K_BIN\" -i generated/cp2k/automd.inp -o {run_directory}/cp2k.out"
                ),
                vec![format!("{run_directory}/cp2k.out"), format!("{run_directory}/automd-1.restart")],
            ),
        ],
        warnings: vec!["CP2K 模板是 QM/MM 与从头算 MD 的保守入口；basis、potential、cell 和 KIND 需要用户校验。".to_string()],
        scope_note: "CP2K preview template for QUICKSTEP-style MD. Real calculations require validated basis sets, pseudopotentials, and cell parameters.".to_string(),
    }
}

fn genesis_preview_spec(plan: &SimulationPlan, run_directory: &str) -> PreviewRunSpec {
    PreviewRunSpec {
        display_name: "GENESIS".to_string(),
        generated_slug: "genesis".to_string(),
        run_script_name: "run-genesis.sh".to_string(),
        files: vec![preview_file("generated/genesis/automd.inp", "ini", genesis_input(plan, run_directory))],
        commands: vec![
            preview_command(
                "genesis-env",
                "检测 GENESIS 可执行文件",
                "command -v atdyn >/dev/null 2>&1 || command -v spdyn >/dev/null 2>&1".to_string(),
                Vec::new(),
            ),
            preview_command(
                "genesis-run",
                "运行 GENESIS input 模板",
                format!(
                    "GENESIS_BIN=\"${{GENESIS_BIN:-$(command -v spdyn || command -v atdyn)}}\"; \"$GENESIS_BIN\" generated/genesis/automd.inp > {run_directory}/genesis.log 2>&1"
                ),
                vec![format!("{run_directory}/genesis.log"), format!("{run_directory}/prod.dcd")],
            ),
        ],
        warnings: vec!["GENESIS 模板需要用户提供 PSF/PDB/parameter 文件，并按 atdyn/spdyn 环境调整。".to_string()],
        scope_note: "Biomolecular GENESIS preview template with native input editor escape hatch.".to_string(),
    }
}

fn hoomd_preview_spec(plan: &SimulationPlan, run_directory: &str) -> PreviewRunSpec {
    PreviewRunSpec {
        display_name: "HOOMD-blue".to_string(),
        generated_slug: "hoomd".to_string(),
        run_script_name: "run-hoomd.sh".to_string(),
        files: vec![preview_file("generated/hoomd/run_hoomd.py", "python", hoomd_runner_py(plan, run_directory))],
        commands: vec![
            preview_command(
                "hoomd-env",
                "检测 HOOMD-blue Python 模块",
                "python -c \"import hoomd; print(hoomd.version.version)\"".to_string(),
                Vec::new(),
            ),
            preview_command(
                "hoomd-run",
                "运行 HOOMD-blue Python 模板",
                format!("python generated/hoomd/run_hoomd.py --out {run_directory} > {run_directory}/hoomd.log 2>&1"),
                vec![format!("{run_directory}/hoomd.log"), "trajectories/hoomd.gsd".to_string()],
            ),
        ],
        warnings: vec!["HOOMD-blue 模板是软物质/材料体系入口；真实 topology/force field 需要用户脚本化。".to_string()],
        scope_note: "Python-driven HOOMD-blue preview template.".to_string(),
    }
}

fn dl_poly_preview_spec(run_directory: &str) -> PreviewRunSpec {
    PreviewRunSpec {
        display_name: "DL_POLY".to_string(),
        generated_slug: "dl_poly".to_string(),
        run_script_name: "run-dl-poly.sh".to_string(),
        files: vec![
            preview_file("generated/dl_poly/CONTROL", "text", dl_poly_control()),
            preview_file("generated/dl_poly/FIELD", "text", dl_poly_field()),
            preview_file("generated/dl_poly/CONFIG", "text", dl_poly_config()),
        ],
        commands: vec![
            preview_command(
                "dl-poly-env",
                "检测 DL_POLY 可执行文件",
                "command -v DLPOLY.Z >/dev/null 2>&1 || command -v dl_poly >/dev/null 2>&1".to_string(),
                Vec::new(),
            ),
            preview_command(
                "dl-poly-run",
                "运行 DL_POLY 模板",
                format!(
                    "DLPOLY_BIN=\"${{DLPOLY_BIN:-$(command -v DLPOLY.Z || command -v dl_poly)}}\"; cp generated/dl_poly/CONTROL generated/dl_poly/FIELD generated/dl_poly/CONFIG {run_directory}/; (cd {run_directory} && \"$DLPOLY_BIN\" > dl_poly.log 2>&1)"
                ),
                vec![format!("{run_directory}/dl_poly.log"), format!("{run_directory}/HISTORY"), format!("{run_directory}/REVCON")],
            ),
        ],
        warnings: vec!["DL_POLY 需要用户提供有效 CONTROL/FIELD/CONFIG；当前仅生成可编辑骨架。".to_string()],
        scope_note: "DL_POLY materials preview template.".to_string(),
    }
}

fn tinker_preview_spec(run_directory: &str) -> PreviewRunSpec {
    PreviewRunSpec {
        display_name: "Tinker".to_string(),
        generated_slug: "tinker".to_string(),
        run_script_name: "run-tinker.sh".to_string(),
        files: vec![preview_file("generated/tinker/automd.key", "text", tinker_key())],
        commands: vec![
            preview_command(
                "tinker-env",
                "检测 Tinker 可执行文件",
                "command -v dynamic >/dev/null 2>&1 || command -v tinker9 >/dev/null 2>&1".to_string(),
                Vec::new(),
            ),
            preview_command(
                "tinker-run",
                "运行 Tinker 用户命令",
                format!(
                    ": \"${{TINKER_COMMAND:=dynamic inputs/system.xyz 10000 2.0 2.0 300}}\"; $TINKER_COMMAND > {run_directory}/tinker.log 2>&1"
                ),
                vec![format!("{run_directory}/tinker.log"), format!("{run_directory}/system.arc")],
            ),
        ],
        warnings: vec!["Tinker CLI 交互较多；真实运行前建议设置 TINKER_COMMAND 为已验证的非交互命令。".to_string()],
        scope_note: "Tinker/Tinker-HP external command template with editable key file.".to_string(),
    }
}

fn amber_pmemd_preview_spec(plan: &SimulationPlan, run_directory: &str) -> PreviewRunSpec {
    PreviewRunSpec {
        display_name: "AMBER pmemd".to_string(),
        generated_slug: "amber_pmemd".to_string(),
        run_script_name: "run-amber-pmemd.sh".to_string(),
        files: vec![preview_file("generated/amber_pmemd/prod.mdin", "amber", ambertools_prod_mdin(plan))],
        commands: vec![
            preview_command(
                "amber-pmemd-env",
                "检测用户授权 AMBER pmemd",
                "command -v pmemd.cuda >/dev/null 2>&1 || command -v pmemd >/dev/null 2>&1".to_string(),
                Vec::new(),
            ),
            preview_command(
                "amber-pmemd-run",
                "运行用户授权 AMBER pmemd",
                format!(
                    "PMEMD_BIN=\"${{PMEMD_BIN:-$(command -v pmemd.cuda || command -v pmemd)}}\"; \"$PMEMD_BIN\" -O -i generated/amber_pmemd/prod.mdin -o {run_directory}/prod.out -p generated/ambertools/system.prmtop -c generated/ambertools/system.inpcrd -r {run_directory}/prod.rst7 -x trajectories/amber-pmemd-prod.nc"
                ),
                vec![format!("{run_directory}/prod.out"), format!("{run_directory}/prod.rst7"), "trajectories/amber-pmemd-prod.nc".to_string()],
            ),
        ],
        warnings: vec!["AMBER pmemd/pmemd.cuda 需要用户自带 AMBER 授权；模板复用 AmberTools 生成的 prmtop/inpcrd。".to_string()],
        scope_note: "Licensed AMBER pmemd entrypoint. AutoMD only calls a user-provided executable.".to_string(),
    }
}

fn charmm_preview_spec(run_directory: &str) -> PreviewRunSpec {
    PreviewRunSpec {
        display_name: "CHARMM".to_string(),
        generated_slug: "charmm".to_string(),
        run_script_name: "run-charmm.sh".to_string(),
        files: vec![preview_file(
            "generated/charmm/automd.inp",
            "charmm",
            charmm_input(run_directory),
        )],
        commands: vec![
            preview_command(
                "charmm-env",
                "检测用户授权 CHARMM",
                "command -v charmm >/dev/null 2>&1".to_string(),
                Vec::new(),
            ),
            preview_command(
                "charmm-run",
                "运行用户授权 CHARMM",
                format!("charmm -i generated/charmm/automd.inp -o {run_directory}/charmm.log"),
                vec![
                    format!("{run_directory}/charmm.log"),
                    format!("{run_directory}/prod.dcd"),
                ],
            ),
        ],
        warnings: vec![
            "CHARMM 是用户自带授权模块；输入脚本需要按本地 topology/parameter 文件编辑。"
                .to_string(),
        ],
        scope_note: "Licensed CHARMM entrypoint with native input script.".to_string(),
    }
}

fn desmond_preview_spec(run_directory: &str) -> PreviewRunSpec {
    PreviewRunSpec {
        display_name: "Desmond".to_string(),
        generated_slug: "desmond".to_string(),
        run_script_name: "run-desmond.sh".to_string(),
        files: vec![preview_file("generated/desmond/automd.cfg", "ini", desmond_cfg())],
        commands: vec![preview_command(
            "desmond-run",
            "运行用户授权 Desmond 命令",
            format!(
                ": \"${{DESMOND_COMMAND:?Set DESMOND_COMMAND to the licensed Desmond launch command}}\"; $DESMOND_COMMAND > {run_directory}/desmond.log 2>&1"
            ),
            vec![format!("{run_directory}/desmond.log")],
        )],
        warnings: vec!["Desmond/Schrodinger 属商业授权环境；AutoMD 只保存 cfg 和用户提供的命令入口。".to_string()],
        scope_note: "Commercial Desmond entrypoint. Users provide DESMOND_COMMAND from their licensed environment.".to_string(),
    }
}

fn acemd_preview_spec(run_directory: &str) -> PreviewRunSpec {
    PreviewRunSpec {
        display_name: "ACEMD".to_string(),
        generated_slug: "acemd".to_string(),
        run_script_name: "run-acemd.sh".to_string(),
        files: vec![preview_file("generated/acemd/input", "text", acemd_input())],
        commands: vec![
            preview_command(
                "acemd-env",
                "检测用户授权 ACEMD",
                "command -v acemd >/dev/null 2>&1".to_string(),
                Vec::new(),
            ),
            preview_command(
                "acemd-run",
                "运行用户授权 ACEMD",
                format!("ACEMD_BIN=\"${{ACEMD_BIN:-$(command -v acemd)}}\"; \"$ACEMD_BIN\" --input generated/acemd/input > {run_directory}/acemd.log 2>&1"),
                vec![format!("{run_directory}/acemd.log"), format!("{run_directory}/output.dcd")],
            ),
        ],
        warnings: vec!["ACEMD 是用户自带授权模块；AutoMD 不分发商业二进制。".to_string()],
        scope_note: "Commercial ACEMD entrypoint with editable input template.".to_string(),
    }
}

fn write_files(project_path: &str, files: &mut [EngineRunFile]) -> Result<(), EngineAdapterError> {
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

/// Whether a normalized plan stage is enabled (missing stage defaults to enabled).
fn plan_stage_enabled(plan: &SimulationPlan, stage_id: &str) -> bool {
    plan.stages
        .iter()
        .find(|stage| stage.id == stage_id)
        .map(|stage| stage.enabled)
        .unwrap_or(true)
}

fn gromacs_commands(plan: &SimulationPlan, run_directory: &str) -> Vec<EngineCommand> {
    let input_structure = plan
        .system
        .source_path
        .as_deref()
        .unwrap_or("inputs/system.pdb");
    let force_field = gromacs_force_field(&plan.force_field.protein);
    let water = gromacs_water_model(&plan.force_field.water_model);
    let solvent_box = gromacs_solvent_box(&plan.force_field.water_model);
    let box_shape = gromacs_box_shape(&plan.solvent.box_shape);
    let padding = format_number(plan.solvent.padding_nm);
    let salt = format_number(plan.solvent.ionic_strength_molar);
    let threads = plan.resources.cpu_threads.max(1);
    let mdrun_mode = if plan.resources.gpu_count > 0 {
        "auto"
    } else {
        "cpu"
    };

    let do_prepare = plan_stage_enabled(plan, "prepare");
    let do_em = plan_stage_enabled(plan, "em");
    let do_nvt = plan_stage_enabled(plan, "nvt");
    let do_npt = plan_stage_enabled(plan, "npt");
    let do_prod = plan_stage_enabled(plan, "production");
    let do_analysis = plan_stage_enabled(plan, "analysis");
    // Topology/solvation is required whenever any MD stage runs.
    let need_system = do_prepare || do_em || do_nvt || do_npt || do_prod || do_analysis;

    let mut commands = Vec::new();

    if need_system {
        commands.push(EngineCommand {
            stage_id: "prepare-pdb2gmx".to_string(),
            label: "生成 GROMACS 拓扑".to_string(),
            command: format!(
                "AUTOMD_GROMACS_FF=\"$(automd_pick_gromacs_force_field {force_field})\" && gmx pdb2gmx -ignh -f {input_structure} -o generated/gromacs/processed.gro -p generated/gromacs/topol.top -ff \"$AUTOMD_GROMACS_FF\" -water {water}"
            ),
            working_directory: ".".to_string(),
            expected_outputs: vec![
                "generated/gromacs/processed.gro".to_string(),
                "generated/gromacs/topol.top".to_string(),
            ],
        });
        commands.push(EngineCommand {
            stage_id: "prepare-box".to_string(),
            label: "构建周期性盒子".to_string(),
            command: format!(
                "gmx editconf -f generated/gromacs/processed.gro -o generated/gromacs/boxed.gro -bt {box_shape} -d {padding}"
            ),
            working_directory: ".".to_string(),
            expected_outputs: vec!["generated/gromacs/boxed.gro".to_string()],
        });
        commands.push(EngineCommand {
            stage_id: "prepare-solvate".to_string(),
            label: "加水".to_string(),
            command: format!(
                "gmx solvate -cp generated/gromacs/boxed.gro -cs {solvent_box} -o generated/gromacs/solvated.gro -p generated/gromacs/topol.top"
            ),
            working_directory: ".".to_string(),
            expected_outputs: vec!["generated/gromacs/solvated.gro".to_string()],
        });
        commands.push(EngineCommand {
            stage_id: "prepare-ions-tpr".to_string(),
            label: "生成离子 tpr".to_string(),
            command: "gmx grompp -f generated/gromacs/ions.mdp -c generated/gromacs/solvated.gro -p generated/gromacs/topol.top -o generated/gromacs/ions.tpr -maxwarn 1".to_string(),
            working_directory: ".".to_string(),
            expected_outputs: vec!["generated/gromacs/ions.tpr".to_string()],
        });
        let mut genion = format!(
            "printf 'SOL\\n' | gmx genion -s generated/gromacs/ions.tpr -o generated/gromacs/ions.gro -p generated/gromacs/topol.top -pname NA -nname CL"
        );
        if plan.solvent.neutralize {
            genion.push_str(" -neutral");
        }
        if plan.solvent.ionic_strength_molar > 0.0 {
            genion.push_str(&format!(" -conc {salt}"));
        }
        commands.push(EngineCommand {
            stage_id: "prepare-genion".to_string(),
            label: "中和并加离子".to_string(),
            command: genion,
            working_directory: ".".to_string(),
            expected_outputs: vec!["generated/gromacs/ions.gro".to_string()],
        });
    }

    // Track latest coordinate / checkpoint for stage chaining when intermediates are skipped.
    let mut coord = "generated/gromacs/ions.gro".to_string();
    let mut checkpoint: Option<String> = None;
    let mut restraint_ref = coord.clone();

    if do_em {
        commands.push(EngineCommand {
            stage_id: "em".to_string(),
            label: "能量最小化".to_string(),
            command: format!(
                "gmx grompp -f generated/gromacs/em.mdp -c {coord} -p generated/gromacs/topol.top -o {run_directory}/em.tpr && OMP_NUM_THREADS={threads} automd_gromacs_mdrun cpu -deffnm {run_directory}/em -ntomp {threads}"
            ),
            working_directory: ".".to_string(),
            expected_outputs: vec![
                format!("{run_directory}/em.gro"),
                format!("{run_directory}/em.log"),
                format!("{run_directory}/em.edr"),
            ],
        });
        coord = format!("{run_directory}/em.gro");
        restraint_ref = coord.clone();
        checkpoint = None;
    }

    if do_nvt {
        commands.push(EngineCommand {
            stage_id: "nvt".to_string(),
            label: "NVT 平衡".to_string(),
            command: format!(
                "gmx grompp -f generated/gromacs/nvt.mdp -c {coord} -r {restraint_ref} -p generated/gromacs/topol.top -o {run_directory}/nvt.tpr && OMP_NUM_THREADS={threads} automd_gromacs_mdrun {mdrun_mode} -deffnm {run_directory}/nvt -ntomp {threads}"
            ),
            working_directory: ".".to_string(),
            expected_outputs: vec![
                format!("{run_directory}/nvt.gro"),
                format!("{run_directory}/nvt.cpt"),
                format!("{run_directory}/nvt.log"),
            ],
        });
        coord = format!("{run_directory}/nvt.gro");
        restraint_ref = coord.clone();
        checkpoint = Some(format!("{run_directory}/nvt.cpt"));
    }

    if do_npt {
        let t_flag = checkpoint
            .as_ref()
            .map(|path| format!(" -t {path}"))
            .unwrap_or_default();
        commands.push(EngineCommand {
            stage_id: "npt".to_string(),
            label: "NPT 平衡".to_string(),
            command: format!(
                "gmx grompp -f generated/gromacs/npt.mdp -c {coord} -r {restraint_ref}{t_flag} -p generated/gromacs/topol.top -o {run_directory}/npt.tpr && OMP_NUM_THREADS={threads} automd_gromacs_mdrun {mdrun_mode} -deffnm {run_directory}/npt -ntomp {threads}"
            ),
            working_directory: ".".to_string(),
            expected_outputs: vec![
                format!("{run_directory}/npt.gro"),
                format!("{run_directory}/npt.cpt"),
                format!("{run_directory}/npt.log"),
            ],
        });
        coord = format!("{run_directory}/npt.gro");
        checkpoint = Some(format!("{run_directory}/npt.cpt"));
    }

    if do_prod {
        let t_flag = checkpoint
            .as_ref()
            .map(|path| format!(" -t {path}"))
            .unwrap_or_default();
        commands.push(EngineCommand {
            stage_id: "production".to_string(),
            label: "生产模拟".to_string(),
            // Only pass -cpi when a production checkpoint already exists.
            command: format!(
                "gmx grompp -f generated/gromacs/md.mdp -c {coord}{t_flag} -p generated/gromacs/topol.top -o {run_directory}/md.tpr && if [ -f {run_directory}/md.cpt ]; then AUTOMD_MD_CPI=\"-cpi {run_directory}/md.cpt -append\"; else AUTOMD_MD_CPI=\"\"; fi && OMP_NUM_THREADS={threads} automd_gromacs_mdrun {mdrun_mode} -deffnm {run_directory}/md $AUTOMD_MD_CPI -ntomp {threads}"
            ),
            working_directory: ".".to_string(),
            expected_outputs: vec![
                format!("{run_directory}/md.xtc"),
                format!("{run_directory}/md.cpt"),
                format!("{run_directory}/md.edr"),
                format!("{run_directory}/md.log"),
            ],
        });
    }

    if do_analysis && do_prod {
        commands.push(EngineCommand {
            stage_id: "analysis".to_string(),
            label: "基础分析".to_string(),
            command: format!(
                "printf 'Backbone\\nBackbone\\n' | gmx rms -s {run_directory}/md.tpr -f {run_directory}/md.xtc -o analysis/rmsd.xvg && printf 'Backbone\\n' | gmx gyrate -s {run_directory}/md.tpr -f {run_directory}/md.xtc -o analysis/rg.xvg"
            ),
            working_directory: ".".to_string(),
            expected_outputs: vec!["analysis/rmsd.xvg".to_string(), "analysis/rg.xvg".to_string()],
        });
    }

    commands
}

fn openmm_commands(plan: &SimulationPlan, run_directory: &str) -> Vec<EngineCommand> {
    // Runner itself honors stage.enabled for nvt/npt; skip launch only if every MD stage is off.
    let any_md = ["em", "nvt", "npt", "production", "prepare", "analysis"]
        .iter()
        .any(|id| plan_stage_enabled(plan, id));
    if !any_md {
        return Vec::new();
    }
    vec![
        EngineCommand {
            stage_id: "openmm-env".to_string(),
            label: "检测 OpenMM Python 环境".to_string(),
            command: "python -c \"import openmm; print(openmm.version.version)\"".to_string(),
            working_directory: ".".to_string(),
            expected_outputs: Vec::new(),
        },
        EngineCommand {
            stage_id: "openmm-run".to_string(),
            label: "运行 OpenMM workflow".to_string(),
            command: format!(
                "python generated/openmm/run_openmm.py --plan generated/openmm/automd-plan.json --out {run_directory}"
            ),
            working_directory: ".".to_string(),
            expected_outputs: vec![
                format!("{run_directory}/openmm.chk"),
                "checkpoints/openmm.chk".to_string(),
                "trajectories/openmm.dcd".to_string(),
                "trajectories/openmm-final.pdb".to_string(),
                "analysis/openmm_state.csv".to_string(),
            ],
        },
    ]
}

fn ambertools_commands(plan: &SimulationPlan, run_directory: &str) -> Vec<EngineCommand> {
    let do_prepare = plan_stage_enabled(plan, "prepare");
    let do_em = plan_stage_enabled(plan, "em");
    let do_nvt = plan_stage_enabled(plan, "nvt");
    let do_npt = plan_stage_enabled(plan, "npt");
    let do_prod = plan_stage_enabled(plan, "production");
    let do_analysis = plan_stage_enabled(plan, "analysis");
    let need_topo = do_prepare || do_em || do_nvt || do_npt || do_prod || do_analysis;

    let mut commands = Vec::new();
    if need_topo {
        commands.push(EngineCommand {
            stage_id: "ambertools-env".to_string(),
            label: "检测 AmberTools 命令行工具".to_string(),
            command: "tleap -h >/dev/null 2>&1 && sander -h >/dev/null 2>&1 && cpptraj -h >/dev/null 2>&1".to_string(),
            working_directory: ".".to_string(),
            expected_outputs: Vec::new(),
        });
        commands.push(EngineCommand {
            stage_id: "ambertools-tleap".to_string(),
            label: "生成 AMBER topology/restart".to_string(),
            command: "tleap -f generated/ambertools/tleap.in".to_string(),
            working_directory: ".".to_string(),
            expected_outputs: vec![
                "generated/ambertools/system.prmtop".to_string(),
                "generated/ambertools/system.inpcrd".to_string(),
            ],
        });
        if plan.solvent.ionic_strength_molar > 0.0 {
            commands.push(EngineCommand {
                stage_id: "ambertools-salt".to_string(),
                label: "按浓度估算并添加盐离子".to_string(),
                command: "python3 generated/ambertools/add_salt.py && tleap -f generated/ambertools/tleap_salt.in".to_string(),
                working_directory: ".".to_string(),
                expected_outputs: vec![
                    "generated/ambertools/tleap_salt.in".to_string(),
                    "generated/ambertools/system.prmtop".to_string(),
                    "generated/ambertools/system.inpcrd".to_string(),
                    "generated/ambertools/salt_report.json".to_string(),
                ],
            });
        }
    }

    let mut coord = "generated/ambertools/system.inpcrd".to_string();
    let mut ref_coord = coord.clone();

    if do_em {
        commands.push(EngineCommand {
            stage_id: "ambertools-min".to_string(),
            label: "sander 能量最小化".to_string(),
            command: format!(
                "sander -O -i generated/ambertools/min.mdin -o {run_directory}/min.out -p generated/ambertools/system.prmtop -c {coord} -r {run_directory}/min.rst7 -x {run_directory}/min.nc"
            ),
            working_directory: ".".to_string(),
            expected_outputs: vec![
                format!("{run_directory}/min.out"),
                format!("{run_directory}/min.rst7"),
            ],
        });
        coord = format!("{run_directory}/min.rst7");
        ref_coord = coord.clone();
    }

    if do_nvt {
        commands.push(EngineCommand {
            stage_id: "ambertools-heat".to_string(),
            label: "sander NVT 加热".to_string(),
            command: format!(
                "sander -O -i generated/ambertools/heat.mdin -o {run_directory}/heat.out -p generated/ambertools/system.prmtop -c {coord} -r {run_directory}/heat.rst7 -x {run_directory}/heat.nc -ref {ref_coord}"
            ),
            working_directory: ".".to_string(),
            expected_outputs: vec![
                format!("{run_directory}/heat.out"),
                format!("{run_directory}/heat.rst7"),
            ],
        });
        coord = format!("{run_directory}/heat.rst7");
        ref_coord = coord.clone();
    }

    if do_npt {
        commands.push(EngineCommand {
            stage_id: "ambertools-equil".to_string(),
            label: "sander NPT 平衡".to_string(),
            command: format!(
                "sander -O -i generated/ambertools/equil.mdin -o {run_directory}/equil.out -p generated/ambertools/system.prmtop -c {coord} -r {run_directory}/equil.rst7 -x {run_directory}/equil.nc -ref {ref_coord}"
            ),
            working_directory: ".".to_string(),
            expected_outputs: vec![
                format!("{run_directory}/equil.out"),
                format!("{run_directory}/equil.rst7"),
            ],
        });
        coord = format!("{run_directory}/equil.rst7");
    }

    if do_prod {
        commands.push(EngineCommand {
            stage_id: "ambertools-prod".to_string(),
            label: "sander 生产模拟".to_string(),
            command: format!(
                "sander -O -i generated/ambertools/prod.mdin -o {run_directory}/prod.out -p generated/ambertools/system.prmtop -c {coord} -r {run_directory}/prod.rst7 -x trajectories/ambertools-prod.nc"
            ),
            working_directory: ".".to_string(),
            expected_outputs: vec![
                "trajectories/ambertools-prod.nc".to_string(),
                format!("{run_directory}/prod.rst7"),
            ],
        });
    }

    if do_analysis && do_prod {
        commands.push(EngineCommand {
            stage_id: "ambertools-analysis".to_string(),
            label: "cpptraj 基础 RMSD/Rg 分析".to_string(),
            command: "cpptraj -i generated/ambertools/cpptraj.in".to_string(),
            working_directory: ".".to_string(),
            expected_outputs: vec![
                "analysis/amber_rmsd.xvg".to_string(),
                "analysis/amber_rg.xvg".to_string(),
            ],
        });
    }

    commands
}

fn namd_commands(run_directory: &str, cpu_threads: u16) -> Vec<EngineCommand> {
    let threads = cpu_threads.max(1);
    vec![
        EngineCommand {
            stage_id: "namd-env".to_string(),
            label: "检测用户安装的 NAMD".to_string(),
            command: "command -v namd3 >/dev/null 2>&1 || command -v namd2 >/dev/null 2>&1".to_string(),
            working_directory: ".".to_string(),
            expected_outputs: Vec::new(),
        },
        EngineCommand {
            stage_id: "namd-run".to_string(),
            label: "运行用户安装的 NAMD".to_string(),
            command: format!(
                "NAMD_BIN=\"${{NAMD_BIN:-$(command -v namd3 || command -v namd2)}}\"; \"$NAMD_BIN\" +p{threads} generated/namd/automd.conf > {run_directory}/namd.log 2>&1"
            ),
            working_directory: ".".to_string(),
            expected_outputs: vec![
                format!("{run_directory}/namd.log"),
                format!("{run_directory}/prod.dcd"),
                format!("{run_directory}/prod.restart.coor"),
                format!("{run_directory}/prod.restart.xsc"),
            ],
        },
    ]
}

fn ambertools_tleap(plan: &SimulationPlan) -> String {
    let input_structure = plan
        .system
        .source_path
        .as_deref()
        .unwrap_or("inputs/system.pdb");
    let water = amber_water_model(&plan.force_field.water_model);
    // LEaP solvate* distance is in Angstroms; AutoMD stores padding in nm.
    let padding_angstrom = format_number(plan.solvent.padding_nm * 10.0);
    let force_field = amber_force_field(&plan.force_field.protein);
    let solvate_command = if matches!(
        plan.solvent.box_shape.as_str(),
        "octahedron" | "dodecahedron"
    ) {
        "solvateoct"
    } else {
        "solvatebox"
    };
    let ligand_block = if plan.system.has_ligand {
        "# TODO: load ligand mol2/frcmod before combining the system.\n# loadamberparams inputs/ligand.frcmod\n# ligand = loadmol2 inputs/ligand.mol2\n"
    } else {
        ""
    };
    let neutralize_block = if plan.solvent.neutralize {
        "addions system Na+ 0\naddions system Cl- 0\n"
    } else {
        ""
    };
    let salt_note = if plan.solvent.ionic_strength_molar > 0.0 {
        format!(
            "# Target ionic strength {} M will be applied after solvate via add_salt.py (n ≈ C * n_water / 55.5).\n",
            format_number(plan.solvent.ionic_strength_molar)
        )
    } else {
        String::new()
    };

    format!(
        r#"source {force_field}
source leaprc.water.{water}
{ligand_block}
system = loadpdb {input_structure}
# padding_nm={padding_nm} -> {padding_angstrom} Angstrom for LEaP
{solvate_command} system {water_box} {padding_angstrom}
{salt_note}{neutralize_block}saveamberparm system generated/ambertools/system.prmtop generated/ambertools/system.inpcrd
savepdb system generated/ambertools/system_solvated.pdb
quit
"#,
        padding_nm = format_number(plan.solvent.padding_nm),
        water_box = match water {
            "tip4pew" => "TIP4PEWBOX",
            "spce" => "SPCBOX",
            "opc" => "OPCBOX",
            _ => "TIP3PBOX",
        },
    )
}

/// Count solvent waters in the solvated PDB and emit a second tleap script that adds
/// 1:1 monovalent salt pairs using n = round(C_M * n_water / 55.5).
fn ambertools_add_salt_py(plan: &SimulationPlan) -> String {
    let conc = plan.solvent.ionic_strength_molar;
    let force_field = amber_force_field(&plan.force_field.protein);
    let water = amber_water_model(&plan.force_field.water_model);
    format!(
        r#"#!/usr/bin/env python3
"""Estimate monovalent salt ions from water count and write tleap_salt.in.

For aqueous 1:1 electrolytes (NaCl), pure water is ~55.5 M, so
    n_pairs ≈ round(C_M * n_water / 55.5)
after neutralization. This matches common MD workshop practice when LEaP has no
direct molarity keyword.
"""
from __future__ import annotations

import json
import math
import re
from pathlib import Path

CONC_M = {conc}
FORCE_FIELD = {force_field_json}
WATER_LEAP = {water_json}
SOLVATED_PDB = Path("generated/ambertools/system_solvated.pdb")
PRMTOP = Path("generated/ambertools/system.prmtop")
INPCRD = Path("generated/ambertools/system.inpcrd")
TLEAP_SALT = Path("generated/ambertools/tleap_salt.in")
REPORT = Path("generated/ambertools/salt_report.json")

WATER_RES = re.compile(
    r"^(ATOM  |HETATM).{{11}}(?:WAT|HOH|TIP3|TIP4|SPC|T3P|T4P|OPC)\b",
    re.IGNORECASE,
)


def count_waters(pdb_path: Path) -> int:
    if not pdb_path.is_file():
        raise SystemExit(f"solvated PDB not found: {{pdb_path}}")
    residues = set()
    for line in pdb_path.read_text(encoding="utf-8", errors="replace").splitlines():
        if not WATER_RES.match(line):
            continue
        # residue id: chain + resseq + insertion (PDB columns 22-27)
        key = line[21:27]
        residues.add(key)
    return len(residues)


def main() -> int:
    n_water = count_waters(SOLVATED_PDB)
    # Pure water molarity ≈ 55.5 mol/L; monovalent 1:1 salt pairs.
    n_pairs = int(round(CONC_M * n_water / 55.5)) if CONC_M > 0 else 0
    n_pairs = max(0, n_pairs)
    report = {{
        "ionicStrengthMolar": CONC_M,
        "waterMolecules": n_water,
        "saltPairs": n_pairs,
        "method": "n_pairs = round(C_M * n_water / 55.5)",
        "cations": "Na+",
        "anions": "Cl-",
    }}
    REPORT.parent.mkdir(parents=True, exist_ok=True)
    REPORT.write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(
        f"[AutoMD] salt estimate: C={{CONC_M}} M, n_water={{n_water}}, n_pairs={{n_pairs}}",
        flush=True,
    )
    ion_block = ""
    if n_pairs > 0:
        ion_block = f"addions system Na+ {{n_pairs}}\naddions system Cl- {{n_pairs}}\n"
    TLEAP_SALT.write_text(
        f"""source {{FORCE_FIELD}}
source leaprc.water.{{WATER_LEAP}}
system = loadamberparm {{PRMTOP.as_posix()}} {{INPCRD.as_posix()}}
{{ion_block}}saveamberparm system {{PRMTOP.as_posix()}} {{INPCRD.as_posix()}}
savepdb system generated/ambertools/system_solvated.pdb
quit
""",
        encoding="utf-8",
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
"#,
        force_field_json = serde_json::to_string(force_field).unwrap_or_else(|_| "\"leaprc.protein.ff19SB\"".into()),
        water_json = serde_json::to_string(water).unwrap_or_else(|_| "\"tip3p\"".into()),
    )
}

fn ambertools_min_mdin() -> String {
    r#"Minimize AutoMD system
&cntrl
  imin=1, maxcyc=5000, ncyc=2500,
  cut=10.0, ntb=1,
  ntpr=100, ioutfm=1, ntxo=2,
/
"#
    .to_string()
}

fn ambertools_heat_mdin(plan: &SimulationPlan) -> String {
    let temperature = stage_parameter(plan, "nvt", "temperatureK").unwrap_or("300");
    let duration_ps = stage_parameter(plan, "nvt", "durationPs")
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(100.0);
    let timestep_fs = stage_parameter(plan, "production", "timestepFs")
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(2.0);
    let nstlim = nsteps_from_ps_f64(duration_ps, timestep_fs);
    format!(
        r#"Heat AutoMD system
&cntrl
  imin=0, irest=0, ntx=1,
  nstlim={nstlim}, dt={dt},
  ntc=2, ntf=2,
  cut=10.0, ntb=1,
  ntt=3, gamma_ln=2.0,
  tempi=0.0, temp0={temperature},
  ntpr=500, ntwx=500, ntwr=5000, ioutfm=1, ntxo=2,
  ntr=1, restraint_wt=10.0, restraintmask='!:WAT,Na+,Cl-',
/
"#,
        dt = format_number_f64(timestep_fs / 1000.0),
    )
}

fn ambertools_equil_mdin(plan: &SimulationPlan) -> String {
    let temperature = stage_parameter(plan, "npt", "temperatureK")
        .or_else(|| stage_parameter(plan, "nvt", "temperatureK"))
        .unwrap_or("300");
    let pressure = stage_parameter(plan, "npt", "pressureBar").unwrap_or("1.0");
    let duration_ps = stage_parameter(plan, "npt", "durationPs")
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(1000.0);
    let timestep_fs = stage_parameter(plan, "production", "timestepFs")
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(2.0);
    let nstlim = nsteps_from_ps_f64(duration_ps, timestep_fs);
    format!(
        r#"Equilibrate AutoMD system
&cntrl
  imin=0, irest=1, ntx=5,
  nstlim={nstlim}, dt={dt},
  ntc=2, ntf=2,
  cut=10.0, ntb=2, ntp=1, pres0={pressure}, taup=2.0,
  ntt=3, gamma_ln=2.0, temp0={temperature},
  ntpr=1000, ntwx=1000, ntwr=5000, ioutfm=1, ntxo=2,
  ntr=1, restraint_wt=2.0, restraintmask='!:WAT,Na+,Cl-',
/
"#,
        dt = format_number_f64(timestep_fs / 1000.0),
    )
}

fn ambertools_prod_mdin(plan: &SimulationPlan) -> String {
    let temperature = stage_parameter(plan, "npt", "temperatureK")
        .or_else(|| stage_parameter(plan, "nvt", "temperatureK"))
        .unwrap_or("300");
    let pressure = stage_parameter(plan, "npt", "pressureBar").unwrap_or("1.0");
    let seed = stage_parameter(plan, "production", "randomSeed")
        .or_else(|| stage_parameter(plan, "nvt", "velocitySeed"))
        .unwrap_or("-1");
    let duration_ns = stage_parameter(plan, "production", "durationNs")
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(100.0);
    let timestep_fs = stage_parameter(plan, "production", "timestepFs")
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(2.0);
    let nstlim = nsteps_from_ps_f64(duration_ns * 1000.0, timestep_fs);
    format!(
        r#"Production AutoMD system
&cntrl
  imin=0, irest=1, ntx=5,
  nstlim={nstlim}, dt={dt},
  ntc=2, ntf=2,
  cut=10.0, ntb=2, ntp=1, pres0={pressure}, taup=2.0,
  ntt=3, gamma_ln=2.0, temp0={temperature}, ig={seed},
  ntpr=1000, ntwx=1000, ntwr=5000, ioutfm=1, ntxo=2,
/
"#,
        dt = format_number_f64(timestep_fs / 1000.0),
        seed = seed,
    )
}

fn ambertools_cpptraj(_run_directory: &str) -> String {
    r#"parm generated/ambertools/system.prmtop
trajin trajectories/ambertools-prod.nc
autoimage
rms first :1-999&!@H= out analysis/amber_rmsd.xvg
radgyr :1-999 out analysis/amber_rg.xvg
run
"#
    .to_string()
}

fn namd_conf(plan: &SimulationPlan, run_directory: &str) -> String {
    let temperature = stage_parameter(plan, "nvt", "temperatureK").unwrap_or("300");
    let pressure = stage_parameter(plan, "npt", "pressureBar").unwrap_or("1.01325");
    let timestep_fs = stage_parameter(plan, "production", "timestepFs").unwrap_or("2");
    let duration_ns = stage_parameter(plan, "production", "durationNs")
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(1.0);
    let steps = nsteps_from_ps_f64(
        duration_ns * 1000.0,
        timestep_fs.parse::<f64>().unwrap_or(2.0),
    );
    let npt_enabled = plan
        .stages
        .iter()
        .any(|stage| stage.id == "npt" && stage.enabled);
    let pressure_block = if npt_enabled {
        format!(
            r#"
# NPT pressure control (Langevin piston)
useGroupPressure      yes
useFlexibleCell       no
useConstantArea       no
langevinPiston        on
langevinPistonTarget  {pressure}
langevinPistonPeriod  100.0
langevinPistonDecay   50.0
langevinPistonTemp    {temperature}
"#
        )
    } else {
        "\n# NPT stage disabled; running without langevinPiston (NVT-like).\n".to_string()
    };
    format!(
        r#"# AutoMD generated NAMD configuration.
# User must provide compatible PSF/PDB files, cell basis vectors, and satisfy NAMD license terms.

structure          inputs/system.psf
coordinates        {coordinates}
parameters         inputs/par_all36m_prot.prm
paraTypeCharmm     on
set outputname     {run_directory}/prod

# Provide a periodic cell when using PME, e.g.:
# cellBasisVector1  a 0 0
# cellBasisVector2  0 b 0
# cellBasisVector3  0 0 c
# cellOrigin        0 0 0
# wrapAll           on

temperature        {temperature}
timestep           {timestep_fs}
numsteps           {steps}
exclude            scaled1-4
1-4scaling         1.0
cutoff             12.0
switching          on
switchdist         10.0
pairlistdist       14.0

PME                yes
PMEGridSpacing     1.0

langevin           on
langevinDamping    1.0
langevinTemp       {temperature}
langevinHydrogen   off
{pressure_block}
outputName         $outputname
restartname        $outputname.restart
DCDfile            $outputname.dcd
binaryrestart      yes
restartfreq        5000
dcdFreq            1000
xstFreq            1000
outputEnergies     1000

minimize           5000
reinitvels         {temperature}
run                {steps}
"#,
        coordinates = plan
            .system
            .source_path
            .as_deref()
            .unwrap_or("inputs/system.pdb"),
    )
}

fn preview_run_script(plan: &SimulationPlan, spec: &PreviewRunSpec) -> String {
    let body = spec
        .commands
        .iter()
        .map(|command| {
            format!(
                r#"echo "[AutoMD] {label}"
{command}
"#,
                label = command.label,
                command = command.command
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"#!/usr/bin/env bash
set -euo pipefail

echo "AutoMD {display_name} run: {name}"
echo "Plan id: {plan_id}"
mkdir -p generated/{generated_slug} runs analysis reports checkpoints trajectories
mkdir -p {run_directory}

{body}

echo "[AutoMD] {display_name} workflow completed"
"#,
        display_name = spec.display_name,
        name = plan.name,
        plan_id = plan.id,
        generated_slug = spec.generated_slug,
        run_directory = format!(
            "runs/{}-{}",
            plan.engine_id.replace('_', "-"),
            plan.id.simple()
        ),
    )
}

fn preview_run_readme(plan: &SimulationPlan, spec: &PreviewRunSpec) -> String {
    let warnings_md = if spec.warnings.is_empty() {
        "- No warnings.\n".to_string()
    } else {
        spec.warnings
            .iter()
            .map(|warning| format!("- {warning}\n"))
            .collect::<String>()
    };
    let native_files = spec
        .files
        .iter()
        .map(|file| format!("- `{}`\n", file.path))
        .collect::<String>();

    format!(
        r#"# AutoMD {display_name} Run Package

Plan: `{name}`

## Native Files

{native_files}
- `generated/{generated_slug}/automd-plan.json`
- `{run_script_name}`

## Scope

{scope_note}

## Warnings

{warnings_md}
"#,
        display_name = spec.display_name,
        name = plan.name,
        native_files = native_files,
        generated_slug = spec.generated_slug,
        run_script_name = spec.run_script_name,
        scope_note = spec.scope_note,
    )
}

fn lammps_input(plan: &SimulationPlan, run_directory: &str) -> String {
    let timestep_fs = stage_parameter(plan, "production", "timestepFs").unwrap_or("1");
    let duration_ns = stage_parameter(plan, "production", "durationNs")
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(1.0);
    let nsteps = nsteps_from_ps(
        duration_ns * 1000.0,
        timestep_fs.parse::<f32>().unwrap_or(1.0),
    );
    let temperature = stage_parameter(plan, "nvt", "temperatureK").unwrap_or("300");
    format!(
        r#"# AutoMD generated LAMMPS template.
# Replace inputs/system.data and force-field coefficients before real runs.

units           real
atom_style      full
boundary        p p p
read_data       inputs/system.data

pair_style      lj/cut/coul/long 10.0
kspace_style    pppm 1.0e-4
neighbor        2.0 bin
neigh_modify    every 1 delay 0 check yes

timestep        {timestep_fs}
thermo          1000
thermo_style    custom step temp press pe ke etotal
dump            automd all custom 1000 {run_directory}/dump.lammpstrj id type x y z

minimize        1.0e-4 1.0e-6 1000 10000
velocity        all create {temperature} 4928459 mom yes rot yes dist gaussian
fix             nvt_all all nvt temp {temperature} {temperature} 100.0
run             {nsteps}
unfix           nvt_all
write_restart   {run_directory}/lammps.restart
"#
    )
}

fn cp2k_input(plan: &SimulationPlan, run_directory: &str) -> String {
    let timestep_fs = stage_parameter(plan, "production", "timestepFs").unwrap_or("1");
    let duration_ns = stage_parameter(plan, "production", "durationNs")
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(0.001);
    let nsteps = nsteps_from_ps(
        duration_ns * 1000.0,
        timestep_fs.parse::<f32>().unwrap_or(1.0),
    );
    let temperature = stage_parameter(plan, "nvt", "temperatureK").unwrap_or("300");
    format!(
        r#"# AutoMD generated CP2K template. Review basis, potential, cell, and KIND blocks.
&GLOBAL
  PROJECT automd
  RUN_TYPE MD
  PRINT_LEVEL MEDIUM
&END GLOBAL

&MOTION
  &MD
    ENSEMBLE NVT
    STEPS {nsteps}
    TIMESTEP {timestep_fs}
    TEMPERATURE {temperature}
  &END MD
  &PRINT
    &TRAJECTORY
      FILENAME {run_directory}/cp2k-pos
    &END TRAJECTORY
    &RESTART
      FILENAME {run_directory}/automd
    &END RESTART
  &END PRINT
&END MOTION

&FORCE_EVAL
  METHOD Quickstep
  &DFT
    BASIS_SET_FILE_NAME BASIS_SET
    POTENTIAL_FILE_NAME POTENTIAL
  &END DFT
  &SUBSYS
    &CELL
      ABC 30.0 30.0 30.0
    &END CELL
    &TOPOLOGY
      COORD_FILE_NAME inputs/system.xyz
      COORD_FILE_FORMAT XYZ
    &END TOPOLOGY
    &KIND H
      BASIS_SET DZVP-MOLOPT-SR-GTH
      POTENTIAL GTH-PBE-q1
    &END KIND
  &END SUBSYS
&END FORCE_EVAL
"#
    )
}

fn genesis_input(plan: &SimulationPlan, run_directory: &str) -> String {
    let timestep_fs = stage_parameter(plan, "production", "timestepFs").unwrap_or("2");
    let duration_ns = stage_parameter(plan, "production", "durationNs")
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(1.0);
    let nsteps = nsteps_from_ps(
        duration_ns * 1000.0,
        timestep_fs.parse::<f32>().unwrap_or(2.0),
    );
    let temperature = stage_parameter(plan, "nvt", "temperatureK").unwrap_or("300");
    format!(
        r#"[INPUT]
pdbfile = inputs/system.pdb
psffile = inputs/system.psf
parfile = inputs/par_all36m_prot.prm

[OUTPUT]
dcdfile = {run_directory}/prod.dcd
rstfile = {run_directory}/prod.rst

[ENERGY]
forcefield = CHARMM
electrostatic = PME
switchdist = 10.0
cutoffdist = 12.0
pairlistdist = 13.5

[DYNAMICS]
integrator = VVER
nsteps = {nsteps}
timestep = {timestep_fs}
eneout_period = 1000
crdout_period = 1000
rstout_period = 5000

[ENSEMBLE]
ensemble = NVT
tpcontrol = LANGEVIN
temperature = {temperature}
"#
    )
}

fn hoomd_runner_py(plan: &SimulationPlan, run_directory: &str) -> String {
    let duration_ns = stage_parameter(plan, "production", "durationNs").unwrap_or("0.001");
    format!(
        r#"#!/usr/bin/env python3
from __future__ import annotations

import argparse
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser(description="AutoMD HOOMD-blue preview runner")
    parser.add_argument("--out", default="{run_directory}")
    args = parser.parse_args()
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    Path("trajectories").mkdir(exist_ok=True)

    import hoomd

    print("AutoMD HOOMD-blue preview")
    print("Requested duration ns: {duration_ns}")
    print("This template intentionally stops before constructing a topology.")
    print("Edit generated/hoomd/run_hoomd.py with a validated HOOMD simulation script.")
    print("HOOMD version:", hoomd.version.version)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
"#
    )
}

fn dl_poly_control() -> String {
    r#"AutoMD DL_POLY CONTROL template
temperature        300.0
pressure           0.001
ensemble           nvt hoover 0.5
timestep           0.001
steps              10000
equilibration      1000
print              100
trajectory         100 0 0
finish
"#
    .to_string()
}

fn dl_poly_field() -> String {
    r#"AutoMD DL_POLY FIELD template
UNITS kcal
MOLECULES 0
FINISH
"#
    .to_string()
}

fn dl_poly_config() -> String {
    r#"AutoMD DL_POLY CONFIG template
0 0
0.0 0.0 0.0
"#
    .to_string()
}

fn tinker_key() -> String {
    r#"# AutoMD Tinker key template.
# Replace parameters and input XYZ with a validated Tinker system.
parameters          inputs/params.prm
integrator          verlet
thermostat          nose-hoover
archive
"#
    .to_string()
}

fn charmm_input(run_directory: &str) -> String {
    format!(
        r#"* AutoMD CHARMM template
*
! User must provide topology/parameter and coordinate files.
open read card unit 10 name inputs/topology.rtf
read rtf card unit 10
close unit 10
open read card unit 20 name inputs/parameters.prm
read param card flex unit 20
close unit 20

open read card unit 30 name inputs/system.psf
read psf card unit 30
close unit 30
open read card unit 40 name inputs/system.crd
read coor card unit 40
close unit 40

mini sd nstep 5000
open write unit 50 file name {run_directory}/prod.dcd
dyna leap start nstep 10000 timestep 0.002 -
  firstt 300 finalt 300 tbath 300 -
  iuncrd 50 nsavc 1000
stop
"#
    )
}

fn desmond_cfg() -> String {
    r#"# AutoMD Desmond cfg placeholder.
# Set DESMOND_COMMAND in run-desmond.sh or the GUI to a command from your licensed Schrodinger environment.
# Example user-managed command might reference an inputs/system.cms file and site-specific launch options.
"#
    .to_string()
}

fn acemd_input() -> String {
    r#"# AutoMD ACEMD input template.
coordinates inputs/system.pdb
structure inputs/system.psf
parameters inputs/parameters
temperature 300
timestep 2
run 10000
trajectoryfile output.dcd
restart on
"#
    .to_string()
}

fn openmm_runner_py(plan: &SimulationPlan, run_directory: &str) -> String {
    let input_structure = plan
        .system
        .source_path
        .as_deref()
        .unwrap_or("inputs/system.pdb");
    let input_structure_json = serde_json::to_string(input_structure)
        .unwrap_or_else(|_| "\"inputs/system.pdb\"".to_string());
    let force_fields = serde_json::to_string(&openmm_force_field_files(plan))
        .unwrap_or_else(|_| "[\"amber14-all.xml\",\"amber14/tip3pfb.xml\"]".to_string());
    let padding_nm = plan.solvent.padding_nm;
    let ionic = plan.solvent.ionic_strength_molar;
    let neutralize = plan.solvent.neutralize;

    r#"#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

DEFAULT_INPUT = __AUTOMD_INPUT__
DEFAULT_FORCE_FIELDS = __AUTOMD_FORCE_FIELDS__
DEFAULT_PADDING_NM = __AUTOMD_PADDING_NM__
DEFAULT_IONIC = __AUTOMD_IONIC__
DEFAULT_NEUTRALIZE = __AUTOMD_NEUTRALIZE__


def stage_parameter(plan: dict, stage_id: str, key: str, fallback: str) -> str:
    for stage in plan.get("stages", []):
        if stage.get("id") == stage_id:
            return str(stage.get("parameters", {}).get(key, fallback))
    return fallback


def stage_bool(plan: dict, stage_id: str, key: str, fallback: bool) -> bool:
    value = stage_parameter(plan, stage_id, key, "true" if fallback else "false").strip().lower()
    return value in {"1", "true", "yes", "y", "on"}


def stage_enabled(plan: dict, stage_id: str, fallback: bool = True) -> bool:
    for stage in plan.get("stages", []):
        if stage.get("id") == stage_id:
            return bool(stage.get("enabled", fallback))
    return fallback


def resolve_project_root(plan_path: Path) -> Path:
    for parent in plan_path.parents:
        if parent.name == "generated":
            return parent.parent
    return Path.cwd()


def pick_platform():
    from openmm import Platform
    for name in ("CUDA", "OpenCL", "CPU", "Reference"):
        try:
            platform = Platform.getPlatformByName(name)
            print(f"OpenMM platform: {name}", flush=True)
            return platform
        except Exception:
            continue
    return None


def topology_has_periodic_box(topology) -> bool:
    vectors = topology.getPeriodicBoxVectors()
    return vectors is not None


def main() -> int:
    parser = argparse.ArgumentParser(description="Run an AutoMD-generated OpenMM workflow.")
    parser.add_argument("--plan", required=True, help="Path to generated/openmm/automd-plan.json")
    parser.add_argument("--out", default="__AUTOMD_RUN_DIRECTORY__", help="Run output directory")
    parser.add_argument("--resume", default=None, help="Optional OpenMM checkpoint file")
    args = parser.parse_args()

    try:
        from openmm.app import (
            CheckpointReporter,
            DCDReporter,
            ForceField,
            HBonds,
            Modeller,
            PDBFile,
            PME,
            Simulation,
            StateDataReporter,
        )
        from openmm import LangevinMiddleIntegrator, MonteCarloBarostat
        from openmm.unit import bar, kelvin, molar, nanometer, picosecond, picoseconds
    except ModuleNotFoundError as exc:
        print(f"Fatal error: OpenMM Python module not found: {exc}", file=sys.stderr, flush=True)
        return 8

    plan_path = Path(args.plan)
    project_root = resolve_project_root(plan_path)
    out_dir = Path(args.out)
    if not out_dir.is_absolute():
        out_dir = project_root / out_dir
    analysis_dir = project_root / "analysis"
    trajectories_dir = project_root / "trajectories"
    checkpoints_dir = project_root / "checkpoints"
    for directory in (out_dir, analysis_dir, trajectories_dir, checkpoints_dir):
        directory.mkdir(parents=True, exist_ok=True)

    plan = json.loads(plan_path.read_text(encoding="utf-8"))
    solvent = plan.get("solvent", {})
    source = Path(plan.get("system", {}).get("sourcePath") or DEFAULT_INPUT)
    if not source.is_absolute():
        source = project_root / source
    if not source.exists():
        print(f"Fatal error: OpenMM input file not found: {source}", file=sys.stderr, flush=True)
        return 2

    temperature = float(stage_parameter(plan, "nvt", "temperatureK", "300"))
    pressure_bar = float(stage_parameter(plan, "npt", "pressureBar", "1.0"))
    timestep_fs = float(stage_parameter(plan, "production", "timestepFs", "2"))
    duration_ns = float(stage_parameter(plan, "production", "durationNs", "0.1"))
    nvt_ps = float(stage_parameter(plan, "nvt", "durationPs", "100"))
    npt_ps = float(stage_parameter(plan, "npt", "durationPs", "1000"))
    checkpoint_ps = float(stage_parameter(plan, "production", "checkpointEveryPs", "100"))
    random_seed = int(stage_parameter(
        plan,
        "production",
        "randomSeed",
        stage_parameter(plan, "nvt", "velocitySeed", "0"),
    ))
    padding_nm = float(solvent.get("paddingNm", DEFAULT_PADDING_NM))
    ionic = float(solvent.get("ionicStrengthMolar", DEFAULT_IONIC))
    neutralize = bool(solvent.get("neutralize", DEFAULT_NEUTRALIZE))

    nvt_steps = max(1, int(round(nvt_ps * 1000 / max(timestep_fs, 0.001))))
    npt_steps = max(1, int(round(npt_ps * 1000 / max(timestep_fs, 0.001))))
    total_steps = max(1, int(round(duration_ns * 1_000_000 / max(timestep_fs, 0.001))))
    report_interval = max(1, min(total_steps, int(round(checkpoint_ps * 1000 / max(timestep_fs, 0.001)))))
    do_nvt = stage_enabled(plan, "nvt", True)
    do_npt = stage_enabled(plan, "npt", True)

    print(f"AutoMD OpenMM workflow started: {plan.get('name', 'unnamed')}", flush=True)
    print(f"OpenMM force fields: {', '.join(DEFAULT_FORCE_FIELDS)}", flush=True)
    pdb = PDBFile(str(source))
    forcefield = ForceField(*DEFAULT_FORCE_FIELDS)
    modeller = Modeller(pdb.topology, pdb.positions)
    if stage_bool(plan, "prepare", "addHydrogens", True):
        print("Stage 0: adding hydrogens with OpenMM Modeller", flush=True)
        modeller.addHydrogens(forcefield)

    if not topology_has_periodic_box(modeller.topology):
        print(
            f"Stage 0b: no periodic box on input; adding solvent (padding={padding_nm} nm, ionic={ionic} M)",
            flush=True,
        )
        try:
            modeller.addSolvent(
                forcefield,
                padding=padding_nm * nanometer,
                ionicStrength=(ionic * molar if ionic > 0 else 0 * molar),
                neutralize=neutralize,
            )
        except Exception as exc:
            print(
                f"Fatal error: input has no unit cell and automatic solvation failed: {exc}. "
                "Provide a pre-solvated structure with CRYST1/box vectors, or install compatible force-field water templates.",
                file=sys.stderr,
                flush=True,
            )
            return 3
    elif solvent.get("model", "explicit") == "explicit":
        print("Stage 0b: using periodic box from input structure", flush=True)

    topology = modeller.topology
    positions = modeller.positions
    if not topology_has_periodic_box(topology):
        print(
            "Fatal error: OpenMM PME requires a periodic box. Solvation did not produce box vectors.",
            file=sys.stderr,
            flush=True,
        )
        return 3

    system = forcefield.createSystem(
        topology,
        nonbondedMethod=PME,
        nonbondedCutoff=1 * nanometer,
        constraints=HBonds,
    )
    if do_npt:
        system.addForce(MonteCarloBarostat(pressure_bar * bar, temperature * kelvin))
        print(f"Added MonteCarloBarostat at {pressure_bar} bar, {temperature} K", flush=True)

    integrator = LangevinMiddleIntegrator(temperature * kelvin, 1 / picosecond, timestep_fs * 0.001 * picoseconds)
    if random_seed != 0:
        integrator.setRandomNumberSeed(abs(random_seed))
    platform = pick_platform()
    if platform is not None:
        simulation = Simulation(topology, system, integrator, platform)
    else:
        simulation = Simulation(topology, system, integrator)

    if args.resume:
        checkpoint_path = Path(args.resume)
        if not checkpoint_path.is_absolute():
            checkpoint_path = project_root / checkpoint_path
        with checkpoint_path.open("rb") as handle:
            simulation.context.loadCheckpoint(handle.read())
        print(f"Loaded checkpoint: {checkpoint_path}", flush=True)
    else:
        simulation.context.setPositions(positions)
        if random_seed != 0:
            simulation.context.setVelocitiesToTemperature(temperature * kelvin, abs(random_seed))
        else:
            simulation.context.setVelocitiesToTemperature(temperature * kelvin)
        print("Stage 1: energy minimization", flush=True)
        simulation.minimizeEnergy()
        if do_nvt:
            print(f"Stage 2: NVT equilibration ({nvt_steps} steps)", flush=True)
            simulation.step(nvt_steps)
        if do_npt:
            print(f"Stage 3: NPT equilibration ({npt_steps} steps)", flush=True)
            simulation.step(npt_steps)

    state_path = analysis_dir / "openmm_state.csv"
    fixed_outputs = [
        state_path,
        trajectories_dir / "openmm.dcd",
        trajectories_dir / "openmm-final.pdb",
        checkpoints_dir / "openmm.chk",
        out_dir / "openmm.chk",
    ]
    for output_path in fixed_outputs:
        try:
            output_path.unlink()
        except FileNotFoundError:
            pass

    state_handle = state_path.open("w", encoding="utf-8")
    try:
        simulation.reporters.append(StateDataReporter(
            state_handle,
            report_interval,
            step=True,
            potentialEnergy=True,
            temperature=True,
            separator=",",
        ))
        simulation.reporters.append(DCDReporter(str(trajectories_dir / "openmm.dcd"), report_interval))
        # Single checkpoint path under checkpoints/; mirror to run dir at end.
        simulation.reporters.append(CheckpointReporter(str(checkpoints_dir / "openmm.chk"), report_interval))

        completed = 0
        print("Stage 4: production", flush=True)
        while completed < total_steps:
            steps = min(report_interval, total_steps - completed)
            simulation.step(steps)
            completed += steps
            print(f"step {completed} of {total_steps}", flush=True)
    finally:
        state_handle.close()

    simulation.saveCheckpoint(str(out_dir / "openmm.chk"))
    state = simulation.context.getState(getPositions=True)
    with (trajectories_dir / "openmm-final.pdb").open("w", encoding="utf-8") as handle:
        PDBFile.writeFile(simulation.topology, state.getPositions(), handle)
    print("Stage 5: trajectory and checkpoint written", flush=True)
    print("OpenMM workflow completed", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
"#
    .replace("__AUTOMD_INPUT__", &input_structure_json)
    .replace("__AUTOMD_FORCE_FIELDS__", &force_fields)
    .replace("__AUTOMD_RUN_DIRECTORY__", run_directory)
    .replace("__AUTOMD_PADDING_NM__", &format_number(padding_nm))
    .replace("__AUTOMD_IONIC__", &format_number(ionic))
    .replace(
        "__AUTOMD_NEUTRALIZE__",
        if neutralize { "True" } else { "False" },
    )
}

fn openmm_run_script(plan: &SimulationPlan, commands: &[EngineCommand]) -> String {
    let body = commands
        .iter()
        .map(|command| {
            format!(
                r#"echo "[AutoMD] {label}"
{command}
"#,
                label = command.label,
                command = command.command
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"#!/usr/bin/env bash
set -euo pipefail

echo "AutoMD OpenMM run: {name}"
echo "Plan id: {plan_id}"
mkdir -p generated/openmm runs analysis reports checkpoints trajectories

{prelude}

{body}

echo "[AutoMD] OpenMM workflow completed"
"#,
        name = plan.name,
        plan_id = plan.id,
        prelude = openmm_shell_prelude()
    )
}

fn openmm_shell_prelude() -> &'static str {
    r#"if [ -n "${AUTOMD_OPENMM_PYTHON:-}" ] && [ -x "$AUTOMD_OPENMM_PYTHON" ]; then
  :
elif [ -x "$HOME/.automd/engines/openmm/bin/python" ]; then
  AUTOMD_OPENMM_PYTHON="$HOME/.automd/engines/openmm/bin/python"
elif [ -x "$HOME/.automd/engines/_tools/automd-science/bin/python" ]; then
  AUTOMD_OPENMM_PYTHON="$HOME/.automd/engines/_tools/automd-science/bin/python"
elif [ -x "$HOME/Library/Application Support/com.noir.automd/engines/openmm/bin/python" ]; then
  AUTOMD_OPENMM_PYTHON="$HOME/Library/Application Support/com.noir.automd/engines/openmm/bin/python"
elif [ -x "$HOME/Library/Application Support/com.noir.automd/engines/_tools/automd-science/bin/python" ]; then
  AUTOMD_OPENMM_PYTHON="$HOME/Library/Application Support/com.noir.automd/engines/_tools/automd-science/bin/python"
elif [ -x "$HOME/.local/share/com.noir.automd/engines/_tools/automd-science/bin/python" ]; then
  AUTOMD_OPENMM_PYTHON="$HOME/.local/share/com.noir.automd/engines/_tools/automd-science/bin/python"
else
  AUTOMD_OPENMM_PYTHON="$(command -v python3 || command -v python || true)"
fi
if [ -z "${AUTOMD_OPENMM_PYTHON:-}" ]; then
  echo "[AutoMD] Python/OpenMM environment not found. Register or install OpenMM on the Engines page." >&2
  exit 127
fi
python() { "$AUTOMD_OPENMM_PYTHON" "$@"; }
echo "[AutoMD] Using Python/OpenMM: $AUTOMD_OPENMM_PYTHON"
"#
}

fn openmm_run_readme(plan: &SimulationPlan, warnings: &[String]) -> String {
    let warnings_md = if warnings.is_empty() {
        "- No warnings.\n".to_string()
    } else {
        warnings
            .iter()
            .map(|warning| format!("- {warning}\n"))
            .collect::<String>()
    };
    let force_fields = openmm_force_field_files(plan).join(", ");

    format!(
        r#"# AutoMD OpenMM Run Package

Plan: `{name}`

## Files

- `generated/openmm/run_openmm.py` contains the Python application-layer runner.
- `generated/openmm/automd-plan.json` preserves the normalized AutoMD plan.
- `run-openmm.sh` checks the Python OpenMM environment and launches the runner.

## Force Field XML

{force_fields}

## Warnings

{warnings_md}
"#,
        name = plan.name,
    )
}

fn ambertools_run_script(plan: &SimulationPlan, commands: &[EngineCommand]) -> String {
    let body = commands
        .iter()
        .map(|command| {
            format!(
                r#"echo "[AutoMD] {label}"
{command}
"#,
                label = command.label,
                command = command.command
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"#!/usr/bin/env bash
set -euo pipefail

echo "AutoMD AmberTools run: {name}"
echo "Plan id: {plan_id}"
mkdir -p generated/ambertools runs analysis reports checkpoints trajectories

{body}

echo "[AutoMD] AmberTools workflow completed"
"#,
        name = plan.name,
        plan_id = plan.id
    )
}

fn ambertools_run_readme(plan: &SimulationPlan, warnings: &[String]) -> String {
    let warnings_md = if warnings.is_empty() {
        "- No warnings.\n".to_string()
    } else {
        warnings
            .iter()
            .map(|warning| format!("- {warning}\n"))
            .collect::<String>()
    };

    format!(
        r#"# AutoMD AmberTools Run Package

Plan: `{name}`

## Files

- `generated/ambertools/tleap.in` builds `system.prmtop` and `system.inpcrd`.
- `generated/ambertools/*.mdin` contains minimization, heating, equilibration, and production settings for `sander`.
- `generated/ambertools/cpptraj.in` imports the production trajectory and writes RMSD/Rg analysis artifacts.
- `generated/ambertools/automd-plan.json` preserves the normalized AutoMD plan.
- `run-ambertools.sh` checks `tleap`, `sander`, and `cpptraj`, then runs the generated workflow.

## Scientific Scope

This package is conservative and CPU-oriented. Ligands, cofactors, membranes, and unusual residues still require validated Amber-compatible parameters before the run is scientifically meaningful.

## Warnings

{warnings_md}
"#,
        name = plan.name,
    )
}

fn namd_run_script(plan: &SimulationPlan, commands: &[EngineCommand]) -> String {
    let body = commands
        .iter()
        .map(|command| {
            format!(
                r#"echo "[AutoMD] {label}"
{command}
"#,
                label = command.label,
                command = command.command
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"#!/usr/bin/env bash
set -euo pipefail

echo "AutoMD NAMD external run: {name}"
echo "Plan id: {plan_id}"
mkdir -p generated/namd runs analysis reports checkpoints trajectories

{body}

echo "[AutoMD] NAMD workflow completed"
"#,
        name = plan.name,
        plan_id = plan.id
    )
}

fn namd_run_readme(plan: &SimulationPlan, warnings: &[String]) -> String {
    let warnings_md = warnings
        .iter()
        .map(|warning| format!("- {warning}\n"))
        .collect::<String>();

    format!(
        r#"# AutoMD NAMD External Run Package

Plan: `{name}`

## Files

- `generated/namd/automd.conf` is an editable NAMD configuration template.
- `generated/namd/automd-plan.json` preserves the normalized AutoMD plan.
- `run-namd.sh` detects `namd3` or `namd2`, honors `NAMD_BIN` when set, and writes `namd.log`.

## User-Managed Requirements

AutoMD does not download, bundle, mirror, or license NAMD. The user must provide a valid NAMD installation and compatible PSF/PDB/parameter files such as `inputs/system.psf`, `inputs/system.pdb`, and `inputs/par_all36m_prot.prm`.

## Warnings

{warnings_md}
"#,
        name = plan.name,
    )
}

fn gromacs_mdp_file(plan: &SimulationPlan, stage_id: &str, path: &str) -> EngineRunFile {
    EngineRunFile {
        path: path.to_string(),
        language: "ini".to_string(),
        contents: gromacs_mdp(plan, stage_id),
        written: false,
    }
}

fn gromacs_mdp(plan: &SimulationPlan, stage_id: &str) -> String {
    match stage_id {
        "ions" => gromacs_em_mdp(plan, "ions", 500),
        "em" => gromacs_em_mdp(
            plan,
            "em",
            stage_parameter(plan, "em", "maxSteps")
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(50_000),
        ),
        "nvt" => gromacs_equilibration_mdp(plan, "nvt", false),
        "npt" => gromacs_equilibration_mdp(plan, "npt", true),
        "production" => gromacs_production_mdp(plan),
        _ => "; unsupported stage\n".to_string(),
    }
}

fn gromacs_em_mdp(plan: &SimulationPlan, label: &str, nsteps: u32) -> String {
    let emtol = stage_parameter(plan, "em", "emtol").unwrap_or("1000");
    format!(
        r#"; AutoMD generated GROMACS MDP: {label}
integrator      = steep
emtol           = {emtol}
emstep          = 0.01
nsteps          = {nsteps}
nstlist         = 20
cutoff-scheme   = Verlet
coulombtype     = PME
rcoulomb        = 1.0
rvdw            = 1.0
pbc             = xyz
"#
    )
}

fn gromacs_equilibration_mdp(
    plan: &SimulationPlan,
    stage_id: &str,
    pressure_coupling: bool,
) -> String {
    let duration_ps = stage_parameter(plan, stage_id, "durationPs")
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(if stage_id == "nvt" { 100.0 } else { 1000.0 });
    let temperature = stage_parameter(plan, stage_id, "temperatureK").unwrap_or("300");
    let pressure = stage_parameter(plan, stage_id, "pressureBar").unwrap_or("1.0");
    let velocity_seed = stage_parameter(plan, stage_id, "velocitySeed")
        .or_else(|| stage_parameter(plan, "production", "randomSeed"))
        .unwrap_or("-1");
    let timestep_fs = stage_parameter(plan, "production", "timestepFs")
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(2.0);
    let nsteps = nsteps_from_ps_f64(duration_ps, timestep_fs);
    let restraints = stage_parameter(plan, stage_id, "restraints")
        .or_else(|| stage_parameter(plan, "nvt", "restraints"))
        .unwrap_or("none");
    let define_line = if restraints != "none" && !restraints.is_empty() && restraints != "false" {
        "define          = -DPOSRES\n"
    } else {
        ""
    };
    let pcoupl = if pressure_coupling {
        format!(
            r#"pcoupl          = C-rescale
pcoupltype      = isotropic
tau_p           = 5.0
ref_p           = {pressure}
compressibility = 4.5e-5"#
        )
    } else {
        "pcoupl          = no".to_string()
    };

    format!(
        r#"; AutoMD generated GROMACS MDP: {stage_id}
{define_line}integrator      = md
dt              = {dt}
nsteps          = {nsteps}
nstxout-compressed = 500
nstenergy       = 500
nstlog          = 500
continuation    = {continuation}
constraint_algorithm = lincs
constraints     = h-bonds
cutoff-scheme   = Verlet
nstlist         = 20
rcoulomb        = 1.0
rvdw            = 1.0
DispCorr        = EnerPres
coulombtype     = PME
tcoupl          = V-rescale
tc-grps         = System
tau_t           = 0.1
ref_t           = {temperature}
{pcoupl}
gen_vel         = {gen_vel}
gen_temp        = {temperature}
gen_seed        = {velocity_seed}
pbc             = xyz
"#,
        dt = format_number_f64(timestep_fs / 1000.0),
        continuation = if pressure_coupling { "yes" } else { "no" },
        gen_vel = if pressure_coupling { "no" } else { "yes" },
        velocity_seed = velocity_seed,
    )
}

fn gromacs_production_mdp(plan: &SimulationPlan) -> String {
    let duration_ns = stage_parameter(plan, "production", "durationNs")
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(100.0);
    let timestep_fs = stage_parameter(plan, "production", "timestepFs")
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(2.0);
    let checkpoint_ps = stage_parameter(plan, "production", "checkpointEveryPs")
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(100.0);
    let temperature = stage_parameter(plan, "npt", "temperatureK")
        .or_else(|| stage_parameter(plan, "nvt", "temperatureK"))
        .unwrap_or("300");
    let pressure = stage_parameter(plan, "npt", "pressureBar").unwrap_or("1.0");
    let nsteps = nsteps_from_ps_f64(duration_ns * 1000.0, timestep_fs);
    let checkpoint_steps = nsteps_from_ps_f64(checkpoint_ps, timestep_fs);

    format!(
        r#"; AutoMD generated GROMACS MDP: production
integrator      = md
dt              = {dt}
nsteps          = {nsteps}
nstxout-compressed = 5000
nstenergy       = 1000
nstlog          = 1000
nstcheckpoint   = {checkpoint_steps}
continuation    = yes
constraint_algorithm = lincs
constraints     = h-bonds
cutoff-scheme   = Verlet
nstlist         = 20
rcoulomb        = 1.0
rvdw            = 1.0
DispCorr        = EnerPres
coulombtype     = PME
tcoupl          = V-rescale
tc-grps         = System
tau_t           = 0.1
ref_t           = {temperature}
pcoupl          = C-rescale
pcoupltype      = isotropic
tau_p           = 5.0
ref_p           = {pressure}
compressibility = 4.5e-5
pbc             = xyz
"#,
        dt = format_number_f64(timestep_fs / 1000.0),
    )
}

fn gromacs_run_script(plan: &SimulationPlan, commands: &[EngineCommand]) -> String {
    let body = commands
        .iter()
        .map(|command| {
            format!(
                r#"echo "[AutoMD] {label}"
{command}
"#,
                label = command.label,
                command = command.command
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"#!/usr/bin/env bash
set -euo pipefail

echo "AutoMD GROMACS run: {name}"
echo "Plan id: {plan_id}"
mkdir -p generated/gromacs runs analysis reports checkpoints trajectories

{prelude}

{body}

echo "[AutoMD] GROMACS workflow completed"
"#,
        name = plan.name,
        plan_id = plan.id,
        prelude = gromacs_shell_prelude()
    )
}

fn gromacs_shell_prelude() -> &'static str {
    r#"if command -v gmx >/dev/null 2>&1; then
  AUTOMD_GMX_BIN="$(command -v gmx)"
elif [ -x "$HOME/Library/Application Support/com.noir.automd/engines/gromacs/bin/gmx" ]; then
  AUTOMD_GMX_BIN="$HOME/Library/Application Support/com.noir.automd/engines/gromacs/bin/gmx"
elif [ -x "$HOME/.automd/engines/gromacs/bin/gmx" ]; then
  AUTOMD_GMX_BIN="$HOME/.automd/engines/gromacs/bin/gmx"
else
  echo "[AutoMD] GROMACS executable not found. Register or install GROMACS on the Engines page." >&2
  exit 127
fi
gmx() { "$AUTOMD_GMX_BIN" "$@"; }
echo "[AutoMD] Using GROMACS: $AUTOMD_GMX_BIN"
AUTOMD_GROMACS_GPU_SUFFIX=""
AUTOMD_GROMACS_GPU_SUFFIX_READY=0
automd_gromacs_gpu_suffix() {
  if [ "${AUTOMD_GROMACS_GPU_SUFFIX_READY:-0}" = "1" ]; then
    printf '%s' "$AUTOMD_GROMACS_GPU_SUFFIX"
    return 0
  fi
  AUTOMD_GROMACS_GPU_SUFFIX_READY=1
  AUTOMD_GROMACS_GPU_SUFFIX=""
  if ! command -v nvidia-smi >/dev/null 2>&1; then
    echo "[AutoMD] GROMACS GPU was requested, but no NVIDIA GPU was detected; running GROMACS CPU mode." >&2
    printf '%s' "$AUTOMD_GROMACS_GPU_SUFFIX"
    return 0
  fi
  local gpu_list
  gpu_list="$(nvidia-smi -L 2>/dev/null || true)"
  if ! printf '%s\n' "$gpu_list" | grep -Eq '^GPU[[:space:]]+[0-9]+'; then
    echo "[AutoMD] GROMACS GPU was requested, but nvidia-smi did not list a usable GPU; running GROMACS CPU mode." >&2
    printf '%s' "$AUTOMD_GROMACS_GPU_SUFFIX"
    return 0
  fi
  local version
  version="$(gmx mdrun -version 2>/dev/null || true)"
  if printf '%s\n' "$version" | grep -Eiq 'GPU support[[:space:]]*:[[:space:]]*(CUDA|SYCL|OpenCL|HIP)'; then
    AUTOMD_GROMACS_GPU_SUFFIX=" -nb gpu -pme gpu"
  else
    echo "[AutoMD] GROMACS GPU was requested, but this gmx build did not report CUDA/SYCL/OpenCL/HIP support; running GROMACS CPU mode." >&2
  fi
  printf '%s' "$AUTOMD_GROMACS_GPU_SUFFIX"
}
automd_gromacs_mdrun() {
  local mode="${1:-cpu}"
  shift || true
  if [ "$mode" != "auto" ]; then
    gmx mdrun "$@"
    return $?
  fi

  local suffix
  suffix="$(automd_gromacs_gpu_suffix)"
  if [ -z "$suffix" ]; then
    gmx mdrun "$@"
    return $?
  fi

  local args=("$@")
  local deffnm=""
  local i
  for ((i = 0; i < ${#args[@]}; i++)); do
    if [ "${args[$i]}" = "-deffnm" ] && [ $((i + 1)) -lt ${#args[@]} ]; then
      deffnm="${args[$((i + 1))]}"
      break
    fi
  done

  echo "[AutoMD] Trying GROMACS GPU mode$suffix" >&2
  set +e
  gmx mdrun "${args[@]}" -nb gpu -pme gpu
  local status=$?
  set -e
  if [ "$status" -eq 0 ]; then
    return 0
  fi

  if [ -n "$deffnm" ] && [ -f "$deffnm.log" ] && grep -Eiq 'no GPU|Cannot run .*GPU|GPU.*not.*detected|No compatible GPU' "$deffnm.log"; then
    echo "[AutoMD] GROMACS GPU mode failed because no compatible GPU was available to mdrun; retrying CPU mode." >&2
    rm -f "$deffnm.log" "$deffnm.edr" "$deffnm.trr" "$deffnm.xtc" "$deffnm.cpt" "$deffnm.gro"
    gmx mdrun "${args[@]}"
    return $?
  fi

  return "$status"
}
automd_gromacs_top_dirs() {
  [ -n "${GMXLIB:-}" ] && printf '%s\n' "$GMXLIB"
  [ -n "${CONDA_PREFIX:-}" ] && printf '%s\n' "$CONDA_PREFIX/share/gromacs/top"
  local bin_dir
  bin_dir="$(dirname "$AUTOMD_GMX_BIN")"
  printf '%s\n' \
    "$bin_dir/../share/gromacs/top" \
    "$bin_dir/../../share/gromacs/top" \
    "$HOME/.automd/engines/gromacs/share/gromacs/top" \
    "$HOME/Library/Application Support/com.noir.automd/engines/gromacs/share/gromacs/top" \
    "/usr/local/gromacs/share/gromacs/top" \
    "/usr/share/gromacs/top"
}
automd_gromacs_force_field_exists() {
  local ff="$1"
  local top_dir
  while IFS= read -r top_dir; do
    [ -n "$top_dir" ] || continue
    [ -d "$top_dir/$ff.ff" ] && return 0
  done <<EOF_AUTOMD_GROMACS_TOP_DIRS
$(automd_gromacs_top_dirs)
EOF_AUTOMD_GROMACS_TOP_DIRS
  return 1
}
automd_pick_gromacs_force_field() {
  local preferred="$1"
  local ff
  for ff in "$preferred" amber14sb amber99sb-ildn charmm36-mar2019 charmm27 oplsaa amber99sb amber03; do
    if automd_gromacs_force_field_exists "$ff"; then
      if [ "$ff" != "$preferred" ]; then
        echo "[AutoMD] Preferred GROMACS force field '$preferred' is not installed; using '$ff'." >&2
      fi
      printf '%s\n' "$ff"
      return 0
    fi
  done
  echo "[AutoMD] Could not confirm installed GROMACS force fields; trying '$preferred'." >&2
  printf '%s\n' "$preferred"
}
"#
}

fn gromacs_run_readme(plan: &SimulationPlan, warnings: &[String]) -> String {
    let warnings_md = if warnings.is_empty() {
        "- No warnings.\n".to_string()
    } else {
        warnings
            .iter()
            .map(|warning| format!("- {warning}\n"))
            .collect::<String>()
    };

    format!(
        r#"# AutoMD GROMACS Run Package

Plan: `{name}`

## Files

- `generated/gromacs/*.mdp` contains generated stage parameters.
- `generated/gromacs/automd-plan.json` preserves the normalized AutoMD plan.
- `run-gromacs.sh` executes preparation, minimization, equilibration, production, and basic analysis.

## Warnings

{warnings_md}
"#,
        name = plan.name,
    )
}

fn parse_gromacs_log(log_contents: &str) -> EngineLogReport {
    let mut events = Vec::new();
    let mut fatal_error = None;
    let mut ns_per_day = None;
    let mut current_step = None;
    let mut progress_percent = None;

    for (index, line) in log_contents.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(value) = parse_ns_per_day(trimmed) {
            ns_per_day = Some(value);
            events.push(event(
                EngineLogEventKind::Performance,
                line_number,
                format!("{value:.3} ns/day"),
            ));
        }

        if let Some((step, total)) = parse_step_progress(trimmed) {
            current_step = Some(step);
            if let Some(total) = total.filter(|value| *value > 0) {
                progress_percent = Some((step as f32 / total as f32 * 100.0).min(100.0));
            }
            events.push(event(
                EngineLogEventKind::Progress,
                line_number,
                trimmed.to_string(),
            ));
        }

        let lower = trimmed.to_ascii_lowercase();
        if lower.contains("writing checkpoint")
            || lower.contains("checkpoint") && lower.contains(".cpt")
        {
            events.push(event(
                EngineLogEventKind::Checkpoint,
                line_number,
                trimmed.to_string(),
            ));
        }
        if trimmed.contains("WARNING") || lower.starts_with("warning") {
            events.push(event(
                EngineLogEventKind::Warning,
                line_number,
                trimmed.to_string(),
            ));
        }
        if lower.contains("fatal error") || lower.contains("error in user input") {
            fatal_error = Some(trimmed.to_string());
            events.push(event(
                EngineLogEventKind::Error,
                line_number,
                trimmed.to_string(),
            ));
        }
    }

    EngineLogReport {
        engine_id: "gromacs".to_string(),
        progress_percent,
        ns_per_day,
        current_step,
        events,
        fatal_error,
    }
}

fn parse_openmm_log(log_contents: &str) -> EngineLogReport {
    let mut report = parse_gromacs_log(log_contents);
    report.engine_id = "openmm".to_string();

    for (index, line) in log_contents.lines().enumerate() {
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();
        if lower.contains("openmm workflow completed") {
            report.progress_percent = Some(100.0);
            report.events.push(event(
                EngineLogEventKind::Info,
                index + 1,
                "OpenMM workflow completed".to_string(),
            ));
        }
        if lower.contains("loaded checkpoint") {
            report.events.push(event(
                EngineLogEventKind::Checkpoint,
                index + 1,
                trimmed.to_string(),
            ));
        }
        if lower.contains("traceback")
            || lower.starts_with("valueerror:")
            || lower.starts_with("runtimeerror:")
            || lower.starts_with("exception:")
            || lower.contains("no template found for residue")
            || lower.contains("openmm python module not found")
        {
            report.fatal_error = Some(trimmed.to_string());
            report.events.push(event(
                EngineLogEventKind::Error,
                index + 1,
                trimmed.to_string(),
            ));
        }
    }

    report
}

fn parse_ambertools_log(log_contents: &str) -> EngineLogReport {
    let mut report = parse_gromacs_log(log_contents);
    report.engine_id = "ambertools".to_string();

    for (index, line) in log_contents.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let line_number = index + 1;
        let lower = trimmed.to_ascii_lowercase();

        if let Some(step) = parse_amber_nstep(trimmed) {
            report.current_step = Some(step);
            report.events.push(event(
                EngineLogEventKind::Progress,
                line_number,
                trimmed.to_string(),
            ));
        }
        if lower.contains("a v e r a g e s") || lower.contains("final performance info") {
            report.events.push(event(
                EngineLogEventKind::Info,
                line_number,
                trimmed.to_string(),
            ));
        }
        if lower.contains("ambertools workflow completed") {
            report.progress_percent = Some(100.0);
            report.events.push(event(
                EngineLogEventKind::Info,
                line_number,
                "AmberTools workflow completed".to_string(),
            ));
        }
        if lower.contains("sander bomb")
            || lower.contains("exiting leap")
            || lower.contains("fatal")
            || lower.starts_with("error")
        {
            report.fatal_error = Some(trimmed.to_string());
            report.events.push(event(
                EngineLogEventKind::Error,
                line_number,
                trimmed.to_string(),
            ));
        }
    }

    report
}

fn parse_namd_log(log_contents: &str) -> EngineLogReport {
    let mut report = parse_gromacs_log(log_contents);
    report.engine_id = "namd".to_string();

    for (index, line) in log_contents.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let line_number = index + 1;
        let lower = trimmed.to_ascii_lowercase();

        if let Some(step) = parse_namd_step(trimmed) {
            report.current_step = Some(step);
            report.events.push(event(
                EngineLogEventKind::Progress,
                line_number,
                trimmed.to_string(),
            ));
        }
        if lower.starts_with("timing:") || lower.contains("benchmark time") {
            report.events.push(event(
                EngineLogEventKind::Performance,
                line_number,
                trimmed.to_string(),
            ));
        }
        if lower.contains("restart") && (lower.contains("writing") || lower.contains("output")) {
            report.events.push(event(
                EngineLogEventKind::Checkpoint,
                line_number,
                trimmed.to_string(),
            ));
        }
        if lower.contains("end of program") || lower.contains("namd workflow completed") {
            report.progress_percent = Some(100.0);
            report.events.push(event(
                EngineLogEventKind::Info,
                line_number,
                "NAMD workflow completed".to_string(),
            ));
        }
        if lower.contains("fatal error") || lower.starts_with("error:") {
            report.fatal_error = Some(trimmed.to_string());
            report.events.push(event(
                EngineLogEventKind::Error,
                line_number,
                trimmed.to_string(),
            ));
        }
    }

    report
}

fn parse_generic_engine_log(engine_id: &str, log_contents: &str) -> EngineLogReport {
    let mut report = parse_gromacs_log(log_contents);
    report.engine_id = engine_id.to_string();

    for (index, line) in log_contents.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let line_number = index + 1;
        let lower = trimmed.to_ascii_lowercase();
        if lower.contains("workflow completed")
            || lower.contains("normal termination")
            || lower.contains("finished")
        {
            report.progress_percent = Some(100.0);
            report.events.push(event(
                EngineLogEventKind::Info,
                line_number,
                trimmed.to_string(),
            ));
        }
        if contains_any(
            &lower,
            &["restart", "checkpoint", "write_restart", "revcon"],
        ) {
            report.events.push(event(
                EngineLogEventKind::Checkpoint,
                line_number,
                trimmed.to_string(),
            ));
        }
        if contains_any(
            &lower,
            &["fatal", "error:", "segmentation fault", "traceback"],
        ) {
            report.fatal_error = Some(trimmed.to_string());
            report.events.push(event(
                EngineLogEventKind::Error,
                line_number,
                trimmed.to_string(),
            ));
        }
        if contains_any(&lower, &["warning", "caution"]) {
            report.events.push(event(
                EngineLogEventKind::Warning,
                line_number,
                trimmed.to_string(),
            ));
        }
    }

    report
}

fn classify_gromacs_failure(log_contents: &str, exit_code: Option<i32>) -> FailureAnalysis {
    let lower = log_contents.to_ascii_lowercase();
    let category = if contains_any(
        &lower,
        &[
            "gmx: command not found",
            "gmx_mpi: command not found",
            "no such file or directory: gmx",
            "failed to spawn process",
        ],
    ) {
        FailureCategory::MissingExecutable
    } else if contains_any(
        &lower,
        &[
            "permission denied",
            "no space left on device",
            "cannot allocate memory",
            "disk quota exceeded",
        ],
    ) {
        FailureCategory::DiskOrPermission
    } else if contains_any(
        &lower,
        &[
            "cannot run short-ranged nonbonded interactions on a gpu",
            "no compatible gpus",
            "cuda error",
            "opencl error",
            "gpu-aware mpi",
        ],
    ) {
        FailureCategory::GpuUnavailable
    } else if contains_any(
        &lower,
        &["mpi_abort", "mpi error", "rank ", "pmi", "pmix", "orted"],
    ) {
        FailureCategory::MpiFailure
    } else if contains_any(
        &lower,
        &[
            "does not match topology",
            "number of coordinates",
            "coordinate file",
            "molecule type",
        ],
    ) {
        FailureCategory::ParameterMismatch
    } else if contains_any(
        &lower,
        &[
            "atomtype",
            "no default",
            "no such moleculetype",
            "residue type",
            "force field",
        ],
    ) {
        if contains_any(&lower, &["force field", "atomtype", "no default"]) {
            FailureCategory::MissingForceField
        } else {
            FailureCategory::MissingTopology
        }
    } else if contains_any(
        &lower,
        &[
            "file input/output error",
            "no such file",
            "cannot open file",
            "required option was not provided",
            ".tpr",
            ".gro",
            ".pdb",
            ".top",
        ],
    ) {
        FailureCategory::MissingInput
    } else if contains_any(
        &lower,
        &[
            "lincs warning",
            "blowing up",
            "not finite",
            "nan",
            "inf",
            "pressure scaling more than",
            "too many warnings",
        ],
    ) {
        FailureCategory::NumericalInstability
    } else if contains_any(
        &lower,
        &["slurm", "sbatch", "scancel", "pbs", "qsub", "lsf", "bsub"],
    ) {
        FailureCategory::SchedulerFailure
    } else {
        FailureCategory::Unknown
    };

    let message = failure_headline("GROMACS", log_contents, exit_code, &category);
    let severity = if matches!(category, FailureCategory::Unknown) {
        ValidationSeverity::Warning
    } else {
        ValidationSeverity::Error
    };

    FailureAnalysis {
        engine_id: "gromacs".to_string(),
        category: category.clone(),
        severity,
        message,
        suggestions: gromacs_failure_suggestions(category),
    }
}

fn classify_openmm_failure(log_contents: &str, exit_code: Option<i32>) -> FailureAnalysis {
    let lower = log_contents.to_ascii_lowercase();
    let category = if contains_any(
        &lower,
        &[
            "openmm python module not found",
            "modulenotfounderror",
            "no module named 'openmm'",
            "no module named openmm",
        ],
    ) {
        FailureCategory::MissingExecutable
    } else if contains_any(
        &lower,
        &["input file not found", "no such file", "cannot open file"],
    ) {
        FailureCategory::MissingInput
    } else if contains_any(
        &lower,
        &[
            "no template found for residue",
            "no template found",
            "forcefield",
            "force field",
            "parameters have not been assigned",
        ],
    ) {
        FailureCategory::MissingForceField
    } else if contains_any(&lower, &["cuda", "opencl", "platform", "gpu"]) {
        FailureCategory::GpuUnavailable
    } else if contains_any(
        &lower,
        &[
            "nan",
            "not finite",
            "particle coordinate is nan",
            "energy is nan",
        ],
    ) {
        FailureCategory::NumericalInstability
    } else if contains_any(
        &lower,
        &[
            "permission denied",
            "no space left on device",
            "disk quota exceeded",
        ],
    ) {
        FailureCategory::DiskOrPermission
    } else {
        FailureCategory::Unknown
    };

    let message = failure_headline("OpenMM", log_contents, exit_code, &category);
    let severity = if matches!(category, FailureCategory::Unknown) {
        ValidationSeverity::Warning
    } else {
        ValidationSeverity::Error
    };

    FailureAnalysis {
        engine_id: "openmm".to_string(),
        category: category.clone(),
        severity,
        message,
        suggestions: openmm_failure_suggestions(category),
    }
}

fn classify_ambertools_failure(log_contents: &str, exit_code: Option<i32>) -> FailureAnalysis {
    let lower = log_contents.to_ascii_lowercase();
    let category = if contains_any(
        &lower,
        &[
            "tleap: command not found",
            "sander: command not found",
            "cpptraj: command not found",
            "no such file or directory: tleap",
            "no such file or directory: sander",
            "failed to spawn process",
        ],
    ) {
        FailureCategory::MissingExecutable
    } else if contains_any(
        &lower,
        &[
            "permission denied",
            "no space left on device",
            "cannot allocate memory",
            "disk quota exceeded",
        ],
    ) {
        FailureCategory::DiskOrPermission
    } else if contains_any(
        &lower,
        &[
            "could not open file",
            "no such file",
            "cannot open",
            "does not exist",
            "not found: inputs/",
        ],
    ) {
        FailureCategory::MissingInput
    } else if contains_any(
        &lower,
        &[
            "unknown atom type",
            "atom type",
            "missing parameters",
            "frcmod",
            "mol2",
            "antechamber",
            "gaff",
            "leap failed",
            "exiting leap",
        ],
    ) {
        FailureCategory::MissingForceField
    } else if contains_any(
        &lower,
        &[
            "sander bomb",
            "vlimit exceeded",
            "nan",
            "not finite",
            "ewald bomb",
            "shake failure",
        ],
    ) {
        FailureCategory::NumericalInstability
    } else if contains_any(
        &lower,
        &["prmtop", "inpcrd", "rst7", "coordinate", "topology"],
    ) {
        FailureCategory::MissingTopology
    } else {
        FailureCategory::Unknown
    };

    let message = failure_headline("AmberTools", log_contents, exit_code, &category);
    let severity = if matches!(category, FailureCategory::Unknown) {
        ValidationSeverity::Warning
    } else {
        ValidationSeverity::Error
    };

    FailureAnalysis {
        engine_id: "ambertools".to_string(),
        category: category.clone(),
        severity,
        message,
        suggestions: ambertools_failure_suggestions(category),
    }
}

fn classify_namd_failure(log_contents: &str, exit_code: Option<i32>) -> FailureAnalysis {
    let lower = log_contents.to_ascii_lowercase();
    let category = if contains_any(
        &lower,
        &[
            "namd3: command not found",
            "namd2: command not found",
            "namd: command not found",
            "no such file or directory: namd",
            "failed to spawn process",
        ],
    ) {
        FailureCategory::MissingExecutable
    } else if contains_any(
        &lower,
        &["license", "registration", "permission to use namd"],
    ) {
        FailureCategory::LicenseRequired
    } else if contains_any(
        &lower,
        &[
            "unable to open",
            "no such file",
            "could not open",
            "cannot open",
        ],
    ) {
        FailureCategory::MissingInput
    } else if contains_any(
        &lower,
        &[
            "unable to find",
            "parameters",
            "unknown atom",
            "psf",
            "parameter file",
        ],
    ) {
        FailureCategory::MissingForceField
    } else if contains_any(&lower, &["cuda", "gpu", "hip", "rocm"]) {
        FailureCategory::GpuUnavailable
    } else if contains_any(
        &lower,
        &[
            "nan",
            "not finite",
            "atoms moving too fast",
            "bad global exclusion count",
        ],
    ) {
        FailureCategory::NumericalInstability
    } else if contains_any(
        &lower,
        &[
            "permission denied",
            "no space left on device",
            "disk quota exceeded",
        ],
    ) {
        FailureCategory::DiskOrPermission
    } else {
        FailureCategory::Unknown
    };

    let message = failure_headline("NAMD", log_contents, exit_code, &category);
    let severity = if matches!(category, FailureCategory::Unknown) {
        ValidationSeverity::Warning
    } else {
        ValidationSeverity::Error
    };

    FailureAnalysis {
        engine_id: "namd".to_string(),
        category: category.clone(),
        severity,
        message,
        suggestions: namd_failure_suggestions(category),
    }
}

fn classify_generic_engine_failure(
    engine_id: &str,
    log_contents: &str,
    exit_code: Option<i32>,
) -> FailureAnalysis {
    let lower = log_contents.to_ascii_lowercase();
    let category = if contains_any(
        &lower,
        &[
            "command not found",
            "no such file or directory",
            "failed to spawn process",
            "not recognized as an internal",
        ],
    ) {
        FailureCategory::MissingExecutable
    } else if contains_any(
        &lower,
        &["license", "licensed", "authorization", "schrodinger"],
    ) {
        FailureCategory::LicenseRequired
    } else if contains_any(
        &lower,
        &[
            "cannot open",
            "could not open",
            "unable to open",
            "missing input",
            "file not found",
        ],
    ) {
        FailureCategory::MissingInput
    } else if contains_any(
        &lower,
        &[
            "missing parameter",
            "unknown atom",
            "pair coeff",
            "potential",
            "basis set",
            "topology",
            "force field",
            "psf",
            "field",
        ],
    ) {
        FailureCategory::MissingForceField
    } else if contains_any(&lower, &["cuda", "gpu", "hip", "rocm", "opencl"]) {
        FailureCategory::GpuUnavailable
    } else if contains_any(&lower, &["mpi", "mpirun", "rank ", "pmi", "pmix"]) {
        FailureCategory::MpiFailure
    } else if contains_any(
        &lower,
        &["nan", "not finite", "blow", "shake", "lincs", "bad contact"],
    ) {
        FailureCategory::NumericalInstability
    } else if contains_any(
        &lower,
        &[
            "permission denied",
            "no space left on device",
            "disk quota exceeded",
        ],
    ) {
        FailureCategory::DiskOrPermission
    } else {
        FailureCategory::Unknown
    };
    let engine_name = engine_display_name(engine_id);
    let message = failure_headline(engine_name, log_contents, exit_code, &category);
    let severity = if matches!(category, FailureCategory::Unknown) {
        ValidationSeverity::Warning
    } else {
        ValidationSeverity::Error
    };

    FailureAnalysis {
        engine_id: engine_id.to_string(),
        category: category.clone(),
        severity,
        message,
        suggestions: generic_failure_suggestions(engine_id, category),
    }
}

fn discover_gromacs_resume_plan(
    request: ResumePlanRequest,
) -> Result<ResumePlan, EngineAdapterError> {
    let project_root = PathBuf::from(&request.project_path);
    let run_root = safe_join(&project_root, &request.run_directory);
    let checkpoint_root = safe_join(&project_root, "checkpoints");
    let mut warnings = Vec::new();
    let mut seen = BTreeSet::new();
    let mut candidates = Vec::new();

    for root in [&run_root, &checkpoint_root] {
        if root.exists() {
            collect_checkpoint_candidates(&project_root, root, &mut seen, &mut candidates)?;
        } else if root == &run_root {
            warnings.push(format!(
                "Run directory not found yet: {}",
                request.run_directory
            ));
        }
    }

    candidates.sort_by(|left, right| {
        right
            .modified_at
            .cmp(&left.modified_at)
            .then_with(|| left.path.cmp(&right.path))
    });
    let recommended = candidates.first().cloned();
    let resume_command = recommended
        .as_ref()
        .and_then(|checkpoint| checkpoint.command_hint.clone());

    if candidates.is_empty() {
        warnings.push("No .cpt checkpoint files were found in the run directory or project checkpoints directory.".to_string());
    }

    Ok(ResumePlan {
        engine_id: "gromacs".to_string(),
        run_directory: request.run_directory,
        checkpoints: candidates,
        recommended,
        resume_command,
        warnings,
    })
}

fn discover_openmm_resume_plan(
    request: ResumePlanRequest,
) -> Result<ResumePlan, EngineAdapterError> {
    let project_root = PathBuf::from(&request.project_path);
    let run_root = safe_join(&project_root, &request.run_directory);
    let checkpoint_root = safe_join(&project_root, "checkpoints");
    let mut warnings = Vec::new();
    let mut seen = BTreeSet::new();
    let mut candidates = Vec::new();

    for root in [&run_root, &checkpoint_root] {
        if root.exists() {
            collect_openmm_checkpoint_candidates(
                &project_root,
                &request.run_directory,
                root,
                &mut seen,
                &mut candidates,
            )?;
        } else if root == &run_root {
            warnings.push(format!(
                "Run directory not found yet: {}",
                request.run_directory
            ));
        }
    }

    candidates.sort_by(|left, right| {
        right
            .modified_at
            .cmp(&left.modified_at)
            .then_with(|| left.path.cmp(&right.path))
    });
    let recommended = candidates.first().cloned();
    let resume_command = recommended
        .as_ref()
        .and_then(|checkpoint| checkpoint.command_hint.clone());

    if candidates.is_empty() {
        warnings.push("No .chk OpenMM checkpoint files were found in the run directory or project checkpoints directory.".to_string());
    }

    Ok(ResumePlan {
        engine_id: "openmm".to_string(),
        run_directory: request.run_directory,
        checkpoints: candidates,
        recommended,
        resume_command,
        warnings,
    })
}

fn collect_checkpoint_candidates(
    project_root: &Path,
    root: &Path,
    seen: &mut BTreeSet<String>,
    candidates: &mut Vec<CheckpointCandidate>,
) -> Result<(), EngineAdapterError> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_checkpoint_candidates(project_root, &path, seen, candidates)?;
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("cpt"))
        {
            let relative_path = relative_path(project_root, &path);
            if seen.insert(relative_path.clone()) {
                let metadata = entry.metadata()?;
                let modified_at = metadata
                    .modified()
                    .ok()
                    .map(chrono::DateTime::<chrono::Utc>::from);
                let stage_hint = stage_hint_from_checkpoint(&relative_path);
                let command_hint = Some(gromacs_resume_command(&relative_path));
                candidates.push(CheckpointCandidate {
                    path: relative_path,
                    size_bytes: metadata.len(),
                    modified_at,
                    stage_hint,
                    command_hint,
                });
            }
        }
    }
    Ok(())
}

fn collect_openmm_checkpoint_candidates(
    project_root: &Path,
    run_directory: &str,
    root: &Path,
    seen: &mut BTreeSet<String>,
    candidates: &mut Vec<CheckpointCandidate>,
) -> Result<(), EngineAdapterError> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_openmm_checkpoint_candidates(
                project_root,
                run_directory,
                &path,
                seen,
                candidates,
            )?;
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("chk"))
        {
            let relative_path = relative_path(project_root, &path);
            if seen.insert(relative_path.clone()) {
                let metadata = entry.metadata()?;
                let modified_at = metadata
                    .modified()
                    .ok()
                    .map(chrono::DateTime::<chrono::Utc>::from);
                candidates.push(CheckpointCandidate {
                    path: relative_path.clone(),
                    size_bytes: metadata.len(),
                    modified_at,
                    stage_hint: Some("production".to_string()),
                    command_hint: Some(openmm_resume_command(run_directory, &relative_path)),
                });
            }
        }
    }
    Ok(())
}

fn relative_path(project_root: &Path, path: &Path) -> String {
    path.strip_prefix(project_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn stage_hint_from_checkpoint(path: &str) -> Option<String> {
    let lower = path.to_ascii_lowercase();
    if lower.contains("nvt") {
        Some("nvt".to_string())
    } else if lower.contains("npt") {
        Some("npt".to_string())
    } else if lower.contains("em") {
        Some("energy minimization".to_string())
    } else if lower.contains("md") || lower.contains("prod") || lower.contains("production") {
        Some("production".to_string())
    } else {
        None
    }
}

fn gromacs_resume_command(checkpoint_path: &str) -> String {
    let deffnm = checkpoint_path.trim_end_matches(".cpt");
    format!(
        "gmx mdrun -deffnm {} -cpi {} -append",
        shell_quote(deffnm),
        shell_quote(checkpoint_path)
    )
}

fn openmm_resume_command(run_directory: &str, checkpoint_path: &str) -> String {
    format!(
        "python generated/openmm/run_openmm.py --plan generated/openmm/automd-plan.json --out {} --resume {}",
        shell_quote(run_directory),
        shell_quote(checkpoint_path)
    )
}

fn shell_quote(value: &str) -> String {
    if value.chars().all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '/' | '.' | '_' | '-')
    }) {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn failure_headline(
    engine_name: &str,
    log_contents: &str,
    exit_code: Option<i32>,
    category: &FailureCategory,
) -> String {
    let interesting = log_contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .find(|line| {
            let lower = line.to_ascii_lowercase();
            contains_any(
                &lower,
                &[
                    "fatal error",
                    "error",
                    "warning",
                    "command not found",
                    "permission denied",
                    "no such file",
                    "lincs",
                    "nan",
                    "cuda",
                    "gpu",
                ],
            )
        })
        .map(str::to_string);

    interesting.unwrap_or_else(|| {
        let code = exit_code
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        format!("{engine_name} run failed with exit code {code}; classified as {category:?}.")
    })
}

fn gromacs_failure_suggestions(category: FailureCategory) -> Vec<FailureSuggestion> {
    match category {
        FailureCategory::MissingExecutable => vec![
            suggestion(
                "GROMACS executable was not found",
                "Run engine detection again after installing GROMACS or adding gmx/gmx_mpi to PATH.",
                "Detect GROMACS",
                Some("gmx --version"),
            ),
            suggestion(
                "Use container or remote Linux",
                "If this platform does not have the required build, run through Docker/Podman, WSL2, or an SSH/SLURM profile.",
                "Switch execution target",
                None,
            ),
        ],
        FailureCategory::MissingInput => vec![
            suggestion(
                "Check generated inputs",
                "Confirm the project has inputs/system.pdb and that generated/gromacs contains the .gro, .top, .mdp, and .tpr files expected by the failing stage.",
                "Open project files",
                Some("find inputs generated runs -maxdepth 3 -type f"),
            ),
            suggestion(
                "Regenerate the run package",
                "Structure preparation may have failed before the current command. Regenerate the package after fixing the source structure path.",
                "Regenerate package",
                None,
            ),
        ],
        FailureCategory::MissingTopology | FailureCategory::MissingForceField => vec![
            suggestion(
                "Resolve topology and force-field gaps",
                "Non-standard residues, ions, cofactors, or ligands need matching atom types, molecule definitions, and include files before grompp can continue.",
                "Review topology",
                Some("gmx grompp -f generated/gromacs/em.mdp -c generated/gromacs/ions.gro -p generated/gromacs/topol.top -o runs/check.tpr"),
            ),
            suggestion(
                "Parameterize ligands separately",
                "Use GAFF2/AmberTools, CGenFF, or a validated external topology source, then import the resulting .itp/.top files.",
                "Open ligand workflow",
                None,
            ),
        ],
        FailureCategory::ParameterMismatch => vec![
            suggestion(
                "Reconcile coordinates and topology",
                "The coordinate atom count or molecule names differ from the topology. Regenerate solvation/ionization outputs and make sure topol.top was updated.",
                "Rebuild prepared system",
                Some("gmx check -f generated/gromacs/ions.gro"),
            ),
        ],
        FailureCategory::GpuUnavailable => vec![
            suggestion(
                "Retry CPU mode",
                "Disable GPU flags for this run to separate physics/input issues from CUDA/OpenCL/SYCL availability.",
                "Run CPU fallback",
                Some("gmx mdrun -deffnm runs/current/md -nb cpu -pme cpu"),
            ),
            suggestion(
                "Verify driver and engine build",
                "GROMACS must be compiled with the selected GPU backend and see a compatible driver/runtime.",
                "Check GPU backend",
                Some("gmx mdrun -version"),
            ),
        ],
        FailureCategory::MpiFailure => vec![
            suggestion(
                "Check MPI launch mode",
                "Use a matching gmx_mpi build and launch it through mpirun/srun only when MPI ranks are requested.",
                "Inspect MPI setup",
                Some("gmx_mpi --version"),
            ),
        ],
        FailureCategory::NumericalInstability => vec![
            suggestion(
                "Stabilize before production",
                "Lower the timestep, increase minimization steps, inspect bad contacts, and repeat restrained NVT/NPT before production.",
                "Adjust stability parameters",
                None,
            ),
            suggestion(
                "Inspect the last checkpoint",
                "Use the discovered checkpoint/resume panel to restart from the last stable point after correcting parameters.",
                "Find checkpoint",
                None,
            ),
        ],
        FailureCategory::DiskOrPermission => vec![
            suggestion(
                "Check writable paths and quota",
                "The run directory must be writable and have enough space for trajectories, checkpoints, energy files, and logs.",
                "Inspect storage",
                Some("df -h ."),
            ),
        ],
        FailureCategory::SchedulerFailure => vec![
            suggestion(
                "Review scheduler profile",
                "Queue, module load, walltime, account, and GPU resource strings must match the remote cluster policy.",
                "Edit remote profile",
                Some("sbatch --test-only run.slurm"),
            ),
        ],
        FailureCategory::LicenseRequired => vec![suggestion(
            "Confirm external license",
            "This engine requires a user-managed license and cannot be bundled by AutoMD.",
            "Open license guide",
            None,
        )],
        FailureCategory::Unknown => vec![
            suggestion(
                "Open full log",
                "The classifier did not match a known failure pattern. Inspect the full .log/.err file and rerun with a smaller test system if needed.",
                "Open diagnostics",
                None,
            ),
            suggestion(
                "Regenerate and retry",
                "If the project was partially edited, regenerate the run package so commands and generated inputs are in sync.",
                "Regenerate package",
                None,
            ),
        ],
    }
}

fn openmm_failure_suggestions(category: FailureCategory) -> Vec<FailureSuggestion> {
    match category {
        FailureCategory::MissingExecutable => vec![suggestion(
            "Install the OpenMM Python package",
            "Create or select the AutoMD Python sidecar environment and install OpenMM before launching the generated runner.",
            "Install OpenMM",
            Some("python -m pip install openmm"),
        )],
        FailureCategory::MissingInput => vec![suggestion(
            "Set a valid source structure",
            "The OpenMM runner needs a readable PDB/mmCIF input path. Import a structure or copy it to inputs/system.pdb.",
            "Choose structure",
            Some("ls inputs generated/openmm"),
        )],
        FailureCategory::MissingForceField | FailureCategory::MissingTopology => vec![
            suggestion(
                "Resolve ForceField templates",
                "OpenMM could not assign parameters to one or more residues, ligands, ions, or cofactors. Add compatible XML templates or parameterize the system upstream.",
                "Review OpenMM XML",
                None,
            ),
            suggestion(
                "Use AmberTools preparation",
                "For ligand-heavy biomolecular systems, prepare topology with AmberTools or another validated pipeline, then import compatible inputs.",
                "Open AmberTools workflow",
                None,
            ),
        ],
        FailureCategory::GpuUnavailable => vec![suggestion(
            "Retry with CPU platform",
            "Use CPU execution to separate model/input errors from CUDA or OpenCL platform availability.",
            "Run CPU fallback",
            Some("OPENMM_DEFAULT_PLATFORM=CPU python generated/openmm/run_openmm.py --plan generated/openmm/automd-plan.json --out runs/openmm-test"),
        )],
        FailureCategory::NumericalInstability => vec![suggestion(
            "Reduce timestep and re-equilibrate",
            "Lower the timestep, inspect bad contacts, minimize longer, and resume from the latest stable checkpoint.",
            "Adjust stability parameters",
            None,
        )],
        FailureCategory::DiskOrPermission => vec![suggestion(
            "Check output permissions",
            "The runner writes checkpoints, DCD trajectory, final PDB, and CSV state data; ensure the project folder is writable.",
            "Inspect storage",
            Some("df -h ."),
        )],
        _ => vec![suggestion(
            "Inspect Python traceback",
            "The OpenMM runner surfaced an unclassified error. Check the full Python traceback and generated/openmm/automd-plan.json.",
            "Open diagnostics",
            None,
        )],
    }
}

fn ambertools_failure_suggestions(category: FailureCategory) -> Vec<FailureSuggestion> {
    match category {
        FailureCategory::MissingExecutable => vec![suggestion(
            "Install AmberTools",
            "The generated workflow needs tleap, sander, and cpptraj on PATH, typically from a Conda/Mamba AmberTools environment or a site module.",
            "Detect AmberTools",
            Some("tleap -h && sander -h && cpptraj -h"),
        )],
        FailureCategory::MissingInput => vec![suggestion(
            "Check AmberTools inputs",
            "Confirm the source structure and any ligand mol2/frcmod files exist under inputs/ before running tleap.",
            "Inspect inputs",
            Some("find inputs generated/ambertools -maxdepth 2 -type f"),
        )],
        FailureCategory::MissingForceField | FailureCategory::MissingTopology => vec![
            suggestion(
                "Resolve Amber parameters",
                "Non-standard residues and ligands need Amber-compatible libraries, mol2, and frcmod files before tleap can produce a valid prmtop.",
                "Review tleap inputs",
                Some("tleap -f generated/ambertools/tleap.in"),
            ),
            suggestion(
                "Parameterize ligands",
                "Use antechamber/parmchk2, a validated GAFF2 workflow, or curated parameters, then uncomment the loadamberparams/loadmol2 lines in tleap.in.",
                "Open ligand workflow",
                None,
            ),
        ],
        FailureCategory::NumericalInstability => vec![suggestion(
            "Stabilize the Amber run",
            "Lower the timestep, minimize longer, reduce heating rate, or add restraints before repeating production.",
            "Adjust mdin settings",
            None,
        )],
        FailureCategory::DiskOrPermission => vec![suggestion(
            "Check writable output paths",
            "AmberTools writes prmtop/inpcrd, restart files, NetCDF trajectories, and analysis tables into the project directory.",
            "Inspect storage",
            Some("df -h ."),
        )],
        _ => vec![suggestion(
            "Inspect Amber logs",
            "Check tleap output and the failing sander .out file for the first error line, then regenerate the run package after editing parameters.",
            "Open diagnostics",
            None,
        )],
    }
}

fn namd_failure_suggestions(category: FailureCategory) -> Vec<FailureSuggestion> {
    match category {
        FailureCategory::MissingExecutable => vec![suggestion(
            "Configure NAMD path",
            "Install NAMD in the user's licensed environment, add namd3/namd2 to PATH, or set NAMD_BIN before running the generated script.",
            "Detect NAMD",
            Some("command -v namd3 || command -v namd2"),
        )],
        FailureCategory::LicenseRequired => vec![suggestion(
            "Confirm NAMD license",
            "NAMD is an external user-managed engine in AutoMD. Confirm local license terms and authorization before launching real runs.",
            "Open license guide",
            None,
        )],
        FailureCategory::MissingInput => vec![suggestion(
            "Provide PSF/PDB/parameter files",
            "The NAMD template expects inputs/system.psf, a coordinate PDB, and CHARMM parameter files unless the user edits automd.conf.",
            "Inspect inputs",
            Some("find inputs generated/namd -maxdepth 2 -type f"),
        )],
        FailureCategory::MissingForceField | FailureCategory::MissingTopology => vec![suggestion(
            "Fix NAMD topology and parameters",
            "PSF atom types, coordinate atom counts, and parameter files must agree. Regenerate with CHARMM-GUI, VMD psfgen, or another validated source.",
            "Review automd.conf",
            Some("sed -n '1,160p' generated/namd/automd.conf"),
        )],
        FailureCategory::GpuUnavailable => vec![suggestion(
            "Retry CPU NAMD",
            "Use a CPU build or remove GPU-specific launch options to separate input problems from CUDA/HIP availability.",
            "Run CPU fallback",
            Some("NAMD_BIN=$(command -v namd3 || command -v namd2) bash runs/namd-test/run-namd.sh"),
        )],
        FailureCategory::NumericalInstability => vec![suggestion(
            "Stabilize the NAMD system",
            "Increase minimization, reduce timestep, check bad contacts, and repeat restrained equilibration before production.",
            "Adjust NAMD config",
            None,
        )],
        FailureCategory::DiskOrPermission => vec![suggestion(
            "Check output permissions",
            "NAMD writes DCD, XST, restart, and log files into the run directory; ensure the project folder has space and write access.",
            "Inspect storage",
            Some("df -h ."),
        )],
        _ => vec![suggestion(
            "Inspect NAMD log",
            "The classifier did not match a known NAMD failure. Read namd.log from the top and fix the first input/configuration error.",
            "Open diagnostics",
            None,
        )],
    }
}

fn generic_failure_suggestions(
    engine_id: &str,
    category: FailureCategory,
) -> Vec<FailureSuggestion> {
    let name = engine_display_name(engine_id);
    match category {
        FailureCategory::MissingExecutable => vec![suggestion(
            &format!("Configure {name} executable"),
            "Install the engine, load the site module, add the executable to PATH, or set the engine-specific *_BIN/COMMAND environment variable used by the generated script.",
            "Detect executable",
            None,
        )],
        FailureCategory::LicenseRequired => vec![suggestion(
            &format!("Confirm {name} license"),
            "This is a user-managed engine. AutoMD can generate files and call configured commands, but it does not provide license files or binaries.",
            "Open license guide",
            None,
        )],
        FailureCategory::MissingInput => vec![suggestion(
            "Provide native input files",
            "The preview package contains editable templates. Replace placeholder structures, topology, parameter, basis, potential, or data files before real execution.",
            "Inspect inputs",
            Some("find inputs generated runs -maxdepth 3 -type f"),
        )],
        FailureCategory::MissingForceField | FailureCategory::MissingTopology => vec![suggestion(
            "Review native parameters",
            "The engine reported missing topology, force-field, basis, potential, or coefficient data. Validate the native input deck before launching again.",
            "Edit native input",
            None,
        )],
        FailureCategory::GpuUnavailable => vec![suggestion(
            "Use CPU or a matching build",
            "Retry with a CPU executable or load an engine build compiled for the local CUDA/HIP/OpenCL backend.",
            "Switch backend",
            None,
        )],
        FailureCategory::MpiFailure => vec![suggestion(
            "Check MPI launcher",
            "Use the engine build and mpirun/srun launcher expected by the cluster or local MPI stack.",
            "Inspect MPI",
            None,
        )],
        FailureCategory::NumericalInstability => vec![suggestion(
            "Stabilize the system",
            "Reduce timestep, minimize longer, add restraints, and inspect bad contacts before rerunning.",
            "Adjust parameters",
            None,
        )],
        FailureCategory::DiskOrPermission => vec![suggestion(
            "Check output paths",
            "Ensure the project and run directories are writable and have enough free space for trajectories and restart files.",
            "Inspect storage",
            Some("df -h ."),
        )],
        _ => vec![suggestion(
            "Inspect native log",
            "The generic classifier did not match a known pattern. Open the full engine log and fix the first reported native input error.",
            "Open diagnostics",
            None,
        )],
    }
}

fn engine_display_name(engine_id: &str) -> &'static str {
    match engine_id {
        "gromacs" => "GROMACS",
        "openmm" => "OpenMM",
        "ambertools" => "AmberTools",
        "namd" => "NAMD",
        "lammps" => "LAMMPS",
        "cp2k" => "CP2K",
        "genesis" => "GENESIS",
        "hoomd" => "HOOMD-blue",
        "dl_poly" => "DL_POLY",
        "tinker" => "Tinker",
        "amber_pmemd" => "AMBER pmemd",
        "charmm" => "CHARMM",
        "desmond" => "Desmond",
        "acemd" => "ACEMD",
        _ => "MD engine",
    }
}

fn suggestion(
    title: &str,
    detail: &str,
    action_label: &str,
    command_hint: Option<&str>,
) -> FailureSuggestion {
    FailureSuggestion {
        title: title.to_string(),
        detail: detail.to_string(),
        action_label: action_label.to_string(),
        command_hint: command_hint.map(str::to_string),
    }
}

fn parse_ns_per_day(line: &str) -> Option<f32> {
    let marker = "Performance:";
    let start = line.find(marker)? + marker.len();
    let value = line[start..].split_whitespace().next()?;
    value.parse().ok()
}

fn parse_step_progress(line: &str) -> Option<(u64, Option<u64>)> {
    let normalized = line.replace(',', " ");
    let tokens = normalized.split_whitespace().collect::<Vec<_>>();
    for (index, token) in tokens.iter().enumerate() {
        if token.eq_ignore_ascii_case("step") {
            let step = tokens.get(index + 1)?.parse::<u64>().ok()?;
            let total = tokens
                .get(index + 2)
                .filter(|token| token.eq_ignore_ascii_case(&"of"))
                .and_then(|_| tokens.get(index + 3))
                .and_then(|value| value.parse::<u64>().ok());
            return Some((step, total));
        }
    }
    None
}

fn parse_amber_nstep(line: &str) -> Option<u64> {
    let normalized = line.replace('=', " ");
    let tokens = normalized.split_whitespace().collect::<Vec<_>>();
    for (index, token) in tokens.iter().enumerate() {
        if token.eq_ignore_ascii_case("nstep") {
            return tokens.get(index + 1)?.parse::<u64>().ok();
        }
    }
    None
}

fn parse_namd_step(line: &str) -> Option<u64> {
    let mut tokens = line.split_whitespace();
    match tokens
        .next()?
        .trim_end_matches(':')
        .to_ascii_lowercase()
        .as_str()
    {
        "energy" | "timing" => tokens.next()?.parse::<u64>().ok(),
        _ => None,
    }
}

fn event(kind: EngineLogEventKind, line_number: usize, message: String) -> EngineLogEvent {
    EngineLogEvent {
        kind,
        line_number,
        message,
    }
}

fn stage_parameter<'a>(plan: &'a SimulationPlan, stage_id: &str, key: &str) -> Option<&'a str> {
    plan.stages
        .iter()
        .find(|stage| stage.id == stage_id)
        .and_then(|stage| stage.parameters.get(key))
        .map(String::as_str)
}

fn nsteps_from_ps(duration_ps: f32, timestep_fs: f32) -> u64 {
    nsteps_from_ps_f64(f64::from(duration_ps), f64::from(timestep_fs))
}

fn nsteps_from_ps_f64(duration_ps: f64, timestep_fs: f64) -> u64 {
    ((duration_ps * 1000.0) / timestep_fs.max(0.001))
        .round()
        .max(1.0) as u64
}

fn format_number(value: f32) -> String {
    format_number_f64(f64::from(value))
}

fn format_number_f64(value: f64) -> String {
    let rounded = format!("{value:.6}");
    rounded
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

fn openmm_force_field_files(plan: &SimulationPlan) -> Vec<String> {
    let protein = plan.force_field.protein.to_ascii_lowercase();
    let water = plan.force_field.water_model.to_ascii_lowercase();

    if protein.contains("charmm") {
        vec!["charmm36.xml".to_string(), "charmm36/water.xml".to_string()]
    } else if protein.contains("opls") {
        vec!["oplsaa.xml".to_string()]
    } else {
        let water_xml = if water.contains("tip4p") {
            "amber14/tip4pew.xml"
        } else if water.contains("spc") {
            "amber14/spce.xml"
        } else {
            "amber14/tip3pfb.xml"
        };
        vec!["amber14-all.xml".to_string(), water_xml.to_string()]
    }
}

fn gromacs_force_field(value: &str) -> &'static str {
    let lower = value.to_ascii_lowercase();
    if lower.contains("ff19") || lower.contains("19sb") {
        // Prefer amber14sb when installed; shell picker falls back if missing.
        "amber14sb"
    } else if lower.contains("ff14") || lower.contains("14sb") {
        "amber14sb"
    } else if lower.contains("ff99") || lower.contains("ildn") {
        "amber99sb-ildn"
    } else if lower.contains("opls") {
        "oplsaa"
    } else if lower.contains("amber") {
        "amber14sb"
    } else {
        "charmm36-mar2019"
    }
}

fn gromacs_water_model(value: &str) -> &'static str {
    let lower = value.to_ascii_lowercase();
    if lower.contains("spc/e") || lower.contains("spce") {
        "spce"
    } else if lower.contains("tip4p") {
        "tip4p"
    } else if lower.contains("opc") {
        // OPC is not always packaged; warn via mapping and prefer tip3p only if picker must fall back.
        // When available, GROMACS uses "opc"; keep literal for modern installs.
        "opc"
    } else {
        "tip3p"
    }
}

fn gromacs_solvent_box(value: &str) -> &'static str {
    let lower = value.to_ascii_lowercase();
    if lower.contains("tip4p") {
        "tip4p.gro"
    } else if lower.contains("spc") {
        "spc216.gro"
    } else {
        // tip3p / opc / default: classic 3-site solvent configuration
        "spc216.gro"
    }
}

fn gromacs_box_shape(value: &str) -> &'static str {
    match value {
        "cubic" => "cubic",
        "octahedron" => "octahedron",
        _ => "dodecahedron",
    }
}

fn amber_force_field(value: &str) -> &'static str {
    let lower = value.to_ascii_lowercase();
    if lower.contains("ff14") {
        "leaprc.protein.ff14SB"
    } else if lower.contains("ff99") {
        "oldff/leaprc.ff99SB"
    } else {
        "leaprc.protein.ff19SB"
    }
}

fn amber_water_model(value: &str) -> &'static str {
    let lower = value.to_ascii_lowercase();
    if lower.contains("tip4p") {
        "tip4pew"
    } else if lower.contains("spc") {
        "spce"
    } else if lower.contains("opc") {
        "opc"
    } else {
        "tip3p"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine_registry;
    use crate::planner;
    use uuid::Uuid;

    fn test_plan() -> SimulationPlan {
        let mut plan = planner::default_simulation_plan(PlanRequest {
            project_id: None,
            name: "adapter-test".to_string(),
            engine_id: "gromacs".to_string(),
            domain: ProjectDomain::Biomolecular,
        });
        plan.id = Uuid::nil();
        plan
    }

    #[test]
    fn gromacs_package_contains_mdp_files_and_run_script() {
        let package = prepare_run_package(EngineRunRequest {
            plan: test_plan(),
            project_path: None,
            write_to_disk: false,
        })
        .expect("gromacs package");

        assert_eq!(package.engine_id, "gromacs");
        assert!(package
            .files
            .iter()
            .any(|file| file.path == "generated/gromacs/em.mdp"));
        assert!(package
            .files
            .iter()
            .any(|file| file.path.ends_with("run-gromacs.sh")));
        assert!(package
            .commands
            .iter()
            .any(|command| command.command.contains("gmx grompp")));
    }

    #[test]
    fn openmm_package_contains_python_runner_and_run_script() {
        let mut plan = test_plan();
        plan.engine_id = "openmm".to_string();

        let package = prepare_run_package(EngineRunRequest {
            plan,
            project_path: None,
            write_to_disk: false,
        })
        .expect("openmm package");

        assert_eq!(package.engine_id, "openmm");
        assert!(package
            .files
            .iter()
            .any(|file| file.path == "generated/openmm/run_openmm.py"));
        assert!(package
            .files
            .iter()
            .any(|file| file.path.ends_with("run-openmm.sh")));
        let run_script = package
            .files
            .iter()
            .find(|file| file.path.ends_with("run-openmm.sh"))
            .expect("OpenMM run script");
        assert!(run_script
            .contents
            .contains("engines/_tools/automd-science/bin/python"));
        assert!(package
            .commands
            .iter()
            .any(|command| command.command.contains("run_openmm.py")));
    }

    #[test]
    fn openmm_runner_adds_hydrogens_before_create_system() {
        let mut plan = test_plan();
        plan.engine_id = "openmm".to_string();

        let package = prepare_run_package(EngineRunRequest {
            plan,
            project_path: None,
            write_to_disk: false,
        })
        .expect("openmm package");
        let runner = package
            .files
            .iter()
            .find(|file| file.path == "generated/openmm/run_openmm.py")
            .expect("openmm runner")
            .contents
            .as_str();

        assert!(runner.contains("Modeller,") || runner.contains("Modeller("));
        assert!(runner.contains("modeller.addHydrogens(forcefield)"));
        assert!(runner.contains("forcefield.createSystem("));
        assert!(runner.contains("simulation.context.setPositions(positions)"));
        assert!(runner.contains("MonteCarloBarostat"));
    }

    #[test]
    fn ambertools_package_contains_tleap_mdin_and_cpptraj() {
        let mut plan = test_plan();
        plan.engine_id = "ambertools".to_string();
        plan.system.has_ligand = true;

        let package = prepare_run_package(EngineRunRequest {
            plan,
            project_path: None,
            write_to_disk: false,
        })
        .expect("ambertools package");

        assert_eq!(package.engine_id, "ambertools");
        assert!(package
            .files
            .iter()
            .any(|file| file.path == "generated/ambertools/tleap.in"));
        assert!(package
            .files
            .iter()
            .any(|file| file.path == "generated/ambertools/prod.mdin"));
        assert!(package
            .files
            .iter()
            .any(|file| file.path == "generated/ambertools/cpptraj.in"));
        assert!(package
            .files
            .iter()
            .any(|file| file.path.ends_with("run-ambertools.sh")));
        assert!(package
            .commands
            .iter()
            .any(|command| command.command.contains("sander -O")));
        assert!(package
            .commands
            .iter()
            .any(|command| command.command.contains("cpptraj")));
        assert!(package
            .warnings
            .iter()
            .any(|warning| warning.contains("mol2/frcmod")));
    }

    #[test]
    fn namd_package_contains_conf_and_license_warning() {
        let mut plan = test_plan();
        plan.engine_id = "namd".to_string();

        let package = prepare_run_package(EngineRunRequest {
            plan,
            project_path: None,
            write_to_disk: false,
        })
        .expect("namd package");

        assert_eq!(package.engine_id, "namd");
        assert!(package.files.iter().any(|file| {
            file.path == "generated/namd/automd.conf"
                && file
                    .contents
                    .contains("structure          inputs/system.psf")
        }));
        assert!(package
            .files
            .iter()
            .any(|file| file.path.ends_with("run-namd.sh")));
        assert!(package
            .commands
            .iter()
            .any(|command| command.command.contains("NAMD_BIN")));
        assert!(package
            .warnings
            .iter()
            .any(|warning| warning.contains("不下载")));
    }

    #[test]
    fn every_registered_engine_can_prepare_a_run_package() {
        for engine_id in engine_registry::known_engine_ids() {
            let mut plan = test_plan();
            plan.engine_id = engine_id.clone();

            let package = prepare_run_package(EngineRunRequest {
                plan,
                project_path: None,
                write_to_disk: false,
            })
            .unwrap_or_else(|error| panic!("{engine_id} should prepare package: {error}"));

            assert_eq!(package.engine_id, engine_id);
            assert!(
                !package.commands.is_empty(),
                "{engine_id} should expose commands"
            );
            assert!(
                package
                    .files
                    .iter()
                    .any(|file| file.path.contains("automd-plan.json")),
                "{engine_id} should preserve the normalized plan"
            );
        }
    }

    #[test]
    fn production_mdp_uses_duration_and_timestep() {
        let plan = test_plan();
        let mdp = gromacs_mdp(&plan, "production");
        assert!(mdp.contains("nsteps          = 50000000"));
        assert!(mdp.contains("dt              = 0.002"));
        assert!(mdp.contains("tc-grps         = System"));
        assert!(mdp.contains("tau_t           = 0.1"));
        assert!(mdp.contains("ref_t           = 300"));
        assert!(mdp.contains("DispCorr        = EnerPres"));
        assert!(mdp.contains("ref_p           = 1.0"));
        assert!(!mdp.contains("Protein Non-Protein"));
        assert!(!mdp.contains("-DPOSRES"));
    }

    #[test]
    fn equilibration_mdp_enables_posres_when_restraints_requested() {
        let plan = test_plan();
        let nvt = gromacs_mdp(&plan, "nvt");
        assert!(nvt.contains("tc-grps         = System"));
        assert!(nvt.contains("tau_t           = 0.1"));
        assert!(nvt.contains("ref_t           = 300"));
        assert!(nvt.contains("DispCorr        = EnerPres"));
        // Default plan sets nvt.restraints=heavy-atoms
        assert!(nvt.contains("define          = -DPOSRES"));
        let npt = gromacs_mdp(&plan, "npt");
        // NPT inherits restraints from nvt when present
        assert!(npt.contains("define          = -DPOSRES"));
    }

    #[test]
    fn gromacs_run_script_selects_available_force_field() {
        let plan = test_plan();
        let commands = gromacs_commands(&plan, "runs/gromacs-test");
        let run_script = gromacs_run_script(&plan, &commands);

        assert!(run_script.contains("automd_pick_gromacs_force_field"));
        assert!(run_script.contains("automd_gromacs_gpu_suffix"));
        assert!(run_script.contains("automd_gromacs_mdrun()"));
        assert!(run_script.contains("retrying CPU mode"));
        assert!(run_script.contains("nvidia-smi -L"));
        assert!(run_script.contains("AUTOMD_GROMACS_FF="));
        assert!(run_script.contains("gmx pdb2gmx -ignh"));
        assert!(run_script.contains(
            "OMP_NUM_THREADS=8 automd_gromacs_mdrun cpu -deffnm runs/gromacs-test/em -ntomp 8"
        ));
        // First production must not require a missing md.cpt; only pass -cpi when file exists.
        assert!(run_script.contains("if [ -f runs/gromacs-test/md.cpt ]"));
        assert!(run_script.contains("AUTOMD_MD_CPI"));
        assert!(run_script.contains("printf 'Backbone\\nBackbone\\n' | gmx rms"));
        assert!(run_script.contains("printf 'Backbone\\n' | gmx gyrate"));
        assert!(run_script.contains("-ff \"$AUTOMD_GROMACS_FF\""));
    }

    #[test]
    fn ambertools_tleap_converts_padding_nm_to_angstrom() {
        let mut plan = test_plan();
        plan.engine_id = "ambertools".to_string();
        plan.solvent.padding_nm = 1.0;
        let package = prepare_run_package(EngineRunRequest {
            plan,
            project_path: None,
            write_to_disk: false,
        })
        .expect("ambertools package");
        let tleap = package
            .files
            .iter()
            .find(|file| file.path.ends_with("tleap.in"))
            .expect("tleap")
            .contents
            .as_str();
        assert!(
            tleap.contains("TIP3PBOX 10") || tleap.contains("TIP3PBOX 10.0"),
            "expected 1.0 nm -> 10 Å padding, got:\n{tleap}"
        );
        assert!(
            !tleap.contains("TIP3PBOX 1\n") && !tleap.contains("TIP3PBOX 1.0\n"),
            "must not pass padding_nm raw as Angstrom"
        );
    }


    #[test]
    fn ambertools_add_salt_script_estimates_pairs_from_water() {
        let mut plan = test_plan();
        plan.engine_id = "ambertools".to_string();
        plan.solvent.ionic_strength_molar = 0.15;
        let package = prepare_run_package(EngineRunRequest {
            plan,
            project_path: None,
            write_to_disk: false,
        })
        .expect("ambertools package");
        let salt_py = package
            .files
            .iter()
            .find(|f| f.path.ends_with("add_salt.py"))
            .expect("add_salt.py")
            .contents
            .as_str();
        assert!(salt_py.contains("55.5"));
        assert!(salt_py.contains("n_pairs"));
        assert!(package.commands.iter().any(|c| c.stage_id == "ambertools-salt"));
        assert!(package
            .commands
            .iter()
            .any(|c| c.command.contains("add_salt.py") && c.command.contains("tleap_salt.in")));
    }

    #[test]
    fn ambertools_mdin_requests_netcdf_trajectory() {
        let mut plan = test_plan();
        plan.engine_id = "ambertools".to_string();
        let package = prepare_run_package(EngineRunRequest {
            plan,
            project_path: None,
            write_to_disk: false,
        })
        .expect("ambertools package");
        for name in ["heat.mdin", "equil.mdin", "prod.mdin"] {
            let mdin = package
                .files
                .iter()
                .find(|file| file.path.ends_with(name))
                .unwrap_or_else(|| panic!("missing {name}"))
                .contents
                .as_str();
            assert!(mdin.contains("ioutfm=1"), "{name} should set ioutfm=1");
            assert!(mdin.contains("ntxo=2"), "{name} should set ntxo=2");
        }
    }

    #[test]
    fn openmm_runner_emits_barostat_and_solvation() {
        let mut plan = test_plan();
        plan.engine_id = "openmm".to_string();
        let package = prepare_run_package(EngineRunRequest {
            plan,
            project_path: None,
            write_to_disk: false,
        })
        .expect("openmm package");
        let runner = package
            .files
            .iter()
            .find(|file| file.path.ends_with("run_openmm.py"))
            .expect("runner")
            .contents
            .as_str();
        assert!(runner.contains("MonteCarloBarostat"));
        assert!(runner.contains("addSolvent"));
        assert!(runner.contains("NVT equilibration"));
        assert!(runner.contains("NPT equilibration"));
        assert!(runner.contains("pick_platform"));
    }

    #[test]
    fn namd_conf_includes_langevin_piston_for_npt() {
        let mut plan = test_plan();
        plan.engine_id = "namd".to_string();
        let package = prepare_run_package(EngineRunRequest {
            plan,
            project_path: None,
            write_to_disk: false,
        })
        .expect("namd package");
        let conf = package
            .files
            .iter()
            .find(|file| file.path.ends_with("automd.conf"))
            .expect("conf")
            .contents
            .as_str();
        assert!(conf.contains("langevinPiston        on"));
        assert!(conf.contains("langevinPistonTarget"));
    }

    #[test]
    fn gromacs_commands_respect_disabled_stages_and_chain_coords() {
        let mut plan = test_plan();
        // Disable NVT and analysis; keep em, npt, production.
        for stage in &mut plan.stages {
            if stage.id == "nvt" || stage.id == "analysis" {
                stage.enabled = false;
            }
        }
        let commands = gromacs_commands(&plan, "runs/gromacs-test");
        let ids: Vec<_> = commands.iter().map(|c| c.stage_id.as_str()).collect();
        assert!(ids.contains(&"em"));
        assert!(!ids.contains(&"nvt"));
        assert!(ids.contains(&"npt"));
        assert!(ids.contains(&"production"));
        assert!(!ids.contains(&"analysis"));
        // NPT should chain from EM coordinates when NVT is skipped.
        let npt = commands.iter().find(|c| c.stage_id == "npt").expect("npt");
        assert!(
            npt.command.contains("-c runs/gromacs-test/em.gro"),
            "NPT should use EM coordinates when NVT is disabled: {}",
            npt.command
        );
        assert!(
            !npt.command.contains("-t runs/gromacs-test/nvt.cpt"),
            "NPT must not require missing NVT checkpoint"
        );
    }

    #[test]
    fn ambertools_commands_skip_disabled_production_and_analysis() {
        let mut plan = test_plan();
        plan.engine_id = "ambertools".to_string();
        for stage in &mut plan.stages {
            if stage.id == "production" || stage.id == "analysis" {
                stage.enabled = false;
            }
        }
        let commands = ambertools_commands(&plan, "runs/amber-test");
        let ids: Vec<_> = commands.iter().map(|c| c.stage_id.as_str()).collect();
        assert!(ids.iter().any(|id| id.contains("tleap") || id.contains("min")));
        assert!(!ids.iter().any(|id| *id == "ambertools-prod"));
        assert!(!ids.iter().any(|id| *id == "ambertools-analysis"));
    }

    #[test]
    fn gromacs_log_parser_extracts_performance_and_errors() {
        let report = parse_gromacs_log(
            r#"
step 2500 of 10000
Writing checkpoint, step 2500
Performance:        82.125        ns/day
Fatal error:
Bad topology
"#,
        );
        assert_eq!(report.current_step, Some(2500));
        assert_eq!(report.ns_per_day, Some(82.125));
        assert_eq!(report.progress_percent, Some(25.0));
        assert!(report.fatal_error.is_some());
        assert!(report
            .events
            .iter()
            .any(|event| event.kind == EngineLogEventKind::Checkpoint));
    }

    #[test]
    fn openmm_log_parser_marks_python_tracebacks_as_fatal() {
        let report = parse_engine_log(EngineLogParseRequest {
            engine_id: "openmm".to_string(),
            log_contents: "Traceback (most recent call last):\nValueError: No template found for residue 104 (LEU).\n".to_string(),
        })
        .expect("openmm log report");

        assert_eq!(report.engine_id, "openmm");
        assert!(report.fatal_error.is_some());
        assert!(report
            .events
            .iter()
            .any(|event| event.kind == EngineLogEventKind::Error));
    }

    #[test]
    fn gromacs_failure_classifier_identifies_missing_force_field() {
        let analysis = classify_engine_failure(FailureAnalysisRequest {
            engine_id: "gromacs".to_string(),
            log_contents: "Fatal error:\nAtomtype CG2R61 not found".to_string(),
            exit_code: Some(1),
        })
        .expect("failure analysis");

        assert_eq!(analysis.category, FailureCategory::MissingForceField);
        assert!(!analysis.suggestions.is_empty());
    }

    #[test]
    fn ambertools_log_parser_and_classifier_identify_parameter_gap() {
        let report = parse_engine_log(EngineLogParseRequest {
            engine_id: "ambertools".to_string(),
            log_contents:
                "NSTEP = 500 TIME(PS) = 1.0\nSANDER BOMB in subroutine\nUnknown atom type c3\n"
                    .to_string(),
        })
        .expect("ambertools log report");

        assert_eq!(report.engine_id, "ambertools");
        assert_eq!(report.current_step, Some(500));
        assert!(report.fatal_error.is_some());

        let analysis = classify_engine_failure(FailureAnalysisRequest {
            engine_id: "ambertools".to_string(),
            log_contents: "Could not find mol2/frcmod parameters for ligand".to_string(),
            exit_code: Some(1),
        })
        .expect("ambertools failure analysis");

        assert_eq!(analysis.category, FailureCategory::MissingForceField);
        assert!(!analysis.suggestions.is_empty());
    }

    #[test]
    fn namd_log_parser_and_classifier_identify_missing_executable() {
        let report = parse_engine_log(EngineLogParseRequest {
            engine_id: "namd".to_string(),
            log_contents:
                "ENERGY: 100 0 0 0\nWRITING COORDINATES TO RESTART FILE\nEnd of program\n"
                    .to_string(),
        })
        .expect("namd log report");

        assert_eq!(report.engine_id, "namd");
        assert_eq!(report.current_step, Some(100));
        assert_eq!(report.progress_percent, Some(100.0));
        assert!(report
            .events
            .iter()
            .any(|event| event.kind == EngineLogEventKind::Checkpoint));

        let analysis = classify_engine_failure(FailureAnalysisRequest {
            engine_id: "namd".to_string(),
            log_contents: "namd3: command not found".to_string(),
            exit_code: Some(127),
        })
        .expect("namd failure analysis");

        assert_eq!(analysis.category, FailureCategory::MissingExecutable);
        assert!(!analysis.suggestions.is_empty());
    }

    #[test]
    fn gromacs_resume_plan_discovers_checkpoint_and_command() {
        let root = std::env::temp_dir().join(format!("automd-resume-test-{}", Uuid::new_v4()));
        let run_dir = "runs/gromacs-test";
        let checkpoint_path = root.join(run_dir).join("md.cpt");
        fs::create_dir_all(checkpoint_path.parent().expect("checkpoint parent")).expect("run dir");
        fs::write(&checkpoint_path, b"checkpoint").expect("checkpoint");

        let resume_plan = discover_resume_plan(ResumePlanRequest {
            project_path: root.display().to_string(),
            run_directory: run_dir.to_string(),
            engine_id: "gromacs".to_string(),
        })
        .expect("resume plan");

        assert_eq!(resume_plan.checkpoints.len(), 1);
        assert_eq!(
            resume_plan.recommended.expect("recommended").path,
            "runs/gromacs-test/md.cpt"
        );
        assert_eq!(
            resume_plan.resume_command.expect("resume command"),
            "gmx mdrun -deffnm runs/gromacs-test/md -cpi runs/gromacs-test/md.cpt -append"
        );

        fs::remove_dir_all(root).expect("cleanup temp root");
    }

    #[test]
    fn openmm_resume_plan_discovers_checkpoint_and_command() {
        let root =
            std::env::temp_dir().join(format!("automd-openmm-resume-test-{}", Uuid::new_v4()));
        let run_dir = "runs/openmm-test";
        let checkpoint_path = root.join(run_dir).join("openmm.chk");
        fs::create_dir_all(checkpoint_path.parent().expect("checkpoint parent")).expect("run dir");
        fs::write(&checkpoint_path, b"checkpoint").expect("checkpoint");

        let resume_plan = discover_resume_plan(ResumePlanRequest {
            project_path: root.display().to_string(),
            run_directory: run_dir.to_string(),
            engine_id: "openmm".to_string(),
        })
        .expect("resume plan");

        assert_eq!(resume_plan.checkpoints.len(), 1);
        assert_eq!(
            resume_plan.recommended.expect("recommended").path,
            "runs/openmm-test/openmm.chk"
        );
        assert_eq!(
            resume_plan.resume_command.expect("resume command"),
            "python generated/openmm/run_openmm.py --plan generated/openmm/automd-plan.json --out runs/openmm-test --resume runs/openmm-test/openmm.chk"
        );

        fs::remove_dir_all(root).expect("cleanup temp root");
    }

    #[test]
    fn gromacs_package_can_be_written_to_project_directory() {
        let root = std::env::temp_dir().join(format!("automd-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("temp root");

        let package = prepare_run_package(EngineRunRequest {
            plan: test_plan(),
            project_path: Some(root.display().to_string()),
            write_to_disk: true,
        })
        .expect("written package");

        assert!(package.files.iter().all(|file| file.written));
        assert!(root.join("generated/gromacs/em.mdp").exists());
        assert!(root
            .join(&package.run_directory)
            .join("run-gromacs.sh")
            .exists());

        fs::remove_dir_all(root).expect("cleanup temp root");
    }
}
