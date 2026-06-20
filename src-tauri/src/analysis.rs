use crate::models::*;
use chrono::Utc;
use std::fs;
use std::path::{Component, Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnalysisError {
    #[error("project path does not exist: {0}")]
    MissingProjectPath(String),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
}

pub fn parse_analysis_results(
    request: AnalysisParseRequest,
) -> Result<AnalysisParseResult, AnalysisError> {
    let project_root = PathBuf::from(&request.project_path);
    if !project_root.exists() {
        return Err(AnalysisError::MissingProjectPath(request.project_path));
    }

    let max_points = request.max_points.unwrap_or(1_000).clamp(50, 10_000);
    let paths = match request.artifact_paths {
        Some(paths) => paths,
        None => discover_analysis_paths(&project_root)?,
    };

    let mut series = Vec::new();
    let mut warnings = Vec::new();
    for relative in paths {
        let lower = relative.to_ascii_lowercase();
        if !lower.starts_with("analysis/") || !(lower.ends_with(".xvg") || lower.ends_with(".csv"))
        {
            continue;
        }
        let path = safe_join(&project_root, &relative);
        if !path.exists() {
            warnings.push(format!("Analysis artifact not found: {relative}"));
            continue;
        }
        let contents = fs::read_to_string(&path)?;
        let parsed = if lower.ends_with(".xvg") {
            parse_xvg(&relative, &contents, max_points)
        } else {
            parse_csv(&relative, &contents, max_points)
        };
        if parsed.is_empty() {
            warnings.push(format!("No numeric analysis series parsed from {relative}"));
        } else {
            series.extend(parsed);
        }
    }

    series.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.label.cmp(&right.label))
    });
    Ok(AnalysisParseResult {
        project_path: project_root.display().to_string(),
        series,
        warnings,
        generated_at: Utc::now(),
    })
}

fn discover_analysis_paths(project_root: &Path) -> Result<Vec<String>, AnalysisError> {
    let analysis_root = project_root.join("analysis");
    let mut paths = Vec::new();
    if analysis_root.exists() {
        visit_analysis_files(project_root, &analysis_root, &mut paths)?;
    }
    Ok(paths)
}

fn visit_analysis_files(
    project_root: &Path,
    current: &Path,
    paths: &mut Vec<String>,
) -> Result<(), AnalysisError> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            visit_analysis_files(project_root, &path, paths)?;
            continue;
        }
        let lower = path.to_string_lossy().to_ascii_lowercase();
        if lower.ends_with(".xvg") || lower.ends_with(".csv") {
            paths.push(relative_path(project_root, &path));
        }
    }
    Ok(())
}

fn parse_xvg(relative: &str, contents: &str, max_points: usize) -> Vec<AnalysisSeries> {
    let mut label = label_from_path(relative);
    let mut x_label = "x".to_string();
    let mut y_label = "y".to_string();
    let mut points = Vec::new();

    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('@') {
            if let Some(value) = quoted_directive(trimmed, "title") {
                label = value;
            } else if trimmed.contains("xaxis") {
                if let Some(value) = quoted_directive(trimmed, "label") {
                    x_label = value;
                }
            } else if trimmed.contains("yaxis") {
                if let Some(value) = quoted_directive(trimmed, "label") {
                    y_label = value;
                }
            }
            continue;
        }

        let values = trimmed
            .split_whitespace()
            .filter_map(|token| token.parse::<f64>().ok())
            .collect::<Vec<_>>();
        if values.len() >= 2 {
            points.push(AnalysisPoint {
                x: values[0],
                y: values[1],
            });
        }
    }

    let points = downsample(points, max_points);
    if points.is_empty() {
        Vec::new()
    } else {
        vec![series(relative, label, x_label, y_label, points)]
    }
}

