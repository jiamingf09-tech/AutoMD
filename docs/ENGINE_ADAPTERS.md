# Engine Adapter Contract

Every MD engine integration implements the same behavioral surface, even when the underlying engine uses different files or commands.

## Required Operations

- `detect`: find executable, Python module, container image, remote module, and license status.
- `validate`: check `SimulationPlan` compatibility, required files, GPU/MPI options, and authorization status.
- `map_parameters`: explain how normalized GUI parameters map to native fields, generated files, derived step counts, unsupported settings, and manual-review notes.
- `prepare`: convert imported structures and parameters into engine-native inputs.
- `prepare_batch`: clone a validated `SimulationPlan` into bounded multi-replica plans, inject deterministic seeds, and reuse the engine-native `prepare` output without path collisions.
- `edit_native`: expose generated text inputs such as `.mdp`, `.mdin`, `.conf`, `.inp`, `.key`, scheduler scripts, and run scripts through the project-scoped GUI editor.
- `run`: launch local, container, SSH, or scheduler-backed execution.
- `parse_progress`: extract stage, step, performance, warnings, and fatal errors from logs.
- `classify_failure`: map logs and exit codes to user-facing failure categories and recovery suggestions.
- `resume`: restart from checkpoint or engine-native restart files.
- `analyze`: import trajectories and generate common analysis artifacts.
- `parse_analysis`: convert `.xvg` and CSV analysis outputs into chart-ready series.
- `export`: write reproducibility bundle and native project files.

## License Policy

Open-source engines may receive installer, container, and build recipes. Restricted or commercial engines must use `UserLicenseRequired`; AutoMD must not download, bundle, mirror, or redistribute those binaries.

## First Adapter Milestones

- GROMACS: full local workflow, `.mdp` generation, `grompp`, `mdrun`, `energy`, `trjconv`, and log parsing.
- OpenMM: Python sidecar runner, script templates, PDBFixer/RDKit/Open Babel preparation.
- AmberTools: `tleap`, `sander`, `cpptraj`, and AMBER topology preparation.
- NAMD: external-only detection, `.conf` template, and license confirmation gate.

## Implemented GROMACS Package

The current GROMACS adapter generates a local run package with:

- `generated/gromacs/ions.mdp`, `em.mdp`, `nvt.mdp`, `npt.mdp`, and `md.mdp`.
- `generated/gromacs/automd-plan.json` for reproducibility.
- `runs/gromacs-<plan-id>/run-gromacs.sh` with preparation, minimization, equilibration, production, and basic RMSD/Rg analysis commands.
- Structured command metadata for GUI preview and scheduler substitution.
- Log parsing for `step`, `Performance: ... ns/day`, checkpoint, warning, and fatal-error lines.
- Failure classification for missing executables, missing inputs, topology and force-field gaps, parameter mismatches, unavailable GPU backends, MPI launch failures, numerical instability, disk/permission errors, scheduler failures, and unknown errors.
- Checkpoint discovery in `runs/<run-id>/` and project-level `checkpoints/`, with recommended `gmx mdrun -deffnm ... -cpi ... -append` resume commands.

Ligand and membrane systems currently produce explicit warnings because they require external topology preparation before the generated GROMACS sequence is scientifically valid.

### GROMACS scientific notes (current)

- Equilibration MDP emits `define = -DPOSRES` when restraints are requested; production uses conditional `-cpi` only if `md.cpt` already exists.
- MDP templates include `DispCorr = EnerPres`; production `ref_p` follows the NPT stage pressure.
- Disabled plan stages are omitted from the run script; coordinates chain from the last enabled stage.
- Analysis defaults to Backbone groups for RMSD/Rg.

## Implemented OpenMM Package

The current OpenMM adapter generates a first Python runner package with:

- `generated/openmm/automd-plan.json` for reproducibility.
- `generated/openmm/run_openmm.py`, based on the OpenMM Python application layer.
- `runs/openmm-<plan-id>/run-openmm.sh` with a Python module check and runner launch command.
- Outputs for `runs/openmm-*/openmm.chk`, `checkpoints/openmm.chk`, `trajectories/openmm.dcd`, `trajectories/openmm-final.pdb`, and `analysis/openmm_state.csv`.
- Failure classification for missing Python modules, missing input structures, ForceField template gaps, GPU platform issues, numerical instability, and output permission/storage errors.
- Resume discovery for `.chk` files with a generated `python generated/openmm/run_openmm.py ... --resume <checkpoint>` command.

### OpenMM scientific notes (current)

