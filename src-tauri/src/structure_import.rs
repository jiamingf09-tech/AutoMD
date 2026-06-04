use crate::models::*;
use chrono::Utc;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StructureImportError {
    #[error("project path does not exist: {0}")]
    MissingProjectPath(String),
    #[error("source file is required for this structure kind")]
    MissingSourcePath,
    #[error("SMILES input is required")]
    MissingSmiles,
    #[error("destination already exists: {0}")]
    DestinationExists(String),
    #[error("structure file is too large for inline viewer loading: {size_bytes} bytes")]
    StructureFileTooLarge { size_bytes: u64 },
    #[error("unsupported structure viewer format: {0}")]
    UnsupportedViewerFormat(String),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
}

const VIEWER_STRUCTURE_MAX_BYTES: u64 = 50 * 1024 * 1024;
const STRUCTURE_INDEX_FILE: &str = ".automd-structures.json";

pub fn import_structure(request: StructureImportRequest) -> Result<StructureImportResult, StructureImportError> {
    let project_root = PathBuf::from(&request.project_path);
    if !project_root.exists() {
        return Err(StructureImportError::MissingProjectPath(request.project_path));
    }
    let inputs_dir = project_root.join("inputs");
    fs::create_dir_all(&inputs_dir)?;

    let display_name = request
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| default_display_name(&request));
    let slug = slugify(&display_name);
    let extension = extension_for(&request);
    let destination = unique_destination(&inputs_dir, &slug, extension, request.overwrite);

    if destination.exists() && !request.overwrite {
        return Err(StructureImportError::DestinationExists(relative_path(&project_root, &destination)));
    }

    let contents = match request.source_kind {
        StructureSourceKind::Smiles => {
            let smiles = request
                .smiles
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or(StructureImportError::MissingSmiles)?;
            let name = display_name.replace('\n', " ");
            format!("{smiles} {name}\n")
        }
        StructureSourceKind::EngineProject => {
            let source = request
                .source_path
                .as_deref()
                .map(PathBuf::from)
                .ok_or(StructureImportError::MissingSourcePath)?;
            if source.is_dir() {
                engine_project_manifest(&source)?
            } else {
                fs::read_to_string(source)?
            }
        }
        _ => {
            let source = request
                .source_path
                .as_deref()
                .map(PathBuf::from)
                .ok_or(StructureImportError::MissingSourcePath)?;
            fs::read_to_string(source)?
        }
    };

    fs::write(&destination, &contents)?;
    let source_kind = request.source_kind.clone();
    let summary = summarize_contents(&source_kind, &contents);
    let relative = relative_path(&project_root, &destination);
    let warnings = import_warnings(&source_kind, &summary);
    let imported_at = Utc::now();
    upsert_imported_structure(
        &project_root,
        ImportedStructureEntry {
            id: relative.clone(),
            name: display_name.clone(),
            source_path: request.source_path.clone().or_else(|| request.smiles.clone()),
            imported_path: relative.clone(),
            source_kind: source_kind.clone(),
            imported_at,
            summary: Some(summary.clone()),
        },
    )?;

    Ok(StructureImportResult {
        system: SystemSpec {
            source_kind,
            source_path: Some(relative.clone()),
            name: display_name,
            molecule_count: summary.molecule_count.or(summary.residue_count),
            has_ligand: infer_ligand(&contents, &summary),
            has_membrane: infer_membrane(&contents),
            notes: system_notes(&summary),
        },
        imported_path: relative,
        summary,
        warnings,
        imported_at,
    })
}

