use crate::models::*;
use chrono::{DateTime, Utc};
use std::fs;
use std::path::{Component, Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ArtifactError {
    #[error("project path does not exist: {0}")]
    MissingProjectPath(String),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
}

pub fn collect_artifacts(request: ArtifactIndexRequest) -> Result<ArtifactIndex, ArtifactError> {
    let project_root = PathBuf::from(&request.project_path);
    if !project_root.exists() {
        return Err(ArtifactError::MissingProjectPath(request.project_path));
    }

    let mut artifacts = Vec::new();
    let roots = [
        "inputs",
        "generated",
        "runs",
        "checkpoints",
        "trajectories",
        "analysis",
        "reports",
        "remote",
        "build-recipes",
    ];

    for root in roots {
        let path = project_root.join(root);
        if path.exists() {
            visit_files(&project_root, &path, &request.run_directory, &mut artifacts)?;
        }
    }

    artifacts.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(ArtifactIndex {
        project_path: project_root.display().to_string(),
        run_directory: request.run_directory,
        artifacts,
        generated_at: Utc::now(),
    })
}

pub fn export_report(request: ReportExportRequest) -> Result<ExportedReport, ArtifactError> {
    let project_root = PathBuf::from(&request.project_path);
    if !project_root.exists() {
        return Err(ArtifactError::MissingProjectPath(request.project_path));
    }

    let index = match request.artifact_index {
        Some(index) => index,
        None => collect_artifacts(ArtifactIndexRequest {
            project_path: request.project_path.clone(),
            run_directory: request.task.as_ref().map(|task| task.run_directory.clone()),
        })?,
    };

    let markdown = report_markdown(&request.plan, request.task.as_ref(), &index);
    let (format, extension, contents) = match request.format {
        ReportFormat::Markdown => (ReportFormat::Markdown, "md", markdown),
        ReportFormat::Html => (ReportFormat::Html, "html", report_html(&markdown)),
        ReportFormat::Pdf => (ReportFormat::Pdf, "pdf", report_pdf(&markdown)),
    };
    let report_dir = project_root.join("reports");
    fs::create_dir_all(&report_dir)?;
    let path = report_dir.join(format!("automd-report-{}.{}", request.plan.id.simple(), extension));
    fs::write(&path, &contents)?;

    Ok(ExportedReport {
        path: relative_path(&project_root, &path),
        format,
        contents,
    })
}

fn visit_files(
    project_root: &Path,
    current: &Path,
    run_directory: &Option<String>,
    artifacts: &mut Vec<RunArtifact>,
) -> Result<(), ArtifactError> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            visit_files(project_root, &path, run_directory, artifacts)?;
            continue;
        }

        let relative = relative_path(project_root, &path);
        if let Some(run_directory) = run_directory {
            let in_run = relative.starts_with(run_directory);
            let broadly_relevant = relative.starts_with("generated/")
                || relative.starts_with("analysis/")
                || relative.starts_with("reports/")
                || relative.starts_with("checkpoints/")
                || relative.starts_with("trajectories/")
                || relative.starts_with("remote/")
                || relative.starts_with("build-recipes/");
            if !in_run && !broadly_relevant {
                continue;
            }
        }

        let metadata = fs::metadata(&path)?;
        artifacts.push(RunArtifact {
            kind: classify_artifact(&relative),
            size_bytes: metadata.len(),
            modified_at: metadata.modified().ok().map(DateTime::<Utc>::from),
            summary: summarize_artifact(&path, &relative, metadata.len()),
            path: relative,
        });
    }
    Ok(())
}

