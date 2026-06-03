# AutoMD Project Format

Each project has a stable directory layout so simulations are reproducible and portable across local and remote runs.

```text
project/
  inputs/
  generated/
  runs/
  checkpoints/
  trajectories/
    .automd-index/
  analysis/
  reports/
  remote/
  build-recipes/
```

## Stored Artifacts

- Raw imported structures and user-provided files remain in `inputs/`.
- Generated topology, parameter, and engine-native files go to `generated/`.
- Python science-sidecar preparation packages go to `generated/prep/`, including `automd-plan.json`, `prepare_structure.py`, `environment.yml`, ligand-parameterization notes, and preparation reports.
- MDAnalysis trajectory-analysis packages go to `generated/analysis/`, including `run_mdanalysis.py`, `environment.yml`, and analysis package README files.
- Batch repeat-experiment packages go to `generated/batch/`. Each replica gets a namespaced directory such as `generated/batch/replica-01/gromacs/` plus a per-replica `automd-plan.json`, while `generated/batch/automd-batch.json` and `run-batch.sh` record the aggregate run.
- Each run gets a timestamped folder under `runs/` with command, stdout, stderr, environment snapshot, and scheduler metadata.
- Checkpoints and restart files are copied or linked under `checkpoints/`.
- Trajectories are indexed and stored under `trajectories/`; generated frame manifests live under `trajectories/.automd-index/`.
- Analysis tables and figures go to `analysis/`.
- HTML, PDF, and Markdown outputs go to `reports/`.
- Remote submission and synchronization helpers go to `remote/`.
- Source-build scripts, Containerfiles, build manifests, and optional build logs go to `build-recipes/`.

## Structure Import

`StructureImportRequest` imports source structures into `inputs/` and returns an updated `SystemSpec` for the active `SimulationPlan`. The current importer supports:

- PDB and mmCIF-style atom records, with lightweight atom, residue, chain, ligand, and membrane-residue detection.
- SDF, MOL2, and SMILES as ligand-oriented inputs that still require topology/parameter generation before production MD.
- Existing engine-project directories as manifest files so native inputs can be tracked before a full adapter consumes them.

Imported paths are stored relative to the project root, such as `inputs/system.pdb`, so generated GROMACS/OpenMM scripts can run locally or after remote synchronization without absolute-path rewrites.

## Structure Preparation Package

`StructurePreparationPackage` writes a conservative Python sidecar package under `generated/prep/`. It can run dependency diagnostics for OpenMM, PDBFixer, MDAnalysis, RDKit, Open Babel, and AmberTools, then optionally run PDBFixer/OpenMM-based structure repair against the imported source path.

The sidecar produces `prepared_structure.pdb` and `structure-prep-report.json` after execution. If scientific Python dependencies are unavailable, the script copies the input structure and records that limitation instead of silently claiming a repair. Ligand parameterization and membrane construction are intentionally documented as user-reviewed workflows, not automatic mutations.

See `docs/SCIENCE_SIDECAR.md` for the generated files, command preview, and dependency environment recipe.

## Structure Viewer Loading

The GUI loads imported PDB/mmCIF structures into Mol* through a project-scoped read API. The desktop command only reads files inside the selected project directory, supports `.pdb`, `.cif`, and `.mmcif`, and rejects inline viewer loads above 50 MB so large structures or trajectories do not block the UI. SDF/MOL2/SMILES remain valid ligand inputs, but they are not automatically opened in the first Mol* viewer path.

## SQLite Index

The desktop app keeps a local SQLite index with project summaries, engine installations, remote profiles, task states, and analysis metadata. Scientific outputs remain in the project folder so the project can be archived or moved.

`remote_profiles` stores user-defined SSH/HPC entries:

- Stable profile id and display name.
- Host, scheduler (`ssh`, `slurm`, `pbs`, or `lsf`), and remote work directory.
- Module/setup commands as JSON so multi-line environment initialization is reproducible.
- Optional default queue/partition.

Built-in templates remain available even when no custom profiles are saved. If a saved profile uses the same id as a template, the saved profile wins in the GUI list.

`engine_installations` stores user-reviewed engine locations:

- Engine id and executable/module location, such as `/usr/local/bin/gmx`, `python3::openmm`, or a user-licensed commercial binary path.
- Optional version string.
- Authorization/detection status: ready, missing install, missing license, platform unsupported, or remote recommended.
- Last checked timestamp.

This table is especially important for restricted or commercial adapters. AutoMD can remember where the user has installed NAMD, AMBER pmemd, CHARMM, Desmond, or ACEMD, but it does not download, bundle, mirror, or license those binaries.

`tasks` stores persisted local run snapshots:

