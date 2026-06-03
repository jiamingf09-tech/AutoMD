mod analysis;
mod artifacts;
mod batch;
mod build_runner;
mod engine_adapters;
mod engine_registry;
mod models;
mod parameter_mapping;
mod planner;
mod plugins;
mod project_files;
mod project_store;
mod recipes;
mod remote_monitor;
mod remote_runner;
mod runtime;
mod science_sidecar;
mod structure_import;
mod task_runner;
mod trajectory;

use crate::models::*;
use crate::project_store::ProjectDatabase;
use crate::task_runner::TaskManager;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Manager;

struct AppState {
    db: Mutex<ProjectDatabase>,
    project_root: PathBuf,
    plugin_root: PathBuf,
    task_manager: TaskManager,
}

#[tauri::command]
fn get_engine_capabilities() -> Vec<EngineCapability> {
    engine_registry::detect_all()
}

#[tauri::command]
fn list_engine_capabilities(state: tauri::State<'_, AppState>) -> Vec<EngineCapability> {
    let mut capabilities = engine_registry::detect_all();
    if let Ok(db) = state.db.lock() {
        if let Ok(records) = db.list_engine_installations() {
            apply_installation_records(&mut capabilities, &records);
        }
    }
    capabilities
}

fn apply_installation_records(capabilities: &mut [EngineCapability], records: &[EngineInstallationRecord]) {
    for capability in capabilities {
        if let Some(record) = records.iter().find(|record| record.engine_id == capability.id) {
            capability.detection = DetectionState {
                status: record.authorization_status.clone(),
                path: Some(record.location.clone()),
                version: record.version.clone(),
                message: match record.authorization_status {
                    DetectionStatus::Ready => "用户保存的引擎路径已标记可用。".to_string(),
                    DetectionStatus::MissingLicense => "用户保存的引擎路径仍需许可/授权确认。".to_string(),
                    DetectionStatus::MissingInstall => "用户保存的引擎路径标记为需要安装检查。".to_string(),
                    DetectionStatus::PlatformUnsupported => "用户保存的引擎路径标记为当前平台不支持。".to_string(),
                    DetectionStatus::RemoteRecommended => "用户保存的引擎配置建议通过远程环境运行。".to_string(),
                },
            };
        }
    }
}

#[tauri::command]
fn get_runtime_diagnostics() -> RuntimeDiagnostics {
    runtime::diagnostics()
}

#[tauri::command]
fn get_science_sidecar_diagnostics() -> ScienceSidecarDiagnostics {
    science_sidecar::diagnostics()
}

#[tauri::command]
fn list_remote_profile_templates() -> Vec<RemoteProfile> {
    runtime::remote_profile_templates()
}

#[tauri::command]
fn list_remote_profiles(state: tauri::State<'_, AppState>) -> Result<Vec<RemoteProfile>, String> {
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    let saved = db.list_remote_profiles().map_err(|error| error.to_string())?;
    let mut profiles = saved;
    for template in runtime::remote_profile_templates() {
        if !profiles.iter().any(|profile| profile.id == template.id) {
            profiles.push(template);
        }
    }
    Ok(profiles)
}

