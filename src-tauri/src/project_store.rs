use crate::models::*;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid project name")]
    InvalidProjectName,
    #[error("invalid timestamp: {0}")]
    InvalidTimestamp(String),
    #[error("invalid enum value in database: {0}")]
    InvalidEnumValue(String),
    #[error("invalid remote profile")]
    InvalidRemoteProfile,
    #[error("invalid engine installation")]
    InvalidEngineInstallation,
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub struct ProjectDatabase {
    connection: Connection,
}

impl ProjectDatabase {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path)?;
        let db = Self { connection };
        db.migrate()?;
        Ok(db)
    }

    pub fn create_project(
        &self,
        request: CreateProjectRequest,
        default_root: &Path,
    ) -> Result<ProjectSummary, StoreError> {
        let trimmed_name = request.name.trim();
        if trimmed_name.is_empty() {
            return Err(StoreError::InvalidProjectName);
        }

        let id = Uuid::new_v4();
        let created_at = Utc::now();
        let safe_name = slugify(trimmed_name);
        let project_root = request
            .project_root
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| default_root.to_path_buf());
        let path = project_root.join(format!("{safe_name}-{id}"));

        for child in [
            "inputs",
            "generated",
            "runs",
            "checkpoints",
            "trajectories",
            "analysis",
            "reports",
            "remote",
            "build-recipes",
        ] {
            fs::create_dir_all(path.join(child))?;
        }

        let summary = ProjectSummary {
            id,
            name: trimmed_name.to_string(),
            domain: request.domain,
            path: path.display().to_string(),
            created_at,
            last_opened_at: None,
            preferred_engine_id: request.preferred_engine_id,
            status: ProjectStatus::Draft,
        };

        self.connection.execute(
            "INSERT INTO projects
                (id, name, domain, path, created_at, last_opened_at, preferred_engine_id, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                summary.id.to_string(),
                summary.name,
                domain_to_str(&summary.domain),
                summary.path,
                summary.created_at.to_rfc3339(),
                summary.last_opened_at.map(|value| value.to_rfc3339()),
                summary.preferred_engine_id,
                status_to_str(&summary.status),
            ],
        )?;

        Ok(summary)
    }

    pub fn list_projects(&self) -> Result<Vec<ProjectSummary>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, name, domain, path, created_at, last_opened_at, preferred_engine_id, status
             FROM projects
             ORDER BY datetime(created_at) DESC",
        )?;

        let rows = statement.query_map([], |row| {
            let id: String = row.get(0)?;
            let domain: String = row.get(2)?;
            let created_at: String = row.get(4)?;
            let last_opened_at: Option<String> = row.get(5)?;
            let status: String = row.get(7)?;

            Ok(ProjectSummary {
                id: Uuid::parse_str(&id).map_err(|err| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err)))?,
                name: row.get(1)?,
                domain: domain_from_str(&domain).map_err(to_sql_conversion_error)?,
                path: row.get(3)?,
                created_at: parse_datetime(&created_at).map_err(to_sql_conversion_error)?,
                last_opened_at: last_opened_at
                    .as_deref()
                    .map(parse_datetime)
                    .transpose()
                    .map_err(to_sql_conversion_error)?,
                preferred_engine_id: row.get(6)?,
                status: status_from_str(&status).map_err(to_sql_conversion_error)?,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::Database)
    }

    pub fn list_remote_profiles(&self) -> Result<Vec<RemoteProfile>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, name, host, scheduler, workdir, module_load_json, default_queue
             FROM remote_profiles
             ORDER BY name ASC",
        )?;

        let rows = statement.query_map([], |row| {
            let scheduler: String = row.get(3)?;
            let module_load_json: String = row.get(5)?;
            let module_load = serde_json::from_str::<Vec<String>>(&module_load_json)
                .map_err(|err| rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(err)))?;
            Ok(RemoteProfile {
                id: row.get(0)?,
                name: row.get(1)?,
                host: row.get(2)?,
                scheduler: execution_mode_from_str(&scheduler).map_err(to_sql_conversion_error)?,
                workdir: row.get(4)?,
                module_load,
                default_queue: row.get(6)?,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::Database)
    }

    pub fn save_remote_profile(&self, profile: RemoteProfile) -> Result<RemoteProfile, StoreError> {
        let profile = normalize_remote_profile(profile)?;
        let module_load_json = serde_json::to_string(&profile.module_load)?;
        self.connection.execute(
            "INSERT INTO remote_profiles
                (id, name, host, scheduler, workdir, module_load_json, default_queue)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                host = excluded.host,
                scheduler = excluded.scheduler,
                workdir = excluded.workdir,
                module_load_json = excluded.module_load_json,
                default_queue = excluded.default_queue",
            params![
                profile.id,
                profile.name,
                profile.host,
                execution_mode_to_str(&profile.scheduler),
                profile.workdir,
                module_load_json,
                profile.default_queue,
            ],
        )?;
        Ok(profile)
    }

    pub fn delete_remote_profile(&self, id: String) -> Result<bool, StoreError> {
        let deleted = self
            .connection
            .execute("DELETE FROM remote_profiles WHERE id = ?1", params![id])?;
        Ok(deleted > 0)
    }

    pub fn list_engine_installations(&self) -> Result<Vec<EngineInstallationRecord>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT engine_id, location, version, authorization_status, checked_at
             FROM engine_installations
             ORDER BY engine_id ASC, location ASC",
        )?;

        let rows = statement.query_map([], |row| {
            let authorization_status: String = row.get(3)?;
            let checked_at: String = row.get(4)?;
            Ok(EngineInstallationRecord {
                engine_id: row.get(0)?,
                location: row.get(1)?,
                version: row.get(2)?,
                authorization_status: detection_status_from_str(&authorization_status).map_err(to_sql_conversion_error)?,
                checked_at: parse_datetime(&checked_at).map_err(to_sql_conversion_error)?,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::Database)
    }

    pub fn save_engine_installation(
        &self,
        record: EngineInstallationRecord,
    ) -> Result<EngineInstallationRecord, StoreError> {
        let record = normalize_engine_installation(record)?;
        self.connection.execute(
            "INSERT INTO engine_installations
                (engine_id, location, version, authorization_status, checked_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(engine_id, location) DO UPDATE SET
                version = excluded.version,
                authorization_status = excluded.authorization_status,
                checked_at = excluded.checked_at",
            params![
                record.engine_id,
                record.location,
                record.version,
                detection_status_to_str(&record.authorization_status),
                record.checked_at.to_rfc3339(),
            ],
        )?;
        Ok(record)
    }

    pub fn delete_engine_installation(&self, engine_id: String, location: String) -> Result<bool, StoreError> {
        let deleted = self.connection.execute(
            "DELETE FROM engine_installations WHERE engine_id = ?1 AND location = ?2",
            params![engine_id, location],
        )?;
        Ok(deleted > 0)
    }

    pub fn upsert_task_snapshot(&self, snapshot: &LocalTaskSnapshot, project_id: Option<Uuid>) -> Result<TaskRecord, StoreError> {
        let updated_at = Utc::now();
        let record = TaskRecord {
            id: snapshot.id,
            project_id,
            plan_id: snapshot.plan_id,
            engine_id: snapshot.engine_id.clone(),
            status: snapshot.status.clone(),
            current_stage: None,
            progress_percent: snapshot.progress_percent,
            created_at: snapshot.started_at,
            updated_at,
        };

        self.connection.execute(
            "INSERT INTO tasks
                (id, project_id, plan_id, engine_id, status, current_stage, progress_percent, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
                project_id = COALESCE(excluded.project_id, tasks.project_id),
                plan_id = excluded.plan_id,
                engine_id = excluded.engine_id,
                status = excluded.status,
                current_stage = excluded.current_stage,
                progress_percent = excluded.progress_percent,
                updated_at = excluded.updated_at",
            params![
                record.id.to_string(),
                record.project_id.map(|value| value.to_string()),
                record.plan_id.to_string(),
                record.engine_id,
                task_status_to_str(&record.status),
                record.current_stage.as_ref().map(stage_to_str),
                record.progress_percent,
                record.created_at.to_rfc3339(),
                record.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(record)
    }

    pub fn list_task_records(&self, project_id: Option<Uuid>) -> Result<Vec<TaskRecord>, StoreError> {
        let sql = match project_id {
            Some(_) => {
                "SELECT id, project_id, plan_id, engine_id, status, current_stage, progress_percent, created_at, updated_at
                 FROM tasks
                 WHERE project_id = ?1
                 ORDER BY datetime(updated_at) DESC"
            }
            None => {
                "SELECT id, project_id, plan_id, engine_id, status, current_stage, progress_percent, created_at, updated_at
                 FROM tasks
                 ORDER BY datetime(updated_at) DESC"
            }
        };
        let mut statement = self.connection.prepare(sql)?;
        let map_row = |row: &rusqlite::Row<'_>| {
            let id: String = row.get(0)?;
            let project_id: Option<String> = row.get(1)?;
            let plan_id: String = row.get(2)?;
            let status: String = row.get(4)?;
            let current_stage: Option<String> = row.get(5)?;
            let created_at: String = row.get(7)?;
            let updated_at: String = row.get(8)?;
            Ok(TaskRecord {
                id: Uuid::parse_str(&id).map_err(|err| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err)))?,
                project_id: project_id
                    .as_deref()
                    .map(Uuid::parse_str)
                    .transpose()
                    .map_err(|err| rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(err)))?,
                plan_id: Uuid::parse_str(&plan_id).map_err(|err| rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(err)))?,
                engine_id: row.get(3)?,
                status: task_status_from_str(&status).map_err(to_sql_conversion_error)?,
                current_stage: current_stage
                    .as_deref()
                    .map(stage_from_str)
                    .transpose()
                    .map_err(to_sql_conversion_error)?,
                progress_percent: row.get(6)?,
                created_at: parse_datetime(&created_at).map_err(to_sql_conversion_error)?,
                updated_at: parse_datetime(&updated_at).map_err(to_sql_conversion_error)?,
            })
        };

        let rows = match project_id {
            Some(project_id) => statement.query_map(params![project_id.to_string()], map_row)?,
            None => statement.query_map([], map_row)?,
        };
        rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::Database)
    }

    pub fn upsert_artifact_index(&self, index: &ArtifactIndex) -> Result<Vec<ArtifactRecord>, StoreError> {
        self.connection.execute(
            "DELETE FROM artifact_records WHERE project_path = ?1",
            params![&index.project_path],
        )?;

        let mut records = Vec::with_capacity(index.artifacts.len());
        for artifact in &index.artifacts {
            let record = ArtifactRecord {
                project_path: index.project_path.clone(),
                path: artifact.path.clone(),
                kind: artifact.kind.clone(),
                size_bytes: artifact.size_bytes,
                modified_at: artifact.modified_at,
                summary: artifact.summary.clone(),
                run_directory: index.run_directory.clone(),
                indexed_at: index.generated_at,
            };
            self.connection.execute(
                "INSERT INTO artifact_records
                    (project_path, path, kind, size_bytes, modified_at, summary, run_directory, indexed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    &record.project_path,
                    &record.path,
                    artifact_kind_to_str(&record.kind),
                    record.size_bytes as i64,
                    record.modified_at.as_ref().map(DateTime::to_rfc3339),
                    record.summary.as_deref(),
                    record.run_directory.as_deref(),
                    record.indexed_at.to_rfc3339(),
                ],
            )?;
            records.push(record);
        }
        Ok(records)
    }

    pub fn list_artifact_records(&self, project_path: String) -> Result<Vec<ArtifactRecord>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT project_path, path, kind, size_bytes, modified_at, summary, run_directory, indexed_at
             FROM artifact_records
             WHERE project_path = ?1
             ORDER BY path ASC",
        )?;
        let rows = statement.query_map(params![project_path], |row| {
            let kind: String = row.get(2)?;
            let size_bytes: i64 = row.get(3)?;
            let modified_at: Option<String> = row.get(4)?;
            let indexed_at: String = row.get(7)?;
            Ok(ArtifactRecord {
                project_path: row.get(0)?,
                path: row.get(1)?,
                kind: artifact_kind_from_str(&kind).map_err(to_sql_conversion_error)?,
                size_bytes: size_bytes.max(0) as u64,
                modified_at: modified_at
                    .as_deref()
                    .map(parse_datetime)
                    .transpose()
                    .map_err(to_sql_conversion_error)?,
                summary: row.get(5)?,
                run_directory: row.get(6)?,
                indexed_at: parse_datetime(&indexed_at).map_err(to_sql_conversion_error)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::Database)
    }

    pub fn upsert_analysis_cache(&self, result: &AnalysisParseResult) -> Result<Vec<AnalysisCacheRecord>, StoreError> {
        self.connection.execute(
            "DELETE FROM analysis_cache WHERE project_path = ?1",
            params![&result.project_path],
        )?;

        let mut records = Vec::with_capacity(result.series.len());
        for series in &result.series {
            let record = AnalysisCacheRecord {
                project_path: result.project_path.clone(),
                path: series.path.clone(),
                label: series.label.clone(),
                x_label: series.x_label.clone(),
                y_label: series.y_label.clone(),
                point_count: series.points.len(),
                min_y: series.min_y,
                max_y: series.max_y,
                last_y: series.last_y,
                generated_at: result.generated_at,
            };
            let series_json = serde_json::to_string(series)?;
            self.connection.execute(
                "INSERT INTO analysis_cache
                    (project_path, path, label, x_label, y_label, point_count, min_y, max_y, last_y, generated_at, series_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    &record.project_path,
                    &record.path,
                    &record.label,
                    &record.x_label,
                    &record.y_label,
                    record.point_count as i64,
                    record.min_y,
                    record.max_y,
                    record.last_y,
                    record.generated_at.to_rfc3339(),
                    series_json,
                ],
            )?;
            records.push(record);
        }
        Ok(records)
    }

    pub fn list_analysis_cache_records(&self, project_path: String) -> Result<Vec<AnalysisCacheRecord>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT project_path, path, label, x_label, y_label, point_count, min_y, max_y, last_y, generated_at
             FROM analysis_cache
             WHERE project_path = ?1
             ORDER BY datetime(generated_at) DESC, path ASC, label ASC",
        )?;
        let rows = statement.query_map(params![project_path], |row| {
            let point_count: i64 = row.get(5)?;
            let generated_at: String = row.get(9)?;
            Ok(AnalysisCacheRecord {
                project_path: row.get(0)?,
                path: row.get(1)?,
                label: row.get(2)?,
                x_label: row.get(3)?,
                y_label: row.get(4)?,
                point_count: point_count.max(0) as usize,
                min_y: row.get(6)?,
                max_y: row.get(7)?,
                last_y: row.get(8)?,
                generated_at: parse_datetime(&generated_at).map_err(to_sql_conversion_error)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::Database)
    }

    fn migrate(&self) -> Result<(), StoreError> {
        self.connection.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            CREATE TABLE IF NOT EXISTS projects (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                domain TEXT NOT NULL,
                path TEXT NOT NULL,
                created_at TEXT NOT NULL,
                last_opened_at TEXT,
                preferred_engine_id TEXT,
                status TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS tasks (
                id TEXT PRIMARY KEY,
                project_id TEXT,
                plan_id TEXT NOT NULL,
                engine_id TEXT NOT NULL,
                status TEXT NOT NULL,
                current_stage TEXT,
                progress_percent REAL NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS engine_installations (
                engine_id TEXT NOT NULL,
                location TEXT NOT NULL,
                version TEXT,
                authorization_status TEXT NOT NULL,
                checked_at TEXT NOT NULL,
                PRIMARY KEY (engine_id, location)
            );
            CREATE TABLE IF NOT EXISTS remote_profiles (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                host TEXT NOT NULL,
                scheduler TEXT NOT NULL,
                workdir TEXT NOT NULL,
                module_load_json TEXT NOT NULL,
                default_queue TEXT
            );
            CREATE TABLE IF NOT EXISTS artifact_records (
                project_path TEXT NOT NULL,
                path TEXT NOT NULL,
                kind TEXT NOT NULL,
                size_bytes INTEGER NOT NULL,
                modified_at TEXT,
                summary TEXT,
                run_directory TEXT,
                indexed_at TEXT NOT NULL,
                PRIMARY KEY (project_path, path)
            );
            CREATE TABLE IF NOT EXISTS analysis_cache (
                project_path TEXT NOT NULL,
                path TEXT NOT NULL,
                label TEXT NOT NULL,
                x_label TEXT NOT NULL,
                y_label TEXT NOT NULL,
                point_count INTEGER NOT NULL,
                min_y REAL,
                max_y REAL,
                last_y REAL,
                generated_at TEXT NOT NULL,
                series_json TEXT NOT NULL,
                PRIMARY KEY (project_path, path, label)
            );
            ",
        )?;
        Ok(())
    }
}

fn normalize_engine_installation(mut record: EngineInstallationRecord) -> Result<EngineInstallationRecord, StoreError> {
    record.engine_id = record.engine_id.trim().to_string();
    record.location = record.location.trim().to_string();
    record.version = record
        .version
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    if record.engine_id.is_empty() || record.location.is_empty() {
        return Err(StoreError::InvalidEngineInstallation);
    }
    Ok(record)
}

fn normalize_remote_profile(mut profile: RemoteProfile) -> Result<RemoteProfile, StoreError> {
    profile.id = profile.id.trim().to_string();
    profile.name = profile.name.trim().to_string();
    profile.host = profile.host.trim().to_string();
    profile.workdir = profile.workdir.trim().to_string();
    profile.default_queue = profile
        .default_queue
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    profile.module_load = profile
        .module_load
        .into_iter()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();
    if profile.id.is_empty() || profile.name.is_empty() || profile.host.is_empty() || profile.workdir.is_empty() {
        return Err(StoreError::InvalidRemoteProfile);
    }
    Ok(profile)
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if ch.is_whitespace() || ch == '-' || ch == '_' {
            if !slug.ends_with('-') {
                slug.push('-');
            }
        }
    }
    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        "automd-project".to_string()
    } else {
        trimmed.to_string()
    }
}

fn parse_datetime(value: &str) -> Result<DateTime<Utc>, StoreError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| StoreError::InvalidTimestamp(value.to_string()))
}

