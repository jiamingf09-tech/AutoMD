# Python Science Sidecar

AutoMD uses a Python science sidecar for workflows that are better handled by established molecular-science libraries than by the Rust desktop core.

## Current Responsibilities

- Detect Python modules: OpenMM, PDBFixer, MDAnalysis, RDKit, and Open Babel.
- Detect AmberTools command-line tools: `tleap`, `antechamber`, `parmchk2`, and `cpptraj`.
- Generate `generated/prep/prepare_structure.py`.
- Generate `generated/prep/environment.yml`.
- Generate `generated/prep/automd-plan.json`.
- Generate `generated/prep/ligand_parameterization.md`.
- Generate `generated/prep/README.md`.
- Produce `generated/prep/prepared_structure.pdb` and `generated/prep/structure-prep-report.json` when the preparation script is run.
- Generate `generated/analysis/run_mdanalysis.py` for trajectory-backed analysis.
- Generate MDAnalysis CSV outputs under `analysis/` for GUI chart parsing.

## Generated Package

The Workflow tab can write a preparation package under `generated/prep/` for the active project:

| Path | Purpose |
| --- | --- |
| `automd-plan.json` | Snapshot of the normalized `SimulationPlan` used by the sidecar script. |
| `prepare_structure.py` | Executable Python entrypoint for dependency diagnostics and conservative structure repair. |
| `environment.yml` | Conda/Mamba environment recipe for the optional science dependencies. |
| `ligand_parameterization.md` | Reviewed manual path for ligand conversion and AmberTools parameter generation. |
| `README.md` | Project-local usage notes and warnings generated from the current plan. |
| `prepared_structure.pdb` | Output written by `prepare_structure.py` after a preparation run. |
| `structure-prep-report.json` | Machine-readable record of actions, warnings, input path, output path, and solvent settings. |

The generated command preview currently includes:

```bash
python3 generated/prep/prepare_structure.py --diagnostics
python3 generated/prep/prepare_structure.py --plan generated/prep/automd-plan.json --project .
```

## Preparation Behavior

The generated `prepare_structure.py` is conservative:

- If PDBFixer and OpenMM are available, it replaces non-standard residues, adds missing atoms, and adds hydrogens.
- If PDBFixer/OpenMM are unavailable, it copies the source structure and records a warning instead of claiming repair.
- The first script path is PDB/mmCIF-oriented. Ligand-only SDF, MOL2, and SMILES imports remain valid project inputs, but they require a later topology/parameter workflow before production MD.
- Ligand parameterization is documented but not silently merged. Users must review charges, atom types, protonation, stereochemistry, and generated `mol2`/`frcmod` files before production MD.
- Membrane systems remain outside the first sidecar path and require a validated external builder.
- Solvation, ion placement, force-field topology generation, and engine-native input generation remain engine-adapter responsibilities.

## Recommended Environment

The generated environment file uses conda-forge:

```yaml
name: automd-science
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
```

## GUI Surface

The Workflow tab shows science-sidecar dependency diagnostics and can generate the preparation package into the active project. The package is written under `generated/prep/` so it is included in remote synchronization, artifact indexing, and reproducibility reports.

The Run tab can generate an MDAnalysis trajectory-analysis package under `generated/analysis/`. It does not execute automatically; the user can review and run the generated command locally, in a Conda/Mamba sidecar environment, or after remote synchronization.

## Trajectory Analysis Package

The generated analysis package contains:

| Path | Purpose |
| --- | --- |
| `generated/analysis/automd-plan.json` | Snapshot of the normalized `SimulationPlan`. |
| `generated/analysis/run_mdanalysis.py` | Python entrypoint for dependency diagnostics and trajectory analysis. |
| `generated/analysis/environment.yml` | Conda/Mamba environment recipe shared with the science sidecar. |
| `generated/analysis/README.md` | Project-local inputs, command, outputs, and warnings. |

The default command shape is:

```bash
python3 generated/analysis/run_mdanalysis.py --diagnostics
python3 generated/analysis/run_mdanalysis.py --plan generated/analysis/automd-plan.json --project . --topology inputs/system.pdb --trajectory trajectories/md.xtc --selection 'protein and name CA'
```

Expected outputs:

- `analysis/mdanalysis_rmsd.csv`
- `analysis/mdanalysis_rg.csv`
- `analysis/mdanalysis_rmsf.csv`
- `analysis/mdanalysis_contacts.csv`
- `analysis/mdanalysis_hbonds.csv`
- `analysis/mdanalysis_distances.csv`
- `analysis/mdanalysis_angles.csv`
- `analysis/mdanalysis_dihedrals.csv`
- `analysis/mdanalysis-summary.json`

The CSV files are intentionally simple numeric tables so the existing AutoMD analysis parser can plot them without a bespoke viewer path.

## Analysis Limits

- RMSD, RMSF, radius of gyration, contact-count, distance, angle, and dihedral outputs require a compatible topology, trajectory, and atom selection.
- Hydrogen-bond counts are best-effort because donor/acceptor inference depends on topology detail and MDAnalysis support for the source files.
- Dense contact counts are skipped for very large selections to avoid quadratic memory pressure.
- Distance, angle, and dihedral CSVs currently use the first 2/3/4 atoms from the selected atom group as representative series. A richer user-defined atom-picking UI is still needed before these become fully configurable analysis widgets.
