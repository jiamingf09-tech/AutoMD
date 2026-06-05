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
  EngineDeployRequest,
  EngineDeployResult,
  EngineInstallationRecord,
  EngineTarget,
  EngineLogParseRequest,
  EngineLogReport,
  EngineRunPackage,
  EngineRunRequest,
  ExecutableSearchRequest,
  ExecutableSearchResult,
  FailureAnalysis,
  FailureAnalysisRequest,
  FilePickRequest,
  DeleteImportedStructureRequest,
  ImportedStructureEntry,
  ArtifactIndex,
  ArtifactIndexRequest,
  ArtifactRecord,
  ExportedReport,
  LocalTaskSnapshot,
  ParameterMappingReport,
  ParameterMappingRequest,
  PlanRequest,
  PluginConfigRequest,
  PluginImportRequest,
  PluginRunRequest,
  PluginRunResult,
  PluginRegistrySnapshot,
  PluginTemplateRequest,
  ProjectTextFilePayload,
  ProjectTextFileRequest,
  ProjectTextFileWriteRequest,
  ProjectSummary,
  RemoteConnectionTest,
  RemoteExecutionPackage,
  RemoteExecutionRequest,
  RemoteFetchRequest,
  RemoteFetchResult,
  RemoteHelperStatus,
  RemoteJobSnapshot,
  RemoteJobSubmission,
  RemotePollRequest,
  RemotePreflightRequest,
  RemoteProfile,
  RemoteSubmitPreflight,
  RemoteSubmitRequest,
  RemoteWorkflowStepRequest,
  RemoteWorkflowStepResult,
  RecipeExportRequest,
  RecipeExportResult,
  RemoteStatusParseRequest,
  RuntimeDiagnostics,
  ScienceSidecarDiagnostics,
  ScienceToolDiagnostic,
  ScienceToolInspectRequest,
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
  mockRemoteConnectionTest,
  mockRemoteExecutionPackage,
  mockRemoteJobSnapshot,
  mockRemotePreflight,
  mockRemoteSubmission,
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
  engineTargets: () =>
    call<EngineTarget[]>("list_engine_targets", undefined, () => [
      {
        id: "local",
        kind: "local",
        profileId: null,
        label: "本机",
        detail: "web-preview · browser",
        status: "ready",
        platform: null,
        arch: "browser",
        hostname: null
      },
      ...mockRemoteProfiles.map((profile) => ({
        id: `remote:${profile.id}`,
        kind: "remote" as const,
        profileId: profile.id,
        label: profile.name,
        detail: `${profile.host} · 未安装 helper`,
        status: "missing" as const,
        platform: null,
        arch: null,
        hostname: null
      }))
    ]),
  engineCapabilitiesForTarget: (targetId: string) =>
    call<EngineCapability[]>("list_engine_capabilities_for_target", { targetId }, () =>
      mockEngines.map((engine) =>
        targetId === "local"
          ? engine
          : {
              ...engine,
              detection: {
                status: "missingInstall",
                path: null,
                version: null,
                message: "Web 预览模式：远程 helper 未安装。"
              }
            }
      )
    ),
  listEngineInstallations: () =>
    call<EngineInstallationRecord[]>("list_engine_installations", undefined, () => mockEngineInstallations),
  saveEngineInstallation: (record: EngineInstallationRecord) =>
    call<EngineInstallationRecord>("save_engine_installation", { record }, () => record),
  deleteEngineInstallation: (engineId: string, location: string) =>
    call<boolean>("delete_engine_installation", { engineId, location }, () => true),
  deleteEngineInstallationForTarget: (targetId: string, engineId: string, location: string) =>
    call<boolean>("delete_engine_installation_for_target", { targetId, engineId, location }, () => true),
  scanEnginesOnTarget: (targetId: string) =>
    call<EngineCapability[]>("scan_engines_on_target", { targetId }, () => mockEngines),
  installRemoteHelper: (profileId: string) =>
    call<RemoteHelperStatus>("install_remote_helper", { profileId }, () => ({
      profileId,
      helperVersion: "0.1.0",
      status: "ready",
      installPath: `/mock/${profileId}/.automd/helper/0.1.0`,
      platform: "linux",
      arch: "x86_64",
      hostname: "mock-host",
      hardwareJson: "{\"cpuCount\":32}",
      checkedAt: new Date().toISOString(),
      lastError: null
    })),
  checkRemoteHelper: (profileId: string) =>
    call<RemoteHelperStatus>("check_remote_helper", { profileId }, () => ({
      profileId,
      helperVersion: "0.1.0",
      status: "ready",
      installPath: `/mock/${profileId}/.automd/helper/0.1.0`,
      platform: "linux",
      arch: "x86_64",
      hostname: "mock-host",
      hardwareJson: "{\"cpuCount\":32}",
      checkedAt: new Date().toISOString(),
      lastError: null
    })),
  listInstallableEngines: () =>
    call<string[]>("list_installable_engines", undefined, () => ["gromacs", "openmm", "ambertools", "lammps", "cp2k", "hoomd"]),
  installEngine: (engineId: string) =>
    call<EngineInstallationRecord>("install_engine", { engineId }, () => ({
      targetKind: "local",
      targetId: "local",
      targetLabel: "本机",
      engineId,
      location: `/mock/engines/${engineId}/bin/${engineId}`,
      version: "conda-forge (mock)",
      authorizationStatus: "ready",
      platform: null,
      arch: "browser",
      checkedAt: new Date().toISOString()
    })),
  installOrBuildEngine: (request: EngineDeployRequest) =>
    call<EngineDeployResult>("install_or_build_engine", { request }, () => ({
      targetId: request.targetId,
      engineId: request.engineId,
      strategy: request.strategy === "auto" ? "package" : request.strategy,
      mode: request.mode,
      record: {
        targetKind: request.targetId === "local" ? "local" : "remote",
        targetId: request.targetId,
        targetLabel: request.targetId === "local" ? "本机" : "Mock remote",
        engineId: request.engineId,
        location: `/mock/engines/${request.engineId}/bin/${request.engineId}`,
        version: "mock deploy",
        authorizationStatus: "ready",
        platform: request.targetId === "local" ? null : "linux",
        arch: "x86_64",
        checkedAt: new Date().toISOString()
      },
      buildResult: null,
      status: "completed",
      stdout: "Web 预览模式：部署完成。",
      stderr: "",
      warnings: []
    })),
  listInstallableTools: () =>
    call<string[]>("list_installable_tools", undefined, () => ["conda", "mamba", "mpirun", "plumed"]),
  installTool: (toolId: string) =>
    call<string>("install_tool", { toolId }, () => `/mock/tools/${toolId}/bin/${toolId}`),
  runtimeDiagnostics: () =>
    call<RuntimeDiagnostics>("get_runtime_diagnostics", undefined, () => mockDiagnostics),
  scienceSidecarDiagnostics: () =>
    call<ScienceSidecarDiagnostics>("get_science_sidecar_diagnostics", undefined, () =>
      mockScienceSidecarDiagnostics
    ),
  installScienceSidecar: () =>
    call<ScienceSidecarDiagnostics>("install_science_sidecar", undefined, () => mockScienceSidecarDiagnostics),
  inspectScienceTool: (request: ScienceToolInspectRequest) =>
    call<ScienceToolDiagnostic>("inspect_science_tool", { request }, () => ({
      id: request.id,
      label: request.label,
      importName: request.importName ?? null,
      command: request.command ?? null,
      status: "ready",
      version: "mock",
      detail: request.executablePath
    })),
  remoteProfiles: () =>
    call<RemoteProfile[]>("list_remote_profiles", undefined, () => mockRemoteProfiles),
  saveRemoteProfile: (profile: RemoteProfile) =>
    call<RemoteProfile>("save_remote_profile", { profile }, () => profile),
  deleteRemoteProfile: (id: string) =>
    call<boolean>("delete_remote_profile", { id }, () => true),
  pluginManifests: () =>
    call<PluginRegistrySnapshot>("list_plugin_manifests", undefined, () => mockPluginRegistry),
  importPlugin: (request: PluginImportRequest) =>
    call<PluginRegistrySnapshot>("import_plugin", { request }, () => mockPluginRegistry),
  createPluginTemplate: (request: PluginTemplateRequest) =>
    call<PluginRegistrySnapshot>("create_plugin_template", { request }, () => mockPluginRegistry),
  setPluginEnabled: (pluginId: string, enabled: boolean) =>
    call<PluginRegistrySnapshot>("set_plugin_enabled", { pluginId, enabled }, () => ({
      ...mockPluginRegistry,
      manifests: mockPluginRegistry.manifests.map((manifest) =>
        manifest.id === pluginId && manifest.origin === "user" ? { ...manifest, enabled } : manifest
      )
    })),
  deletePlugin: (pluginId: string) =>
    call<PluginRegistrySnapshot>("delete_plugin", { pluginId }, () => ({
      ...mockPluginRegistry,
      manifests: mockPluginRegistry.manifests.filter((manifest) => manifest.id !== pluginId)
    })),
  savePluginConfig: (request: PluginConfigRequest) =>
    call<PluginRegistrySnapshot>("save_plugin_config", { request }, () => ({
      ...mockPluginRegistry,
      manifests: mockPluginRegistry.manifests.map((manifest) =>
        manifest.id === request.pluginId ? { ...manifest, config: request.config } : manifest
      )
    })),
  runPluginAction: (request: PluginRunRequest) =>
    call<PluginRunResult>("run_plugin_action", { request }, () => ({
      record: {
        id: globalThis.crypto?.randomUUID?.() ?? String(Date.now()),
        pluginId: request.pluginId,
        actionId: request.actionId,
        mode: request.mode,
        status: "completed",
        startedAt: new Date().toISOString(),
        finishedAt: new Date().toISOString(),
        stdoutTail: "{\"artifacts\":[],\"warnings\":[\"Web 预览模式\"]}",
        stderrTail: null
      },
      stdout: "{\"artifacts\":[],\"warnings\":[\"Web 预览模式\"]}",
      stderr: "",
      parsedOutput: { artifacts: [], warnings: ["Web 预览模式"] },
      warnings: []
    })),
  openPluginFolder: () =>
    call<boolean>("open_plugin_folder", undefined, () => true),
  openPluginInstallFolder: (pluginId: string) =>
    call<boolean>("open_plugin_install_folder", { pluginId }, () => true),
  openPath: (path: string) =>
    call<boolean>("open_path_in_system", { path }, () => true),
  pickFile: (request: FilePickRequest) =>
    call<string | null>("pick_file_in_system", { request }, () => null),
  findExecutable: (request: ExecutableSearchRequest) =>
    call<ExecutableSearchResult>("find_executable", { request }, () => ({
      found: false,
      command: null,
      path: null,
      checkedLocations: [],
      message: "Web 预览模式无法访问本机文件系统。"
    })),
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
  listImportedStructures: (projectPath: string) =>
    call<ImportedStructureEntry[]>("list_imported_structures", { projectPath }, () => []),
  deleteImportedStructure: (request: DeleteImportedStructureRequest) =>
    call<boolean>("delete_imported_structure", { request }, () => true),
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
  testRemoteConnection: (profile: RemoteProfile, password?: string | null) =>
    call<RemoteConnectionTest>("test_remote_connection", { profile, password: password ?? null }, () =>
      mockRemoteConnectionTest(profile)
    ),
  preflightRemoteSubmit: (request: RemotePreflightRequest) =>
    call<RemoteSubmitPreflight>("preflight_remote_submit", { request }, () => mockRemotePreflight(request)),
  submitRemoteJob: (request: RemoteSubmitRequest) =>
    call<RemoteJobSubmission>("submit_remote_job", { request }, () => mockRemoteSubmission(request)),
  pollRemoteJob: (request: RemotePollRequest) =>
    call<RemoteJobSnapshot>("poll_remote_job", { request }, () =>
      mockRemoteJobSnapshot({
        engineId: request.engineId,
        scheduler: request.scheduler,
        submitOutput: null,
        statusOutput: null,
        logOutput: null
      })
    ),
  cancelRemoteJob: (request: RemotePollRequest) =>
    call<string>("cancel_remote_job", { request }, () => "Web 预览模式：已模拟取消。"),
  fetchRemoteResults: (request: RemoteFetchRequest) =>
    call<RemoteFetchResult>("fetch_remote_results", { request }, () => ({
      filesDownloaded: 0,
      localDir: request.localProjectPath,
      message: "Web 预览模式：未实际下载结果。",
      warnings: []
    })),
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
