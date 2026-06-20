use crate::models::*;
use chrono::Utc;
use std::fs;
use std::path::{Component, Path, PathBuf};
use thiserror::Error;

const MAX_TEXT_INDEX_BYTES: u64 = 256 * 1024 * 1024;
const DEFAULT_MAX_PREVIEW_FRAMES: usize = 120;
const DEFAULT_CHUNK_FRAMES: usize = 5;
const DEFAULT_MAX_CHUNK_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum TrajectoryError {
    #[error("project path does not exist: {0}")]
    MissingProjectPath(String),
    #[error("unsafe trajectory path: {0}")]
    UnsafePath(String),
    #[error("trajectory path does not exist: {0}")]
    MissingTrajectory(String),
    #[error("trajectory is not UTF-8 text and cannot be chunked by this reader")]
    NonUtf8Trajectory,
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn index_trajectory(
    request: TrajectoryIndexRequest,
) -> Result<TrajectoryIndex, TrajectoryError> {
    let project_root = project_root(&request.project_path)?;
    let relative = safe_relative(&request.trajectory_path)?;
    let trajectory_path = project_root.join(&relative);
    if !trajectory_path.exists() {
        return Err(TrajectoryError::MissingTrajectory(request.trajectory_path));
    }

    let metadata = fs::metadata(&trajectory_path)?;
    let size_bytes = metadata.len();
    let format = trajectory_format(&relative);
    let mut warnings = Vec::new();
    let (strategy, frames) = match format {
        TrajectoryFormat::Pdb | TrajectoryFormat::Xyz | TrajectoryFormat::LammpsDump => {
            if size_bytes > MAX_TEXT_INDEX_BYTES {
                warnings.push(format!(
                    "Text trajectory is larger than {} MB; indexed as metadata only until a streaming indexer is used.",
                    MAX_TEXT_INDEX_BYTES / 1024 / 1024
                ));
                (TrajectoryIndexStrategy::MetadataOnly, Vec::new())
            } else {
                let contents = fs::read_to_string(&trajectory_path)
                    .map_err(|_| TrajectoryError::NonUtf8Trajectory)?;
                let frames = parse_text_frames(&contents, &format, &mut warnings);
                if frames.is_empty() {
                    warnings.push(
                        "No frame boundaries were detected in the trajectory text.".to_string(),
                    );
                }
                (TrajectoryIndexStrategy::TextOffsets, frames)
            }
        }
        TrajectoryFormat::Xtc
        | TrajectoryFormat::Trr
        | TrajectoryFormat::Dcd
        | TrajectoryFormat::Netcdf
        | TrajectoryFormat::Gsd => {
            warnings.push(
                "Binary trajectory registered as metadata-only; use the Python/MDAnalysis sidecar or Mol* binary loader for decoded frames."
                    .to_string(),
            );
            (TrajectoryIndexStrategy::MetadataOnly, Vec::new())
        }
        TrajectoryFormat::Unknown => {
            warnings.push("Unknown trajectory format; only file metadata was indexed.".to_string());
            (TrajectoryIndexStrategy::Unsupported, Vec::new())
        }
    };

    let sampled_frames = sample_frames(
        &frames,
        request.frame_stride.unwrap_or(1).max(1),
        request
            .max_preview_frames
            .unwrap_or(DEFAULT_MAX_PREVIEW_FRAMES),
    );
    let frame_count = if frames.is_empty() && strategy != TrajectoryIndexStrategy::TextOffsets {
        None
    } else {
        Some(frames.len())
    };

    let mut index = TrajectoryIndex {
        project_path: project_root.display().to_string(),
        trajectory_path: relative_path_string(&relative),
        format,
        strategy,
        size_bytes,
        frame_count,
        sampled_frames,
        index_path: None,
        warnings,
        generated_at: Utc::now(),
    };

    if request.write_index {
        let index_relative = index_manifest_path(&relative);
        let index_path = project_root.join(&index_relative);
        if let Some(parent) = index_path.parent() {
            fs::create_dir_all(parent)?;
        }
        index.index_path = Some(relative_path_string(&index_relative));
        fs::write(&index_path, serde_json::to_string_pretty(&index)?)?;
    }

    Ok(index)
}

pub fn read_trajectory_chunk(
    request: TrajectoryChunkRequest,
) -> Result<TrajectoryChunk, TrajectoryError> {
    let project_root = project_root(&request.project_path)?;
    let relative = safe_relative(&request.trajectory_path)?;
    let trajectory_path = project_root.join(&relative);
    if !trajectory_path.exists() {
        return Err(TrajectoryError::MissingTrajectory(request.trajectory_path));
    }

    let format = trajectory_format(&relative);
    let mut warnings = Vec::new();
    if !matches!(
        format,
        TrajectoryFormat::Pdb | TrajectoryFormat::Xyz | TrajectoryFormat::LammpsDump
    ) {
        return Ok(TrajectoryChunk {
            project_path: project_root.display().to_string(),
            trajectory_path: relative_path_string(&relative),
            frames: Vec::new(),
            truncated: false,
            warnings: vec![
                "This trajectory format is metadata-only in the Rust reader; decoded chunks require the Python sidecar or a native binary decoder."
                    .to_string(),
            ],
            generated_at: Utc::now(),
        });
    }

    let contents =
        fs::read_to_string(&trajectory_path).map_err(|_| TrajectoryError::NonUtf8Trajectory)?;
    let frames = parse_text_frames(&contents, &format, &mut warnings);
    let selected = select_frames(&frames, &request);
    let max_bytes = request.max_bytes.unwrap_or(DEFAULT_MAX_CHUNK_BYTES);
    let mut used_bytes = 0u64;
    let mut truncated = false;
    let mut payloads = Vec::new();

    for descriptor in selected {
        let frame_size = descriptor.byte_end.saturating_sub(descriptor.byte_start);
        if used_bytes.saturating_add(frame_size) > max_bytes && !payloads.is_empty() {
            truncated = true;
            break;
        }
        let Some(slice) =
            contents.get(descriptor.byte_start as usize..descriptor.byte_end as usize)
        else {
            warnings.push(format!(
                "Frame {} byte range was not valid UTF-8.",
                descriptor.frame_index
            ));
            continue;
        };
        used_bytes = used_bytes.saturating_add(frame_size);
        payloads.push(TrajectoryFramePayload {
            frame_index: descriptor.frame_index,
            label: descriptor.label.clone(),
            format: format.clone(),
            contents: slice.to_string(),
            atom_count: descriptor.atom_count,
            time_ps: descriptor.time_ps,
        });
    }

    Ok(TrajectoryChunk {
        project_path: project_root.display().to_string(),
        trajectory_path: relative_path_string(&relative),
        frames: payloads,
        truncated,
        warnings,
        generated_at: Utc::now(),
    })
}

fn project_root(project_path: &str) -> Result<PathBuf, TrajectoryError> {
    let root = PathBuf::from(project_path);
    if !root.exists() {
        return Err(TrajectoryError::MissingProjectPath(
            project_path.to_string(),
        ));
    }
    Ok(root)
}

fn safe_relative(relative: &str) -> Result<PathBuf, TrajectoryError> {
    let path = Path::new(relative);
    let mut safe = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => safe.push(value),
            Component::CurDir => {}
            _ => return Err(TrajectoryError::UnsafePath(relative.to_string())),
        }
    }
    if safe.as_os_str().is_empty() {
        return Err(TrajectoryError::UnsafePath(relative.to_string()));
    }
    Ok(safe)
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