- Stable task id, optional project id, plan id, and engine id.
- Task status, current stage when known, and progress percentage.
- Created and updated timestamps so the Run page can show recent history after restart.

The task record is updated when a local run starts, when the GUI polls a live task, and when a task is cancelled. Poll updates preserve the original project id even when the live task lookup only knows the task id.

`artifact_records` stores the latest project scan for each artifact path:

- Project path, artifact path, artifact kind, size, optional modified timestamp, and summary.
- Optional run directory that scoped the scan.
- Indexed timestamp from the `ArtifactIndex`.

`analysis_cache` stores the latest parsed analysis series:

- Project path, artifact path, series label, axis labels, point count, min/max/last values, and generated timestamp.
- Full `AnalysisSeries` JSON is retained in SQLite so a future GUI path can reload chart data without reparsing every table.

## Local Run State

Generated engine packages are written into `generated/` and `runs/`. Local task snapshots track the command, mode, run directory, task status, progress, `ns/day`, current step, exit code, error message, failure analysis, resume plan, and a bounded log tail. This snapshot model is what the GUI polls for realtime monitoring.

`SimulationPlan.outputs` stores normalized expected artifact patterns independent from a specific engine adapter: generated inputs, run logs, checkpoints, trajectories, energy/state outputs, analysis tables, and reports. Stage-level `expectedOutputs` still describe semantic outputs for workflow progress, while `outputs` describes filesystem artifacts for reports, indexing, and cross-engine package mapping.

`ParameterMappingReport` is generated on demand from the active `SimulationPlan`. It records the selected engine, plan id, normalized parameter key/value, native engine key/value, generated target file, mapping status, warnings, and explanatory notes. The report is deliberately derived rather than persisted in the project folder, so regenerated native inputs and GUI edits stay anchored to the current plan.

`BatchExperimentPackage` clones the active `SimulationPlan` into 1-64 replica plans, assigns each replica a new plan id, writes `velocitySeed` for NVT and `randomSeed` for production, and reuses the selected engine adapter to generate native files. Generated paths are rewritten from `generated/<engine>/...` to `generated/batch/replica-XX/<engine>/...` so repeated runs do not overwrite each other's parameter files. The batch script runs replica scripts sequentially and writes `runs/<engine-plan>/batch-replica-XX.log` files for later indexing.

The Run page includes a project-scoped native text-file editor for generated engine inputs and scripts. It accepts only relative paths inside editable project areas (`generated/`, `runs/`, `remote/`, `build-recipes/`, `analysis/`, and `reports/`) and only known text extensions such as `.mdp`, `.mdin`, `.conf`, `.inp`, `.in`, `.key`, `.cfg`, `.json`, `.yaml`, `.py`, `.sh`, scheduler scripts, and Markdown. Inline editing is capped at 2 MB per file so large logs or trajectories do not block the UI.

Every dry-run, mock-run, or real local run also writes `runs/<engine-plan>/automd-run-manifest.json`. The manifest captures:

- Task id, plan id, engine id, local run mode, command, project path, and run directory.
- A full `SimulationPlan` snapshot so regenerated inputs can be compared with the run that produced results.
- OS, CPU architecture, current project directory, selected environment variables (`PATH`, Conda/virtualenv, OpenMP, CUDA/ROCm/HIP, PLUMED, and library paths), and runtime tool diagnostics.

The manifest is indexed as metadata and appears in reports under the environment snapshot section, giving the user a concrete reproducibility record instead of only a live GUI state.

## Failure Analysis and Resume Plans

`FailureAnalysis` stores the engine id, category, severity, diagnostic message, and suggested next actions. The first GROMACS implementation classifies missing executables, missing inputs, topology/force-field gaps, parameter mismatches, GPU/MPI issues, numerical instability, storage/permission problems, scheduler failures, and unknown errors.

`ResumePlan` stores discovered checkpoint candidates, the recommended checkpoint, warnings, and a ready-to-run command. For GROMACS, AutoMD scans the active run directory and project-level `checkpoints/` folder for `.cpt` files and emits `gmx mdrun -deffnm <prefix> -cpi <checkpoint> -append`.

## Artifact Index and Reports

AutoMD indexes files under `inputs/`, `generated/`, `runs/`, `checkpoints/`, `trajectories/`, `analysis/`, `reports/`, `remote/`, and `build-recipes/`. Artifacts are classified as generated input, run log, checkpoint, trajectory, energy, analysis table, figure, report, metadata, or other. Small `.xvg`, `.jsonl`, and `.log` files receive lightweight summaries; large trajectory files are indexed by metadata only.

Build recipe exports under `build-recipes/` are indexed as metadata so compiler scripts, Containerfiles, JSON manifests, and captured build logs can be included in reproducibility reports.

