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
                id: Uuid::parse_str(&id).map_err(|err| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(err),
                    )
                })?,
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

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::Database)
    }

    /// Permanently delete a project: its on-disk directory (inputs, generated,
    /// runs, trajectories, analysis, reports, …) and all database records that
    /// reference it. Returns false if no project with `id` exists.
    pub fn delete_project(&self, id: String) -> Result<bool, StoreError> {
        let path: String = match self.connection.query_row(
            "SELECT path FROM projects WHERE id = ?1",
            params![id],
            |row| row.get::<_, String>(0),
        ) {
            Ok(value) => value,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(false),
            Err(error) => return Err(StoreError::Database(error)),
        };

        let directory = PathBuf::from(&path);
        if directory.exists() {
            fs::remove_dir_all(&directory)?;
        }

        self.connection
            .execute("DELETE FROM projects WHERE id = ?1", params![id])?;
        self.connection
            .execute("DELETE FROM tasks WHERE project_id = ?1", params![id])?;
        self.connection.execute(
            "DELETE FROM artifact_records WHERE project_path = ?1",
            params![path],
        )?;
        self.connection.execute(
            "DELETE FROM analysis_cache WHERE project_path = ?1",
            params![path],
        )?;

        Ok(true)
    }

    pub fn list_remote_profiles(&self) -> Result<Vec<RemoteProfile>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, name, host, scheduler, workdir, module_load_json, default_queue,
                    username, port, auth_method, identity_file
             FROM remote_profiles
             ORDER BY name ASC",
        )?;

        let rows = statement.query_map([], |row| {
            let scheduler: String = row.get(3)?;
            let module_load_json: String = row.get(5)?;
            let module_load =
                serde_json::from_str::<Vec<String>>(&module_load_json).map_err(|err| {
                    rusqlite::Error::FromSqlConversionFailure(
                        5,
                        rusqlite::types::Type::Text,
                        Box::new(err),
                    )
                })?;
            let auth_method: String = row.get(9)?;
            let port: i64 = row.get(8)?;
            Ok(RemoteProfile {
                id: row.get(0)?,
                name: row.get(1)?,
                host: row.get(2)?,
                username: row.get(7)?,
                port: u16::try_from(port).unwrap_or(22),
                auth_method: remote_auth_method_from_str(&auth_method),
                identity_file: row.get(10)?,
                scheduler: execution_mode_from_str(&scheduler).map_err(to_sql_conversion_error)?,
                workdir: row.get(4)?,
                module_load,
                default_queue: row.get(6)?,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::Database)
    }

    pub fn save_remote_profile(&self, profile: RemoteProfile) -> Result<RemoteProfile, StoreError> {
        let profile = normalize_remote_profile(profile)?;
        let module_load_json = serde_json::to_string(&profile.module_load)?;
        self.connection.execute(
            "INSERT INTO remote_profiles
                (id, name, host, scheduler, workdir, module_load_json, default_queue,
                 username, port, auth_method, identity_file)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                host = excluded.host,
                scheduler = excluded.scheduler,
                workdir = excluded.workdir,
                module_load_json = excluded.module_load_json,
                default_queue = excluded.default_queue,
                username = excluded.username,
                port = excluded.port,
                auth_method = excluded.auth_method,
                identity_file = excluded.identity_file",
            params![
                profile.id,
                profile.name,
                profile.host,
                execution_mode_to_str(&profile.scheduler),
                profile.workdir,
                module_load_json,
                profile.default_queue,
                profile.username,
                profile.port as i64,
                remote_auth_method_to_str(&profile.auth_method),
                profile.identity_file,
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
            "SELECT target_kind, target_id, target_label, engine_id, location, version,
                    authorization_status, platform, arch, checked_at
             FROM engine_installations
             ORDER BY target_kind ASC, target_label ASC, engine_id ASC, location ASC",
        )?;

        let rows = statement.query_map([], |row| {
            let target_kind: String = row.get(0)?;
            let authorization_status: String = row.get(6)?;
            let platform: Option<String> = row.get(7)?;
            let checked_at: String = row.get(9)?;
            Ok(EngineInstallationRecord {
                target_kind: engine_target_kind_from_str(&target_kind)
                    .map_err(to_sql_conversion_error)?,
                target_id: row.get(1)?,
                target_label: row.get(2)?,
                engine_id: row.get(3)?,
                location: row.get(4)?,
                version: row.get(5)?,
                authorization_status: detection_status_from_str(&authorization_status)
                    .map_err(to_sql_conversion_error)?,
                platform: platform
                    .as_deref()
                    .map(platform_from_str)
                    .transpose()
                    .map_err(to_sql_conversion_error)?,
                arch: row.get(8)?,
                checked_at: parse_datetime(&checked_at).map_err(to_sql_conversion_error)?,
            })
        })?;

        let records = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::Database)?;
        dedupe_engine_installations(records)
    }

    pub fn save_engine_installation(
        &self,
        record: EngineInstallationRecord,
    ) -> Result<EngineInstallationRecord, StoreError> {
        let record = normalize_engine_installation(record)?;
        self.connection.execute(
            "INSERT INTO engine_installations
                (target_kind, target_id, target_label, engine_id, location, version,
                 authorization_status, platform, arch, checked_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(target_kind, target_id, engine_id, location) DO UPDATE SET
                target_label = excluded.target_label,
                version = CASE
                    WHEN trim(excluded.version) = ''
                      OR lower(trim(excluded.version)) IN ('unknown', 'version unknown')
                    THEN engine_installations.version
                    ELSE excluded.version
                END,
                authorization_status = excluded.authorization_status,
                platform = excluded.platform,
                arch = excluded.arch,
                checked_at = excluded.checked_at",
            params![
                engine_target_kind_to_str(&record.target_kind),
                &record.target_id,
                &record.target_label,
                &record.engine_id,
                &record.location,
                &record.version,
                detection_status_to_str(&record.authorization_status),
                record.platform.as_ref().map(platform_to_str),
                &record.arch,
                record.checked_at.to_rfc3339(),
            ],
        )?;
        Ok(record)
    }

    pub fn delete_engine_installation(
        &self,
        engine_id: String,
        location: String,
    ) -> Result<bool, StoreError> {
        let deleted = self.connection.execute(
            "DELETE FROM engine_installations WHERE target_id = 'local' AND engine_id = ?1 AND location = ?2",
            params![engine_id, location],
        )?;
        Ok(deleted > 0)
    }

    pub fn delete_engine_installation_for_target(
        &self,
        target_id: String,
        engine_id: String,
        location: String,
    ) -> Result<bool, StoreError> {
        let deleted = self.connection.execute(
            "DELETE FROM engine_installations WHERE target_id = ?1 AND engine_id = ?2 AND location = ?3",
            params![target_id, engine_id, location],
        )?;
        Ok(deleted > 0)
    }

    pub fn list_remote_helper_statuses(&self) -> Result<Vec<RemoteHelperStatus>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT profile_id, helper_version, status, install_path, platform, arch, hostname,
                    hardware_json, checked_at, last_error
             FROM remote_helper_statuses
             ORDER BY profile_id ASC",
        )?;
        let rows = statement.query_map([], |row| {
            let status: String = row.get(2)?;
            let platform: Option<String> = row.get(4)?;
            let checked_at: String = row.get(8)?;
            Ok(RemoteHelperStatus {
                profile_id: row.get(0)?,
                helper_version: row.get(1)?,
                status: remote_helper_state_from_str(&status).map_err(to_sql_conversion_error)?,
                install_path: row.get(3)?,
                platform: platform
                    .as_deref()
                    .map(platform_from_str)
                    .transpose()
                    .map_err(to_sql_conversion_error)?,
                arch: row.get(5)?,
                hostname: row.get(6)?,
                hardware_json: row.get(7)?,
                checked_at: parse_datetime(&checked_at).map_err(to_sql_conversion_error)?,
                last_error: row.get(9)?,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::Database)
    }

    pub fn save_remote_helper_status(
        &self,
        status: RemoteHelperStatus,
    ) -> Result<RemoteHelperStatus, StoreError> {
        let status = normalize_remote_helper_status(status)?;
        self.connection.execute(
            "INSERT INTO remote_helper_statuses
                (profile_id, helper_version, status, install_path, platform, arch, hostname,
                 hardware_json, checked_at, last_error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(profile_id) DO UPDATE SET
                helper_version = excluded.helper_version,
                status = excluded.status,
                install_path = excluded.install_path,
                platform = excluded.platform,
                arch = excluded.arch,
                hostname = excluded.hostname,
                hardware_json = excluded.hardware_json,
                checked_at = excluded.checked_at,
                last_error = excluded.last_error",
            params![
                &status.profile_id,
                &status.helper_version,
                remote_helper_state_to_str(&status.status),
                &status.install_path,
                status.platform.as_ref().map(platform_to_str),
                &status.arch,
                &status.hostname,
                &status.hardware_json,
                status.checked_at.to_rfc3339(),
                &status.last_error,
            ],
        )?;
        Ok(status)
    }

    pub fn list_plugin_states(&self) -> Result<Vec<PluginStateRecord>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT plugin_id, enabled, config_json, installed_at, updated_at, last_run_at, last_error
             FROM plugin_states
             ORDER BY plugin_id ASC",
        )?;
        let rows = statement.query_map([], |row| {
            let config_json: String = row.get(2)?;
            let installed_at: String = row.get(3)?;
            let updated_at: String = row.get(4)?;
            let last_run_at: Option<String> = row.get(5)?;
            Ok(PluginStateRecord {
                plugin_id: row.get(0)?,
                enabled: row.get::<_, i64>(1)? != 0,
                config: serde_json::from_str(&config_json).map_err(|err| {
                    rusqlite::Error::FromSqlConversionFailure(
                        2,
                        rusqlite::types::Type::Text,
                        Box::new(err),
                    )
                })?,
                installed_at: parse_datetime(&installed_at).map_err(to_sql_conversion_error)?,
                updated_at: parse_datetime(&updated_at).map_err(to_sql_conversion_error)?,
                last_run_at: last_run_at
                    .as_deref()
                    .map(parse_datetime)
                    .transpose()
                    .map_err(to_sql_conversion_error)?,
                last_error: row.get(6)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::Database)
    }

    pub fn set_plugin_enabled(
        &self,
        plugin_id: &str,
        enabled: bool,
    ) -> Result<PluginStateRecord, StoreError> {
        let now = Utc::now();
        self.connection.execute(
            "INSERT INTO plugin_states
                (plugin_id, enabled, config_json, installed_at, updated_at, last_run_at, last_error)
             VALUES (?1, ?2, ?3, ?4, ?4, NULL, NULL)
             ON CONFLICT(plugin_id) DO UPDATE SET
                enabled = excluded.enabled,
                updated_at = excluded.updated_at",
            params![
                plugin_id,
                if enabled { 1 } else { 0 },
                serde_json::Value::Null.to_string(),
                now.to_rfc3339(),
            ],
        )?;
        self.plugin_state(plugin_id)
    }

    pub fn save_plugin_config(
        &self,
        plugin_id: &str,
        config: serde_json::Value,
    ) -> Result<PluginStateRecord, StoreError> {
        let now = Utc::now();
        self.connection.execute(
            "INSERT INTO plugin_states
                (plugin_id, enabled, config_json, installed_at, updated_at, last_run_at, last_error)
             VALUES (?1, 1, ?2, ?3, ?3, NULL, NULL)
             ON CONFLICT(plugin_id) DO UPDATE SET
                config_json = excluded.config_json,
                updated_at = excluded.updated_at",
            params![plugin_id, config.to_string(), now.to_rfc3339()],
        )?;
        self.plugin_state(plugin_id)
    }

    pub fn delete_plugin_state(&self, plugin_id: &str) -> Result<bool, StoreError> {
        let deleted = self.connection.execute(
            "DELETE FROM plugin_states WHERE plugin_id = ?1",
            params![plugin_id],
        )?;
        self.connection.execute(
            "DELETE FROM plugin_runs WHERE plugin_id = ?1",
            params![plugin_id],
        )?;
        Ok(deleted > 0)
    }

    pub fn insert_plugin_run(
        &self,
        id: Uuid,
        plugin_id: &str,
        action_id: &str,
        mode: PluginRunMode,
    ) -> Result<PluginRunRecord, StoreError> {
        let started_at = Utc::now();
        self.connection.execute(
            "INSERT INTO plugin_runs
                (id, plugin_id, action_id, mode, status, started_at, finished_at, stdout_tail, stderr_tail)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, NULL, NULL)",
            params![
                id.to_string(),
                plugin_id,
                action_id,
                plugin_run_mode_to_str(&mode),
                plugin_run_status_to_str(&PluginRunStatus::Running),
                started_at.to_rfc3339(),
            ],
        )?;
        Ok(PluginRunRecord {
            id,
            plugin_id: plugin_id.to_string(),
            action_id: action_id.to_string(),
            mode,
            status: PluginRunStatus::Running,
            started_at,
            finished_at: None,
            stdout_tail: None,
            stderr_tail: None,
        })
    }

    pub fn finish_plugin_run(
        &self,
        id: Uuid,
        status: PluginRunStatus,
        stdout: &str,
        stderr: &str,
    ) -> Result<PluginRunRecord, StoreError> {
        let finished_at = Utc::now();
        let stdout_tail = tail_string(stdout, 4096);
        let stderr_tail = tail_string(stderr, 4096);
        self.connection.execute(
            "UPDATE plugin_runs
             SET status = ?1, finished_at = ?2, stdout_tail = ?3, stderr_tail = ?4
             WHERE id = ?5",
            params![
                plugin_run_status_to_str(&status),
                finished_at.to_rfc3339(),
                stdout_tail,
                stderr_tail,
                id.to_string(),
            ],
        )?;
        let record = self.plugin_run(id)?;
        let last_error = if matches!(status, PluginRunStatus::Failed) {
            record.stderr_tail.clone()
        } else {
            None
        };
        self.connection.execute(
            "INSERT INTO plugin_states
                (plugin_id, enabled, config_json, installed_at, updated_at, last_run_at, last_error)
             VALUES (?1, 1, 'null', ?2, ?2, ?2, ?3)
             ON CONFLICT(plugin_id) DO UPDATE SET
                updated_at = excluded.updated_at,
                last_run_at = excluded.last_run_at,
                last_error = excluded.last_error",
            params![record.plugin_id, finished_at.to_rfc3339(), last_error],
        )?;
        Ok(record)
    }

    fn plugin_state(&self, plugin_id: &str) -> Result<PluginStateRecord, StoreError> {
        self.connection
            .query_row(
                "SELECT plugin_id, enabled, config_json, installed_at, updated_at, last_run_at, last_error
                 FROM plugin_states WHERE plugin_id = ?1",
                params![plugin_id],
                |row| {
                    let config_json: String = row.get(2)?;
                    let installed_at: String = row.get(3)?;
                    let updated_at: String = row.get(4)?;
                    let last_run_at: Option<String> = row.get(5)?;
                    Ok(PluginStateRecord {
                        plugin_id: row.get(0)?,
                        enabled: row.get::<_, i64>(1)? != 0,
                        config: serde_json::from_str(&config_json)
                            .map_err(|err| rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(err)))?,
                        installed_at: parse_datetime(&installed_at).map_err(to_sql_conversion_error)?,
                        updated_at: parse_datetime(&updated_at).map_err(to_sql_conversion_error)?,
                        last_run_at: last_run_at
                            .as_deref()
                            .map(parse_datetime)
                            .transpose()
                            .map_err(to_sql_conversion_error)?,
                        last_error: row.get(6)?,
                    })
                },
            )
            .map_err(StoreError::Database)
    }

    fn plugin_run(&self, id: Uuid) -> Result<PluginRunRecord, StoreError> {
        self.connection
            .query_row(
                "SELECT id, plugin_id, action_id, mode, status, started_at, finished_at, stdout_tail, stderr_tail
                 FROM plugin_runs WHERE id = ?1",
                params![id.to_string()],
                |row| {
                    let id_text: String = row.get(0)?;
                    let mode: String = row.get(3)?;
                    let status: String = row.get(4)?;
                    let started_at: String = row.get(5)?;
                    let finished_at: Option<String> = row.get(6)?;
                    Ok(PluginRunRecord {
                        id: Uuid::parse_str(&id_text)
                            .map_err(|err| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err)))?,
                        plugin_id: row.get(1)?,
                        action_id: row.get(2)?,
                        mode: plugin_run_mode_from_str(&mode).map_err(to_sql_conversion_error)?,
                        status: plugin_run_status_from_str(&status).map_err(to_sql_conversion_error)?,
                        started_at: parse_datetime(&started_at).map_err(to_sql_conversion_error)?,
                        finished_at: finished_at
                            .as_deref()
                            .map(parse_datetime)
                            .transpose()
                            .map_err(to_sql_conversion_error)?,
                        stdout_tail: row.get(7)?,
                        stderr_tail: row.get(8)?,
                    })
                },
            )
            .map_err(StoreError::Database)
    }

    pub fn upsert_task_snapshot(
        &self,
        snapshot: &LocalTaskSnapshot,
        project_id: Option<Uuid>,
    ) -> Result<TaskRecord, StoreError> {
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

    pub fn list_task_records(
        &self,
        project_id: Option<Uuid>,
    ) -> Result<Vec<TaskRecord>, StoreError> {
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
                id: Uuid::parse_str(&id).map_err(|err| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(err),
                    )
                })?,
                project_id: project_id
                    .as_deref()
                    .map(Uuid::parse_str)
                    .transpose()
                    .map_err(|err| {
                        rusqlite::Error::FromSqlConversionFailure(
                            1,
                            rusqlite::types::Type::Text,
                            Box::new(err),
                        )
                    })?,
                plan_id: Uuid::parse_str(&plan_id).map_err(|err| {
                    rusqlite::Error::FromSqlConversionFailure(
                        2,
                        rusqlite::types::Type::Text,
                        Box::new(err),
                    )
                })?,
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
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::Database)
    }

    pub fn upsert_artifact_index(
        &self,
        index: &ArtifactIndex,
    ) -> Result<Vec<ArtifactRecord>, StoreError> {
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

    pub fn list_artifact_records(
        &self,
        project_path: String,
    ) -> Result<Vec<ArtifactRecord>, StoreError> {
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
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::Database)
    }

    pub fn upsert_analysis_cache(
        &self,
        result: &AnalysisParseResult,
    ) -> Result<Vec<AnalysisCacheRecord>, StoreError> {
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

    pub fn list_analysis_cache_records(
        &self,
        project_path: String,
    ) -> Result<Vec<AnalysisCacheRecord>, StoreError> {
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
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::Database)
    }

    fn migrate(&self) -> Result<(), StoreError> {
        self.connection.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA busy_timeout = 5000;
            PRAGMA synchronous = NORMAL;
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
                target_kind TEXT NOT NULL DEFAULT 'local',
                target_id TEXT NOT NULL DEFAULT 'local',
                target_label TEXT NOT NULL DEFAULT '本机',
                engine_id TEXT NOT NULL,
                location TEXT NOT NULL,
                version TEXT,
                authorization_status TEXT NOT NULL,
                platform TEXT,
                arch TEXT,
                checked_at TEXT NOT NULL,
                PRIMARY KEY (target_kind, target_id, engine_id, location)
            );
            CREATE TABLE IF NOT EXISTS remote_profiles (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                host TEXT NOT NULL,
                username TEXT NOT NULL DEFAULT '',
                port INTEGER NOT NULL DEFAULT 22,
                auth_method TEXT NOT NULL DEFAULT 'agent',
                identity_file TEXT,
                scheduler TEXT NOT NULL,
                workdir TEXT NOT NULL,
                module_load_json TEXT NOT NULL,
                default_queue TEXT
            );
            CREATE TABLE IF NOT EXISTS remote_helper_statuses (
                profile_id TEXT PRIMARY KEY,
                helper_version TEXT,
                status TEXT NOT NULL,
                install_path TEXT,
                platform TEXT,
                arch TEXT,
                hostname TEXT,
                hardware_json TEXT,
                checked_at TEXT NOT NULL,
                last_error TEXT
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
            CREATE TABLE IF NOT EXISTS plugin_states (
                plugin_id TEXT PRIMARY KEY,
                enabled INTEGER NOT NULL DEFAULT 1,
                config_json TEXT NOT NULL DEFAULT 'null',
                installed_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                last_run_at TEXT,
                last_error TEXT
            );
            CREATE TABLE IF NOT EXISTS plugin_runs (
                id TEXT PRIMARY KEY,
                plugin_id TEXT NOT NULL,
                action_id TEXT NOT NULL,
                mode TEXT NOT NULL,
                status TEXT NOT NULL,
                started_at TEXT NOT NULL,
                finished_at TEXT,
                stdout_tail TEXT,
                stderr_tail TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_tasks_project_updated
                ON tasks(project_id, updated_at);
            CREATE INDEX IF NOT EXISTS idx_tasks_status
                ON tasks(status);
            CREATE INDEX IF NOT EXISTS idx_artifact_records_project
                ON artifact_records(project_path, indexed_at);
            CREATE INDEX IF NOT EXISTS idx_analysis_cache_project
                ON analysis_cache(project_path, generated_at);
            ",
        )?;
        self.migrate_engine_installation_targets()?;
        self.migrate_remote_profile_connection()?;
        Ok(())
    }

    /// Add the SSH connection columns (username/port/auth_method/identity_file)
    /// to pre-existing `remote_profiles` tables. Idempotent — only adds what's
    /// missing, so older databases upgrade in place without losing saved hosts.
    fn migrate_remote_profile_connection(&self) -> Result<(), StoreError> {
        let columns = self
            .connection
            .prepare("PRAGMA table_info(remote_profiles)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        let has = |name: &str| columns.iter().any(|column| column == name);
        if !has("username") {
            self.connection.execute_batch(
                "ALTER TABLE remote_profiles ADD COLUMN username TEXT NOT NULL DEFAULT ''",
            )?;
        }
        if !has("port") {
            self.connection.execute_batch(
                "ALTER TABLE remote_profiles ADD COLUMN port INTEGER NOT NULL DEFAULT 22",
            )?;
        }
        if !has("auth_method") {
            self.connection.execute_batch(
                "ALTER TABLE remote_profiles ADD COLUMN auth_method TEXT NOT NULL DEFAULT 'agent'",
            )?;
        }
        if !has("identity_file") {
            self.connection
                .execute_batch("ALTER TABLE remote_profiles ADD COLUMN identity_file TEXT")?;
        }
        Ok(())
    }

    fn migrate_engine_installation_targets(&self) -> Result<(), StoreError> {
        let columns = self
            .connection
            .prepare("PRAGMA table_info(engine_installations)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        if columns.iter().any(|column| column == "target_kind") {
            return Ok(());
        }

        self.connection.execute_batch(
            "
            ALTER TABLE engine_installations RENAME TO engine_installations_legacy;
            CREATE TABLE engine_installations (
                target_kind TEXT NOT NULL DEFAULT 'local',
                target_id TEXT NOT NULL DEFAULT 'local',
                target_label TEXT NOT NULL DEFAULT '本机',
                engine_id TEXT NOT NULL,
                location TEXT NOT NULL,
                version TEXT,
                authorization_status TEXT NOT NULL,
                platform TEXT,
                arch TEXT,
                checked_at TEXT NOT NULL,
                PRIMARY KEY (target_kind, target_id, engine_id, location)
            );
            INSERT INTO engine_installations
                (target_kind, target_id, target_label, engine_id, location, version,
                 authorization_status, platform, arch, checked_at)
            SELECT
                'local', 'local', '本机', engine_id, location, version,
                authorization_status, NULL, NULL, checked_at
            FROM engine_installations_legacy;
            DROP TABLE engine_installations_legacy;
            ",
        )?;
        Ok(())
    }
}