fn trajectory_format(path: &Path) -> TrajectoryFormat {
    let lower = relative_path_string(path).to_ascii_lowercase();
    if lower.ends_with(".pdb") || lower.ends_with(".ent") {
        TrajectoryFormat::Pdb
    } else if lower.ends_with(".xyz") {
        TrajectoryFormat::Xyz
    } else if lower.ends_with(".lammpstrj") || lower.ends_with(".dump") {
        TrajectoryFormat::LammpsDump
    } else if lower.ends_with(".xtc") {
        TrajectoryFormat::Xtc
    } else if lower.ends_with(".trr") {
        TrajectoryFormat::Trr
    } else if lower.ends_with(".dcd") {
        TrajectoryFormat::Dcd
    } else if lower.ends_with(".nc") || lower.ends_with(".netcdf") {
        TrajectoryFormat::Netcdf
    } else if lower.ends_with(".gsd") {
        TrajectoryFormat::Gsd
    } else {
        TrajectoryFormat::Unknown
    }
}

fn parse_text_frames(
    contents: &str,
    format: &TrajectoryFormat,
    warnings: &mut Vec<String>,
) -> Vec<TrajectoryFrameDescriptor> {
    match format {
        TrajectoryFormat::Pdb => parse_pdb_frames(contents),
        TrajectoryFormat::Xyz => parse_xyz_frames(contents, warnings),
        TrajectoryFormat::LammpsDump => parse_lammps_frames(contents),
        _ => Vec::new(),
    }
}

