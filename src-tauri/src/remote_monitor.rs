use crate::engine_adapters;
use crate::models::*;
use chrono::Utc;

pub fn parse_remote_status(request: RemoteStatusParseRequest) -> RemoteJobSnapshot {
    let mut warnings = Vec::new();
    let job_id = request
        .submit_output
        .as_deref()
        .and_then(|output| parse_job_id(&request.scheduler, output));
    if request.submit_output.as_deref().is_some_and(|value| !value.trim().is_empty()) && job_id.is_none() {
        warnings.push("Could not extract a remote job id from submit output.".to_string());
    }

    let status_parse = request
        .status_output
        .as_deref()
        .and_then(|output| parse_queue_status(&request.scheduler, output, job_id.as_deref()));
    if request.status_output.as_deref().is_some_and(|value| !value.trim().is_empty()) && status_parse.is_none() {
        warnings.push("Could not classify scheduler status output.".to_string());
    }

    let log_report = request.log_output.as_ref().and_then(|log_output| {
        engine_adapters::parse_engine_log(EngineLogParseRequest {
            engine_id: request.engine_id.clone(),
            log_contents: log_output.clone(),
        })
        .map_err(|error| warnings.push(error.to_string()))
        .ok()
    });

    let mut status = status_parse
        .as_ref()
        .map(|parsed| parsed.status.clone())
        .unwrap_or(TaskStatus::Queued);
    if let Some(report) = &log_report {
        if report.fatal_error.is_some() {
            status = TaskStatus::Failed;
        } else if status == TaskStatus::Queued
            && (report.progress_percent.is_some() || report.current_step.is_some() || report.ns_per_day.is_some())
        {
            status = TaskStatus::Running;
        }
    }
    if let Some(log_output) = request.log_output.as_deref() {
        let lower = log_output.to_ascii_lowercase();
        if lower.contains("automd remote") && lower.contains("finished at") && !matches!(status, TaskStatus::Failed | TaskStatus::Cancelled) {
            status = TaskStatus::Completed;
        }
    }

    RemoteJobSnapshot {
        scheduler: request.scheduler,
        job_id,
        status,
        queue_state: status_parse.as_ref().and_then(|parsed| parsed.queue_state.clone()),
        reason: status_parse.and_then(|parsed| parsed.reason),
        progress_percent: log_report.as_ref().and_then(|report| report.progress_percent),
        ns_per_day: log_report.as_ref().and_then(|report| report.ns_per_day),
        current_step: log_report.as_ref().and_then(|report| report.current_step),
        log_report,
        warnings,
        generated_at: Utc::now(),
    }
}

#[derive(Debug, Clone)]
struct ParsedQueueStatus {
    status: TaskStatus,
    queue_state: Option<String>,
    reason: Option<String>,
}

fn parse_job_id(scheduler: &ExecutionMode, output: &str) -> Option<String> {
    match scheduler {
        ExecutionMode::Slurm => parse_slurm_job_id(output),
        ExecutionMode::Pbs => parse_pbs_job_id(output),
        ExecutionMode::Lsf => parse_lsf_job_id(output),
        ExecutionMode::Ssh => first_integer_token(output),
        _ => first_integer_token(output),
    }
}

fn parse_slurm_job_id(output: &str) -> Option<String> {
    for token in split_words(output) {
        let head = token.split(';').next().unwrap_or(token);
        if head.chars().all(|ch| ch.is_ascii_digit()) {
            return Some(head.to_string());
        }
    }
    None
}

fn parse_pbs_job_id(output: &str) -> Option<String> {
    split_words(output)
        .into_iter()
        .find(|token| token.chars().next().is_some_and(|ch| ch.is_ascii_digit()))
        .map(|token| token.trim_end_matches('.').to_string())
}

fn parse_lsf_job_id(output: &str) -> Option<String> {
    if let Some(start) = output.find('<') {
        let after = &output[start + 1..];
        if let Some(end) = after.find('>') {
            let candidate = &after[..end];
            if candidate.chars().all(|ch| ch.is_ascii_digit()) {
                return Some(candidate.to_string());
            }
        }
    }
    first_integer_token(output)
}