fn tail_string(value: &str, max_chars: usize) -> String {
    let chars = value.chars().collect::<Vec<_>>();
    let start = chars.len().saturating_sub(max_chars);
    chars[start..].iter().collect()
}

fn normalize_engine_installation(
    mut record: EngineInstallationRecord,
) -> Result<EngineInstallationRecord, StoreError> {
    record.target_id = record.target_id.trim().to_string();
    record.target_label = record.target_label.trim().to_string();
    record.engine_id = record.engine_id.trim().to_string();
    record.location = normalize_engine_installation_location(&record.engine_id, &record.location);
    record.arch = record
        .arch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    record.version = record
        .version
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    if record.target_id.is_empty() {
        record.target_id = "local".to_string();
    }
    if record.target_label.is_empty() {
        record.target_label = "本机".to_string();
    }
    if record.engine_id.is_empty() || record.location.is_empty() {
        return Err(StoreError::InvalidEngineInstallation);
    }
    Ok(record)
}

fn normalize_engine_installation_location(engine_id: &str, location: &str) -> String {
    let trimmed = location.trim();
    if !matches!(engine_id, "openmm" | "hoomd") || cfg!(target_os = "windows") {
        return trimmed.to_string();
    }
    let path = PathBuf::from(trimmed);
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if name == "python3" || name.starts_with("python3.") {
        if let Some(parent) = path.parent() {
            let python = parent.join("python");
            if python.is_file() {
                return python.display().to_string();
            }
        }
    }
    trimmed.to_string()
}