fn classify_artifact(relative: &str) -> ArtifactKind {
    let lower = relative.to_ascii_lowercase();
    if lower.starts_with("inputs/") {
        ArtifactKind::Input
    } else if lower.starts_with("generated/") {
        ArtifactKind::GeneratedInput
    } else if lower.ends_with(".log") || lower.ends_with(".out") || lower.ends_with(".err") {
        ArtifactKind::RunLog
    } else if lower.ends_with(".cpt")
        || lower.ends_with(".checkpoint")
        || lower.ends_with(".restrt")
        || lower.ends_with(".rst")
    {
        ArtifactKind::Checkpoint
    } else if lower.ends_with(".xtc")
        || lower.ends_with(".trr")
        || lower.ends_with(".dcd")
        || lower.ends_with(".nc")
        || lower.ends_with(".gsd")
        || (lower.starts_with("trajectories/")
            && (lower.ends_with(".pdb")
                || lower.ends_with(".ent")
                || lower.ends_with(".xyz")
                || lower.ends_with(".lammpstrj")
                || lower.ends_with(".dump")))
    {
        ArtifactKind::Trajectory
    } else if lower.ends_with(".edr") || lower.contains("energy") {
        ArtifactKind::Energy
    } else if lower.starts_with("analysis/") && (lower.ends_with(".xvg") || lower.ends_with(".csv") || lower.ends_with(".jsonl")) {
        ArtifactKind::AnalysisTable
    } else if lower.ends_with(".png") || lower.ends_with(".svg") || lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        ArtifactKind::Figure
    } else if lower.starts_with("reports/") || lower.ends_with(".html") || lower.ends_with(".md") || lower.ends_with(".pdf") {
        ArtifactKind::Report
    } else if lower.starts_with("remote/")
        || lower.starts_with("build-recipes/")
        || lower.ends_with(".json")
        || lower.ends_with(".toml")
        || lower.ends_with(".yaml")
        || lower.ends_with(".yml")
    {
        ArtifactKind::Metadata
    } else {
        ArtifactKind::Other
    }
}

fn summarize_artifact(path: &Path, relative: &str, size: u64) -> Option<String> {
    let lower = relative.to_ascii_lowercase();
    if size > 2_000_000 {
        return Some("Large file indexed without inline parsing.".to_string());
    }

    if lower.ends_with(".xvg") {
        return summarize_xvg(path).ok();
    }
    if lower.ends_with(".jsonl") {
        return summarize_jsonl(path).ok();
    }
    if lower.ends_with(".log") {
        return summarize_log(path).ok();
    }
    None
}

fn summarize_xvg(path: &Path) -> Result<String, std::io::Error> {
    let contents = fs::read_to_string(path)?;
    let mut rows = 0usize;
    let mut last_data = None;
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('@') {
            continue;
        }
        rows += 1;
        last_data = Some(trimmed.to_string());
    }
    Ok(match last_data {
        Some(last) => format!("{rows} data rows; last={last}"),
        None => "No data rows detected.".to_string(),
    })
}

fn summarize_jsonl(path: &Path) -> Result<String, std::io::Error> {
    let contents = fs::read_to_string(path)?;
    let rows = contents.lines().filter(|line| !line.trim().is_empty()).count();
    Ok(format!("{rows} JSONL rows"))
}

fn summarize_log(path: &Path) -> Result<String, std::io::Error> {
    let contents = fs::read_to_string(path)?;
    let lines = contents.lines().count();
    let warnings = contents.matches("WARNING").count() + contents.matches("Warning").count();
    let fatal = contents.to_ascii_lowercase().contains("fatal error");
    Ok(format!("{lines} lines; warnings={warnings}; fatal={fatal}"))
}

fn report_markdown(plan: &SimulationPlan, task: Option<&LocalTaskSnapshot>, index: &ArtifactIndex) -> String {
    let task_section = match task {
        Some(task) => format!(
            r#"## Task

- Task id: `{}`
- Mode: `{:?}`
- Status: `{:?}`
- Run directory: `{}`
- Command: `{}`
- Progress: {:.1}%
- ns/day: {}
- Exit code: {}
- Error: {}
"#,
            task.id,
            task.mode,
            task.status,
            task.run_directory,
            task.command,
            task.progress_percent,
            task.ns_per_day
                .map(|value| format!("{value:.3}"))
                .unwrap_or_else(|| "n/a".to_string()),
            task.exit_code
                .map(|value| value.to_string())
                .unwrap_or_else(|| "n/a".to_string()),
            task.error_message.as_deref().unwrap_or("none")
        ),
        None => "## Task\n\nNo task snapshot attached.\n".to_string(),
    };

    let environment_snapshots = index
        .artifacts
        .iter()
        .filter(|artifact| artifact.path.ends_with("automd-run-manifest.json"))
        .map(|artifact| format!("- `{}` ({} bytes)\n", artifact.path, artifact.size_bytes))
        .collect::<String>();
    let environment_section = if environment_snapshots.is_empty() {
        "## Environment Snapshot\n\nNo run manifest indexed yet.\n".to_string()
    } else {
        format!("## Environment Snapshot\n\n{environment_snapshots}")
    };

    let artifacts = index
        .artifacts
        .iter()
        .map(|artifact| {
            format!(
                "- `{:?}` `{}` ({} bytes){}\n",
                artifact.kind,
                artifact.path,
                artifact.size_bytes,
                artifact
                    .summary
                    .as_ref()
                    .map(|summary| format!(" - {summary}"))
                    .unwrap_or_default()
            )
        })
        .collect::<String>();

    format!(
        r#"# AutoMD Simulation Report

Generated: {}

## Plan

- Plan id: `{}`
- Name: {}
- Engine: {}
- System: {}
- Force field: {}
- Water model: {}
- Solvent padding: {:.2} nm
- Ionic strength: {:.2} M

## Stages

{}

{}

{}

## Artifacts

{}
"#,
        Utc::now().to_rfc3339(),
        plan.id,
        plan.name,
        plan.engine_id,
        plan.system.name,
        plan.force_field.protein,
        plan.force_field.water_model,
        plan.solvent.padding_nm,
        plan.solvent.ionic_strength_molar,
        plan.stages
            .iter()
            .map(|stage| format!(
                "- [{}] {} (`{}`)\n",
                if stage.enabled { "x" } else { " " },
                stage.label,
                stage.id
            ))
            .collect::<String>(),
        task_section,
        environment_section,
        if artifacts.is_empty() {
            "No artifacts indexed.\n".to_string()
        } else {
            artifacts
        }
    )
}