fn domain_to_str(value: &ProjectDomain) -> &'static str {
    match value {
        ProjectDomain::Biomolecular => "biomolecular",
        ProjectDomain::Materials => "materials",
        ProjectDomain::Qmmm => "qmmm",
    }
}

fn domain_from_str(value: &str) -> Result<ProjectDomain, StoreError> {
    match value {
        "biomolecular" => Ok(ProjectDomain::Biomolecular),
        "materials" => Ok(ProjectDomain::Materials),
        "qmmm" => Ok(ProjectDomain::Qmmm),
        other => Err(StoreError::InvalidEnumValue(other.to_string())),
    }
}

fn status_to_str(value: &ProjectStatus) -> &'static str {
    match value {
        ProjectStatus::Draft => "draft",
        ProjectStatus::Ready => "ready",
        ProjectStatus::Running => "running",
        ProjectStatus::Completed => "completed",
        ProjectStatus::Failed => "failed",
    }
}

fn status_from_str(value: &str) -> Result<ProjectStatus, StoreError> {
    match value {
        "draft" => Ok(ProjectStatus::Draft),
        "ready" => Ok(ProjectStatus::Ready),
        "running" => Ok(ProjectStatus::Running),
        "completed" => Ok(ProjectStatus::Completed),
        "failed" => Ok(ProjectStatus::Failed),
        other => Err(StoreError::InvalidEnumValue(other.to_string())),
    }
}