fn dedupe_engine_installations(
    records: Vec<EngineInstallationRecord>,
) -> Result<Vec<EngineInstallationRecord>, StoreError> {
    let mut deduped: Vec<EngineInstallationRecord> = Vec::new();
    for record in records {
        let record = normalize_engine_installation(record)?;
        if let Some(existing) = deduped.iter_mut().find(|existing| {
            existing.target_kind == record.target_kind
                && existing.target_id == record.target_id
                && existing.engine_id == record.engine_id
                && existing.location == record.location
        }) {
            if engine_installation_record_is_better(&record, existing) {
                *existing = record;
            }
        } else {
            deduped.push(record);
        }
    }
    Ok(deduped)
}

fn engine_installation_record_is_better(
    candidate: &EngineInstallationRecord,
    current: &EngineInstallationRecord,
) -> bool {
    let candidate_version = informative_version(candidate.version.as_deref());
    let current_version = informative_version(current.version.as_deref());
    if candidate_version != current_version {
        return candidate_version;
    }
    candidate.checked_at > current.checked_at
}

fn informative_version(version: Option<&str>) -> bool {
    version
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            let lower = value.to_ascii_lowercase();
            lower != "version unknown" && lower != "unknown"
        })
        .unwrap_or(false)
}

fn normalize_remote_helper_status(
    mut status: RemoteHelperStatus,
) -> Result<RemoteHelperStatus, StoreError> {
    status.profile_id = status.profile_id.trim().to_string();
    status.helper_version = status
        .helper_version
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    status.install_path = status
        .install_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    status.arch = status
        .arch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    status.hostname = status
        .hostname
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    status.last_error = status
        .last_error
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    if status.profile_id.is_empty() {
        return Err(StoreError::InvalidRemoteProfile);
    }
    Ok(status)
}