pub fn list_imported_structures(project_path: String) -> Result<Vec<ImportedStructureEntry>, StructureImportError> {
    let project_root = PathBuf::from(&project_path);
    if !project_root.exists() {
        return Err(StructureImportError::MissingProjectPath(project_path));
    }
    let mut entries = read_structure_index(&project_root)?;
    let mut seen: BTreeSet<String> = BTreeSet::new();
    entries.retain(|entry| {
        let exists = safe_project_path(&project_root, &entry.imported_path).is_file();
        exists && seen.insert(entry.imported_path.clone())
    });

    let inputs_dir = project_root.join("inputs");
    if let Ok(read) = fs::read_dir(&inputs_dir) {
        for item in read.flatten() {
            let path = item.path();
            if !path.is_file() {
                continue;
            }
            let relative = relative_path(&project_root, &path);
            if seen.contains(&relative) {
                continue;
            }
            if let Some(source_kind) = source_kind_from_path(&relative) {
                let contents = fs::read_to_string(&path).unwrap_or_default();
                let summary = (!contents.is_empty()).then(|| summarize_contents(&source_kind, &contents));
                let imported_at = item
                    .metadata()
                    .ok()
                    .and_then(|metadata| metadata.modified().ok())
                    .map(chrono::DateTime::<Utc>::from)
                    .unwrap_or_else(Utc::now);
                entries.push(ImportedStructureEntry {
                    id: relative.clone(),
                    name: display_name_from_imported_path(&relative),
                    source_path: None,
                    imported_path: relative.clone(),
                    source_kind,
                    imported_at,
                    summary,
                });
                seen.insert(relative);
            }
        }
    }

    entries.sort_by(|a, b| b.imported_at.cmp(&a.imported_at).then_with(|| a.imported_path.cmp(&b.imported_path)));
    write_structure_index(&project_root, &entries)?;
    Ok(entries)
}

pub fn delete_imported_structure(request: DeleteImportedStructureRequest) -> Result<bool, StructureImportError> {
    let project_root = PathBuf::from(&request.project_path);
    if !project_root.exists() {
        return Err(StructureImportError::MissingProjectPath(request.project_path));
    }
    let relative = request.imported_path.trim();
    if !relative.starts_with("inputs/") {
        return Err(StructureImportError::MissingSourcePath);
    }
    let target = safe_project_path(&project_root, relative);
    if target.is_file() {
        fs::remove_file(&target)?;
    }
    let mut entries = read_structure_index(&project_root)?;
    entries.retain(|entry| entry.imported_path != relative);
    write_structure_index(&project_root, &entries)?;
    Ok(true)
}

pub fn read_structure_file(request: StructureFileRequest) -> Result<StructureFilePayload, StructureImportError> {
    let project_root = PathBuf::from(&request.project_path);
    if !project_root.exists() {
        return Err(StructureImportError::MissingProjectPath(request.project_path));
    }
    let source_path = request.source_path.trim();
    if source_path.is_empty() {
        return Err(StructureImportError::MissingSourcePath);
    }
    let format = viewer_format(source_path)
        .ok_or_else(|| StructureImportError::UnsupportedViewerFormat(source_path.to_string()))?;
    let path = safe_project_path(&project_root, source_path);
    let metadata = fs::metadata(&path)?;
    if metadata.len() > VIEWER_STRUCTURE_MAX_BYTES {
        return Err(StructureImportError::StructureFileTooLarge {
            size_bytes: metadata.len(),
        });
    }
    let contents = fs::read_to_string(path)?;

    Ok(StructureFilePayload {
        source_path: source_path.to_string(),
        format: format.to_string(),
        contents,
        size_bytes: metadata.len(),
    })
}

fn summarize_contents(kind: &StructureSourceKind, contents: &str) -> StructureSummary {
    match kind {
        StructureSourceKind::Pdb => summarize_pdb_like(contents, "PDB atom records"),
        StructureSourceKind::Mmcif => summarize_pdb_like(contents, "mmCIF atom_site-style records"),
        StructureSourceKind::Sdf => summarize_sdf(contents),
        StructureSourceKind::Mol2 => summarize_mol2(contents),
        StructureSourceKind::Smiles => StructureSummary {
            atom_count: None,
            residue_count: None,
            chain_count: None,
            molecule_count: Some(contents.lines().filter(|line| !line.trim().is_empty()).count() as u32),
            model_count: None,
            format_note: "SMILES line input".to_string(),
        },
        StructureSourceKind::EngineProject => StructureSummary {
            atom_count: None,
            residue_count: None,
            chain_count: None,
            molecule_count: None,
            model_count: None,
            format_note: "Existing engine project imported as an input manifest".to_string(),
        },
    }
}