fn first_integer_token(output: &str) -> Option<String> {
    split_words(output)
        .into_iter()
        .find(|token| token.chars().all(|ch| ch.is_ascii_digit()))
        .map(ToString::to_string)
}

fn parse_queue_status(scheduler: &ExecutionMode, output: &str, job_id: Option<&str>) -> Option<ParsedQueueStatus> {
    match scheduler {
        ExecutionMode::Slurm => parse_slurm_status(output, job_id),
        ExecutionMode::Pbs => parse_pbs_status(output, job_id),
        ExecutionMode::Lsf => parse_lsf_status(output, job_id),
        ExecutionMode::Ssh => parse_ssh_status(output),
        _ => parse_word_status(output),
    }
}

fn parse_slurm_status(output: &str, job_id: Option<&str>) -> Option<ParsedQueueStatus> {
    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        if line.to_ascii_uppercase().contains("JOBID") {
            continue;
        }
        if job_id.is_some_and(|id| !line.contains(id)) {
            continue;
        }
        if let Some(state) = split_words(line)
            .into_iter()
            .find(|token| matches!(token.to_ascii_uppercase().as_str(), "PD" | "PENDING" | "R" | "RUNNING" | "CG" | "COMPLETING" | "CD" | "COMPLETED" | "F" | "FAILED" | "CA" | "CANCELLED" | "TO" | "TIMEOUT" | "NF" | "NODE_FAIL"))
        {
            return Some(queue_state(state));
        }
    }
    parse_word_status(output)
}

fn parse_pbs_status(output: &str, job_id: Option<&str>) -> Option<ParsedQueueStatus> {
    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        if line.to_ascii_lowercase().contains("job id") || line.starts_with("---") {
            continue;
        }
        if job_id.is_some_and(|id| !line.contains(id)) {
            continue;
        }
        if let Some(state) = split_words(line)
            .into_iter()
            .rev()
            .find(|token| matches!(token.to_ascii_uppercase().as_str(), "Q" | "R" | "C" | "E" | "H" | "W" | "S"))
        {
            return Some(queue_state(state));
        }
    }
    parse_word_status(output)
}

fn parse_lsf_status(output: &str, job_id: Option<&str>) -> Option<ParsedQueueStatus> {
    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        if line.to_ascii_uppercase().contains("JOBID") {
            continue;
        }
        if job_id.is_some_and(|id| !line.contains(id)) {
            continue;
        }
        if let Some(state) = split_words(line)
            .into_iter()
            .find(|token| matches!(token.to_ascii_uppercase().as_str(), "PEND" | "RUN" | "DONE" | "EXIT" | "PSUSP" | "USUSP" | "SSUSP"))
        {
            return Some(queue_state(state));
        }
    }
    parse_word_status(output)
}

fn parse_ssh_status(output: &str) -> Option<ParsedQueueStatus> {
    let lower = output.to_ascii_lowercase();
    if lower.contains("no such process") || lower.contains("not found") || lower.contains("not-running") {
        Some(ParsedQueueStatus {
            status: TaskStatus::Completed,
            queue_state: Some("process-missing".to_string()),
            reason: Some("Process is not present in ps output; inspect logs to distinguish completed vs failed.".to_string()),
        })
    } else if output.lines().any(|line| line.split_whitespace().next().is_some_and(|token| token.chars().all(|ch| ch.is_ascii_digit()))) {
        Some(ParsedQueueStatus {
            status: TaskStatus::Running,
            queue_state: Some("process-running".to_string()),
            reason: None,
        })
    } else {
        parse_word_status(output)
    }
}

fn parse_word_status(output: &str) -> Option<ParsedQueueStatus> {
    let upper = output.to_ascii_uppercase();
    for state in [
        "CANCELLED",
        "COMPLETED",
        "FAILED",
        "TIMEOUT",
        "NODE_FAIL",
        "RUNNING",
        "PENDING",
        "QUEUED",
        "DONE",
        "EXIT",
        "RUN",
        "PEND",
    ] {
        if upper.contains(state) {
            return Some(queue_state(state));
        }
    }
    None
}