fn task_status_to_str(value: &TaskStatus) -> &'static str {
    match value {
        TaskStatus::Queued => "queued",
        TaskStatus::Preparing => "preparing",
        TaskStatus::Running => "running",
        TaskStatus::Completed => "completed",
        TaskStatus::Failed => "failed",
        TaskStatus::Cancelled => "cancelled",
    }
}

fn task_status_from_str(value: &str) -> Result<TaskStatus, StoreError> {
    match value {
        "queued" => Ok(TaskStatus::Queued),
        "preparing" => Ok(TaskStatus::Preparing),
        "running" => Ok(TaskStatus::Running),
        "completed" => Ok(TaskStatus::Completed),
        "failed" => Ok(TaskStatus::Failed),
        "cancelled" => Ok(TaskStatus::Cancelled),
        other => Err(StoreError::InvalidEnumValue(other.to_string())),
    }
}

fn stage_to_str(value: &SimulationStageKind) -> &'static str {
    match value {
        SimulationStageKind::StructurePreparation => "structurePreparation",
        SimulationStageKind::EnergyMinimization => "energyMinimization",
        SimulationStageKind::NvtEquilibration => "nvtEquilibration",
        SimulationStageKind::NptEquilibration => "nptEquilibration",
        SimulationStageKind::Production => "production",
        SimulationStageKind::Analysis => "analysis",
    }
}