- Runner adds hydrogens, then solventizes with `Modeller.addSolvent` when the input has no periodic box (using plan padding/ionic/neutralize).
- NPT stage enables `MonteCarloBarostat`; NVT and NPT equilibration stages are honored before production.
- Platform selection tries CUDA → OpenCL → CPU.
- Ligands/cofactors still need compatible XML templates or upstream parameterization.

## Implemented AmberTools Package

The current AmberTools adapter generates a CPU-oriented AMBER input package with:

- `generated/ambertools/tleap.in` for loading protein/water force fields, solvating the source PDB, neutralizing ions, and writing `system.prmtop` plus `system.inpcrd`.
- `generated/ambertools/min.mdin`, `heat.mdin`, `equil.mdin`, and `prod.mdin` for minimization, NVT heating, NPT equilibration, and production with conservative `sander` settings.
- `generated/ambertools/cpptraj.in` for basic RMSD/Rg analysis from the production NetCDF trajectory.
- `runs/ambertools-<plan-id>/run-ambertools.sh` with AmberTools command checks and ordered `tleap`, `sander`, and `cpptraj` execution.
- Log parsing for `NSTEP`, AmberTools completion, `SANDER BOMB`, LEaP exits, warnings, and fatal-error lines.
- Failure classification for missing AmberTools commands, missing source/ligand files, missing AMBER parameters, topology gaps, numerical instability, and output permission/storage errors.

### AmberTools scientific notes (current)

- LEaP solvent padding converts **nm → Å** (`padding_nm * 10`).
- MD `mdin` files set `ioutfm=1, ntxo=2` for NetCDF trajectories/restarts matching cpptraj `.nc` inputs.
- Heat/equil step counts derive from plan `durationPs` and production `timestepFs`.
- Disabled plan stages are omitted; restart coordinates chain across enabled stages.

Ligands and cofactors produce explicit warnings because AutoMD does not yet run `antechamber`/`parmchk2` automatically. Users can provide `mol2`/`frcmod` files and edit the generated `tleap.in` until that preparation workflow lands.

## Implemented NAMD External Package

The current NAMD adapter is an external-only entrypoint and generates:

- `generated/namd/automd.conf`, an editable NAMD configuration template using user-provided `inputs/system.psf`, coordinates, and CHARMM-style parameter files.
- When the NPT stage is enabled, the conf includes a `langevinPiston` block; users must still supply cell basis vectors for PME.
- `runs/namd-<plan-id>/run-namd.sh`, which detects `namd3` or `namd2`, honors `NAMD_BIN`, and writes `namd.log`.
- Structured warnings that AutoMD does not download, bundle, mirror, or license NAMD binaries.
- Log parsing for `ENERGY`, `TIMING`, restart/checkpoint messages, completion, and `FATAL ERROR` lines.
- Failure classification for missing NAMD executables, license/authorization messages, missing PSF/PDB/parameter inputs, force-field gaps, GPU backend problems, numerical instability, and storage/permission errors.

This adapter intentionally stops at launch/template support. Scientific validity depends on the user supplying compatible PSF/PDB/parameter files from CHARMM-GUI, VMD psfgen, or another validated preparation path.

## Batch Repeat Packages

AutoMD can generate repeat-experiment packages for any engine that implements `prepare_run_package`. The batch layer creates 1-64 replicas, assigns unique plan ids, injects `velocitySeed` into NVT and `randomSeed` into production, and rewrites generated paths into `generated/batch/replica-XX/<engine>/`. This keeps `.mdp`, `.mdin`, `.conf`, `.inp`, and other native files independently editable per replica.

The aggregate package writes `generated/batch/automd-batch.json` and `generated/batch/run-batch.sh`. The script runs each replica's engine script in sequence and captures `runs/<engine-plan>/batch-replica-XX.log`, making the result compatible with the existing artifact indexer and report path. GROMACS, OpenMM, and AmberTools templates consume those seeds directly in their generated inputs or runner scripts; preview and external engines still retain the seed in the per-replica `automd-plan.json` until their full scientific templates mature.

## Parameter Mapping Reports

`ParameterMappingReport` is derived from the active `SimulationPlan` and selected engine. It is not a separate source of truth; it is a GUI-facing audit trail that shows each normalized parameter, the native key/value, the generated target file, and whether the mapping is exact, approximate, unsupported, or requires manual review.

