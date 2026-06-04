use crate::engine_adapters::EngineAdapterError;
use crate::models::*;
use serde_json::to_string_pretty;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

pub fn diagnostics(engines_root: Option<&Path>) -> ScienceSidecarDiagnostics {
    let sidecar_python = engines_root.and_then(sidecar_python_executable);
    let python_executable = sidecar_python
        .or_else(|| which::which("python3").ok())
        .map(|path| path.display().to_string());
    let python_command = python_executable.as_deref().unwrap_or("python3");
    let sidecar_bin = engines_root.map(|root| root.join("_tools").join("automd-science").join(bin_dir_name()));
    let mut tools = vec![
        python_module(python_command, "openmm", "OpenMM", "openmm"),
        python_module(python_command, "pdbfixer", "PDBFixer", "pdbfixer"),
        python_module(python_command, "mdanalysis", "MDAnalysis", "MDAnalysis"),
        python_module(python_command, "mdtraj", "MDTraj", "mdtraj"),
        python_module(python_command, "rdkit", "RDKit", "rdkit"),
        python_module(python_command, "openbabel", "Open Babel Python", "openbabel"),
    ];
    tools.extend([
        executable("tleap", "AmberTools tleap", sidecar_bin.as_deref()),
        executable("antechamber", "AmberTools antechamber", sidecar_bin.as_deref()),
        executable("parmchk2", "AmberTools parmchk2", sidecar_bin.as_deref()),
        executable("cpptraj", "AmberTools cpptraj", sidecar_bin.as_deref()),
    ]);

    let warnings = tools
        .iter()
        .filter(|tool| tool.status != DetectionStatus::Ready)
        .map(|tool| format!("{} is not available: {}", tool.label, tool.detail))
        .collect();

    ScienceSidecarDiagnostics {
        python_executable,
        tools,
        environment_recipe: sidecar_environment_yml(),
        warnings,
    }
}

fn bin_dir_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "Scripts"
    } else {
        "bin"
    }
}

fn sidecar_python_executable(engines_root: &Path) -> Option<PathBuf> {
    let executable = if cfg!(target_os = "windows") {
        "python.exe"
    } else {
        "python"
    };
    let python = engines_root
        .join("_tools")
        .join("automd-science")
        .join(bin_dir_name())
        .join(executable);
    python.is_file().then_some(python)
}