fn normalize_remote_profile(mut profile: RemoteProfile) -> Result<RemoteProfile, StoreError> {
    profile.id = profile.id.trim().to_string();
    profile.name = profile.name.trim().to_string();
    profile.host = profile.host.trim().to_string();
    profile.username = profile.username.trim().to_string();
    if profile.port == 0 {
        profile.port = 22;
    }
    profile.identity_file = profile
        .identity_file
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
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
    if profile.id.is_empty()
        || profile.name.is_empty()
        || profile.host.is_empty()
        || profile.workdir.is_empty()
    {
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

fn plugin_run_mode_to_str(value: &PluginRunMode) -> &'static str {
    match value {
        PluginRunMode::Sandbox => "sandbox",
        PluginRunMode::Direct => "direct",
    }
}

fn plugin_run_mode_from_str(value: &str) -> Result<PluginRunMode, StoreError> {
    match value {
        "sandbox" => Ok(PluginRunMode::Sandbox),
        "direct" => Ok(PluginRunMode::Direct),
        other => Err(StoreError::InvalidEnumValue(other.to_string())),
    }
}

fn plugin_run_status_to_str(value: &PluginRunStatus) -> &'static str {
    match value {
        PluginRunStatus::Running => "running",
        PluginRunStatus::Completed => "completed",
        PluginRunStatus::Failed => "failed",
    }
}