fn stage_from_str(value: &str) -> Result<SimulationStageKind, StoreError> {
    match value {
        "structurePreparation" => Ok(SimulationStageKind::StructurePreparation),
        "energyMinimization" => Ok(SimulationStageKind::EnergyMinimization),
        "nvtEquilibration" => Ok(SimulationStageKind::NvtEquilibration),
        "nptEquilibration" => Ok(SimulationStageKind::NptEquilibration),
        "production" => Ok(SimulationStageKind::Production),
        "analysis" => Ok(SimulationStageKind::Analysis),
        other => Err(StoreError::InvalidEnumValue(other.to_string())),
    }
}

fn artifact_kind_to_str(value: &ArtifactKind) -> &'static str {
    match value {
        ArtifactKind::Input => "input",
        ArtifactKind::GeneratedInput => "generatedInput",
        ArtifactKind::RunLog => "runLog",
        ArtifactKind::Checkpoint => "checkpoint",
        ArtifactKind::Trajectory => "trajectory",
        ArtifactKind::Energy => "energy",
        ArtifactKind::AnalysisTable => "analysisTable",
        ArtifactKind::Figure => "figure",
        ArtifactKind::Report => "report",
        ArtifactKind::Metadata => "metadata",
        ArtifactKind::Other => "other",
    }
}

