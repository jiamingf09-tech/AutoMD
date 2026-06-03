# AutoMD

AutoMD is a cross-platform molecular dynamics workflow studio. The first implementation targets biomolecular MD and provides a Tauri desktop shell, React GUI, Rust command layer, SQLite project index, engine capability registry, HPC script generation, container recipes, and build-script scaffolding.

## Current Scope

- Cross-platform desktop scaffold: Tauri v2 + React + TypeScript + Rust.
- Engine capability and licensing model for open-source and user-licensed engines.
- SQLite-backed engine installation and authorization records for user-configured local, Python-module, or licensed engine paths.
- SQLite-backed local task history for run id, project id, engine, status, progress, and timestamps.
- SQLite-backed artifact metadata and analysis-series cache refreshed from project scans and analysis parsing.
- Initial adapters/registry for GROMACS, OpenMM, AmberTools, LAMMPS, CP2K, GENESIS, HOOMD-blue, DL_POLY, Tinker, NAMD, AMBER pmemd, CHARMM, Desmond, and ACEMD.
- Structured `SimulationPlan` model with preparation, minimization, NVT, NPT, production, analysis stages, and normalized expected outputs.
- Derived multi-engine parameter mapping reports that show how normalized GUI fields become native `.mdp`, OpenMM runner, AMBER `mdin`, NAMD `.conf`, or preview-template values.
- SQLite project index and reproducible project directory layout.
- Structure import for PDB, mmCIF, SDF, MOL2, SMILES, and existing engine-project manifests, with lightweight system summaries.
- Mol* structure viewer integration for imported PDB/mmCIF files, using a safe project-scoped structure-file reader and a fallback canvas while data is unavailable.
- Trajectory artifact indexing with safe project-scoped chunk reads for text PDB/XYZ/LAMMPS dump trajectories and metadata-only registration for binary XTC/TRR/DCD/NetCDF/GSD files.
- Python science sidecar diagnostics for OpenMM, PDBFixer, MDAnalysis, RDKit, Open Babel, and AmberTools command-line tools.
- Structure-preparation package generation with `prepare_structure.py`, `environment.yml`, PDBFixer/OpenMM repair hooks, and reviewed ligand-parameterization guidance.
- MDAnalysis trajectory-analysis package generation for RMSD, RMSF, radius of gyration, hydrogen-bond counts, distance, angle, dihedral, and contact-count CSV outputs that feed the GUI chart parser.
- Runtime diagnostics for Conda/Mamba, Docker/Podman/Apptainer, SSH/rsync, SLURM/PBS/LSF, MPI, PLUMED, CUDA, and ROCm.
- SLURM, container, and source-build recipe generation.
- Build-page export of container recipes, source-build scripts, README guidance, and JSON build manifests into the active project.
- Build wizard runner with dry-run, write-files, and explicit execute modes, captured compiler logs, timeout handling, and failure diagnosis for missing toolchains, GPU/MPI/PLUMED, source fetch, and storage/permission problems.
- Remote execution package generation for SSH/rsync plus SLURM, PBS, LSF, and pure SSH launch workflows.
- Remote job snapshot parsing for SLURM/PBS/LSF/SSH submit output, queue status output, and remote engine log tails.
- Remote workflow step runner with dry-run, write-files, and explicit execute modes for sync-up, submit, status, cancel, log-tail, and sync-down commands.
- SQLite-backed custom remote profile persistence for host, scheduler, workdir, module/setup commands, and default queue.
- GROMACS run-package generation with `.mdp` files, command sequence, run script, warnings, and log parsing.
- OpenMM run-package generation with a Python application-layer runner, environment check, checkpoint, trajectory, final PDB, and state CSV outputs.
- AmberTools run-package generation with `tleap`, `sander`, `cpptraj`, AMBER `mdin` templates, ligand-parameter warnings, and log/failure diagnostics.
- NAMD external-only run-package generation with an editable `.conf`, explicit user-license/install warnings, NAMD binary detection, and log/failure diagnostics.
- Project-scoped native text-file editor for generated `.mdp`, `.mdin`, `.conf`, LAMMPS/CP2K/native input files, run scripts, remote scripts, and manifests.
- Batch repeat-experiment package generation for multi-replica/multi-seed runs, with per-replica namespaced inputs under `generated/batch/`, unique plan ids, seed injection, and a reviewable `run-batch.sh`.
- Native preview run packages for LAMMPS, CP2K, GENESIS, HOOMD-blue, DL_POLY, Tinker, AMBER pmemd, CHARMM, Desmond, and ACEMD so every registered engine has a generated file set, run script, warnings, and diagnostic path.
- Plugin manifest registry for future engine adapters, analysis modules, remote schedulers, build recipes, and report templates.
- GUI plugin registry page showing built-in and external manifests, entrypoints, capabilities, source paths, and manifest warnings.
- Local task runner with dry-run, mock-runner, and explicit real-process modes, plus status polling and cancellation.
- Per-run `automd-run-manifest.json` environment snapshots with OS/arch, selected environment variables, detected runtime tools, command, plan, and run directory.
- Failure classification for first-class adapters plus generic diagnostics for preview/external engines, covering missing executables, missing inputs, topology/force-field gaps, GPU/MPI/license issues, numerical instability, storage/permission problems, and scheduler errors.
- Checkpoint discovery and restart command generation for GROMACS `.cpt` files in run directories and project-level `checkpoints/`.
- Artifact indexing, analysis-series parsing, chart preview, and Markdown/HTML/PDF report export for generated inputs, logs, checkpoints, trajectories, analysis tables, and reports.