fn plugin_run_status_from_str(value: &str) -> Result<PluginRunStatus, StoreError> {
    match value {
        "running" => Ok(PluginRunStatus::Running),
        "completed" => Ok(PluginRunStatus::Completed),
        "failed" => Ok(PluginRunStatus::Failed),
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

fn remote_auth_method_to_str(value: &RemoteAuthMethod) -> &'static str {
    match value {
        RemoteAuthMethod::Agent => "agent",
        RemoteAuthMethod::Key => "key",
        RemoteAuthMethod::Password => "password",
    }
}

fn remote_auth_method_from_str(value: &str) -> RemoteAuthMethod {
    match value {
        "key" => RemoteAuthMethod::Key,
        "password" => RemoteAuthMethod::Password,
        _ => RemoteAuthMethod::Agent,
    }
}

fn engine_target_kind_to_str(value: &EngineTargetKind) -> &'static str {
    match value {
        EngineTargetKind::Local => "local",
        EngineTargetKind::Remote => "remote",
    }
}

fn engine_target_kind_from_str(value: &str) -> Result<EngineTargetKind, StoreError> {
    match value {
        "local" => Ok(EngineTargetKind::Local),
        "remote" => Ok(EngineTargetKind::Remote),
        other => Err(StoreError::InvalidEnumValue(other.to_string())),
    }
}

