import { invoke } from "@tauri-apps/api/core";
import type {
  BuildRecipe,
  BuildRecipeOptions,
  AnalysisCacheRecord,
  AnalysisParseRequest,
  AnalysisParseResult,
  BatchExperimentPackage,
  BatchExperimentRequest,
  ContainerRecipe,
  BuildWorkflowRequest,
  BuildWorkflowResult,
  CreateProjectRequest,
  EngineCapability,
  EngineInstallationRecord,
  EngineLogParseRequest,
  EngineLogReport,
  EngineRunPackage,
  EngineRunRequest,
  FailureAnalysis,
  FailureAnalysisRequest,
  ArtifactIndex,
  ArtifactIndexRequest,
  ArtifactRecord,
  ExportedReport,
  LocalTaskSnapshot,
  ParameterMappingReport,
  ParameterMappingRequest,
  PlanRequest,
  PluginRegistrySnapshot,
  ProjectTextFilePayload,
  ProjectTextFileRequest,
  ProjectTextFileWriteRequest,
  ProjectSummary,
  RemoteExecutionPackage,
  RemoteExecutionRequest,
  RemoteJobSnapshot,
  RemoteProfile,
  RemoteWorkflowStepRequest,
  RemoteWorkflowStepResult,
  RecipeExportRequest,
  RecipeExportResult,
  RemoteStatusParseRequest,
  RuntimeDiagnostics,
  ScienceSidecarDiagnostics,
  SimulationPlan,
  SimulationTask,
  ReportExportRequest,
  ResumePlan,
  ResumePlanRequest,
  StartLocalRunRequest,
  StructurePreparationPackage,
  StructurePreparationRequest,
  StructureFilePayload,
  StructureFileRequest,
  StructureImportRequest,
  StructureImportResult,
  TaskRecord,
  TrajectoryAnalysisPackage,
  TrajectoryAnalysisRequest,
  TrajectoryChunk,
  TrajectoryChunkRequest,
  TrajectoryIndex,
  TrajectoryIndexRequest,
  ValidationReport
} from "../types";
import {
  mockBuildRecipe,
  mockBuildWorkflow,
  mockAnalysisResults,
  mockBatchExperimentPackage,
  mockContainerRecipe,
  mockCreateProject,
  mockRecipeExportResult,
  mockDiagnostics,
  mockRemoteExecutionPackage,
  mockRemoteJobSnapshot,
  mockRemoteWorkflowStep,
  mockRemoteProfiles,
  mockEngines,
  mockEngineInstallations,
  mockScienceSidecarDiagnostics,
  mockArtifactIndex,
  mockArtifactRecords,
  mockAnalysisCacheRecords,
  mockExportReport,
  mockClassifyFailure,
  mockDiscoverResumePlan,
  mockParseLog,
  mockParameterMapping,
  mockPlan,
  mockPluginRegistry,
  mockProjectTextFile,
  mockRunPackage,
  mockStructurePreparationPackage,
  mockSlurm,
  mockStructureFile,
  mockStartLocalRun,
  mockStructureImport,
  mockTaskRecords,
  mockTrajectoryAnalysisPackage,
  mockTrajectoryChunk,
  mockTrajectoryIndex,
  mockTask,
  mockValidate
} from "./mockData";

const isTauri = () => "__TAURI_INTERNALS__" in window;

async function call<T>(command: string, args?: Record<string, unknown>, fallback?: () => T): Promise<T> {
  if (!isTauri()) {
    if (!fallback) {
      throw new Error(`Command ${command} is unavailable outside Tauri`);
    }
    return fallback();
  }
  return invoke<T>(command, args);
}