fn line_spans(contents: &str) -> Vec<(u64, u64, &str)> {
    let mut spans = Vec::new();
    let mut start = 0usize;
    for line in contents.split_inclusive('\n') {
        let end = start + line.len();
        spans.push((
            start as u64,
            end as u64,
            line.trim_end_matches(['\r', '\n']),
        ));
        start = end;
    }
    if start < contents.len() {
        spans.push((start as u64, contents.len() as u64, &contents[start..]));
    }
    spans
}

fn parse_pdb_frames(contents: &str) -> Vec<TrajectoryFrameDescriptor> {
    let lines = line_spans(contents);
    let mut frames = Vec::new();
    let mut model_start = None;
    let mut atom_count = 0u32;
    let mut label = String::new();
    let mut total_atoms = 0u32;
    let mut saw_model = false;

    for (start, end, line) in &lines {
        if line.starts_with("MODEL") {
            if let Some(previous_start) = model_start {
                frames.push(frame_descriptor(
                    frames.len(),
                    previous_start,
                    *start,
                    Some(atom_count),
                    None,
                    &label,
                ));
            }
            saw_model = true;
            model_start = Some(*start);
            atom_count = 0;
            label = line.trim().to_string();
        } else if line.starts_with("ATOM") || line.starts_with("HETATM") {
            if model_start.is_some() {
                atom_count = atom_count.saturating_add(1);
            }
            total_atoms = total_atoms.saturating_add(1);
        } else if line.starts_with("ENDMDL") {
            if let Some(previous_start) = model_start.take() {
                frames.push(frame_descriptor(
                    frames.len(),
                    previous_start,
                    *end,
                    Some(atom_count),
                    None,
                    &label,
                ));
                atom_count = 0;
                label.clear();
            }
        }
    }

    if let Some(previous_start) = model_start {
        frames.push(frame_descriptor(
            frames.len(),
            previous_start,
            contents.len() as u64,
            Some(atom_count),
            None,
            &label,
        ));
    } else if !saw_model && total_atoms > 0 {
        frames.push(frame_descriptor(
            0,
            0,
            contents.len() as u64,
            Some(total_atoms),
            None,
            "single PDB model",
        ));
    }

    frames
}

