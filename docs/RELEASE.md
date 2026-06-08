# AutoMD Release And Packaging

AutoMD ships as a Tauri desktop application for Windows, macOS, and Linux. The application bundle contains the React GUI, Rust command layer, SQLite-backed project state, recipe generators, and local orchestration code. It does not bundle MD engine binaries, Python scientific packages, commercial executables, cluster credentials, or user project data.

## Preflight

Run the full local check before building installers:

```bash
npm run check
```

This runs the TypeScript/Vite production build and the Rust test suite. Current Vite builds may warn about Mol*/`h264-mp4-encoder` browser externalization and a large application chunk; those warnings do not block the release artifact.

Regenerate desktop icons after changing `scripts/generate_icons.mjs` or the icon artwork:

```bash
npm run icons
```

## Desktop Installers

Build the Tauri desktop installer package on each target operating system:

```bash
npm run tauri:build:installers
```

Platform-specific installer commands:

```bash
npm run tauri:build:windows  # Windows: NSIS .exe and MSI .msi
npm run tauri:build:macos    # macOS: DMG .dmg
npm run tauri:build:linux    # Linux: Debian .deb and AppImage
```

On macOS, the raw app bundle can still be built separately for local GUI smoke
testing, but it is not the primary CI release artifact:

```bash
npm run tauri:build:app
```

macOS CI uses Tauri's bundler signing hook with an ad-hoc identity
(`signingIdentity: "-"`) so the `.app` inside the generated DMG is signed
before the DMG is assembled. Local raw `.app` builds can be checked with:

```bash
codesign --verify --deep --strict --verbose=2 src-tauri/target/release/bundle/macos/AutoMD.app
```

The bundle configuration lives in `src-tauri/tauri.conf.json`:

- Product name: `AutoMD`.
- Application id: `com.noir.automd`.
- Bundle targets: `all`.
- macOS signing: ad-hoc signing for non-notarized CI preview builds.
- Icons: `src-tauri/icons/32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.icns`, and `icon.ico`.
- Frontend output: `dist/`.

Build artifacts are produced under `src-tauri/target/release/bundle/`:

- Windows: `bundle/nsis/*-setup.exe` and `bundle/msi/*.msi`.
- macOS: `bundle/dmg/*.dmg`.
- Linux: `bundle/deb/*.deb` and `bundle/appimage/*.AppImage`.

Build on the same OS family as the artifact you want to distribute unless a dedicated cross-compilation pipeline is added. The GitHub Actions workflow does this with one native runner per OS and uploads installer artifacts named `AutoMD-installers-*`.

## Platform Notes

- Windows: native engines can be called directly when present on `PATH`. Linux-only engines should be configured through WSL2, containers, or remote/HPC profiles.
- macOS: distribute separate Apple Silicon and Intel builds when native architecture coverage matters. GPU support is displayed per engine capability rather than assumed globally.
- Linux: validate required WebKit/GTK runtime dependencies for the target distribution. HPC workflows should be tested with site-specific SSH, rsync, module, and scheduler policies.

## Engine Distribution Policy

AutoMD release artifacts do not include GROMACS, OpenMM, AmberTools, LAMMPS, CP2K, GENESIS, HOOMD-blue, DL_POLY, Tinker, NAMD, AMBER pmemd, CHARMM, Desmond, ACEMD, PLUMED, MPI runtimes, CUDA, ROCm, or Python packages.

Open-source engines are integrated through detection, generated inputs, container recipes, and source-build scripts. Restricted or commercial engines are external-only: users must install the engine, satisfy license terms, and save the executable/module path in AutoMD before running adapter entrypoints.

## Scientific Sidecar Delivery

Python scientific dependencies are installed by the user or a site administrator. AutoMD generates reviewable Conda/Mamba environment recipes under project directories, including:

- `generated/prep/environment.yml` for structure preparation.
- `generated/analysis/environment.yml` for MDAnalysis trajectory analysis.
- `build-recipes/<engine>/` for source-build and container guidance.

This keeps the desktop app small and avoids shipping compiled scientific stacks that differ across CPU/GPU platforms.

## Release Checklist

- Run `npm run check`.
- Run `npm run smoke:remote` against a real SSH target when credentials are available. Without `AUTOMD_REMOTE_HOST` the command exits safely after printing a skip message.
- Build platform installer with the appropriate `npm run tauri:build:*` command.
- Launch the packaged app and verify project creation, engine capability display, Workflow parameter mapping, dry-run package generation, mock local run, remote package preview, build recipe export, and report export.
- Confirm restricted/commercial engines show user-license/install guidance and are not bundled.
- Include `README.md`, `docs/ENGINE_ADAPTERS.md`, `docs/PROJECT_FORMAT.md`, `docs/SCIENCE_SIDECAR.md`, `docs/PLUGIN_MANIFESTS.md`, and this release guide with the source distribution.

## Remote Acceptance Smoke

`npm run smoke:remote` is an optional live acceptance check for the in-app SSH/HPC workflow. It validates the current helper script, SSH/rsync upload filters, helper probe, GROMACS scan, SSH-direct detached submit/cancel, result fetch filters, and reconnect behavior. On a real cluster it can also exercise SLURM, PBS, or LSF submission when `AUTOMD_REMOTE_SCHEDULER` is set.

Required:

- `AUTOMD_REMOTE_HOST`

Common options:

- `AUTOMD_REMOTE_PORT`, default `22`.
- `AUTOMD_REMOTE_USER`, optional when `~/.ssh/config` supplies the user.
- `AUTOMD_REMOTE_AUTH`, one of `agent`, `key`, or `password`.
- `AUTOMD_REMOTE_IDENTITY_FILE`, used with `AUTOMD_REMOTE_AUTH=key`.
- `AUTOMD_REMOTE_PASSWORD`, used with `AUTOMD_REMOTE_AUTH=password`; this requires the local `expect` command so the script can create the same ControlMaster-style password session used by the app.
- `AUTOMD_REMOTE_WORKDIR`, default `/tmp/automd-acceptance`.
- `AUTOMD_REMOTE_SCHEDULER`, one of `auto`, `ssh`, `slurm`, `pbs`, or `lsf`; `auto` falls back to SSH direct when no scheduler command is found.
- `AUTOMD_REMOTE_INSTALL_ENGINE=gromacs` to test a real helper-driven conda-forge GROMACS install on the target. Leave unset to scan only.
- `AUTOMD_REMOTE_KEEP=1` to keep local, fetched, and remote evidence directories after the run.

Example for an SSH-direct Linux machine:

```bash
AUTOMD_REMOTE_HOST=example.org \
AUTOMD_REMOTE_USER=root \
AUTOMD_REMOTE_AUTH=agent \
AUTOMD_REMOTE_WORKDIR=/root/automd-acceptance \
npm run smoke:remote
```

Example for a SLURM login node:

```bash
AUTOMD_REMOTE_HOST=login.cluster.edu \
AUTOMD_REMOTE_USER=myuser \
AUTOMD_REMOTE_AUTH=key \
AUTOMD_REMOTE_IDENTITY_FILE=~/.ssh/id_ed25519 \
AUTOMD_REMOTE_WORKDIR=/scratch/$USER/automd-acceptance \
AUTOMD_REMOTE_SCHEDULER=slurm \
npm run smoke:remote
```