fn summarize_pdb_like(contents: &str, format_note: &str) -> StructureSummary {
    let mut atoms = 0u32;
    let mut residues = BTreeSet::new();
    let mut chains = BTreeSet::new();
    let mut models = 0u32;

    for line in contents.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("MODEL") {
            models += 1;
        }
        if trimmed.starts_with("ATOM") || trimmed.starts_with("HETATM") {
            atoms += 1;
            let residue_name = field(line, 17, 20).trim();
            let chain = field(line, 21, 22).trim();
            let residue_number = field(line, 22, 27).trim();
            if !chain.is_empty() {
                chains.insert(chain.to_string());
            }
            if !residue_name.is_empty() || !residue_number.is_empty() {
                residues.insert(format!("{chain}:{residue_name}:{residue_number}"));
            }
        }
    }

    StructureSummary {
        atom_count: Some(atoms),
        residue_count: Some(residues.len() as u32).filter(|value| *value > 0),
        chain_count: Some(chains.len() as u32).filter(|value| *value > 0),
        molecule_count: Some(residues.len() as u32).filter(|value| *value > 0),
        model_count: Some(models.max(1)).filter(|_| atoms > 0),
        format_note: format_note.to_string(),
    }
}

fn summarize_sdf(contents: &str) -> StructureSummary {
    let molecules = contents.matches("$$$$").count().max(if contents.trim().is_empty() { 0 } else { 1 }) as u32;
    StructureSummary {
        atom_count: None,
        residue_count: None,
        chain_count: None,
        molecule_count: Some(molecules).filter(|value| *value > 0),
        model_count: None,
        format_note: "SDF molecule records".to_string(),
    }
}

fn summarize_mol2(contents: &str) -> StructureSummary {
    let molecules = contents.matches("@<TRIPOS>MOLECULE").count() as u32;
    StructureSummary {
        atom_count: None,
        residue_count: None,
        chain_count: None,
        molecule_count: Some(molecules).filter(|value| *value > 0),
        model_count: None,
        format_note: "MOL2 molecule records".to_string(),
    }
}

fn infer_ligand(contents: &str, summary: &StructureSummary) -> bool {
    if summary.format_note.starts_with("SMILES")
        || summary.format_note.starts_with("SDF")
        || summary.format_note.starts_with("MOL2")
    {
        return true;
    }

    contents.lines().any(|line| {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("HETATM") {
            return false;
        }
        let residue = field(line, 17, 20).trim().to_ascii_uppercase();
        !matches!(
            residue.as_str(),
            "HOH" | "WAT" | "SOL" | "TIP" | "NA" | "CL" | "K" | "MG" | "CA" | "ZN" | "MN"
        )
    })
}

fn infer_membrane(contents: &str) -> bool {
    let membrane_residues = [
        "POPC", "POPE", "POPG", "DOPC", "DPPC", "DLPC", "CHOL", "POPS", "DOPS", "LIP",
    ];
    contents.lines().any(|line| {
        let residue = field(line, 17, 20).trim().to_ascii_uppercase();
        membrane_residues.contains(&residue.as_str())
    })
}

fn import_warnings(kind: &StructureSourceKind, summary: &StructureSummary) -> Vec<String> {
    let mut warnings = Vec::new();
    if matches!(kind, StructureSourceKind::Sdf | StructureSourceKind::Mol2 | StructureSourceKind::Smiles) {
        warnings.push("小分子输入已保存；真实 MD 前仍需要配体参数化和拓扑生成。".to_string());
    }
    if summary.atom_count == Some(0) {
        warnings.push("未检测到 ATOM/HETATM 记录；请确认文件格式和内容。".to_string());
    }
    if matches!(kind, StructureSourceKind::EngineProject) {
        warnings.push("已有引擎工程已记录为 manifest；后续适配器会读取其中的原生输入文件。".to_string());
    }
    warnings
}

fn system_notes(summary: &StructureSummary) -> Vec<String> {
    let mut notes = vec![summary.format_note.clone()];
    if let Some(atom_count) = summary.atom_count {
        notes.push(format!("atoms={atom_count}"));
    }
    if let Some(residue_count) = summary.residue_count {
        notes.push(format!("residues={residue_count}"));
    }
    if let Some(chain_count) = summary.chain_count {
        notes.push(format!("chains={chain_count}"));
    }
    notes
}