fn parse_xyz_frames(contents: &str, warnings: &mut Vec<String>) -> Vec<TrajectoryFrameDescriptor> {
    let lines = line_spans(contents);
    let mut frames = Vec::new();
    let mut index = 0usize;

    while index < lines.len() {
        let (start, _, line) = lines[index];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            index += 1;
            continue;
        }

        let Ok(atom_count) = trimmed.parse::<usize>() else {
            warnings.push(format!(
                "Skipped XYZ line {} because atom count was not numeric.",
                index + 1
            ));
            index += 1;
            continue;
        };
        let required = atom_count.saturating_add(2);
        if index + required > lines.len() {
            warnings.push(format!("XYZ frame {} is incomplete.", frames.len()));
            break;
        }
        let comment = lines
            .get(index + 1)
            .map(|(_, _, value)| value.trim())
            .unwrap_or("");
        let end = lines[index + required - 1].1;
        frames.push(frame_descriptor(
            frames.len(),
            start,
            end,
            Some(atom_count as u32),
            parse_time_ps(comment),
            if comment.is_empty() {
                "XYZ frame"
            } else {
                comment
            },
        ));
        index += required;
    }

    frames
}

fn parse_lammps_frames(contents: &str) -> Vec<TrajectoryFrameDescriptor> {
    let lines = line_spans(contents);
    let mut frames = Vec::new();
    let mut current_start = None;
    let mut current_time = None;
    let mut current_atoms = None;
    let mut current_label = "LAMMPS frame".to_string();

    for (idx, (start, _, line)) in lines.iter().enumerate() {
        if line.starts_with("ITEM: TIMESTEP") {
            if let Some(previous_start) = current_start {
                frames.push(frame_descriptor(
                    frames.len(),
                    previous_start,
                    *start,
                    current_atoms,
                    current_time,
                    &current_label,
                ));
            }
            current_start = Some(*start);
            current_time = lines
                .get(idx + 1)
                .and_then(|(_, _, value)| value.trim().parse::<f64>().ok());
            current_label = current_time
                .map(|time| format!("LAMMPS timestep {time}"))
                .unwrap_or_else(|| "LAMMPS frame".to_string());
            current_atoms = None;
        } else if line.starts_with("ITEM: NUMBER OF ATOMS") {
            current_atoms = lines
                .get(idx + 1)
                .and_then(|(_, _, value)| value.trim().parse::<u32>().ok());
        }
    }

    if let Some(previous_start) = current_start {
        frames.push(frame_descriptor(
            frames.len(),
            previous_start,
            contents.len() as u64,
            current_atoms,
            current_time,
            &current_label,
        ));
    }

    frames
}

fn frame_descriptor(
    frame_index: usize,
    byte_start: u64,
    byte_end: u64,
    atom_count: Option<u32>,
    time_ps: Option<f64>,
    label: &str,
) -> TrajectoryFrameDescriptor {
    TrajectoryFrameDescriptor {
        frame_index,
        byte_start,
        byte_end,
        atom_count,
        time_ps,
        label: if label.trim().is_empty() {
            format!("Frame {frame_index}")
        } else {
            label.trim().to_string()
        },
    }
}

fn parse_time_ps(comment: &str) -> Option<f64> {
    for token in comment.split_whitespace() {
        let lower = token.to_ascii_lowercase();
        if let Some(value) = lower.strip_prefix("time=") {
            return value.parse::<f64>().ok();
        }
        if let Some(value) = lower.strip_prefix("t=") {
            return value.parse::<f64>().ok();
        }
    }
    None
}

fn sample_frames(
    frames: &[TrajectoryFrameDescriptor],
    stride: usize,
    max_preview_frames: usize,
) -> Vec<TrajectoryFrameDescriptor> {
    frames
        .iter()
        .step_by(stride)
        .take(max_preview_frames)
        .cloned()
        .collect()
}

fn select_frames(
    frames: &[TrajectoryFrameDescriptor],
    request: &TrajectoryChunkRequest,
) -> Vec<TrajectoryFrameDescriptor> {
    if let Some(indices) = &request.frame_indices {
        return indices
            .iter()
            .filter_map(|index| frames.iter().find(|frame| frame.frame_index == *index))
            .cloned()
            .collect();
    }

    let start = request.start_frame.unwrap_or(0);
    let count = request.frame_count.unwrap_or(DEFAULT_CHUNK_FRAMES);
    frames
        .iter()
        .filter(|frame| frame.frame_index >= start)
        .take(count)
        .cloned()
        .collect()
}