#[tauri::command]
fn save_remote_profile(profile: RemoteProfile, state: tauri::State<'_, AppState>) -> Result<RemoteProfile, String> {
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    db.save_remote_profile(profile).map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_remote_profile(id: String, state: tauri::State<'_, AppState>) -> Result<bool, String> {
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    db.delete_remote_profile(id).map_err(|error| error.to_string())
}

#[tauri::command]
fn list_engine_installations(state: tauri::State<'_, AppState>) -> Result<Vec<EngineInstallationRecord>, String> {
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    db.list_engine_installations().map_err(|error| error.to_string())
}

#[tauri::command]
fn save_engine_installation(
    record: EngineInstallationRecord,
    state: tauri::State<'_, AppState>,
) -> Result<EngineInstallationRecord, String> {
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    db.save_engine_installation(record).map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_engine_installation(
    engine_id: String,
    location: String,
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    db.delete_engine_installation(engine_id, location)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn list_plugin_manifests(state: tauri::State<'_, AppState>) -> Result<PluginRegistrySnapshot, String> {
    plugins::registry_snapshot(&state.plugin_root).map_err(|error| error.to_string())
}

#[tauri::command]
fn list_projects(state: tauri::State<'_, AppState>) -> Result<Vec<ProjectSummary>, String> {
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    db.list_projects().map_err(|error| error.to_string())
}

#[tauri::command]
fn create_project(
    request: CreateProjectRequest,
    state: tauri::State<'_, AppState>,
) -> Result<ProjectSummary, String> {
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    db.create_project(request, &state.project_root)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn generate_simulation_plan(request: PlanRequest) -> SimulationPlan {
    planner::default_simulation_plan(request)
}

#[tauri::command]
fn validate_simulation_plan(plan: SimulationPlan) -> ValidationReport {
    planner::validate_plan(&plan)
}

#[tauri::command]
fn map_engine_parameters(request: ParameterMappingRequest) -> ParameterMappingReport {
    parameter_mapping::map_parameters(request)
}

#[tauri::command]
fn create_mock_task(plan: SimulationPlan) -> SimulationTask {
    planner::mock_task(plan)
}

#[tauri::command]
fn import_structure(request: StructureImportRequest) -> Result<StructureImportResult, String> {
    structure_import::import_structure(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn read_structure_file(request: StructureFileRequest) -> Result<StructureFilePayload, String> {
    structure_import::read_structure_file(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn generate_slurm_script(plan: SimulationPlan) -> String {
    recipes::slurm_script(&plan)
}

#[tauri::command]
fn generate_remote_execution_package(request: RemoteExecutionRequest) -> RemoteExecutionPackage {
    recipes::remote_execution_package(request)
}

#[tauri::command]
fn parse_remote_job_status(request: RemoteStatusParseRequest) -> RemoteJobSnapshot {
    remote_monitor::parse_remote_status(request)
}

#[tauri::command]
fn run_remote_workflow_step(request: RemoteWorkflowStepRequest) -> Result<RemoteWorkflowStepResult, String> {
    remote_runner::run_remote_workflow_step(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn generate_container_recipe(engine_id: String) -> ContainerRecipe {
    recipes::container_recipe(&engine_id)
}

#[tauri::command]
fn generate_build_recipe(options: BuildRecipeOptions) -> BuildRecipe {
    recipes::build_recipe(options)
}

#[tauri::command]
fn export_recipe_package(request: RecipeExportRequest) -> Result<RecipeExportResult, String> {
    recipes::export_recipe_package(request)
}

#[tauri::command]
fn run_build_workflow(request: BuildWorkflowRequest) -> Result<BuildWorkflowResult, String> {
    build_runner::run_build_workflow(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn prepare_engine_run_package(request: EngineRunRequest) -> Result<EngineRunPackage, String> {
    engine_adapters::prepare_run_package(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn prepare_batch_experiment(request: BatchExperimentRequest) -> Result<BatchExperimentPackage, String> {
    batch::prepare_batch_experiment(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn read_project_text_file(request: ProjectTextFileRequest) -> Result<ProjectTextFilePayload, String> {
    project_files::read_project_text_file(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn write_project_text_file(request: ProjectTextFileWriteRequest) -> Result<ProjectTextFilePayload, String> {
    project_files::write_project_text_file(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn prepare_structure_package(request: StructurePreparationRequest) -> Result<StructurePreparationPackage, String> {
    science_sidecar::prepare_structure_package(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn prepare_trajectory_analysis_package(request: TrajectoryAnalysisRequest) -> Result<TrajectoryAnalysisPackage, String> {
    science_sidecar::prepare_analysis_package(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn parse_engine_log(request: EngineLogParseRequest) -> Result<EngineLogReport, String> {
    engine_adapters::parse_engine_log(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn classify_engine_failure(request: FailureAnalysisRequest) -> Result<FailureAnalysis, String> {
    engine_adapters::classify_engine_failure(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn discover_resume_plan(request: ResumePlanRequest) -> Result<ResumePlan, String> {
    engine_adapters::discover_resume_plan(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn start_local_engine_run(
    request: StartLocalRunRequest,
    state: tauri::State<'_, AppState>,
) -> Result<LocalTaskSnapshot, String> {
    let project_id = request.plan.project_id;
    let snapshot = state
        .task_manager
        .start(request)
        .map_err(|error| error.to_string())?;
    persist_task_snapshot(&state, &snapshot, project_id)?;
    Ok(snapshot)
}

#[tauri::command]
fn get_local_task_snapshot(
    task_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<LocalTaskSnapshot, String> {
    let task_id = uuid::Uuid::parse_str(&task_id).map_err(|error| error.to_string())?;
    let snapshot = state
        .task_manager
        .snapshot(task_id)
        .map_err(|error| error.to_string())?;
    persist_task_snapshot(&state, &snapshot, None)?;
    Ok(snapshot)
}

#[tauri::command]
fn list_local_tasks(state: tauri::State<'_, AppState>) -> Vec<LocalTaskSnapshot> {
    state.task_manager.list()
}

#[tauri::command]
fn cancel_local_task(
    task_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<LocalTaskSnapshot, String> {
    let task_id = uuid::Uuid::parse_str(&task_id).map_err(|error| error.to_string())?;
    let snapshot = state
        .task_manager
        .cancel(task_id)
        .map_err(|error| error.to_string())?;
    persist_task_snapshot(&state, &snapshot, None)?;
    Ok(snapshot)
}

#[tauri::command]
fn list_task_records(project_id: Option<String>, state: tauri::State<'_, AppState>) -> Result<Vec<TaskRecord>, String> {
    let project_id = project_id
        .as_deref()
        .map(uuid::Uuid::parse_str)
        .transpose()
        .map_err(|error| error.to_string())?;
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    db.list_task_records(project_id).map_err(|error| error.to_string())
}

fn persist_task_snapshot(
    state: &tauri::State<'_, AppState>,
    snapshot: &LocalTaskSnapshot,
    project_id: Option<uuid::Uuid>,
) -> Result<(), String> {
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    db.upsert_task_snapshot(snapshot, project_id)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn collect_artifact_index(
    request: ArtifactIndexRequest,
    state: tauri::State<'_, AppState>,
) -> Result<ArtifactIndex, String> {
    let index = artifacts::collect_artifacts(request).map_err(|error| error.to_string())?;
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    db.upsert_artifact_index(&index)
        .map_err(|error| error.to_string())?;
    Ok(index)
}

#[tauri::command]
fn list_artifact_records(project_path: String, state: tauri::State<'_, AppState>) -> Result<Vec<ArtifactRecord>, String> {
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    db.list_artifact_records(project_path)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn export_simulation_report(request: ReportExportRequest) -> Result<ExportedReport, String> {
    artifacts::export_report(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn parse_analysis_results(
    request: AnalysisParseRequest,
    state: tauri::State<'_, AppState>,
) -> Result<AnalysisParseResult, String> {
    let result = analysis::parse_analysis_results(request).map_err(|error| error.to_string())?;
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    db.upsert_analysis_cache(&result)
        .map_err(|error| error.to_string())?;
    Ok(result)
}

#[tauri::command]
fn list_analysis_cache_records(
    project_path: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<AnalysisCacheRecord>, String> {
    let db = state.db.lock().map_err(|_| "project database lock poisoned".to_string())?;
    db.list_analysis_cache_records(project_path)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn index_trajectory_file(request: TrajectoryIndexRequest) -> Result<TrajectoryIndex, String> {
    trajectory::index_trajectory(request).map_err(|error| error.to_string())
}

#[tauri::command]
fn read_trajectory_chunk(request: TrajectoryChunkRequest) -> Result<TrajectoryChunk, String> {
    trajectory::read_trajectory_chunk(request).map_err(|error| error.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join(".automd"));
            std::fs::create_dir_all(&app_dir)?;
            let project_root = app_dir.join("projects");
            std::fs::create_dir_all(&project_root)?;
            let plugin_root = app_dir.join("plugins");
            std::fs::create_dir_all(&plugin_root)?;
            let db = ProjectDatabase::open(app_dir.join("automd.sqlite"))
                .map_err(|error| Box::<dyn std::error::Error>::from(error))?;
            app.manage(AppState {
                db: Mutex::new(db),
                project_root,
                plugin_root,
                task_manager: TaskManager::new(std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_engine_capabilities,
            list_engine_capabilities,
            get_runtime_diagnostics,
            get_science_sidecar_diagnostics,
            list_remote_profile_templates,
            list_remote_profiles,
            save_remote_profile,
            delete_remote_profile,
            list_engine_installations,
            save_engine_installation,
            delete_engine_installation,
            list_plugin_manifests,
            list_projects,
            create_project,
            generate_simulation_plan,
            validate_simulation_plan,
            map_engine_parameters,
            create_mock_task,
            import_structure,
            read_structure_file,
            generate_slurm_script,
            generate_remote_execution_package,
            parse_remote_job_status,
            run_remote_workflow_step,
            generate_container_recipe,
            generate_build_recipe,
            export_recipe_package,
            run_build_workflow,
            prepare_engine_run_package,
            prepare_batch_experiment,
            read_project_text_file,
            write_project_text_file,
            prepare_structure_package,
            prepare_trajectory_analysis_package,
            parse_engine_log,
            classify_engine_failure,
            discover_resume_plan,
            start_local_engine_run,
            get_local_task_snapshot,
            list_local_tasks,
            cancel_local_task,
            list_task_records,
            collect_artifact_index,
            list_artifact_records,
            export_simulation_report,
            parse_analysis_results,
            list_analysis_cache_records,
            index_trajectory_file,
            read_trajectory_chunk
        ])
        .run(tauri::generate_context!())
        .expect("error while running AutoMD");
}