fn remote_helper_state_to_str(value: &RemoteHelperState) -> &'static str {
    match value {
        RemoteHelperState::Missing => "missing",
        RemoteHelperState::Ready => "ready",
        RemoteHelperState::Outdated => "outdated",
        RemoteHelperState::Unreachable => "unreachable",
        RemoteHelperState::PermissionDenied => "permissionDenied",
    }
}

fn remote_helper_state_from_str(value: &str) -> Result<RemoteHelperState, StoreError> {
    match value {
        "missing" => Ok(RemoteHelperState::Missing),
        "ready" => Ok(RemoteHelperState::Ready),
        "outdated" => Ok(RemoteHelperState::Outdated),
        "unreachable" => Ok(RemoteHelperState::Unreachable),
        "permissionDenied" => Ok(RemoteHelperState::PermissionDenied),
        other => Err(StoreError::InvalidEnumValue(other.to_string())),
    }
}

fn platform_to_str(value: &Platform) -> &'static str {
    match value {
        Platform::Windows => "windows",
        Platform::Macos => "macos",
        Platform::Linux => "linux",
        Platform::Wsl2 => "wsl2",
        Platform::RemoteLinux => "remoteLinux",
    }
}

fn platform_from_str(value: &str) -> Result<Platform, StoreError> {
    match value {
        "windows" => Ok(Platform::Windows),
        "macos" => Ok(Platform::Macos),
        "linux" => Ok(Platform::Linux),
        "wsl2" => Ok(Platform::Wsl2),
        "remoteLinux" => Ok(Platform::RemoteLinux),
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
        DetectionStatus::NotApplicable => "notApplicable",
    }
}