fn index_manifest_path(trajectory_relative: &Path) -> PathBuf {
    let mut sanitized = relative_path_string(trajectory_relative)
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>();
    if sanitized.is_empty() {
        sanitized = "trajectory".to_string();
    }
    PathBuf::from("trajectories")
        .join(".automd-index")
        .join(format!("{sanitized}.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use uuid::Uuid;

    fn temp_project() -> PathBuf {
        let path = std::env::temp_dir().join(format!("automd-traj-test-{}", Uuid::new_v4()));
        fs::create_dir_all(path.join("trajectories")).expect("temp project");
        path
    }

    fn write_file(root: &Path, relative: &str, contents: &[u8]) {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent");
        }
        let mut file = fs::File::create(path).expect("file");
        file.write_all(contents).expect("write");
    }

    #[test]
    fn indexes_and_chunks_multimodel_pdb() {
        let root = temp_project();
        write_file(
            &root,
            "trajectories/movie.pdb",
            b"MODEL        1\nATOM      1  N   ALA A   1       0.0   0.0   0.0\nENDMDL\nMODEL        2\nATOM      1  N   ALA A   1       1.0   0.0   0.0\nENDMDL\n",
        );

        let index = index_trajectory(TrajectoryIndexRequest {
            project_path: root.display().to_string(),
            trajectory_path: "trajectories/movie.pdb".to_string(),
            frame_stride: Some(1),
            max_preview_frames: Some(10),
            write_index: true,
        })
        .expect("index");

        assert_eq!(index.format, TrajectoryFormat::Pdb);
        assert_eq!(index.strategy, TrajectoryIndexStrategy::TextOffsets);
        assert_eq!(index.frame_count, Some(2));
        assert!(index
            .index_path
            .as_deref()
            .unwrap_or("")
            .starts_with("trajectories/.automd-index/"));

        let chunk = read_trajectory_chunk(TrajectoryChunkRequest {
            project_path: root.display().to_string(),
            trajectory_path: "trajectories/movie.pdb".to_string(),
            frame_indices: Some(vec![1]),
            start_frame: None,
            frame_count: None,
            max_bytes: None,
        })
        .expect("chunk");
        assert_eq!(chunk.frames.len(), 1);
        assert!(chunk.frames[0].contents.contains("MODEL        2"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn indexes_xyz_frames_with_stride() {
        let root = temp_project();
        write_file(
            &root,
            "trajectories/movie.xyz",
            b"2\ntime=0\nH 0 0 0\nO 0 0 1\n2\ntime=2.5\nH 1 0 0\nO 1 0 1\n",
        );

        let index = index_trajectory(TrajectoryIndexRequest {
            project_path: root.display().to_string(),
            trajectory_path: "trajectories/movie.xyz".to_string(),
            frame_stride: Some(2),
            max_preview_frames: Some(4),
            write_index: false,
        })
        .expect("index");

        assert_eq!(index.format, TrajectoryFormat::Xyz);
        assert_eq!(index.frame_count, Some(2));
        assert_eq!(index.sampled_frames.len(), 1);
        assert_eq!(index.sampled_frames[0].time_ps, Some(0.0));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn binary_trajectory_is_metadata_only() {
        let root = temp_project();
        write_file(&root, "trajectories/movie.xtc", b"not-really-xtc");

        let index = index_trajectory(TrajectoryIndexRequest {
            project_path: root.display().to_string(),
            trajectory_path: "trajectories/movie.xtc".to_string(),
            frame_stride: None,
            max_preview_frames: None,
            write_index: false,
        })
        .expect("index");

        assert_eq!(index.format, TrajectoryFormat::Xtc);
        assert_eq!(index.strategy, TrajectoryIndexStrategy::MetadataOnly);
        assert_eq!(index.frame_count, None);
        assert!(index
            .warnings
            .iter()
            .any(|warning| warning.contains("metadata-only")));

        let _ = fs::remove_dir_all(root);
    }
}