The Build page can run the compile wizard in three modes:

- `dryRun`: preview the command and warnings without writing files or running compilers.
- `writeFiles`: export Containerfiles, source-build scripts, README guidance, and JSON manifests under `build-recipes/<engine>/`.
- `execute`: run `build-recipes/<engine>/build-<engine>.sh` locally with timeout-bounded stdout/stderr capture.

Executed builds write `build-recipes/<engine>/logs/build.stdout.log`, `build.stderr.log`, and `build-combined.log`. Failed builds attach `FailureAnalysis` suggestions for common causes such as missing CMake/git/compiler tools, unwritable install prefixes, GPU backend mismatches, MPI wrapper gaps, PLUMED patch problems, source/network fetch failures, and unknown compiler errors.

When a local task completes, AutoMD refreshes the artifact index and exports Markdown, HTML, and PDF reports under `reports/`. Reports include the normalized `SimulationPlan`, task status, run directory, command, environment manifest path, progress, performance, errors, and artifact list.

## Analysis Series

`AnalysisParseResult` converts numeric analysis artifacts into chart-ready series for the GUI and report draft:

- GROMACS-style `.xvg` files are parsed for title, x-axis label, y-axis label, and numeric x/y rows.
- CSV files, including OpenMM state-data CSV output, are parsed into one series per numeric y column using the first numeric column as x.
- MDAnalysis sidecar outputs such as `analysis/mdanalysis_rmsd.csv`, `mdanalysis_rg.csv`, `mdanalysis_rmsf.csv`, `mdanalysis_contacts.csv`, and `mdanalysis_hbonds.csv` are parsed through the same CSV path.
- Large tables are downsampled before they are sent to the frontend so long analysis outputs do not block rendering.

The first GUI chart layer is intentionally lightweight SVG. Trajectory-backed calculations are generated by the MDAnalysis sidecar package and then re-enter this parser as bounded CSV artifacts.

## Trajectory Indexing

`TrajectoryIndexRequest` builds a bounded manifest for trajectory artifacts without pushing whole files into the GUI. The Rust reader currently supports frame-offset indexing for text trajectories:

- Multi-model PDB files under `trajectories/*.pdb` or `*.ent`.
- XYZ trajectories under `trajectories/*.xyz`.
- LAMMPS dump trajectories under `trajectories/*.lammpstrj` or `*.dump`.

The index records byte ranges, frame numbers, optional atom counts, optional time hints, sampling stride, and warnings. When `writeIndex` is true, AutoMD writes a JSON manifest under `trajectories/.automd-index/`.

`TrajectoryChunkRequest` reads only selected frame ranges from those text trajectories, bounded by `maxBytes`, so the UI can preview frames without loading an entire trajectory. Binary trajectories such as XTC, TRR, DCD, NetCDF, and GSD are registered as metadata-only in the Rust reader; decoded frames will come from the Python/MDAnalysis sidecar or a native binary decoder path.

## Remote Execution Packages

`RemoteExecutionPackage` records the selected remote profile, scheduler, remote working directory, run directory, generated scripts, commands, and warnings. The current generator produces:

- `remote/submit.slurm`, `remote/submit.pbs`, `remote/submit.lsf`, or `remote/run-ssh.sh` depending on the profile scheduler.
- `remote/sync-up.sh` for creating the remote workdir and pushing the project with `rsync --partial --append-verify`.
- `remote/sync-down.sh` for collecting `runs/`, `checkpoints/`, `trajectories/`, `analysis/`, and `reports/`.
- GUI command previews for sync-up, submit, status, cancel, log-tail, and sync-down.

These packages are generated for review and reproducibility. The GUI defaults to dry-run mode, where no files are written and no connection is opened. Users can choose write-files mode to stage `remote/` scripts locally, or explicit execute mode to run the generated local `ssh`/`rsync` command with a timeout. Submit, status, and log-tail outputs are fed back through the same `RemoteStatusParseRequest` parser used by manual paste-in diagnostics.

## Remote Job Snapshots

`RemoteStatusParseRequest` normalizes scheduler text output into `RemoteJobSnapshot` records. The parser accepts:

- Submit output from `sbatch --parsable`, `qsub`, `bsub`, or SSH background launch.
- Status output from `squeue`, `qstat`, `bjobs`, or `ps`.
- Remote log tails from engine runs.

The snapshot records the scheduler, job id or PID, normalized `TaskStatus`, raw queue state, current step, progress, `ns/day`, parsed log events, and warnings. This keeps the GUI status model independent from a specific cluster policy: today users can paste command output for review, and a later SSH runner can feed the same parser automatically.