fn parse_csv(relative: &str, contents: &str, max_points: usize) -> Vec<AnalysisSeries> {
    let mut lines = contents.lines().filter(|line| !line.trim().is_empty());
    let header = match lines.next() {
        Some(line) => parse_csv_line(line)
            .into_iter()
            .map(clean_header)
            .collect::<Vec<_>>(),
        None => return Vec::new(),
    };
    if header.len() < 2 {
        return Vec::new();
    }

    let mut columns = vec![Vec::<AnalysisPoint>::new(); header.len().saturating_sub(1)];
    for line in lines {
        let cells = parse_csv_line(line);
        if cells.len() < 2 {
            continue;
        }
        let x = match cells[0].trim().parse::<f64>() {
            Ok(value) => value,
            Err(_) => continue,
        };
        for (index, cell) in cells.iter().enumerate().skip(1) {
            if let Ok(y) = cell.trim().parse::<f64>() {
                if let Some(column) = columns.get_mut(index - 1) {
                    column.push(AnalysisPoint { x, y });
                }
            }
        }
    }

    columns
        .into_iter()
        .enumerate()
        .filter_map(|(index, points)| {
            let points = downsample(points, max_points);
            if points.is_empty() {
                None
            } else {
                Some(series(
                    relative,
                    format!(
                        "{}: {}",
                        label_from_path(relative),
                        header
                            .get(index + 1)
                            .cloned()
                            .unwrap_or_else(|| "value".to_string())
                    ),
                    header[0].clone(),
                    header
                        .get(index + 1)
                        .cloned()
                        .unwrap_or_else(|| "value".to_string()),
                    points,
                ))
            }
        })
        .collect()
}

fn series(
    relative: &str,
    label: String,
    x_label: String,
    y_label: String,
    points: Vec<AnalysisPoint>,
) -> AnalysisSeries {
    let min_y = points.iter().map(|point| point.y).reduce(f64::min);
    let max_y = points.iter().map(|point| point.y).reduce(f64::max);
    let last_y = points.last().map(|point| point.y);
    AnalysisSeries {
        path: relative.to_string(),
        label,
        x_label,
        y_label,
        points,
        min_y,
        max_y,
        last_y,
    }
}

fn downsample(points: Vec<AnalysisPoint>, max_points: usize) -> Vec<AnalysisPoint> {
    if points.len() <= max_points {
        return points;
    }
    let stride = (points.len() as f64 / max_points as f64).ceil() as usize;
    points.into_iter().step_by(stride.max(1)).collect()
}

fn quoted_directive(line: &str, key: &str) -> Option<String> {
    if !line.contains(key) {
        return None;
    }
    let start = line.find('"')? + 1;
    let end = line[start..].find('"')? + start;
    Some(line[start..end].to_string())
}

fn parse_csv_line(line: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in line.trim_start_matches('#').chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                cells.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    cells.push(current.trim().to_string());
    cells
}

fn clean_header(value: String) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string()
}

fn label_from_path(relative: &str) -> String {
    Path::new(relative)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(relative)
        .replace('_', " ")
}

fn safe_join(root: &Path, relative: &str) -> PathBuf {
    let mut destination = root.to_path_buf();
    for component in Path::new(relative).components() {
        if let Component::Normal(value) = component {
            destination.push(value);
        }
    }
    destination
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
    fn parses_xvg_series_with_labels() {
        let root = std::env::temp_dir().join(format!("automd-analysis-xvg-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("analysis")).expect("analysis dir");
        fs::write(
            root.join("analysis/rmsd.xvg"),
            "@ title \"Mock RMSD\"\n@ xaxis label \"Time (ns)\"\n@ yaxis label \"RMSD (nm)\"\n0 0.1\n1 0.2\n",
        )
        .expect("xvg");

        let result = parse_analysis_results(AnalysisParseRequest {
            project_path: root.display().to_string(),
            artifact_paths: None,
            max_points: None,
        })
        .expect("analysis");

        assert_eq!(result.series.len(), 1);
        assert_eq!(result.series[0].label, "Mock RMSD");
        assert_eq!(result.series[0].points.len(), 2);
        assert_eq!(result.series[0].last_y, Some(0.2));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn parses_csv_into_multiple_series() {
        let root = std::env::temp_dir().join(format!("automd-analysis-csv-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("analysis")).expect("analysis dir");
        fs::write(
            root.join("analysis/openmm_state.csv"),
            "#\"Step\",\"Potential Energy (kJ/mole)\",\"Temperature (K)\"\n0,-10.0,300.0\n10,-9.5,301.0\n",
        )
        .expect("csv");

        let result = parse_analysis_results(AnalysisParseRequest {
            project_path: root.display().to_string(),
            artifact_paths: Some(vec!["analysis/openmm_state.csv".to_string()]),
            max_points: None,
        })
        .expect("analysis");

        assert_eq!(result.series.len(), 2);
        assert!(result
            .series
            .iter()
            .any(|series| series.y_label == "Temperature (K)"));
        assert!(result.series.iter().all(|series| series.points.len() == 2));

        fs::remove_dir_all(root).expect("cleanup");
    }
}