fn report_html(markdown: &str) -> String {
    let mut body = String::new();
    for line in markdown.lines() {
        if let Some(title) = line.strip_prefix("# ") {
            body.push_str(&format!("<h1>{}</h1>\n", escape_html(title)));
        } else if let Some(title) = line.strip_prefix("## ") {
            body.push_str(&format!("<h2>{}</h2>\n", escape_html(title)));
        } else if let Some(item) = line.strip_prefix("- ") {
            body.push_str(&format!("<p class=\"item\">{}</p>\n", escape_html(item)));
        } else if line.trim().is_empty() {
            body.push('\n');
        } else {
            body.push_str(&format!("<p>{}</p>\n", escape_html(line)));
        }
    }

    format!(
        r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8" />
  <title>AutoMD Simulation Report</title>
  <style>
    body {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; margin: 32px; color: #1d2522; }}
    h1, h2 {{ color: #0f6f66; }}
    code {{ background: #f3f7f6; padding: 2px 5px; border-radius: 4px; }}
    .item {{ margin-left: 16px; }}
  </style>
</head>
<body>
{body}
</body>
</html>
"#
    )
}

fn report_pdf(markdown: &str) -> String {
    let lines = markdown
        .lines()
        .flat_map(wrap_pdf_line)
        .take(58)
        .collect::<Vec<_>>();
    let mut stream = String::from("BT\n/F1 10 Tf\n50 780 Td\n12 TL\n");
    for line in lines {
        stream.push('(');
        stream.push_str(&escape_pdf_text(&line));
        stream.push_str(") Tj\nT*\n");
    }
    stream.push_str("ET\n");

    let objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("<< /Length {} >>\nstream\n{}endstream", stream.len(), stream),
    ];

    let mut pdf = String::from("%PDF-1.4\n");
    let mut offsets = Vec::with_capacity(objects.len());
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.push_str(&format!("{} 0 obj\n{}\nendobj\n", index + 1, object));
    }
    let xref_offset = pdf.len();
    pdf.push_str(&format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1));
    for offset in offsets {
        pdf.push_str(&format!("{offset:010} 00000 n \n"));
    }
    pdf.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
        objects.len() + 1,
        xref_offset
    ));
    pdf
}

fn wrap_pdf_line(line: &str) -> Vec<String> {
    let ascii = line
        .chars()
        .map(|character| if character.is_ascii() { character } else { '?' })
        .collect::<String>();
    if ascii.len() <= 92 {
        return vec![ascii];
    }

    let mut wrapped = Vec::new();
    let mut remaining = ascii.as_str();
    while remaining.len() > 92 {
        let split_at = remaining
            .char_indices()
            .take_while(|(index, _)| *index <= 92)
            .last()
            .map(|(index, character)| index + character.len_utf8())
            .unwrap_or(92);
        wrapped.push(remaining[..split_at].to_string());
        remaining = remaining[split_at..].trim_start();
    }
    if !remaining.is_empty() {
        wrapped.push(remaining.to_string());
    }
    wrapped
}