fn artifact_kind_from_str(value: &str) -> Result<ArtifactKind, StoreError> {
    match value {
        "input" => Ok(ArtifactKind::Input),
        "generatedInput" => Ok(ArtifactKind::GeneratedInput),
        "runLog" => Ok(ArtifactKind::RunLog),
        "checkpoint" => Ok(ArtifactKind::Checkpoint),
        "trajectory" => Ok(ArtifactKind::Trajectory),
        "energy" => Ok(ArtifactKind::Energy),
        "analysisTable" => Ok(ArtifactKind::AnalysisTable),
        "figure" => Ok(ArtifactKind::Figure),
        "report" => Ok(ArtifactKind::Report),
        "metadata" => Ok(ArtifactKind::Metadata),
        "other" => Ok(ArtifactKind::Other),
        other => Err(StoreError::InvalidEnumValue(other.to_string())),
    }
}

fn execution_mode_to_str(value: &ExecutionMode) -> &'static str {
    match value {
        ExecutionMode::LocalProcess => "localProcess",
        ExecutionMode::CondaEnvironment => "condaEnvironment",
        ExecutionMode::Container => "container",
        ExecutionMode::Wsl2 => "wsl2",
        ExecutionMode::Ssh => "ssh",
        ExecutionMode::Slurm => "slurm",
        ExecutionMode::Pbs => "pbs",
        ExecutionMode::Lsf => "lsf",
    }
}