pub fn prepare_structure_package(
    request: StructurePreparationRequest,
) -> Result<StructurePreparationPackage, EngineAdapterError> {
    let plan = request.plan;
    let generated_directory = "generated/prep".to_string();
    let mut warnings = Vec::new();

    if plan.system.source_path.is_none() {
        warnings.push("未设置输入结构路径；prepare_structure.py 会使用 inputs/system.pdb 占位。".to_string());
    }
    if plan.system.has_ligand {
        warnings.push("检测到配体体系；侧车会生成 ligand_parameterization.md，但不会在无验证参数时自动合并配体拓扑。".to_string());
    }
    if plan.system.has_membrane {
        warnings.push("膜体系需要专门构建流程；当前准备脚本只覆盖普通显式溶剂盒。".to_string());
    }

    let commands = vec![
        EngineCommand {
            stage_id: "science-sidecar-diagnostics".to_string(),
            label: "检测 Python 科学侧车依赖".to_string(),
            command: "python3 generated/prep/prepare_structure.py --diagnostics".to_string(),
            working_directory: ".".to_string(),
            expected_outputs: Vec::new(),
        },
        EngineCommand {
            stage_id: "science-sidecar-prepare".to_string(),
            label: "运行结构修复/加氢/溶剂盒准备".to_string(),
            command: "python3 generated/prep/prepare_structure.py --plan generated/prep/automd-plan.json --project .".to_string(),
            working_directory: ".".to_string(),
            expected_outputs: vec![
                "generated/prep/prepared_structure.pdb".to_string(),
                "generated/prep/structure-prep-report.json".to_string(),
            ],
        },
    ];

    let mut files = vec![
        EngineRunFile {
            path: "generated/prep/automd-plan.json".to_string(),
            language: "json".to_string(),
            contents: to_string_pretty(&plan)?,
            written: false,
        },
        EngineRunFile {
            path: "generated/prep/prepare_structure.py".to_string(),
            language: "python".to_string(),
            contents: prepare_structure_py(&plan),
            written: false,
        },
        EngineRunFile {
            path: "generated/prep/environment.yml".to_string(),
            language: "yaml".to_string(),
            contents: sidecar_environment_yml(),
            written: false,
        },
        EngineRunFile {
            path: "generated/prep/ligand_parameterization.md".to_string(),
            language: "markdown".to_string(),
            contents: ligand_parameterization_md(&plan),
            written: false,
        },
        EngineRunFile {
            path: "generated/prep/README.md".to_string(),
            language: "markdown".to_string(),
            contents: prep_readme(&plan, &warnings),
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

    Ok(StructurePreparationPackage {
        plan_id: plan.id,
        generated_directory,
        commands,
        files,
        warnings,
        writable: request.project_path.is_some(),
    })
}

pub fn prepare_analysis_package(
    request: TrajectoryAnalysisRequest,
) -> Result<TrajectoryAnalysisPackage, EngineAdapterError> {
    let plan = request.plan;
    let generated_directory = "generated/analysis".to_string();
    let topology_path = request
        .topology_path
        .or_else(|| plan.system.source_path.clone())
        .unwrap_or_else(|| "generated/prep/prepared_structure.pdb".to_string());
    let trajectory_path = request
        .trajectory_path
        .unwrap_or_else(|| default_trajectory_path(&plan.engine_id).to_string());
    let selection = if request.selection.trim().is_empty() {
        "protein and name CA".to_string()
    } else {
        request.selection.trim().to_string()
    };
    let expected_outputs = vec![
        "analysis/mdanalysis_rmsd.csv".to_string(),
        "analysis/mdanalysis_rg.csv".to_string(),
        "analysis/mdanalysis_rmsf.csv".to_string(),
        "analysis/mdanalysis_contacts.csv".to_string(),
        "analysis/mdanalysis_hbonds.csv".to_string(),
        "analysis/mdanalysis_distances.csv".to_string(),
        "analysis/mdanalysis_angles.csv".to_string(),
        "analysis/mdanalysis_dihedrals.csv".to_string(),
        "analysis/mdanalysis-summary.json".to_string(),
    ];
    let mut warnings = Vec::new();

    if plan.system.source_path.is_none() && request.project_path.is_none() {
        warnings.push("未设置 topology/source path；脚本会默认尝试 generated/prep/prepared_structure.pdb。".to_string());
    }
    if trajectory_path.ends_with(".xtc")
        || trajectory_path.ends_with(".trr")
        || trajectory_path.ends_with(".dcd")
        || trajectory_path.ends_with(".nc")
        || trajectory_path.ends_with(".gsd")
    {
        warnings.push("二进制轨迹需要 MDAnalysis 可读 topology 和对应 reader；如果缺少依赖，脚本只会写出诊断报告。".to_string());
    }

    let commands = vec![
        EngineCommand {
            stage_id: "science-sidecar-analysis-diagnostics".to_string(),
            label: "检测 MDAnalysis 分析侧车依赖".to_string(),
            command: "python3 generated/analysis/run_mdanalysis.py --diagnostics".to_string(),
            working_directory: ".".to_string(),
            expected_outputs: Vec::new(),
        },
        EngineCommand {
            stage_id: "science-sidecar-analysis".to_string(),
            label: "运行 MDAnalysis RMSD/RMSF/Rg/氢键/接触分析".to_string(),
            command: format!(
                "python3 generated/analysis/run_mdanalysis.py --plan generated/analysis/automd-plan.json --project . --topology {} --trajectory {} --selection {}",
                shell_quote(&topology_path),
                shell_quote(&trajectory_path),
                shell_quote(&selection)
            ),
            working_directory: ".".to_string(),
            expected_outputs: expected_outputs.clone(),
        },
    ];

    let mut files = vec![
        EngineRunFile {
            path: "generated/analysis/automd-plan.json".to_string(),
            language: "json".to_string(),
            contents: to_string_pretty(&plan)?,
            written: false,
        },
        EngineRunFile {
            path: "generated/analysis/run_mdanalysis.py".to_string(),
            language: "python".to_string(),
            contents: analysis_sidecar_py(&topology_path, &trajectory_path, &selection),
            written: false,
        },
        EngineRunFile {
            path: "generated/analysis/environment.yml".to_string(),
            language: "yaml".to_string(),
            contents: sidecar_environment_yml(),
            written: false,
        },
        EngineRunFile {
            path: "generated/analysis/README.md".to_string(),
            language: "markdown".to_string(),
            contents: analysis_readme(&plan, &topology_path, &trajectory_path, &selection, &expected_outputs, &warnings),
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

    Ok(TrajectoryAnalysisPackage {
        plan_id: plan.id,
        generated_directory,
        commands,
        files,
        expected_outputs,
        warnings,
        writable: request.project_path.is_some(),
    })
}

fn python_module(python_command: &str, id: &str, label: &str, import_name: &str) -> ScienceToolDiagnostic {
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
    match Command::new(python_command).args(["-c", &script]).output() {
        Ok(output) if output.status.success() => ScienceToolDiagnostic {
            id: id.to_string(),
            label: label.to_string(),
            import_name: Some(import_name.to_string()),
            command: None,
            status: DetectionStatus::Ready,
            version: String::from_utf8(output.stdout)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            detail: format!("{} can import {import_name}", python_command),
        },
        _ => ScienceToolDiagnostic {
            id: id.to_string(),
            label: label.to_string(),
            import_name: Some(import_name.to_string()),
            command: None,
            status: DetectionStatus::MissingInstall,
            version: None,
            detail: format!("{python_command} cannot import {import_name}"),
        },
    }
}

fn executable(command: &str, label: &str, preferred_bin_dir: Option<&Path>) -> ScienceToolDiagnostic {
    let preferred_path = preferred_bin_dir
        .map(|dir| dir.join(command))
        .filter(|path| path.is_file());
    let found = preferred_path.or_else(|| which::which(command).ok());
    match found {
        Some(path) => ScienceToolDiagnostic {
            id: command.to_string(),
            label: label.to_string(),
            import_name: None,
            command: Some(command.to_string()),
            status: DetectionStatus::Ready,
            version: None,
            detail: path.display().to_string(),
        },
        None => ScienceToolDiagnostic {
            id: command.to_string(),
            label: label.to_string(),
            import_name: None,
            command: Some(command.to_string()),
            status: DetectionStatus::MissingInstall,
            version: None,
            detail: format!("未在 PATH 中找到 {command}"),
        },
    }
}

fn prepare_structure_py(plan: &SimulationPlan) -> String {
    let input = plan
        .system
        .source_path
        .as_deref()
        .unwrap_or("inputs/system.pdb");
    let input_json = serde_json::to_string(input).unwrap_or_else(|_| "\"inputs/system.pdb\"".to_string());
    let water_model = serde_json::to_string(&plan.force_field.water_model).unwrap_or_else(|_| "\"TIP3P\"".to_string());
    let padding_nm = plan.solvent.padding_nm;
    let ionic_strength = plan.solvent.ionic_strength_molar;
    let neutralize = plan.solvent.neutralize;

    format!(
        r#"#!/usr/bin/env python3
from __future__ import annotations

import argparse
import importlib.util
import json
import shutil
from pathlib import Path

DEFAULT_INPUT = {input_json}
DEFAULT_WATER_MODEL = {water_model}
DEFAULT_PADDING_NM = {padding_nm}
DEFAULT_IONIC_STRENGTH = {ionic_strength}
DEFAULT_NEUTRALIZE = {neutralize}


def module_available(name: str) -> bool:
    return importlib.util.find_spec(name) is not None


def diagnostics() -> int:
    modules = ["openmm", "pdbfixer", "MDAnalysis", "rdkit", "openbabel"]
    for module in modules:
        print(f"{{module}}={{'ready' if module_available(module) else 'missing'}}")
    return 0


def resolve_project_root(plan_path: Path | None, project_arg: str | None) -> Path:
    if project_arg:
        return Path(project_arg).resolve()
    if plan_path and len(plan_path.parents) >= 3:
        return plan_path.parents[2].resolve()
    return Path.cwd().resolve()


def run_prepare(plan_path: Path, project: str | None) -> int:
    project_root = resolve_project_root(plan_path, project)
    plan = json.loads(plan_path.read_text(encoding="utf-8"))
    source = Path(plan.get("system", {{}}).get("sourcePath") or DEFAULT_INPUT)
    if not source.is_absolute():
        source = project_root / source
    out_dir = project_root / "generated" / "prep"
    out_dir.mkdir(parents=True, exist_ok=True)
    output_pdb = out_dir / "prepared_structure.pdb"
    report_path = out_dir / "structure-prep-report.json"
    report = {{
        "source": str(source),
        "output": str(output_pdb),
        "actions": [],
        "warnings": [],
        "waterModel": DEFAULT_WATER_MODEL,
        "paddingNm": DEFAULT_PADDING_NM,
        "ionicStrengthMolar": DEFAULT_IONIC_STRENGTH,
        "neutralize": DEFAULT_NEUTRALIZE
    }}

    if not source.exists():
        report["warnings"].append(f"Input structure not found: {{source}}")
        report_path.write_text(json.dumps(report, indent=2), encoding="utf-8")
        return 2

    if module_available("pdbfixer") and module_available("openmm"):
        from pdbfixer import PDBFixer
        from openmm.app import PDBFile

        fixer = PDBFixer(filename=str(source))
        fixer.findMissingResidues()
        fixer.findNonstandardResidues()
        fixer.replaceNonstandardResidues()
        report["actions"].append("replaceNonstandardResidues")
        fixer.findMissingAtoms()
        fixer.addMissingAtoms()
        report["actions"].append("addMissingAtoms")
        fixer.addMissingHydrogens(7.0)
        report["actions"].append("addMissingHydrogens")
        with output_pdb.open("w", encoding="utf-8") as handle:
            PDBFile.writeFile(fixer.topology, fixer.positions, handle)
    else:
        shutil.copyfile(source, output_pdb)
        report["warnings"].append("PDBFixer/OpenMM missing; copied source structure without repair.")

    if plan.get("system", {{}}).get("hasLigand"):
        report["warnings"].append("Ligand parameterization is documented in ligand_parameterization.md but not automatically merged.")

    report_path.write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(f"Prepared structure written to {{output_pdb}}")
    print(f"Report written to {{report_path}}")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="AutoMD science sidecar structure preparation")
    parser.add_argument("--diagnostics", action="store_true")
    parser.add_argument("--plan", default="generated/prep/automd-plan.json")
    parser.add_argument("--project", default=None)
    args = parser.parse_args()
    if args.diagnostics:
        return diagnostics()
    return run_prepare(Path(args.plan), args.project)


if __name__ == "__main__":
    raise SystemExit(main())
"#
    )
}

fn analysis_sidecar_py(topology_path: &str, trajectory_path: &str, selection: &str) -> String {
    let topology_json = serde_json::to_string(topology_path).unwrap_or_else(|_| "\"generated/prep/prepared_structure.pdb\"".to_string());
    let trajectory_json = serde_json::to_string(trajectory_path).unwrap_or_else(|_| "\"trajectories/openmm.dcd\"".to_string());
    let selection_json = serde_json::to_string(selection).unwrap_or_else(|_| "\"protein and name CA\"".to_string());
    let template = r#"#!/usr/bin/env python3
from __future__ import annotations

import argparse
import csv
import importlib.util
import json
import math
from pathlib import Path

DEFAULT_TOPOLOGY = __DEFAULT_TOPOLOGY_JSON__
DEFAULT_TRAJECTORY = __DEFAULT_TRAJECTORY_JSON__
DEFAULT_SELECTION = __DEFAULT_SELECTION_JSON__


def module_available(name: str) -> bool:
    return importlib.util.find_spec(name) is not None


def diagnostics() -> int:
    modules = ["MDAnalysis", "numpy", "pandas"]
    for module in modules:
        print(f"{module}={'ready' if module_available(module) else 'missing'}")
    return 0


def resolve_project_root(plan_path: Path | None, project_arg: str | None) -> Path:
    if project_arg:
        return Path(project_arg).resolve()
    if plan_path and len(plan_path.parents) >= 3:
        return plan_path.parents[2].resolve()
    return Path.cwd().resolve()


def resolve_project_path(project_root: Path, value: str) -> Path:
    path = Path(value)
    if path.is_absolute():
        return path
    return project_root / path


def write_csv(path: Path, header: list[str], rows: list[list[object]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.writer(handle)
        writer.writerow(header)
        writer.writerows(rows)


def write_summary(path: Path, summary: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(summary, indent=2), encoding="utf-8")


def contact_count(coords, np_module, cutoff_angstrom: float = 8.0) -> int | None:
    if len(coords) < 2:
        return 0
    if len(coords) > 3000:
        return None
    delta = coords[:, None, :] - coords[None, :, :]
    distances = np_module.sqrt((delta * delta).sum(axis=2))
    mask = np_module.triu(distances < cutoff_angstrom, k=1)
    return int(mask.sum())


def angle_degrees(a, b, c, np_module) -> float:
    ba = a - b
    bc = c - b
    denom = np_module.linalg.norm(ba) * np_module.linalg.norm(bc)
    if denom == 0:
        return 0.0
    cosine = float(np_module.dot(ba, bc) / denom)
    cosine = max(-1.0, min(1.0, cosine))
    return float(math.degrees(math.acos(cosine)))


def dihedral_degrees(a, b, c, d, np_module) -> float:
    b0 = -(b - a)
    b1 = c - b
    b2 = d - c
    norm = np_module.linalg.norm(b1)
    if norm == 0:
        return 0.0
    b1 = b1 / norm
    v = b0 - np_module.dot(b0, b1) * b1
    w = b2 - np_module.dot(b2, b1) * b1
    x = np_module.dot(v, w)
    y = np_module.dot(np_module.cross(b1, v), w)
    return float(math.degrees(math.atan2(y, x)))


def run_hbond_analysis(universe, output_path: Path, frame_times: dict[int, float], warnings: list[str]) -> None:
    rows = []
    try:
        from MDAnalysis.analysis.hydrogenbonds.hbond_analysis import HydrogenBondAnalysis

        hbonds = HydrogenBondAnalysis(universe=universe)
        hbonds.run()
        counts: dict[int, int] = {}
        for row in getattr(hbonds.results, "hbonds", []):
            frame = int(row[0])
            counts[frame] = counts.get(frame, 0) + 1
        for frame, time_ps in sorted(frame_times.items()):
            rows.append([time_ps, counts.get(frame, 0)])
    except Exception as exc:
        warnings.append(f"Hydrogen bond analysis skipped: {exc}")
    write_csv(output_path, ["time_ps", "hbond_count"], rows)


def run_analysis(plan_path: Path, project_arg: str | None, topology_arg: str, trajectory_arg: str, selection: str) -> int:
    project_root = resolve_project_root(plan_path, project_arg)
    analysis_dir = project_root / "analysis"
    summary_path = analysis_dir / "mdanalysis-summary.json"
    summary = {
        "topology": topology_arg,
        "trajectory": trajectory_arg,
        "selection": selection,
        "frames": 0,
        "selectedAtoms": 0,
        "outputs": [],
        "warnings": []
    }

    topology = resolve_project_path(project_root, topology_arg)
    trajectory = resolve_project_path(project_root, trajectory_arg)
    if not topology.exists():
        summary["warnings"].append(f"Topology file not found: {topology}")
    if not trajectory.exists():
        summary["warnings"].append(f"Trajectory file not found: {trajectory}")
    if summary["warnings"]:
        write_summary(summary_path, summary)
        return 2

    try:
        import MDAnalysis as mda
        import numpy as np
    except Exception as exc:
        summary["warnings"].append(f"MDAnalysis/numpy import failed: {exc}")
        write_summary(summary_path, summary)
        return 2

    try:
        universe = mda.Universe(str(topology), str(trajectory))
    except Exception as exc:
        summary["warnings"].append(f"Could not load topology/trajectory with MDAnalysis: {exc}")
        write_summary(summary_path, summary)
        return 2

    try:
        atoms = universe.select_atoms(selection)
    except Exception as exc:
        summary["warnings"].append(f"Selection failed and all atoms were used instead: {exc}")
        atoms = universe.atoms
    if len(atoms) == 0:
        summary["warnings"].append("Selection matched zero atoms; all atoms were used instead.")
        atoms = universe.atoms
    summary["selectedAtoms"] = int(len(atoms))

    rmsd_rows = []
    rg_rows = []
    contact_rows = []
    distance_rows = []
    angle_rows = []
    dihedral_rows = []
    frame_times: dict[int, float] = {}
    reference = None
    mean = None
    m2 = None
    observed = 0

    for timestep in universe.trajectory:
        coords = atoms.positions.astype(float).copy()
        frame = int(getattr(timestep, "frame", observed))
        time_ps = float(getattr(timestep, "time", frame))
        frame_times[frame] = time_ps
        if reference is None:
            reference = coords.copy()
        observed += 1

        diff = coords - reference
        rmsd = math.sqrt(float((diff * diff).sum(axis=1).mean())) if len(coords) else 0.0
        rmsd_rows.append([time_ps, rmsd])

        try:
            rg_rows.append([time_ps, float(atoms.radius_of_gyration())])
        except Exception as exc:
            summary["warnings"].append(f"Radius of gyration skipped at frame {frame}: {exc}")

        contacts = contact_count(coords, np)
        if contacts is None:
            summary["warnings"].append("Contact count skipped because selected atom group is too large for dense pair distances.")
        else:
            contact_rows.append([time_ps, contacts])

        if len(coords) >= 2:
            distance_rows.append([time_ps, float(np.linalg.norm(coords[1] - coords[0]))])
        if len(coords) >= 3:
            angle_rows.append([time_ps, angle_degrees(coords[0], coords[1], coords[2], np)])
        if len(coords) >= 4:
            dihedral_rows.append([time_ps, dihedral_degrees(coords[0], coords[1], coords[2], coords[3], np)])

        if mean is None:
            mean = np.zeros_like(coords)
            m2 = np.zeros_like(coords)
        delta = coords - mean
        mean = mean + delta / observed
        delta2 = coords - mean
        m2 = m2 + delta * delta2

    summary["frames"] = observed
    write_csv(analysis_dir / "mdanalysis_rmsd.csv", ["time_ps", "rmsd_angstrom"], rmsd_rows)
    write_csv(analysis_dir / "mdanalysis_rg.csv", ["time_ps", "rg_angstrom"], rg_rows)
    write_csv(analysis_dir / "mdanalysis_contacts.csv", ["time_ps", "contact_count"], contact_rows)
    write_csv(analysis_dir / "mdanalysis_distances.csv", ["time_ps", "distance_angstrom"], distance_rows)
    write_csv(analysis_dir / "mdanalysis_angles.csv", ["time_ps", "angle_degrees"], angle_rows)
    write_csv(analysis_dir / "mdanalysis_dihedrals.csv", ["time_ps", "dihedral_degrees"], dihedral_rows)
    if not distance_rows:
        summary["warnings"].append("Distance analysis requires at least two selected atoms.")
    if not angle_rows:
        summary["warnings"].append("Angle analysis requires at least three selected atoms.")
    if not dihedral_rows:
        summary["warnings"].append("Dihedral analysis requires at least four selected atoms.")
    summary["outputs"].extend([
        "analysis/mdanalysis_rmsd.csv",
        "analysis/mdanalysis_rg.csv",
        "analysis/mdanalysis_contacts.csv",
        "analysis/mdanalysis_distances.csv",
        "analysis/mdanalysis_angles.csv",
        "analysis/mdanalysis_dihedrals.csv"
    ])

    rmsf_rows = []
    if observed > 1 and mean is not None and m2 is not None:
        variance = m2 / (observed - 1)
        rmsf = np.sqrt(variance.sum(axis=1))
        for index, value in enumerate(rmsf, start=1):
            rmsf_rows.append([index, float(value)])
    else:
        summary["warnings"].append("RMSF requires at least two frames.")
    write_csv(analysis_dir / "mdanalysis_rmsf.csv", ["atom_index", "rmsf_angstrom"], rmsf_rows)
    summary["outputs"].append("analysis/mdanalysis_rmsf.csv")

    universe.trajectory.rewind()
    run_hbond_analysis(universe, analysis_dir / "mdanalysis_hbonds.csv", frame_times, summary["warnings"])
    summary["outputs"].append("analysis/mdanalysis_hbonds.csv")

    write_summary(summary_path, summary)
    print(f"MDAnalysis summary written to {summary_path}")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="AutoMD MDAnalysis trajectory analysis sidecar")
    parser.add_argument("--diagnostics", action="store_true")
    parser.add_argument("--plan", default="generated/analysis/automd-plan.json")
    parser.add_argument("--project", default=None)
    parser.add_argument("--topology", default=DEFAULT_TOPOLOGY)
    parser.add_argument("--trajectory", default=DEFAULT_TRAJECTORY)
    parser.add_argument("--selection", default=DEFAULT_SELECTION)
    args = parser.parse_args()
    if args.diagnostics:
        return diagnostics()
    return run_analysis(Path(args.plan), args.project, args.topology, args.trajectory, args.selection)


if __name__ == "__main__":
    raise SystemExit(main())
"#;

    template
        .replace("__DEFAULT_TOPOLOGY_JSON__", &topology_json)
        .replace("__DEFAULT_TRAJECTORY_JSON__", &trajectory_json)
        .replace("__DEFAULT_SELECTION_JSON__", &selection_json)
}

fn sidecar_environment_yml() -> String {
    r#"name: automd-science
channels:
  - conda-forge
dependencies:
  - python=3.11
  - openmm
  - pdbfixer
  - mdanalysis
  - mdtraj
  - rdkit
  - openbabel
  - ambertools
  - numpy
  - pandas
"#
    .to_string()
}

fn ligand_parameterization_md(plan: &SimulationPlan) -> String {
    let ligand_hint = plan
        .force_field
        .ligand
        .as_deref()
        .unwrap_or("GAFF2 or user-provided parameters");
    format!(
        r#"# Ligand Parameterization

AutoMD detected ligand support setting: `{ligand_hint}`.

The first science sidecar does not silently parameterize and merge ligands because small-molecule parameters must be reviewed. Recommended user-managed paths:

- Convert an imported SDF/MOL2/SMILES ligand with RDKit or Open Babel.
- Generate AMBER ligand parameters with `antechamber` and `parmchk2`.
- Review charges, atom types, protonation state, and stereochemistry.
- Add the resulting `inputs/ligand.mol2` and `inputs/ligand.frcmod` to the AmberTools `tleap.in` package.

Example commands:

```bash
obabel inputs/ligand.sdf -O inputs/ligand.mol2 --partialcharge gasteiger
antechamber -i inputs/ligand.mol2 -fi mol2 -o inputs/ligand_gaff2.mol2 -fo mol2 -at gaff2 -c bcc
parmchk2 -i inputs/ligand_gaff2.mol2 -f mol2 -o inputs/ligand.frcmod
```
"#
    )
}

fn prep_readme(plan: &SimulationPlan, warnings: &[String]) -> String {
    let warnings_md = if warnings.is_empty() {
        "- No warnings.\n".to_string()
    } else {
        warnings
            .iter()
            .map(|warning| format!("- {warning}\n"))
            .collect::<String>()
    };
    format!(
        r#"# AutoMD Science Sidecar Preparation

Plan: `{name}`

## Files

- `prepare_structure.py` runs dependency diagnostics and conservative PDBFixer/OpenMM-based repair.
- `environment.yml` defines the recommended Conda/Mamba sidecar environment.
- `ligand_parameterization.md` documents reviewed ligand parameterization commands.
- `structure-prep-report.json` is written after execution.

## Scope

This package covers structure repair, non-standard residue replacement, missing atom addition, and hydrogens when PDBFixer/OpenMM are available. Solvation, ions, membranes, ligand merging, and force-field-specific topology generation remain engine-adapter or user-reviewed steps.

## Warnings

{warnings_md}
"#,
        name = plan.name,
    )
}

fn analysis_readme(
    plan: &SimulationPlan,
    topology_path: &str,
    trajectory_path: &str,
    selection: &str,
    expected_outputs: &[String],
    warnings: &[String],
) -> String {
    let warnings_md = if warnings.is_empty() {
        "- No warnings.\n".to_string()
    } else {
        warnings
            .iter()
            .map(|warning| format!("- {warning}\n"))
            .collect::<String>()
    };
    let outputs_md = expected_outputs
        .iter()
        .map(|output| format!("- `{output}`\n"))
        .collect::<String>();
    format!(
        r#"# AutoMD MDAnalysis Trajectory Analysis

Plan: `{name}`

## Inputs

- Topology: `{topology_path}`
- Trajectory: `{trajectory_path}`
- Selection: `{selection}`

## Files

- `run_mdanalysis.py` runs dependency diagnostics and trajectory-backed analysis.
- `environment.yml` defines the recommended Conda/Mamba science-sidecar environment.
- `automd-plan.json` records the normalized `SimulationPlan`.

## Expected Outputs

{outputs_md}
## Scope

The first analysis sidecar computes RMSD, RMSF, radius of gyration, representative distance/angle/dihedral series from the selected atom group, dense contact counts for manageable selections, and hydrogen-bond counts when MDAnalysis can infer donors/acceptors. It writes CSV files that AutoMD can parse into GUI charts. Exact scientific validity still depends on a compatible topology, trajectory, atom selection, protonation state, and trajectory preprocessing.

## Warnings

{warnings_md}
"#,
        name = plan.name,
    )
}

fn default_trajectory_path(engine_id: &str) -> &'static str {
    match engine_id {
        "openmm" => "trajectories/openmm.dcd",
        "ambertools" | "amber_pmemd" => "trajectories/amber-prod.nc",
        "namd" => "trajectories/namd.dcd",
        "lammps" => "trajectories/lammps.lammpstrj",
        "hoomd" => "trajectories/hoomd.gsd",
        _ => "trajectories/md.xtc",
    }
}

fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-' | ':'))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
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
        if let Component::Normal(value) = component {
            destination.push(value);
        }
    }
    destination
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planner;
    use uuid::Uuid;

    fn test_plan() -> SimulationPlan {
        let mut plan = planner::default_simulation_plan(PlanRequest {
            project_id: None,
            name: "prep-test".to_string(),
            engine_id: "openmm".to_string(),
            domain: ProjectDomain::Biomolecular,
        });
        plan.id = Uuid::nil();
        plan
    }

    #[test]
    fn structure_preparation_package_contains_sidecar_files() {
        let package = prepare_structure_package(StructurePreparationRequest {
            plan: test_plan(),
            project_path: None,
            write_to_disk: false,
        })
        .expect("prep package");

        assert_eq!(package.generated_directory, "generated/prep");
        assert!(package
            .files
            .iter()
            .any(|file| file.path == "generated/prep/prepare_structure.py"));
        assert!(package
            .files
            .iter()
            .any(|file| file.path == "generated/prep/environment.yml"));
        assert!(package
            .commands
            .iter()
            .any(|command| command.command.contains("--diagnostics")));
    }

    #[test]
    fn trajectory_analysis_package_contains_mdanalysis_script_and_outputs() {
        let package = prepare_analysis_package(TrajectoryAnalysisRequest {
            plan: test_plan(),
            project_path: None,
            topology_path: Some("inputs/system.pdb".to_string()),
            trajectory_path: Some("trajectories/openmm.dcd".to_string()),
            selection: "protein and name CA".to_string(),
            write_to_disk: false,
        })
        .expect("analysis package");

        assert_eq!(package.generated_directory, "generated/analysis");
        assert!(package
            .files
            .iter()
            .any(|file| file.path == "generated/analysis/run_mdanalysis.py"
                && file.contents.contains("mdanalysis_rmsd.csv")));
        assert!(package
            .files
            .iter()
            .any(|file| file.path == "generated/analysis/environment.yml"));
        assert!(package
            .expected_outputs
            .iter()
            .any(|path| path == "analysis/mdanalysis_rmsf.csv"));
        assert!(package
            .expected_outputs
            .iter()
            .any(|path| path == "analysis/mdanalysis_dihedrals.csv"));
        assert!(package
            .commands
            .iter()
            .any(|command| command.command.contains("--selection")));
    }
}
