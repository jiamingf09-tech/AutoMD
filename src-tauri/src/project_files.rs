use crate::models::*;
use chrono::{DateTime, Utc};
use std::fs;
use std::path::{Component, Path, PathBuf};
use thiserror::Error;

const TEXT_FILE_MAX_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum ProjectFileError {
    #[error("project path does not exist: {0}")]
    MissingProjectPath(String),
    #[error("project file path must be relative and inside an editable project area")]
    UnsafePath,
    #[error("unsupported editable text file extension: {0}")]
    UnsupportedExtension(String),
    #[error("project text file is too large for inline editing: {size_bytes} bytes")]
    FileTooLarge { size_bytes: u64 },
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
}

pub fn read_project_text_file(request: ProjectTextFileRequest) -> Result<ProjectTextFilePayload, ProjectFileError> {
    let project_root = project_root(&request.project_path)?;
    let relative = editable_relative_path(&request.path)?;
    let path = project_root.join(&relative);
    let metadata = fs::metadata(&path)?;
    if metadata.len() > TEXT_FILE_MAX_BYTES {
        return Err(ProjectFileError::FileTooLarge {
            size_bytes: metadata.len(),
        });
    }
    let contents = fs::read_to_string(&path)?;
    Ok(ProjectTextFilePayload {
        path: relative_path_string(&relative),
        language: language_for(&relative),
        contents,
        size_bytes: metadata.len(),
        modified_at: metadata.modified().ok().map(DateTime::<Utc>::from),
    })
}

pub fn write_project_text_file(request: ProjectTextFileWriteRequest) -> Result<ProjectTextFilePayload, ProjectFileError> {
    let project_root = project_root(&request.project_path)?;
    let relative = editable_relative_path(&request.path)?;
    if request.contents.len() as u64 > TEXT_FILE_MAX_BYTES {
        return Err(ProjectFileError::FileTooLarge {
            size_bytes: request.contents.len() as u64,
        });
    }
    let path = project_root.join(&relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, request.contents)?;
    read_project_text_file(ProjectTextFileRequest {
        project_path: project_root.display().to_string(),
        path: relative_path_string(&relative),
    })
}

fn project_root(path: &str) -> Result<PathBuf, ProjectFileError> {
    let root = PathBuf::from(path);
    if root.exists() {
        Ok(root)
    } else {
        Err(ProjectFileError::MissingProjectPath(path.to_string()))
    }
}

fn editable_relative_path(path: &str) -> Result<PathBuf, ProjectFileError> {
    let trimmed = path.trim();
    if trimmed.is_empty() || Path::new(trimmed).is_absolute() {
        return Err(ProjectFileError::UnsafePath);
    }
    let mut relative = PathBuf::new();
    for component in Path::new(trimmed).components() {
        match component {
            Component::Normal(value) => relative.push(value),
            _ => return Err(ProjectFileError::UnsafePath),
        }
    }
    if !is_editable_root(&relative) {
        return Err(ProjectFileError::UnsafePath);
    }
    let extension = relative
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !is_supported_extension(&extension) {
        return Err(ProjectFileError::UnsupportedExtension(extension));
    }
    Ok(relative)
}

fn is_editable_root(path: &Path) -> bool {
    matches!(
        path.components().next(),
        Some(Component::Normal(value))
            if matches!(
                value.to_str(),
                Some("generated" | "runs" | "remote" | "build-recipes" | "analysis" | "reports")
            )
    )
}

fn is_supported_extension(extension: &str) -> bool {
    matches!(
        extension,
        "mdp"
            | "mdin"
            | "conf"
            | "cfg"
            | "inp"
            | "in"
            | "key"
            | "txt"
            | "json"
            | "yaml"
            | "yml"
            | "py"
            | "sh"
            | "slurm"
            | "pbs"
            | "lsf"
            | "md"
    )
}

fn language_for(path: &Path) -> String {
    match path.extension().and_then(|value| value.to_str()).unwrap_or_default() {
        "mdp" => "gromacs-mdp",
        "mdin" => "amber-mdin",
        "conf" => "namd",
        "inp" | "in" => "native-input",
        "key" => "tinker-key",
        "py" => "python",
        "sh" | "slurm" | "pbs" | "lsf" => "bash",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "md" => "markdown",
        _ => "text",
    }
    .to_string()
}

fn relative_path_string(path: &Path) -> String {
    path.components()
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
    fn reads_and_writes_editable_project_file() {
        let root = std::env::temp_dir().join(format!("automd-project-files-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("generated/gromacs")).expect("dir");
        let request = ProjectTextFileWriteRequest {
            project_path: root.display().to_string(),
            path: "generated/gromacs/md.mdp".to_string(),
            contents: "integrator = md\nnsteps = 1000\n".to_string(),
        };
        let written = write_project_text_file(request).expect("write");
        assert_eq!(written.language, "gromacs-mdp");
        assert!(written.contents.contains("nsteps"));

        let read = read_project_text_file(ProjectTextFileRequest {
            project_path: root.display().to_string(),
            path: "generated/gromacs/md.mdp".to_string(),
        })
        .expect("read");
        assert_eq!(read.contents, written.contents);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn rejects_parent_traversal_and_unsupported_extensions() {
        assert!(editable_relative_path("../outside.mdp").is_err());
        assert!(editable_relative_path("generated/gromacs/md.bin").is_err());
        assert!(editable_relative_path("inputs/system.pdb").is_err());
    }
}