The current mapper covers exact derived values for GROMACS and OpenMM production timing, including `durationNs` plus `timestepFs` to native step counts and `checkpointEveryPs` to checkpoint/report intervals. AmberTools and AMBER pmemd expose production `nstlim`, `dt`, and `ig` mappings while flagging fixed equilibration/checkpoint template fields for review. NAMD maps the external `.conf` production timing and thermostat fields while flagging NPT and checkpoint cadence as user-reviewed template edits. Preview engines surface common timing and temperature targets where the template already contains them, and otherwise keep entries as manual-review rows.

## Preview And External Packages

Every engine registered in AutoMD now has a package-generation path, even when the adapter is not yet a full scientific workflow:

- LAMMPS: `generated/lammps/in.automd` plus `run-lammps.sh`.
- CP2K: `generated/cp2k/automd.inp` plus `run-cp2k.sh`.
- GENESIS: `generated/genesis/automd.inp` plus `run-genesis.sh`.
- HOOMD-blue: `generated/hoomd/run_hoomd.py` plus `run-hoomd.sh`.
- DL_POLY: `generated/dl_poly/CONTROL`, `FIELD`, and `CONFIG` plus `run-dl-poly.sh`.
- Tinker: `generated/tinker/automd.key` plus `run-tinker.sh`.
- AMBER pmemd: `generated/amber_pmemd/prod.mdin` plus `run-amber-pmemd.sh`.
- CHARMM: `generated/charmm/automd.inp` plus `run-charmm.sh`.
- Desmond: `generated/desmond/automd.cfg` plus `run-desmond.sh`.
- ACEMD: `generated/acemd/input` plus `run-acemd.sh`.

These packages are intentionally marked as preview or external where appropriate. They provide GUI visibility, reproducible project files, scheduler-compatible run scripts, and generic diagnostics, but users must still provide validated native topology, force-field, basis/potential, data, or licensed-command inputs before real production calculations.

## Local Execution Safety

AutoMD exposes three local execution modes:

- `DryRun`: generate and validate the run package without starting any process.
- `Mock`: start `scripts/automd_mock_engine.py` to exercise task lifecycle, stdout parsing, progress updates, and completion handling.
- `Real`: execute the generated engine run script in the project directory. This mode is explicit because a real MD job can run for a long time and consume CPU/GPU resources.

The GUI should default to `Mock` while an adapter is under development. Users must intentionally choose `Real` before AutoMD launches a generated engine workflow.

Completed local tasks trigger checkpoint discovery, artifact indexing, and report export. Failed and cancelled tasks still attempt artifact indexing and checkpoint discovery so logs, partial outputs, and restart options remain available for diagnostics.

## Trajectory indexing performance

Text trajectories (PDB multi-model, XYZ, LAMMPS dump) are indexed with a **streaming line scanner**:

- Frame boundaries are recorded as byte offsets without loading the whole file as a UTF-8 `String`.
- Chunk previews `seek` to each frame range and read only those bytes.
- Index manifests under `trajectories/.automd-index/` cache the full offset table for reuse.
- Frame count is capped at 2,000,000 descriptors to bound memory; larger series should be strided upstream.

Binary trajectories (XTC/TRR/DCD/NetCDF/GSD) remain metadata-only in the Rust reader.

## Analysis Visualization

The GUI can parse and plot numeric analysis outputs before a full Python/MDAnalysis sidecar is available:

- `.xvg` files from GROMACS RMSD/Rg commands are parsed into labeled x/y series.
- OpenMM-style CSV state tables are parsed into one series per numeric observable, such as potential energy or temperature.
- The frontend renders compact SVG line charts in the Run and Report views.

This layer is for visualization of already-produced analysis outputs. Expensive calculations from raw trajectories remain the responsibility of engine commands or the future Python analysis sidecar.

## Remote and HPC Packages

AutoMD can generate a remote execution package from a `SimulationPlan` and `RemoteProfile`. The package includes:

- SSH/rsync sync-up and sync-down commands.
- Scheduler scripts for SLURM (`#SBATCH`), PBS (`#PBS`), and LSF (`#BSUB`).
- A pure SSH background-run script for workstation-style hosts.
- Submit, status, cancel, log-tail, and result-recovery command previews.
- Module load commands, queue/partition settings, CPU, MPI rank, GPU, walltime, and run-directory metadata.

Remote generation is conservative because cluster policies differ. GPU directives for PBS/LSF and module names should be reviewed against the target site before running. SLURM submission uses `sbatch --parsable` so the GUI can store the returned job id cleanly. The remote workflow runner has three modes: dry-run, write-files, and explicit execute. Only execute mode starts local `ssh`/`rsync` commands, and captured submit/status/log output is normalized by the remote status parser.
