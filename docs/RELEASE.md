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

## Desktop Bundles

Build the Tauri desktop package on each target operating system:

```bash
npm run tauri:build
```

On macOS, the app bundle and DMG target can also be built separately:

```bash
npm run tauri:build:app
npm run tauri:build:dmg
```

For unsigned local macOS app bundles, apply an ad-hoc signature before testing
or uploading the `.app` directory:

```bash
codesign --force --deep --sign - src-tauri/target/release/bundle/macos/AutoMD.app
codesign --verify --deep --strict --verbose=2 src-tauri/target/release/bundle/macos/AutoMD.app
```

The bundle configuration lives in `src-tauri/tauri.conf.json`:

- Product name: `AutoMD`.
- Application id: `com.noir.automd`.
- Bundle targets: `all`.
- Icons: `src-tauri/icons/32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.icns`, and `icon.ico`.
- Frontend output: `dist/`.

Build artifacts are produced under `src-tauri/target/release/bundle/`. On this macOS workspace, `npm run tauri:build:app` produces `bundle/macos/AutoMD.app`, and `npm run tauri:build:dmg` produces `bundle/dmg/AutoMD_0.1.0_aarch64.dmg`. Build on the same OS family as the artifact you want to distribute unless a dedicated cross-compilation pipeline is added.

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
- Build platform bundle with `npm run tauri:build`.
- Launch the packaged app and verify project creation, engine capability display, Workflow parameter mapping, dry-run package generation, mock local run, remote package preview, build recipe export, and report export.
- Confirm restricted/commercial engines show user-license/install guidance and are not bundled.
- Include `README.md`, `docs/ENGINE_ADAPTERS.md`, `docs/PROJECT_FORMAT.md`, `docs/SCIENCE_SIDECAR.md`, `docs/PLUGIN_MANIFESTS.md`, and this release guide with the source distribution.