export const api = {
  engineCapabilities: () =>
    call<EngineCapability[]>("list_engine_capabilities", undefined, () => mockEngines),
  listEngineInstallations: () =>
    call<EngineInstallationRecord[]>("list_engine_installations", undefined, () => mockEngineInstallations),
  saveEngineInstallation: (record: EngineInstallationRecord) =>
    call<EngineInstallationRecord>("save_engine_installation", { record }, () => record),
  deleteEngineInstallation: (engineId: string, location: string) =>
    call<boolean>("delete_engine_installation", { engineId, location }, () => true),
  runtimeDiagnostics: () =>
    call<RuntimeDiagnostics>("get_runtime_diagnostics", undefined, () => mockDiagnostics),
  scienceSidecarDiagnostics: () =>
    call<ScienceSidecarDiagnostics>("get_science_sidecar_diagnostics", undefined, () =>
      mockScienceSidecarDiagnostics
    ),
  remoteProfiles: () =>
    call<RemoteProfile[]>("list_remote_profiles", undefined, () => mockRemoteProfiles),
  saveRemoteProfile: (profile: RemoteProfile) =>
    call<RemoteProfile>("save_remote_profile", { profile }, () => profile),
  deleteRemoteProfile: (id: string) =>
    call<boolean>("delete_remote_profile", { id }, () => true),
  pluginManifests: () =>
    call<PluginRegistrySnapshot>("list_plugin_manifests", undefined, () => mockPluginRegistry),
  openPluginFolder: () =>
    call<boolean>("open_plugin_folder", undefined, () => true),
  openPath: (path: string) =>
    call<boolean>("open_path_in_system", { path }, () => true),
  listProjects: () =>
    call<ProjectSummary[]>("list_projects", undefined, () => []),
  createProject: (request: CreateProjectRequest) =>
    call<ProjectSummary>("create_project", { request }, () => mockCreateProject(request)),
  deleteProject: (id: string) =>
    call<boolean>("delete_project", { id }, () => true),
  generatePlan: (request: PlanRequest) =>
    call<SimulationPlan>("generate_simulation_plan", { request }, () => mockPlan(request)),
  validatePlan: (plan: SimulationPlan) =>
    call<ValidationReport>("validate_simulation_plan", { plan }, () => mockValidate(plan)),
  mapEngineParameters: (request: ParameterMappingRequest) =>
    call<ParameterMappingReport>("map_engine_parameters", { request }, () => mockParameterMapping(request)),
  createMockTask: (plan: SimulationPlan) =>
    call<SimulationTask>("create_mock_task", { plan }, () => mockTask(plan)),
  importStructure: (request: StructureImportRequest) =>
    call<StructureImportResult>("import_structure", { request }, () => mockStructureImport(request)),
  readStructureFile: (request: StructureFileRequest) =>
    call<StructureFilePayload>("read_structure_file", { request }, () => mockStructureFile(request)),
  slurmScript: (plan: SimulationPlan) =>
    call<string>("generate_slurm_script", { plan }, () => mockSlurm(plan)),
  remoteExecutionPackage: (request: RemoteExecutionRequest) =>
    call<RemoteExecutionPackage>("generate_remote_execution_package", { request }, () =>
      mockRemoteExecutionPackage(request)
    ),
  parseRemoteJobStatus: (request: RemoteStatusParseRequest) =>
    call<RemoteJobSnapshot>("parse_remote_job_status", { request }, () => mockRemoteJobSnapshot(request)),
  runRemoteWorkflowStep: (request: RemoteWorkflowStepRequest) =>
    call<RemoteWorkflowStepResult>("run_remote_workflow_step", { request }, () => mockRemoteWorkflowStep(request)),
  containerRecipe: (engineId: string) =>
    call<ContainerRecipe>("generate_container_recipe", { engineId }, () => mockContainerRecipe(engineId)),
  buildRecipe: (options: BuildRecipeOptions) =>
    call<BuildRecipe>("generate_build_recipe", { options }, () => mockBuildRecipe(options)),
  exportRecipePackage: (request: RecipeExportRequest) =>
    call<RecipeExportResult>("export_recipe_package", { request }, () => mockRecipeExportResult(request)),
  runBuildWorkflow: (request: BuildWorkflowRequest) =>
    call<BuildWorkflowResult>("run_build_workflow", { request }, () => mockBuildWorkflow(request)),
  prepareRunPackage: (request: EngineRunRequest) =>
    call<EngineRunPackage>("prepare_engine_run_package", { request }, () => mockRunPackage(request)),
  prepareBatchExperiment: (request: BatchExperimentRequest) =>
    call<BatchExperimentPackage>("prepare_batch_experiment", { request }, () => mockBatchExperimentPackage(request)),
  readProjectTextFile: (request: ProjectTextFileRequest) =>
    call<ProjectTextFilePayload>("read_project_text_file", { request }, () => mockProjectTextFile(request)),
  writeProjectTextFile: (request: ProjectTextFileWriteRequest) =>
    call<ProjectTextFilePayload>("write_project_text_file", { request }, () => ({
      path: request.path,
      language: "text",
      contents: request.contents,
      sizeBytes: request.contents.length,
      modifiedAt: new Date().toISOString()
    })),
  prepareStructurePackage: (request: StructurePreparationRequest) =>
    call<StructurePreparationPackage>("prepare_structure_package", { request }, () =>
      mockStructurePreparationPackage(request)
    ),
  prepareTrajectoryAnalysisPackage: (request: TrajectoryAnalysisRequest) =>
    call<TrajectoryAnalysisPackage>("prepare_trajectory_analysis_package", { request }, () =>
      mockTrajectoryAnalysisPackage(request)
    ),
  parseEngineLog: (request: EngineLogParseRequest) =>
    call<EngineLogReport>("parse_engine_log", { request }, () => mockParseLog(request)),
  classifyFailure: (request: FailureAnalysisRequest) =>
    call<FailureAnalysis>("classify_engine_failure", { request }, () => mockClassifyFailure(request)),
  discoverResumePlan: (request: ResumePlanRequest) =>
    call<ResumePlan>("discover_resume_plan", { request }, () => mockDiscoverResumePlan(request)),
  startLocalRun: (request: StartLocalRunRequest) =>
    call<LocalTaskSnapshot>("start_local_engine_run", { request }, () => mockStartLocalRun(request)),
  getLocalTask: (taskId: string) =>
    call<LocalTaskSnapshot>("get_local_task_snapshot", { taskId }, () =>
      mockStartLocalRun({
        plan: mockPlan({ name: "mock", engineId: "gromacs", domain: "biomolecular" }),
        mode: "mock",
        writePackage: false
      })
    ),
  listLocalTasks: () =>
    call<LocalTaskSnapshot[]>("list_local_tasks", undefined, () => []),
  listTaskRecords: (projectId?: string | null) =>
    call<TaskRecord[]>("list_task_records", { projectId: projectId ?? null }, () => mockTaskRecords(projectId)),
  cancelLocalTask: (taskId: string) =>
    call<LocalTaskSnapshot>("cancel_local_task", { taskId }, () => {
      const snapshot = mockStartLocalRun({
        plan: mockPlan({ name: "mock", engineId: "gromacs", domain: "biomolecular" }),
        mode: "mock",
        writePackage: false
      });
      return { ...snapshot, id: taskId, status: "cancelled", finishedAt: new Date().toISOString() };
    }),
  collectArtifactIndex: (request: ArtifactIndexRequest) =>
    call<ArtifactIndex>("collect_artifact_index", { request }, () => mockArtifactIndex(request)),
  listArtifactRecords: (projectPath: string) =>
    call<ArtifactRecord[]>("list_artifact_records", { projectPath }, () => mockArtifactRecords(projectPath)),
  exportReport: (request: ReportExportRequest) =>
    call<ExportedReport>("export_simulation_report", { request }, () => mockExportReport(request)),
  parseAnalysisResults: (request: AnalysisParseRequest) =>
    call<AnalysisParseResult>("parse_analysis_results", { request }, () => mockAnalysisResults(request)),
  listAnalysisCacheRecords: (projectPath: string) =>
    call<AnalysisCacheRecord[]>("list_analysis_cache_records", { projectPath }, () => mockAnalysisCacheRecords(projectPath)),
  indexTrajectory: (request: TrajectoryIndexRequest) =>
    call<TrajectoryIndex>("index_trajectory_file", { request }, () => mockTrajectoryIndex(request)),
  readTrajectoryChunk: (request: TrajectoryChunkRequest) =>
    call<TrajectoryChunk>("read_trajectory_chunk", { request }, () => mockTrajectoryChunk(request))
};