fn escape_pdf_text(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
        .replace('\t', " ")
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
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
    use crate::planner;

    fn plan() -> SimulationPlan {
        planner::default_simulation_plan(PlanRequest {
            project_id: None,
            name: "artifact-test".to_string(),
            engine_id: "gromacs".to_string(),
            domain: ProjectDomain::Biomolecular,
        })
    }

    #[test]
    fn artifact_index_classifies_analysis_and_checkpoint_files() {
        let root = std::env::temp_dir().join(format!("automd-artifacts-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("analysis")).expect("analysis dir");
        fs::create_dir_all(root.join("runs/gromacs-test")).expect("run dir");
        fs::create_dir_all(root.join("remote")).expect("remote dir");
        fs::create_dir_all(root.join("build-recipes/gromacs")).expect("build recipe dir");
        fs::write(root.join("analysis/rmsd.xvg"), "@ title \"RMSD\"\n0 0.1\n1 0.2\n").expect("xvg");
        fs::write(root.join("runs/gromacs-test/md.cpt"), "checkpoint").expect("cpt");
        fs::write(root.join("remote/submit.slurm"), "#!/usr/bin/env bash\n#SBATCH --job-name=test\n").expect("slurm");
        fs::write(root.join("build-recipes/gromacs/automd-build-recipe.json"), "{}").expect("build manifest");

        let index = collect_artifacts(ArtifactIndexRequest {
            project_path: root.display().to_string(),
            run_directory: Some("runs/gromacs-test".to_string()),
        })
        .expect("artifact index");

        assert!(index.artifacts.iter().any(|artifact| artifact.kind == ArtifactKind::AnalysisTable));
        assert!(index.artifacts.iter().any(|artifact| artifact.kind == ArtifactKind::Checkpoint));
        assert!(index
            .artifacts
            .iter()
            .any(|artifact| artifact.summary.as_deref().unwrap_or_default().contains("2 data rows")));
        assert!(index
            .artifacts
            .iter()
            .any(|artifact| artifact.path == "remote/submit.slurm" && artifact.kind == ArtifactKind::Metadata));
        assert!(index
            .artifacts
            .iter()
            .any(|artifact| artifact.path == "build-recipes/gromacs/automd-build-recipe.json" && artifact.kind == ArtifactKind::Metadata));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn report_export_writes_markdown() {
        let root = std::env::temp_dir().join(format!("automd-report-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("analysis")).expect("analysis dir");
        fs::write(root.join("analysis/rg.xvg"), "0 1.0\n").expect("xvg");
        let plan = plan();
        let task = LocalTaskSnapshot {
            id: uuid::Uuid::new_v4(),
            plan_id: plan.id,
            engine_id: plan.engine_id.clone(),
            mode: LocalRunMode::DryRun,
            status: TaskStatus::Completed,
            run_directory: "runs/gromacs-test".to_string(),
            command: "dry-run package generation only".to_string(),
            progress_percent: 100.0,
            ns_per_day: None,
            current_step: None,
            log_tail: Vec::new(),
            error_message: None,
            exit_code: Some(0),
            artifacts: Vec::new(),
            report_path: None,
            failure_analysis: None,
            resume_plan: None,
            started_at: Utc::now(),
            finished_at: Some(Utc::now()),
        };
        let artifact_index = ArtifactIndex {
            project_path: root.display().to_string(),
            run_directory: Some(task.run_directory.clone()),
            artifacts: vec![RunArtifact {
                path: "runs/gromacs-test/automd-run-manifest.json".to_string(),
                kind: ArtifactKind::Metadata,
                size_bytes: 1024,
                modified_at: None,
                summary: Some("Run manifest".to_string()),
            }],
            generated_at: Utc::now(),
        };
        let exported = export_report(ReportExportRequest {
            project_path: root.display().to_string(),
            plan,
            task: Some(task),
            artifact_index: Some(artifact_index),
            format: ReportFormat::Markdown,
        })
        .expect("report");

        assert!(exported.path.starts_with("reports/"));
        assert!(root.join(&exported.path).exists());
        assert!(exported.contents.contains("AutoMD Simulation Report"));
        assert!(exported.contents.contains("- Command: `dry-run package generation only`"));
        assert!(exported.contents.contains("runs/gromacs-test/automd-run-manifest.json"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn report_export_writes_pdf() {
        let root = std::env::temp_dir().join(format!("automd-report-pdf-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("analysis")).expect("analysis dir");
        fs::write(root.join("analysis/rg.xvg"), "0 1.0\n").expect("xvg");
        let exported = export_report(ReportExportRequest {
            project_path: root.display().to_string(),
            plan: plan(),
            task: None,
            artifact_index: None,
            format: ReportFormat::Pdf,
        })
        .expect("pdf report");

        assert!(exported.path.ends_with(".pdf"));
        assert!(root.join(&exported.path).exists());
        assert!(exported.contents.starts_with("%PDF-1.4"));

        fs::remove_dir_all(root).expect("cleanup");
    }
}