fn default_display_name(request: &StructureImportRequest) -> String {
    request
        .source_path
        .as_deref()
        .and_then(|path| Path::new(path).file_stem())
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "imported-system".to_string())
}

fn extension_for(request: &StructureImportRequest) -> &'static str {
    match request.source_kind {
        StructureSourceKind::Pdb => "pdb",
        StructureSourceKind::Mmcif => "cif",
        StructureSourceKind::Sdf => "sdf",
        StructureSourceKind::Mol2 => "mol2",
        StructureSourceKind::Smiles => "smi",
        StructureSourceKind::EngineProject => "manifest.txt",
    }
}

fn viewer_format(path: &str) -> Option<&'static str> {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".pdb") {
        Some("pdb")
    } else if lower.ends_with(".cif") || lower.ends_with(".mmcif") {
        Some("mmcif")
    } else {
        None
    }
}

fn source_kind_from_path(path: &str) -> Option<StructureSourceKind> {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".pdb") || lower.ends_with(".ent") {
        Some(StructureSourceKind::Pdb)
    } else if lower.ends_with(".cif") || lower.ends_with(".mmcif") {
        Some(StructureSourceKind::Mmcif)
    } else if lower.ends_with(".sdf") {
        Some(StructureSourceKind::Sdf)
    } else if lower.ends_with(".mol2") {
        Some(StructureSourceKind::Mol2)
    } else if lower.ends_with(".smi") || lower.ends_with(".smiles") {
        Some(StructureSourceKind::Smiles)
    } else if lower.ends_with(".manifest.txt") {
        Some(StructureSourceKind::EngineProject)
    } else {
        None
    }
}

fn safe_project_path(root: &Path, relative: &str) -> PathBuf {
    let mut destination = root.to_path_buf();
    for component in Path::new(relative).components() {
        if let Component::Normal(value) = component {
            destination.push(value);
        }
    }
    destination
}

fn structure_index_path(project_root: &Path) -> PathBuf {
    project_root.join(STRUCTURE_INDEX_FILE)
}

fn read_structure_index(project_root: &Path) -> Result<Vec<ImportedStructureEntry>, StructureImportError> {
    let path = structure_index_path(project_root);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let contents = fs::read_to_string(path)?;
    Ok(serde_json::from_str::<Vec<ImportedStructureEntry>>(&contents).unwrap_or_default())
}

fn write_structure_index(project_root: &Path, entries: &[ImportedStructureEntry]) -> Result<(), StructureImportError> {
    let contents = serde_json::to_string_pretty(entries).unwrap_or_else(|_| "[]".to_string());
    fs::write(structure_index_path(project_root), contents)?;
    Ok(())
}

fn upsert_imported_structure(project_root: &Path, entry: ImportedStructureEntry) -> Result<(), StructureImportError> {
    let mut entries = read_structure_index(project_root)?;
    entries.retain(|existing| existing.imported_path != entry.imported_path);
    entries.push(entry);
    write_structure_index(project_root, &entries)
}

fn field(line: &str, start: usize, end: usize) -> &str {
    line.get(start..end).unwrap_or("")
}

fn engine_project_manifest(source: &Path) -> Result<String, std::io::Error> {
    let mut lines = vec![format!("AutoMD engine project manifest: {}", source.display())];
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        let label = if path.is_dir() { "dir" } else { "file" };
        lines.push(format!("{label}\t{}", path.display()));
    }
    lines.sort();
    Ok(format!("{}\n", lines.join("\n")))
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if (ch.is_whitespace() || ch == '-' || ch == '_') && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        "imported-system".to_string()
    } else {
        trimmed.to_string()
    }
}