fn queue_state(state: &str) -> ParsedQueueStatus {
    let normalized = state.trim().trim_matches(',').to_ascii_uppercase();
    let status = match normalized.as_str() {
        "PD" | "PENDING" | "Q" | "QUEUED" | "PEND" | "H" | "W" | "S" | "PSUSP" | "USUSP" | "SSUSP" => {
            TaskStatus::Queued
        }
        "R" | "RUNNING" | "RUN" | "CG" | "COMPLETING" | "E" => TaskStatus::Running,
        "CD" | "COMPLETED" | "DONE" | "C" => TaskStatus::Completed,
        "CA" | "CANCELLED" => TaskStatus::Cancelled,
        "F" | "FAILED" | "TO" | "TIMEOUT" | "NF" | "NODE_FAIL" | "EXIT" => TaskStatus::Failed,
        _ => TaskStatus::Queued,
    };
    ParsedQueueStatus {
        status,
        queue_state: Some(normalized),
        reason: None,
    }
}

fn split_words(output: &str) -> Vec<&str> {
    output
        .split(|ch: char| ch.is_whitespace() || matches!(ch, '<' | '>' | '"' | '\''))
        .map(|token| token.trim_matches(|ch: char| matches!(ch, ',' | ';' | ':')))
        .filter(|token| !token.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_slurm_submit_status_and_gromacs_log() {
        let snapshot = parse_remote_status(RemoteStatusParseRequest {
            engine_id: "gromacs".to_string(),
            scheduler: ExecutionMode::Slurm,
            submit_output: Some("123456;cluster".to_string()),
            status_output: Some("JOBID PARTITION NAME USER ST TIME NODES NODELIST\n123456 gpu md noir R 00:02 1 node01".to_string()),
            log_output: Some("step 5000 of 10000\nPerformance: 75.5 ns/day".to_string()),
        });

        assert_eq!(snapshot.job_id.as_deref(), Some("123456"));
        assert_eq!(snapshot.status, TaskStatus::Running);
        assert_eq!(snapshot.current_step, Some(5000));
        assert_eq!(snapshot.ns_per_day, Some(75.5));
    }

    #[test]
    fn parses_pbs_completed_state() {
        let snapshot = parse_remote_status(RemoteStatusParseRequest {
            engine_id: "openmm".to_string(),
            scheduler: ExecutionMode::Pbs,
            submit_output: Some("98765.server".to_string()),
            status_output: Some("Job id Name User Time Use S Queue\n98765.server md noir 00:10:00 C workq".to_string()),
            log_output: None,
        });

        assert_eq!(snapshot.job_id.as_deref(), Some("98765.server"));
        assert_eq!(snapshot.status, TaskStatus::Completed);
        assert_eq!(snapshot.queue_state.as_deref(), Some("C"));
    }

    #[test]
    fn parses_lsf_failed_state() {
        let snapshot = parse_remote_status(RemoteStatusParseRequest {
            engine_id: "namd".to_string(),
            scheduler: ExecutionMode::Lsf,
            submit_output: Some("Job <2468> is submitted to queue <gpu>.".to_string()),
            status_output: Some("JOBID USER STAT QUEUE FROM_HOST EXEC_HOST JOB_NAME SUBMIT_TIME\n2468 noir EXIT gpu login node md Jun 03".to_string()),
            log_output: None,
        });

        assert_eq!(snapshot.job_id.as_deref(), Some("2468"));
        assert_eq!(snapshot.status, TaskStatus::Failed);
        assert_eq!(snapshot.queue_state.as_deref(), Some("EXIT"));
    }

    #[test]
    fn parses_ssh_not_running_sentinel_as_terminal() {
        let snapshot = parse_remote_status(RemoteStatusParseRequest {
            engine_id: "gromacs".to_string(),
            scheduler: ExecutionMode::Ssh,
            submit_output: Some("2441".to_string()),
            status_output: Some("not-running".to_string()),
            log_output: None,
        });

        assert_eq!(snapshot.job_id.as_deref(), Some("2441"));
        assert_eq!(snapshot.status, TaskStatus::Completed);
        assert_eq!(snapshot.queue_state.as_deref(), Some("process-missing"));
    }
}