fn execution_mode_from_str(value: &str) -> Result<ExecutionMode, StoreError> {
    match value {
        "localProcess" => Ok(ExecutionMode::LocalProcess),
        "condaEnvironment" => Ok(ExecutionMode::CondaEnvironment),
        "container" => Ok(ExecutionMode::Container),
        "wsl2" => Ok(ExecutionMode::Wsl2),
        "ssh" => Ok(ExecutionMode::Ssh),
        "slurm" => Ok(ExecutionMode::Slurm),
        "pbs" => Ok(ExecutionMode::Pbs),
        "lsf" => Ok(ExecutionMode::Lsf),
        other => Err(StoreError::InvalidEnumValue(other.to_string())),
    }
}

fn detection_status_to_str(value: &DetectionStatus) -> &'static str {
    match value {
        DetectionStatus::Ready => "ready",
        DetectionStatus::MissingInstall => "missingInstall",
        DetectionStatus::MissingLicense => "missingLicense",
        DetectionStatus::PlatformUnsupported => "platformUnsupported",
        DetectionStatus::RemoteRecommended => "remoteRecommended",
    }
}

fn detection_status_from_str(value: &str) -> Result<DetectionStatus, StoreError> {
    match value {
        "ready" => Ok(DetectionStatus::Ready),
        "missingInstall" => Ok(DetectionStatus::MissingInstall),
        "missingLicense" => Ok(DetectionStatus::MissingLicense),
        "platformUnsupported" => Ok(DetectionStatus::PlatformUnsupported),
        "remoteRecommended" => Ok(DetectionStatus::RemoteRecommended),
        other => Err(StoreError::InvalidEnumValue(other.to_string())),
    }
}