## Run

```bash
npm install
npm run dev
```

For the desktop shell:

```bash
npm run tauri:dev
```

Rust-only checks:

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

## Product Direction

Open-source or freely available engines are integrated first through installer, container, and build recipes. Restricted or commercial engines are represented as external modules: AutoMD can detect binaries and generate adapter entrypoints, but users must install them and satisfy license requirements in their own environment.

## Project Layout

- `src/` contains the React GUI and Tauri API bridge.
- `src-tauri/src/` contains Rust models, registry, SQLite store, runtime diagnostics, and recipe generation.
- `src-tauri/src/engine_adapters.rs` contains concrete workflow generators for GROMACS, OpenMM, AmberTools, and the external-only NAMD entrypoint.
- `src-tauri/src/parameter_mapping.rs` derives GUI-to-native parameter mapping reports for first-class, external, and preview engine templates.
- `src-tauri/src/batch.rs` clones `SimulationPlan` data into bounded multi-replica packages and namespaces generated engine inputs so repeated experiments do not overwrite each other.
- `src-tauri/src/structure_import.rs` imports user structures into `inputs/` and updates normalized `SystemSpec` data.
- `src-tauri/src/task_runner.rs` manages local process lifecycle, log tailing, task snapshots, failure diagnosis, checkpoint discovery, and cancellation.
- `src-tauri/src/artifacts.rs` scans project outputs and exports reproducible simulation reports.
- `src-tauri/src/analysis.rs` parses `.xvg` and CSV analysis artifacts into chart-ready series.
- `src-tauri/src/trajectory.rs` indexes trajectory artifacts and reads bounded text-frame chunks for UI previews.
- `src-tauri/src/science_sidecar.rs` generates the Python preparation sidecar package and detects scientific Python/AmberTools dependencies.
- `src-tauri/src/science_sidecar.rs` also generates the MDAnalysis trajectory-analysis package under `generated/analysis/`.
- `src-tauri/src/recipes.rs` generates SLURM/PBS/LSF remote submission packages, sync scripts, container recipes, and source-build scripts.
- `src-tauri/src/build_runner.rs` runs the compile wizard in dry-run/write-files/execute modes and captures build logs under `build-recipes/<engine>/logs/`.
- `src-tauri/src/remote_runner.rs` writes remote package files and, only in explicit execute mode, runs generated local `ssh`/`rsync` workflow steps with timeout-bounded output capture.
- `scripts/` contains sidecar and mock runner utilities.
- `docs/` contains implementation contracts and project-format notes.
- `docs/PLUGIN_MANIFESTS.md` documents the manifest-only plugin discovery surface.
- `docs/SCIENCE_SIDECAR.md` documents the Python sidecar preparation and diagnostics flow.
- `docs/RELEASE.md` documents cross-platform Tauri packaging, release checks, and engine distribution policy.
