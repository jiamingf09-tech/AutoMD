# Plugin Manifests

AutoMD supports a lightweight plugin discovery surface for future extension packs.

The current implementation scans the app plugin directory for files ending with:

```text
*.automd-plugin.json
```

The registry currently treats plugins as manifests only. It records capabilities and entrypoints so the GUI can show available engine adapters, analysis modules, remote schedulers, build recipes, and report templates. It does not execute arbitrary plugin code yet.

The desktop GUI exposes this snapshot in the Plugins tab, including the plugin root, built-in manifests, external manifest source paths, capabilities, and parse warnings.

## Manifest Shape

```json
{
  "id": "example-lammps-pack",
  "name": "Example LAMMPS Pack",
  "version": "0.1.0",
  "kind": "engineAdapter",
  "entrypoint": "plugins/example-lammps/run.js",
  "engineId": "lammps",
  "capabilities": ["prepare", "run", "parse_progress"],
  "licensePolicy": "openSource",
  "warnings": []
}
```

## Kinds

- `engineAdapter`
- `analysisModule`
- `remoteScheduler`
- `buildRecipe`
- `reportTemplate`

## Built-In Manifests

AutoMD always exposes built-in manifests for:

- Core engine adapters.
- Core XVG/CSV analysis parsers.
- SSH/SLURM/PBS/LSF remote schedulers.
- Container/source build recipes.
- Markdown/HTML/PDF report templates.

External manifests are additive. If a manifest is malformed, AutoMD reports a warning but keeps loading the rest of the registry.