fn to_sql_conversion_error(error: StoreError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_keeps_ascii_safe_project_names() {
        assert_eq!(slugify("Protein Ligand 01"), "protein-ligand-01");
        assert_eq!(slugify("  ---  "), "automd-project");
    }

    #[test]
    fn saves_updates_and_deletes_remote_profiles() {
        let path = std::env::temp_dir().join(format!("automd-store-{}.sqlite", Uuid::new_v4()));
        let db = ProjectDatabase::open(&path).expect("db");
        let profile = RemoteProfile {
            id: "custom-slurm".to_string(),
            name: "Custom SLURM".to_string(),
            host: "login.example".to_string(),
            scheduler: ExecutionMode::Slurm,
            workdir: "/scratch/noir/automd".to_string(),
            module_load: vec!["module load gromacs".to_string(), " ".to_string()],
            default_queue: Some("gpu".to_string()),
        };

        let saved = db.save_remote_profile(profile).expect("save");
        assert_eq!(saved.module_load, vec!["module load gromacs"]);

        let profiles = db.list_remote_profiles().expect("list");
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].scheduler, ExecutionMode::Slurm);
        assert_eq!(profiles[0].default_queue.as_deref(), Some("gpu"));

        let mut updated = profiles[0].clone();
        updated.scheduler = ExecutionMode::Pbs;
        updated.default_queue = None;
        db.save_remote_profile(updated).expect("update");
        let profiles = db.list_remote_profiles().expect("list updated");
        assert_eq!(profiles[0].scheduler, ExecutionMode::Pbs);
        assert_eq!(profiles[0].default_queue, None);

        assert!(db.delete_remote_profile("custom-slurm".to_string()).expect("delete"));
        assert!(db.list_remote_profiles().expect("empty").is_empty());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn saves_updates_and_deletes_engine_installations() {
        let path = std::env::temp_dir().join(format!("automd-engines-{}.sqlite", Uuid::new_v4()));
        let db = ProjectDatabase::open(&path).expect("db");
        let record = EngineInstallationRecord {
            engine_id: "namd".to_string(),
            location: "/opt/namd/namd3".to_string(),
            version: Some("NAMD 3.0".to_string()),
            authorization_status: DetectionStatus::MissingLicense,
            checked_at: Utc::now(),
        };

        db.save_engine_installation(record).expect("save");
        let records = db.list_engine_installations().expect("list");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].authorization_status, DetectionStatus::MissingLicense);

        let mut updated = records[0].clone();
        updated.authorization_status = DetectionStatus::Ready;
        updated.version = None;
        db.save_engine_installation(updated).expect("update");
        let records = db.list_engine_installations().expect("updated");
        assert_eq!(records[0].authorization_status, DetectionStatus::Ready);
        assert_eq!(records[0].version, None);

        assert!(db
            .delete_engine_installation("namd".to_string(), "/opt/namd/namd3".to_string())
            .expect("delete"));
        assert!(db.list_engine_installations().expect("empty").is_empty());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn upserts_and_lists_task_records() {
        let path = std::env::temp_dir().join(format!("automd-tasks-{}.sqlite", Uuid::new_v4()));
        let db = ProjectDatabase::open(&path).expect("db");
        let project_id = Uuid::new_v4();
        let mut snapshot = LocalTaskSnapshot {
            id: Uuid::new_v4(),
            plan_id: Uuid::new_v4(),
            engine_id: "gromacs".to_string(),
            mode: LocalRunMode::Mock,
            status: TaskStatus::Running,
            run_directory: "runs/gromacs-test".to_string(),
            command: "python3 mock".to_string(),
            progress_percent: 25.0,
            ns_per_day: Some(12.0),
            current_step: Some(250),
            log_tail: Vec::new(),
            error_message: None,
            exit_code: None,
            artifacts: Vec::new(),
            report_path: None,
            failure_analysis: None,
            resume_plan: None,
            started_at: Utc::now(),
            finished_at: None,
        };

        let record = db
            .upsert_task_snapshot(&snapshot, Some(project_id))
            .expect("insert");
        assert_eq!(record.status, TaskStatus::Running);

        snapshot.status = TaskStatus::Completed;
        snapshot.progress_percent = 100.0;
        db.upsert_task_snapshot(&snapshot, Some(project_id))
            .expect("update");

        let records = db.list_task_records(Some(project_id)).expect("list");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].status, TaskStatus::Completed);
        assert_eq!(records[0].progress_percent, 100.0);
        assert!(db.list_task_records(Some(Uuid::new_v4())).expect("empty").is_empty());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn persists_artifact_metadata_and_analysis_cache() {
        let path = std::env::temp_dir().join(format!("automd-cache-{}.sqlite", Uuid::new_v4()));
        let db = ProjectDatabase::open(&path).expect("db");
        let project_path = "/tmp/automd-cache-project".to_string();
        let generated_at = Utc::now();

        let index = ArtifactIndex {
            project_path: project_path.clone(),
            run_directory: Some("runs/gromacs-test".to_string()),
            artifacts: vec![
                RunArtifact {
                    path: "analysis/rmsd.xvg".to_string(),
                    kind: ArtifactKind::AnalysisTable,
                    size_bytes: 42,
                    modified_at: Some(generated_at),
                    summary: Some("2 data rows".to_string()),
                },
                RunArtifact {
                    path: "reports/automd-report.md".to_string(),
                    kind: ArtifactKind::Report,
                    size_bytes: 99,
                    modified_at: None,
                    summary: None,
                },
            ],
            generated_at,
        };
        let records = db.upsert_artifact_index(&index).expect("artifact upsert");
        assert_eq!(records.len(), 2);
        let records = db.list_artifact_records(project_path.clone()).expect("artifact list");
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].path, "analysis/rmsd.xvg");
        assert_eq!(records[0].kind, ArtifactKind::AnalysisTable);

        let parsed = AnalysisParseResult {
            project_path: project_path.clone(),
            series: vec![AnalysisSeries {
                path: "analysis/rmsd.xvg".to_string(),
                label: "RMSD".to_string(),
                x_label: "Time (ns)".to_string(),
                y_label: "RMSD (nm)".to_string(),
                points: vec![AnalysisPoint { x: 0.0, y: 0.1 }, AnalysisPoint { x: 1.0, y: 0.2 }],
                min_y: Some(0.1),
                max_y: Some(0.2),
                last_y: Some(0.2),
            }],
            warnings: Vec::new(),
            generated_at,
        };
        db.upsert_analysis_cache(&parsed).expect("analysis cache");
        let cached = db
            .list_analysis_cache_records(project_path)
            .expect("analysis cache list");
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].point_count, 2);
        assert_eq!(cached[0].last_y, Some(0.2));

        let _ = std::fs::remove_file(path);
    }
}
