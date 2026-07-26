use crate::models::*;
use chrono::Utc;
use std::fs;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};
use thiserror::Error;

/// Soft threshold: below this we still stream-index, but warn less.
const LARGE_TEXT_INDEX_BYTES: u64 = 256 * 1024 * 1024;
/// Hard cap on stored frame descriptors to keep index JSON/memory bounded.
const MAX_INDEXED_FRAMES: usize = 2_000_000;
const DEFAULT_MAX_PREVIEW_FRAMES: usize = 120;
const DEFAULT_CHUNK_FRAMES: usize = 5;
const DEFAULT_MAX_CHUNK_BYTES: u64 = 2 * 1024 * 1024;
const STREAM_BUFFER_BYTES: usize = 1024 * 1024;

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
            if size_bytes > LARGE_TEXT_INDEX_BYTES {
                warnings.push(format!(
                    "Large text trajectory ({} MB); building frame offsets with streaming indexer.",
                    size_bytes / 1024 / 1024
                ));
            }
            let frames = stream_index_text_trajectory(&trajectory_path, &format, &mut warnings)?;
            if frames.is_empty() {
                warnings.push(
                    "No frame boundaries were detected in the trajectory text.".to_string(),
                );
            }
            (TrajectoryIndexStrategy::TextOffsets, frames)
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
        frames: frames.clone(),
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

    // Prefer on-disk index frame offsets so we never re-read the whole trajectory for each chunk.
    let frames = load_or_build_frame_index(
        &project_root,
        &relative,
        &trajectory_path,
        &format,
        &mut warnings,
    )?;
    let selected = select_frames(&frames, &request);
    let max_bytes = request.max_bytes.unwrap_or(DEFAULT_MAX_CHUNK_BYTES);
    let mut used_bytes = 0u64;
    let mut truncated = false;
    let mut payloads = Vec::new();
    let mut file = fs::File::open(&trajectory_path)?;

    for descriptor in selected {
        let frame_size = descriptor.byte_end.saturating_sub(descriptor.byte_start);
        if used_bytes.saturating_add(frame_size) > max_bytes && !payloads.is_empty() {
            truncated = true;
            break;
        }
        match read_frame_bytes(&mut file, descriptor.byte_start, descriptor.byte_end) {
            Ok(contents) => {
                used_bytes = used_bytes.saturating_add(frame_size);
                payloads.push(TrajectoryFramePayload {
                    frame_index: descriptor.frame_index,
                    label: descriptor.label.clone(),
                    format: format.clone(),
                    contents,
                    atom_count: descriptor.atom_count,
                    time_ps: descriptor.time_ps,
                });
            }
            Err(error) => {
                warnings.push(format!(
                    "Frame {} seek/read failed: {error}",
                    descriptor.frame_index
                ));
            }
        }
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

fn read_frame_bytes(
    file: &mut fs::File,
    byte_start: u64,
    byte_end: u64,
) -> Result<String, TrajectoryError> {
    let len = byte_end.saturating_sub(byte_start) as usize;
    file.seek(SeekFrom::Start(byte_start))?;
    let mut buf = vec![0u8; len];
    file.read_exact(&mut buf)?;
    String::from_utf8(buf).map_err(|_| TrajectoryError::NonUtf8Trajectory)
}

/// Load full frame offsets from an existing index manifest, or build them once.
fn load_or_build_frame_index(
    project_root: &Path,
    relative: &Path,
    trajectory_path: &Path,
    format: &TrajectoryFormat,
    warnings: &mut Vec<String>,
) -> Result<Vec<TrajectoryFrameDescriptor>, TrajectoryError> {
    let index_path = project_root.join(index_manifest_path(relative));
    if index_path.is_file() {
        if let Ok(raw) = fs::read_to_string(&index_path) {
            if let Ok(index) = serde_json::from_str::<TrajectoryIndex>(&raw) {
                if !index.frames.is_empty()
                    && index.trajectory_path == relative_path_string(relative)
                    && index.strategy == TrajectoryIndexStrategy::TextOffsets
                {
                    return Ok(index.frames);
                }
                // Older indexes only stored sampled_frames; still usable if complete enough.
                if !index.sampled_frames.is_empty()
                    && index.frame_count == Some(index.sampled_frames.len())
                {
                    warnings.push(
                        "Using sampled frame offsets from legacy index; re-index for full seek table."
                            .to_string(),
                    );
                    return Ok(index.sampled_frames);
                }
            }
        }
    }

    // Stream-build offsets once (no full-file String). Cache to disk for later seeks.
    let size_bytes = fs::metadata(trajectory_path)?.len();
    let frames = stream_index_text_trajectory(trajectory_path, format, warnings)?;
    if !frames.is_empty() {
        let index = TrajectoryIndex {
            project_path: project_root.display().to_string(),
            trajectory_path: relative_path_string(relative),
            format: format.clone(),
            strategy: TrajectoryIndexStrategy::TextOffsets,
            size_bytes,
            frame_count: Some(frames.len()),
            frames: frames.clone(),
            sampled_frames: sample_frames(&frames, 1, DEFAULT_MAX_PREVIEW_FRAMES),
            index_path: Some(relative_path_string(&index_manifest_path(relative))),
            warnings: warnings.clone(),
            generated_at: Utc::now(),
        };
        let path = project_root.join(index_manifest_path(relative));
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(path, serde_json::to_string_pretty(&index).unwrap_or_default());
    }
    Ok(frames)
}

/// Line-oriented streaming indexer: O(file size) I/O, O(frame count) memory, never holds the whole file as a String.
fn stream_index_text_trajectory(
    path: &Path,
    format: &TrajectoryFormat,
    warnings: &mut Vec<String>,
) -> Result<Vec<TrajectoryFrameDescriptor>, TrajectoryError> {
    let file = fs::File::open(path)?;
    let mut reader = BufReader::with_capacity(STREAM_BUFFER_BYTES, file);
    match format {
        TrajectoryFormat::Pdb => stream_index_pdb(&mut reader, warnings),
        TrajectoryFormat::Xyz => stream_index_xyz(&mut reader, warnings),
        TrajectoryFormat::LammpsDump => stream_index_lammps(&mut reader, warnings),
        _ => Ok(Vec::new()),
    }
}

fn push_frame_capped(
    frames: &mut Vec<TrajectoryFrameDescriptor>,
    descriptor: TrajectoryFrameDescriptor,
    warnings: &mut Vec<String>,
) -> bool {
    if frames.len() >= MAX_INDEXED_FRAMES {
        if frames.len() == MAX_INDEXED_FRAMES {
            warnings.push(format!(
                "Stopped indexing after {MAX_INDEXED_FRAMES} frames to bound memory; re-export a strided trajectory for full coverage."
            ));
        }
        return false;
    }
    frames.push(descriptor);
    true
}

fn stream_index_pdb(
    reader: &mut BufReader<fs::File>,
    warnings: &mut Vec<String>,
) -> Result<Vec<TrajectoryFrameDescriptor>, TrajectoryError> {
    let mut frames = Vec::new();
    let mut offset = 0u64;
    let mut line_buf = Vec::with_capacity(256);
    let mut model_start: Option<u64> = None;
    let mut atom_count = 0u32;
    let mut label = String::new();
    let mut total_atoms = 0u32;
    let mut saw_model = false;
    let mut last_line_end = 0u64;

    loop {
        line_buf.clear();
        let bytes = reader.read_until(b'\n', &mut line_buf)?;
        if bytes == 0 {
            break;
        }
        let line_start = offset;
        let line_end = offset + bytes as u64;
        offset = line_end;
        last_line_end = line_end;
        let line = std::str::from_utf8(&line_buf)
            .map_err(|_| TrajectoryError::NonUtf8Trajectory)?
            .trim_end_matches(['\r', '\n']);

        if line.starts_with("MODEL") {
            if let Some(previous_start) = model_start {
                let index = frames.len();
                let desc = frame_descriptor(
                    index,
                    previous_start,
                    line_start,
                    Some(atom_count),
                    None,
                    &label,
                );
                if !push_frame_capped(&mut frames, desc, warnings) {
                    return Ok(frames);
                }
            }
            saw_model = true;
            model_start = Some(line_start);
            atom_count = 0;
            label = line.trim().to_string();
        } else if line.starts_with("ATOM") || line.starts_with("HETATM") {
            if model_start.is_some() {
                atom_count = atom_count.saturating_add(1);
            }
            total_atoms = total_atoms.saturating_add(1);
        } else if line.starts_with("ENDMDL") {
            if let Some(previous_start) = model_start.take() {
                let index = frames.len();
                let desc = frame_descriptor(
                    index,
                    previous_start,
                    line_end,
                    Some(atom_count),
                    None,
                    &label,
                );
                if !push_frame_capped(&mut frames, desc, warnings) {
                    return Ok(frames);
                }
                atom_count = 0;
                label.clear();
            }
        }
    }

    if let Some(previous_start) = model_start {
        let index = frames.len();
        let desc = frame_descriptor(
            index,
            previous_start,
            last_line_end,
            Some(atom_count),
            None,
            &label,
        );
        let _ = push_frame_capped(&mut frames, desc, warnings);
    } else if !saw_model && total_atoms > 0 {
        let desc =
            frame_descriptor(0, 0, last_line_end, Some(total_atoms), None, "single PDB model");
        let _ = push_frame_capped(&mut frames, desc, warnings);
    }
    Ok(frames)
}

fn stream_index_xyz(
    reader: &mut BufReader<fs::File>,
    warnings: &mut Vec<String>,
) -> Result<Vec<TrajectoryFrameDescriptor>, TrajectoryError> {
    let mut frames = Vec::new();
    let mut offset = 0u64;
    let mut line_buf = Vec::with_capacity(256);
    let mut line_no = 0usize;

    loop {
        line_buf.clear();
        let bytes = reader.read_until(b'\n', &mut line_buf)?;
        if bytes == 0 {
            break;
        }
        line_no += 1;
        let line_start = offset;
        offset += bytes as u64;
        let line = std::str::from_utf8(&line_buf)
            .map_err(|_| TrajectoryError::NonUtf8Trajectory)?
            .trim_end_matches(['\r', '\n']);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(atom_count) = trimmed.parse::<usize>() else {
            warnings.push(format!(
                "Skipped XYZ line {line_no} because atom count was not numeric."
            ));
            continue;
        };

        // Comment line
        line_buf.clear();
        let comment_bytes = reader.read_until(b'\n', &mut line_buf)?;
        if comment_bytes == 0 {
            warnings.push(format!("XYZ frame {} is incomplete.", frames.len()));
            break;
        }
        offset += comment_bytes as u64;
        let comment = std::str::from_utf8(&line_buf)
            .map_err(|_| TrajectoryError::NonUtf8Trajectory)?
            .trim_end_matches(['\r', '\n'])
            .trim()
            .to_string();
        line_no += 1;

        let mut end = offset;
        let mut incomplete = false;
        for _ in 0..atom_count {
            line_buf.clear();
            let atom_bytes = reader.read_until(b'\n', &mut line_buf)?;
            if atom_bytes == 0 {
                incomplete = true;
                break;
            }
            end = offset + atom_bytes as u64;
            offset = end;
            line_no += 1;
        }
        if incomplete {
            warnings.push(format!("XYZ frame {} is incomplete.", frames.len()));
            break;
        }

        let index = frames.len();
        let label = if comment.is_empty() {
            "XYZ frame"
        } else {
            comment.as_str()
        };
        let desc = frame_descriptor(
            index,
            line_start,
            end,
            Some(atom_count as u32),
            parse_time_ps(&comment),
            label,
        );
        if !push_frame_capped(&mut frames, desc, warnings) {
            break;
        }
    }
    Ok(frames)
}

fn stream_index_lammps(
    reader: &mut BufReader<fs::File>,
    warnings: &mut Vec<String>,
) -> Result<Vec<TrajectoryFrameDescriptor>, TrajectoryError> {
    let mut frames = Vec::new();
    let mut offset = 0u64;
    let mut line_buf = Vec::with_capacity(256);
    let mut current_start: Option<u64> = None;
    let mut current_atoms: Option<u32> = None;
    let mut current_label = "LAMMPS frame".to_string();
    let mut expect_timestep_value = false;
    let mut expect_atom_count = false;
    let mut last_end = 0u64;

    loop {
        line_buf.clear();
        let bytes = reader.read_until(b'\n', &mut line_buf)?;
        if bytes == 0 {
            break;
        }
        let line_start = offset;
        let line_end = offset + bytes as u64;
        offset = line_end;
        last_end = line_end;
        let line = std::str::from_utf8(&line_buf)
            .map_err(|_| TrajectoryError::NonUtf8Trajectory)?
            .trim_end_matches(['\r', '\n']);

        if expect_timestep_value {
            expect_timestep_value = false;
            let step = line.trim().parse::<f64>().ok();
            current_label = step
                .map(|s| format!("LAMMPS timestep {s}"))
                .unwrap_or_else(|| "LAMMPS frame".to_string());
            continue;
        }
        if expect_atom_count {
            expect_atom_count = false;
            current_atoms = line.trim().parse::<u32>().ok();
            continue;
        }

        if line.starts_with("ITEM: TIMESTEP") {
            if let Some(previous_start) = current_start {
                let index = frames.len();
                let desc = frame_descriptor(
                    index,
                    previous_start,
                    line_start,
                    current_atoms,
                    None,
                    &current_label,
                );
                if !push_frame_capped(&mut frames, desc, warnings) {
                    return Ok(frames);
                }
            }
            current_start = Some(line_start);
            current_atoms = None;
            current_label = "LAMMPS frame".to_string();
            expect_timestep_value = true;
        } else if line.starts_with("ITEM: NUMBER OF ATOMS") {
            expect_atom_count = true;
        }
    }

    if let Some(previous_start) = current_start {
        let index = frames.len();
        let desc = frame_descriptor(
            index,
            previous_start,
            last_end,
            current_atoms,
            None,
            &current_label,
        );
        let _ = push_frame_capped(&mut frames, desc, warnings);
    }
    Ok(frames)
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
            // ITEM: TIMESTEP value is a step index, not physical time. Store None in
            // time_ps so the UI does not treat step numbers as picoseconds.
            let step = lines
                .get(idx + 1)
                .and_then(|(_, _, value)| value.trim().parse::<f64>().ok());
            current_time = None;
            current_label = step
                .map(|s| format!("LAMMPS timestep {s}"))
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
        assert!(
            !index.frames.is_empty(),
            "index should retain full frame offset table for seek reads"
        );

        // Second chunk read must reuse the on-disk index (seek path).
        let chunk2 = read_trajectory_chunk(TrajectoryChunkRequest {
            project_path: root.display().to_string(),
            trajectory_path: "trajectories/movie.pdb".to_string(),
            frame_indices: Some(vec![0]),
            start_frame: None,
            frame_count: None,
            max_bytes: None,
        })
        .expect("chunk2");
        assert_eq!(chunk2.frames.len(), 1);
        assert!(chunk2.frames[0].contents.contains("MODEL        1"));

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

    #[test]
    fn streaming_index_matches_string_parser_for_pdb_and_lammps() {
        let root = temp_project();
        let pdb = b"MODEL        1\nATOM      1  N   ALA A   1       0.0   0.0   0.0\nENDMDL\nMODEL        2\nATOM      1  N   ALA A   1       1.0   0.0   0.0\nATOM      2  CA  ALA A   1       1.1   0.0   0.0\nENDMDL\n";
        write_file(&root, "trajectories/stream.pdb", pdb);
        let mut warnings = Vec::new();
        let streamed = stream_index_text_trajectory(
            &root.join("trajectories/stream.pdb"),
            &TrajectoryFormat::Pdb,
            &mut warnings,
        )
        .expect("stream pdb");
        let from_string = parse_pdb_frames(std::str::from_utf8(pdb).unwrap());
        assert_eq!(streamed.len(), from_string.len());
        assert_eq!(streamed[0].byte_start, from_string[0].byte_start);
        assert_eq!(streamed[0].byte_end, from_string[0].byte_end);
        assert_eq!(streamed[1].atom_count, Some(2));

        let dump = b"ITEM: TIMESTEP\n0\nITEM: NUMBER OF ATOMS\n2\nITEM: ATOMS id x y z\n1 0 0 0\n2 1 0 0\nITEM: TIMESTEP\n100\nITEM: NUMBER OF ATOMS\n2\nITEM: ATOMS id x y z\n1 0 1 0\n2 1 1 0\n";
        write_file(&root, "trajectories/stream.dump", dump);
        let mut warnings = Vec::new();
        let streamed = stream_index_text_trajectory(
            &root.join("trajectories/stream.dump"),
            &TrajectoryFormat::LammpsDump,
            &mut warnings,
        )
        .expect("stream dump");
        assert_eq!(streamed.len(), 2);
        assert!(streamed[0].label.contains("timestep 0"));
        assert!(streamed[1].label.contains("timestep 100"));
        assert_eq!(streamed[0].time_ps, None); // step is not physical time
        assert_eq!(streamed[0].atom_count, Some(2));

        let _ = fs::remove_dir_all(root);
    }
}