fn detection_status_from_str(value: &str) -> Result<DetectionStatus, StoreError> {
    match value {
        "ready" => Ok(DetectionStatus::Ready),
        "missingInstall" => Ok(DetectionStatus::MissingInstall),
        "missingLicense" => Ok(DetectionStatus::MissingLicense),
        "platformUnsupported" => Ok(DetectionStatus::PlatformUnsupported),
        "remoteRecommended" => Ok(DetectionStatus::RemoteRecommended),
        "notApplicable" => Ok(DetectionStatus::NotApplicable),
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
            username: "noir".to_string(),
            port: 2222,
            auth_method: RemoteAuthMethod::Key,
            identity_file: Some("~/.ssh/id_ed25519".to_string()),
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
        assert_eq!(profiles[0].username, "noir");
        assert_eq!(profiles[0].port, 2222);
        assert_eq!(profiles[0].auth_method, RemoteAuthMethod::Key);
        assert_eq!(
            profiles[0].identity_file.as_deref(),
            Some("~/.ssh/id_ed25519")
        );

        let mut updated = profiles[0].clone();
        updated.scheduler = ExecutionMode::Pbs;
        updated.default_queue = None;
        db.save_remote_profile(updated).expect("update");
        let profiles = db.list_remote_profiles().expect("list updated");
        assert_eq!(profiles[0].scheduler, ExecutionMode::Pbs);
        assert_eq!(profiles[0].default_queue, None);

        assert!(db
            .delete_remote_profile("custom-slurm".to_string())
            .expect("delete"));
        assert!(db.list_remote_profiles().expect("empty").is_empty());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn saves_updates_and_deletes_engine_installations() {
        let path = std::env::temp_dir().join(format!("automd-engines-{}.sqlite", Uuid::new_v4()));
        let db = ProjectDatabase::open(&path).expect("db");
        let record = EngineInstallationRecord {
            target_kind: EngineTargetKind::Local,
            target_id: "local".to_string(),
            target_label: "本机".to_string(),
            engine_id: "namd".to_string(),
            location: "/opt/namd/namd3".to_string(),
            version: Some("NAMD 3.0".to_string()),
            authorization_status: DetectionStatus::MissingLicense,
            platform: Some(Platform::Linux),
            arch: Some("x86_64".to_string()),
            checked_at: Utc::now(),
        };

        db.save_engine_installation(record).expect("save");
        let records = db.list_engine_installations().expect("list");
        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0].authorization_status,
            DetectionStatus::MissingLicense
        );

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
    fn engine_installations_normalize_python_module_duplicates() {
        let path =
            std::env::temp_dir().join(format!("automd-openmm-engines-{}.sqlite", Uuid::new_v4()));
        let root = std::env::temp_dir().join(format!("automd-openmm-prefix-{}", Uuid::new_v4()));
        let bin = root.join("bin");
        std::fs::create_dir_all(&bin).expect("bin");
        let python = bin.join("python");
        let python3 = bin.join("python3");
        std::fs::write(&python, "").expect("python");
        std::fs::write(&python3, "").expect("python3");

        let db = ProjectDatabase::open(&path).expect("db");
        let base = EngineInstallationRecord {
            target_kind: EngineTargetKind::Local,
            target_id: "local".to_string(),
            target_label: "本机".to_string(),
            engine_id: "openmm".to_string(),
            location: python.display().to_string(),
            version: Some("8.5.1".to_string()),
            authorization_status: DetectionStatus::Ready,
            platform: Some(Platform::Macos),
            arch: Some("aarch64".to_string()),
            checked_at: Utc::now(),
        };
        let mut duplicate = base.clone();
        duplicate.location = python3.display().to_string();
        duplicate.version = Some("version unknown".to_string());
        duplicate.checked_at = duplicate.checked_at + chrono::Duration::seconds(60);

        db.save_engine_installation(base).expect("save base");
        db.save_engine_installation(duplicate)
            .expect("save duplicate");
        let records = db.list_engine_installations().expect("list");

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].location, python.display().to_string());
        assert_eq!(records[0].version.as_deref(), Some("8.5.1"));

        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_dir_all(root);
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
        assert!(db
            .list_task_records(Some(Uuid::new_v4()))
            .expect("empty")
            .is_empty());
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
        let records = db
            .list_artifact_records(project_path.clone())
            .expect("artifact list");
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
                points: vec![
                    AnalysisPoint { x: 0.0, y: 0.1 },
                    AnalysisPoint { x: 1.0, y: 0.2 },
                ],
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