fn unique_destination(inputs_dir: &Path, slug: &str, extension: &str, overwrite: bool) -> PathBuf {
    let first = inputs_dir.join(format!("{slug}.{extension}"));
    if overwrite || !first.exists() {
        return first;
    }
    for index in 2..10_000 {
        let candidate = inputs_dir.join(format!("{slug}-{index}.{extension}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    first
}

fn display_name_from_imported_path(relative: &str) -> String {
    let filename = relative.rsplit('/').next().unwrap_or(relative);
    let stem = filename
        .strip_suffix(".manifest.txt")
        .or_else(|| filename.rsplit_once('.').map(|(head, _)| head))
        .unwrap_or(filename);
    let mut name = String::new();
    for part in stem.split(['-', '_']).filter(|part| !part.is_empty()) {
        if !name.is_empty() {
            name.push(' ');
        }
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            name.push(first.to_ascii_uppercase());
            name.push_str(chars.as_str());
        }
    }
    if name.is_empty() {
        filename.to_string()
    } else {
        name
    }
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn imports_pdb_and_summarizes_atoms() {
        let root = std::env::temp_dir().join(format!("automd-import-pdb-{}", Uuid::new_v4()));
        let source = root.join("source.pdb");
        fs::create_dir_all(&root).expect("root");
        fs::write(
            &source,
            "ATOM      1  N   ALA A   1      11.104  13.207   9.010  1.00 20.00           N\nHETATM    2  C1  LIG B   2      12.000  14.000  10.000  1.00 20.00           C\n",
        )
        .expect("pdb");

        let result = import_structure(StructureImportRequest {
            project_path: root.display().to_string(),
            source_kind: StructureSourceKind::Pdb,
            source_path: Some(source.display().to_string()),
            smiles: None,
            display_name: Some("Demo system".to_string()),
            overwrite: false,
        })
        .expect("import");

        assert_eq!(result.imported_path, "inputs/demo-system.pdb");
        assert_eq!(result.summary.atom_count, Some(2));
        assert_eq!(result.summary.chain_count, Some(2));
        assert!(result.system.has_ligand);
        assert!(root.join("inputs/demo-system.pdb").exists());

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn reads_imported_pdb_for_viewer() {
        let root = std::env::temp_dir().join(format!("automd-viewer-pdb-{}", Uuid::new_v4()));
        let source = root.join("source.pdb");
        fs::create_dir_all(&root).expect("root");
        fs::write(
            &source,
            "ATOM      1  N   ALA A   1      11.104  13.207   9.010  1.00 20.00           N\n",
        )
        .expect("pdb");

        let imported = import_structure(StructureImportRequest {
            project_path: root.display().to_string(),
            source_kind: StructureSourceKind::Pdb,
            source_path: Some(source.display().to_string()),
            smiles: None,
            display_name: Some("viewer system".to_string()),
            overwrite: false,
        })
        .expect("import");

        let payload = read_structure_file(StructureFileRequest {
            project_path: root.display().to_string(),
            source_path: imported.imported_path,
        })
        .expect("viewer payload");

        assert_eq!(payload.format, "pdb");
        assert!(payload.contents.contains("ATOM"));
        assert!(payload.size_bytes > 0);

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn imports_smiles_as_ligand_input() {
        let root = std::env::temp_dir().join(format!("automd-import-smiles-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("root");

        let result = import_structure(StructureImportRequest {
            project_path: root.display().to_string(),
            source_kind: StructureSourceKind::Smiles,
            source_path: None,
            smiles: Some("CCO".to_string()),
            display_name: Some("ethanol".to_string()),
            overwrite: false,
        })
        .expect("import");

        assert_eq!(result.imported_path, "inputs/ethanol.smi");
        assert_eq!(result.summary.molecule_count, Some(1));
        assert!(result.system.has_ligand);
        assert!(result.warnings.iter().any(|warning| warning.contains("配体参数化")));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn imports_engine_project_directory_as_manifest() {
        let root = std::env::temp_dir().join(format!("automd-import-engine-{}", Uuid::new_v4()));
        let native = root.join("native");
        fs::create_dir_all(&native).expect("native");
        fs::write(native.join("topol.top"), "mock topology").expect("topology");

        let result = import_structure(StructureImportRequest {
            project_path: root.display().to_string(),
            source_kind: StructureSourceKind::EngineProject,
            source_path: Some(native.display().to_string()),
            smiles: None,
            display_name: Some("native gromacs".to_string()),
            overwrite: false,
        })
        .expect("import");

        assert_eq!(result.imported_path, "inputs/native-gromacs.manifest.txt");
        assert!(root.join("inputs/native-gromacs.manifest.txt").exists());
        assert!(result.warnings.iter().any(|warning| warning.contains("manifest")));

        fs::remove_dir_all(root).expect("cleanup");
    }
}
