export type EngineCategory = "biomolecular" | "materials" | "quantum" | "hybrid";
export type EngineMaturity = "firstClass" | "supported" | "preview" | "externalOnly";
export type LicenseClass = "openSource" | "freeToolkit" | "restrictedAcademic" | "commercial" | "mixed";
export type DistributionPolicy =
  | "bundledAllowed"
  | "installerRecipe"
  | "userInstallRequired"
  | "userLicenseRequired";
export type DetectionStatus =
  | "ready"
  | "missingInstall"
  | "missingLicense"
  | "platformUnsupported"
  | "remoteRecommended"
  | "notApplicable";
export type ExecutionMode =
  | "localProcess"
  | "condaEnvironment"
  | "container"
  | "wsl2"
  | "ssh"
  | "slurm"
  | "pbs"
  | "lsf";
export type GpuBackend = "cuda" | "rocm" | "openCl" | "metal" | "sycl" | "cpuOnly";
export type Platform = "windows" | "macos" | "linux" | "wsl2" | "remoteLinux";
export type ProjectDomain = "biomolecular" | "materials" | "qmmm";
export type ProjectStatus = "draft" | "ready" | "running" | "completed" | "failed";
export type StructureSourceKind = "pdb" | "mmcif" | "sdf" | "mol2" | "smiles" | "engineProject";
export type SimulationStageKind =
  | "structurePreparation"
  | "energyMinimization"
  | "nvtEquilibration"
  | "nptEquilibration"
  | "production"
  | "analysis";
export type AnalysisKind =
  | "rmsd"
  | "rmsf"
  | "radiusOfGyration"
  | "hydrogenBonds"
  | "distances"
  | "angles"
  | "dihedrals"
  | "contacts"
  | "energyTerms";
export type ValidationSeverity = "info" | "warning" | "error";
export type ValidationStatus = "valid" | "validWithWarnings" | "invalid";
export type ParameterMappingStatus = "mapped" | "approximated" | "unsupported" | "manualReview";
export type TaskStatus = "queued" | "preparing" | "running" | "completed" | "failed" | "cancelled";
export type LocalRunMode = "dryRun" | "mock" | "real";

export interface LicensePolicy {
  class: LicenseClass;
  distribution: DistributionPolicy;
  bundledByAutomd: boolean;
  requiresUserLicense: boolean;
  guidance: string;
}

export interface PlatformSupport {
  native: Platform[];
  recommendedFallbacks: Platform[];
}

export interface DetectionState {
  status: DetectionStatus;
  path?: string | null;
  version?: string | null;
  message: string;
}

export interface EngineCapability {
  id: string;
  name: string;
  category: EngineCategory;
  maturity: EngineMaturity;
  license: LicensePolicy;
  platformSupport: PlatformSupport;
  executableNames: string[];
  gpuBackends: GpuBackend[];
  executionModes: ExecutionMode[];
  supportedInputs: string[];
  supportedOutputs: string[];
  supportedStages: SimulationStageKind[];
  detection: DetectionState;
  docsUrl: string;
  notes: string[];
}

export interface EngineInstallationRecord {
  engineId: string;
  location: string;
  version?: string | null;
  authorizationStatus: DetectionStatus;
  checkedAt: string;
}

export type PluginKind =
  | "engineAdapter"
  | "analysisModule"
  | "remoteScheduler"
  | "buildRecipe"
  | "reportTemplate";

export interface PluginManifest {
  id: string;
  name: string;
  version: string;
  kind: PluginKind;
  entrypoint: string;
  engineId?: string | null;
  capabilities: string[];
  licensePolicy?: string | null;
  warnings: string[];
  sourcePath?: string | null;
}

export interface PluginRegistrySnapshot {
  pluginRoot: string;
  manifests: PluginManifest[];
  warnings: string[];
}

export interface ProjectSummary {
  id: string;
  name: string;
  domain: ProjectDomain;
  path: string;
  createdAt: string;
  lastOpenedAt?: string | null;
  preferredEngineId?: string | null;
  status: ProjectStatus;
}

export interface CreateProjectRequest {
  name: string;
  domain: ProjectDomain;
  preferredEngineId?: string | null;
  projectRoot?: string | null;
}

export interface SystemSpec {
  sourceKind: StructureSourceKind;
  sourcePath?: string | null;
  name: string;
  moleculeCount?: number | null;
  hasLigand: boolean;
  hasMembrane: boolean;
  notes: string[];
}

export interface StructureImportRequest {
  projectPath: string;
  sourceKind: StructureSourceKind;
  sourcePath?: string | null;
  smiles?: string | null;
  displayName?: string | null;
  overwrite: boolean;
}

export interface StructureSummary {
  atomCount?: number | null;
  residueCount?: number | null;
  chainCount?: number | null;
  moleculeCount?: number | null;
  modelCount?: number | null;
  formatNote: string;
}

export interface StructureImportResult {
  system: SystemSpec;
  importedPath: string;
  summary: StructureSummary;
  warnings: string[];
  importedAt: string;
}

export interface ImportedStructureEntry {
  id: string;
  name: string;
  sourcePath?: string | null;
  importedPath: string;
  sourceKind: StructureSourceKind;
  importedAt: string;
  summary?: StructureSummary | null;
}

export interface DeleteImportedStructureRequest {
  projectPath: string;
  importedPath: string;
}

export interface StructureFileRequest {
  projectPath: string;
  sourcePath: string;
}

export interface StructureFilePayload {
  sourcePath: string;
  format: string;
  contents: string;
  sizeBytes: number;
}

export interface ForceFieldSpec {
  protein: string;
  waterModel: string;
  ligand?: string | null;
  ions: string;
}

export interface SolventSpec {
  model: string;
  boxShape: string;
  paddingNm: number;
  ionicStrengthMolar: number;
  neutralize: boolean;
}

export interface ResourceSpec {
  executionMode: ExecutionMode;
  cpuThreads: number;
  gpuCount: number;
  mpiRanks: number;
  walltimeHours: number;
  remoteProfileId?: string | null;
  queue?: string | null;
}

export interface GpuAvailability {
  available: boolean;
  mode: "gpu" | "cpuFallback";
  backend?: GpuBackend | null;
  label: string;
  reason: string;
  detail: string;
  checkedAt: string;
}

export interface SimulationStage {
  id: string;
  kind: SimulationStageKind;
  label: string;
  enabled: boolean;
  parameters: Record<string, string>;
  expectedOutputs: string[];
}

export interface AnalysisModule {
  kind: AnalysisKind;
  enabled: boolean;
  parameters: Record<string, string>;
}

export interface OutputSpec {
  generatedInputs: string[];
  runLogs: string[];
  checkpoints: string[];
  trajectories: string[];
  energy: string[];
  analysisTables: string[];
  reports: string[];
}

export interface SimulationPlan {
  id: string;
  projectId?: string | null;
  name: string;
  engineId: string;
  system: SystemSpec;
  forceField: ForceFieldSpec;
  solvent: SolventSpec;
  resources: ResourceSpec;
  stages: SimulationStage[];
  outputs: OutputSpec;
  analysis: AnalysisModule[];
  createdAt: string;
}

export interface PlanRequest {
  projectId?: string | null;
  name: string;
  engineId: string;
  domain: ProjectDomain;
}

export interface ValidationItem {
  severity: ValidationSeverity;
  field: string;
  message: string;
}

export interface ValidationReport {
  status: ValidationStatus;
  items: ValidationItem[];
}

export interface ParameterMappingRequest {
  plan: SimulationPlan;
  engineId?: string | null;
}

export interface ParameterMappingItem {
  stageId: string;
  stageLabel: string;
  normalizedKey: string;
  normalizedValue: string;
  engineKey: string;
  engineValue: string;
  targetFile: string;
  status: ParameterMappingStatus;
  notes: string[];
}

export interface ParameterMappingReport {
  engineId: string;
  planId: string;
  items: ParameterMappingItem[];
  warnings: string[];
  generatedAt: string;
}

export interface SimulationTask {
  id: string;
  planId: string;
  engineId: string;
  status: TaskStatus;
  currentStage?: SimulationStageKind | null;
  progressPercent: number;
  nsPerDay?: number | null;
  logTail: string[];
  createdAt: string;
}

export interface TaskRecord {
  id: string;
  projectId?: string | null;
  planId: string;
  engineId: string;
  status: TaskStatus;
  currentStage?: SimulationStageKind | null;
  progressPercent: number;
  createdAt: string;
  updatedAt: string;
}

export interface RemoteProfile {
  id: string;
  name: string;
  host: string;
  scheduler: ExecutionMode;
  workdir: string;
  moduleLoad: string[];
  defaultQueue?: string | null;
}

export interface RemoteExecutionRequest {
  plan: SimulationPlan;
  profile: RemoteProfile;
  localProjectPath?: string | null;
  includeSubmit: boolean;
}

export interface RemoteCommand {
  id: string;
  label: string;
  command: string;
  description: string;
}

export interface RemoteExecutionPackage {
  engineId: string;
  scheduler: ExecutionMode;
  profileId: string;
  remoteWorkdir: string;
  runDirectory: string;
  files: GeneratedFile[];
  commands: RemoteCommand[];
  warnings: string[];
}

export interface RemoteStatusParseRequest {
  engineId: string;
  scheduler: ExecutionMode;
  submitOutput?: string | null;
  statusOutput?: string | null;
  logOutput?: string | null;
}

export interface RemoteJobSnapshot {
  scheduler: ExecutionMode;
  jobId?: string | null;
  status: TaskStatus;
  queueState?: string | null;
  reason?: string | null;
  progressPercent?: number | null;
  nsPerDay?: number | null;
  currentStep?: number | null;
  logReport?: EngineLogReport | null;
  warnings: string[];
  generatedAt: string;
}

export type RemoteWorkflowMode = "dryRun" | "writeFiles" | "execute";

export interface RemoteWorkflowStepRequest {
  projectPath: string;
  package: RemoteExecutionPackage;
  stepId: string;
  mode: RemoteWorkflowMode;
  jobId?: string | null;
  timeoutSeconds?: number | null;
}

export interface RemoteWorkflowStepResult {
  stepId: string;
  label: string;
  command: string;
  mode: RemoteWorkflowMode;
  filesWritten: string[];
  status: TaskStatus;
  exitCode?: number | null;
  stdout: string;
  stderr: string;
  snapshot?: RemoteJobSnapshot | null;
  startedAt: string;
  finishedAt?: string | null;
  durationMs?: number | null;
  warnings: string[];
}

export interface ToolDiagnostic {
  id: string;
  label: string;
  command: string;
  status: DetectionStatus;
  detail: string;
}

export interface FilePickRequest {
  title?: string | null;
  extensions: string[];
}

export interface ExecutableSearchRequest {
  commands: string[];
  extraDirs: string[];
}

export interface ExecutableSearchResult {
  found: boolean;
  command?: string | null;
  path?: string | null;
  checkedLocations: string[];
  message: string;
}

export interface RuntimeDiagnostics {
  os: string;
  arch: string;
  tools: ToolDiagnostic[];
  gpu: GpuAvailability;
}

export interface ScienceToolDiagnostic {
  id: string;
  label: string;
  importName?: string | null;
  command?: string | null;
  status: DetectionStatus;
  version?: string | null;
  detail: string;
}

export interface ScienceSidecarDiagnostics {
  pythonExecutable?: string | null;
  tools: ScienceToolDiagnostic[];
  environmentRecipe: string;
  warnings: string[];
}

export interface ScienceToolInspectRequest {
  id: string;
  label: string;
  importName?: string | null;
  command?: string | null;
  executablePath: string;
}

export interface StructurePreparationRequest {
  plan: SimulationPlan;
  projectPath?: string | null;
  writeToDisk: boolean;
}

export interface StructurePreparationPackage {
  planId: string;
  generatedDirectory: string;
  commands: EngineCommand[];
  files: EngineRunFile[];
  warnings: string[];
  writable: boolean;
}

export interface TrajectoryAnalysisRequest {
  plan: SimulationPlan;
  projectPath?: string | null;
  topologyPath?: string | null;
  trajectoryPath?: string | null;
  selection: string;
  writeToDisk: boolean;
}

export interface TrajectoryAnalysisPackage {
  planId: string;
  generatedDirectory: string;
  commands: EngineCommand[];
  files: EngineRunFile[];
  expectedOutputs: string[];
  warnings: string[];
  writable: boolean;
}

export interface GeneratedFile {
  path: string;
  language: string;
  contents: string;
}

export interface ContainerRecipe {
  engineId: string;
  title: string;
  files: GeneratedFile[];
  notes: string[];
}

export interface BuildRecipeOptions {
  engineId: string;
  enableMpi: boolean;
  enableGpu: boolean;
  gpuBackend?: GpuBackend | null;
  enablePlumed: boolean;
  installPrefix?: string | null;
}

export interface BuildRecipe {
  engineId: string;
  title: string;
  script: string;
  steps: string[];
  warnings: string[];
}

export interface RecipeExportRequest {
  projectPath: string;
  buildOptions: BuildRecipeOptions;
  includeContainer: boolean;
  includeBuildScript: boolean;
}

export interface RecipeExportResult {
  engineId: string;
  directory: string;
  files: GeneratedFile[];
  warnings: string[];
}

export type BuildWorkflowMode = "dryRun" | "writeFiles" | "execute";

export interface BuildWorkflowRequest {
  projectPath: string;
  buildOptions: BuildRecipeOptions;
  includeContainer: boolean;
  includeBuildScript: boolean;
  mode: BuildWorkflowMode;
  timeoutSeconds?: number | null;
}

export interface BuildWorkflowResult {
  engineId: string;
  directory: string;
  command: string;
  mode: BuildWorkflowMode;
  filesWritten: string[];
  status: TaskStatus;
  exitCode?: number | null;
  stdout: string;
  stderr: string;
  logPath?: string | null;
  failureAnalysis?: FailureAnalysis | null;
  startedAt: string;
  finishedAt?: string | null;
  durationMs?: number | null;
  warnings: string[];
}

export interface EngineRunRequest {
  plan: SimulationPlan;
  projectPath?: string | null;
  writeToDisk: boolean;
}

export interface EngineCommand {
  stageId: string;
  label: string;
  command: string;
  workingDirectory: string;
  expectedOutputs: string[];
}

export interface EngineRunFile {
  path: string;
  language: string;
  contents: string;
  written: boolean;
}

export interface EngineRunPackage {
  engineId: string;
  planId: string;
  runDirectory: string;
  commands: EngineCommand[];
  files: EngineRunFile[];
  warnings: string[];
  writable: boolean;
}

export interface BatchExperimentRequest {
  plan: SimulationPlan;
  projectPath?: string | null;
  replicateCount: number;
  seedStart: number;
  writeToDisk: boolean;
}

export interface BatchReplicaPlan {
  replicaIndex: number;
  seed: number;
  plan: SimulationPlan;
  runDirectory: string;
}

export interface BatchExperimentPackage {
  engineId: string;
  planId: string;
  generatedDirectory: string;
  replicas: BatchReplicaPlan[];
  files: EngineRunFile[];
  commands: EngineCommand[];
  warnings: string[];
  writable: boolean;
}

export interface ProjectTextFileRequest {
  projectPath: string;
  path: string;
}

export interface ProjectTextFileWriteRequest {
  projectPath: string;
  path: string;
  contents: string;
}

export interface ProjectTextFilePayload {
  path: string;
  language: string;
  contents: string;
  sizeBytes: number;
  modifiedAt?: string | null;
}

export interface EngineLogParseRequest {
  engineId: string;
  logContents: string;
}

export type EngineLogEventKind = "progress" | "performance" | "warning" | "error" | "checkpoint" | "info";

export interface EngineLogEvent {
  kind: EngineLogEventKind;
  lineNumber: number;
  message: string;
}

export interface EngineLogReport {
  engineId: string;
  progressPercent?: number | null;
  nsPerDay?: number | null;
  currentStep?: number | null;
  events: EngineLogEvent[];
  fatalError?: string | null;
}

export type FailureCategory =
  | "missingExecutable"
  | "missingInput"
  | "missingTopology"
  | "parameterMismatch"
  | "missingForceField"
  | "licenseRequired"
  | "gpuUnavailable"
  | "mpiFailure"
  | "numericalInstability"
  | "diskOrPermission"
  | "schedulerFailure"
  | "unknown";

export interface FailureSuggestion {
  title: string;
  detail: string;
  actionLabel: string;
  commandHint?: string | null;
}

export interface FailureAnalysis {
  engineId: string;
  category: FailureCategory;
  severity: ValidationSeverity;
  message: string;
  suggestions: FailureSuggestion[];
}

export interface FailureAnalysisRequest {
  engineId: string;
  logContents: string;
  exitCode?: number | null;
}

export interface CheckpointCandidate {
  path: string;
  sizeBytes: number;
  modifiedAt?: string | null;
  stageHint?: string | null;
  commandHint?: string | null;
}

export interface ResumePlanRequest {
  projectPath: string;
  runDirectory: string;
  engineId: string;
}

export interface ResumePlan {
  engineId: string;
  runDirectory: string;
  checkpoints: CheckpointCandidate[];
  recommended?: CheckpointCandidate | null;
  resumeCommand?: string | null;
  warnings: string[];
}

export interface StartLocalRunRequest {
  plan: SimulationPlan;
  projectPath?: string | null;
  mode: LocalRunMode;
  writePackage: boolean;
}

export interface LocalTaskSnapshot {
  id: string;
  planId: string;
  engineId: string;
  mode: LocalRunMode;
  status: TaskStatus;
  runDirectory: string;
  command: string;
  progressPercent: number;
  nsPerDay?: number | null;
  currentStep?: number | null;
  logTail: string[];
  errorMessage?: string | null;
  exitCode?: number | null;
  artifacts: RunArtifact[];
  reportPath?: string | null;
  failureAnalysis?: FailureAnalysis | null;
  resumePlan?: ResumePlan | null;
  startedAt: string;
  finishedAt?: string | null;
}

export type ArtifactKind =
  | "input"
  | "generatedInput"
  | "runLog"
  | "checkpoint"
  | "trajectory"
  | "energy"
  | "analysisTable"
  | "figure"
  | "report"
  | "metadata"
  | "other";

export interface RunArtifact {
  path: string;
  kind: ArtifactKind;
  sizeBytes: number;
  modifiedAt?: string | null;
  summary?: string | null;
}

export interface ArtifactIndexRequest {
  projectPath: string;
  runDirectory?: string | null;
}

export interface ArtifactIndex {
  projectPath: string;
  runDirectory?: string | null;
  artifacts: RunArtifact[];
  generatedAt: string;
}

export interface ArtifactRecord {
  projectPath: string;
  path: string;
  kind: ArtifactKind;
  sizeBytes: number;
  modifiedAt?: string | null;
  summary?: string | null;
  runDirectory?: string | null;
  indexedAt: string;
}

export interface AnalysisParseRequest {
  projectPath: string;
  artifactPaths?: string[] | null;
  maxPoints?: number | null;
}

export interface AnalysisPoint {
  x: number;
  y: number;
}

export interface AnalysisSeries {
  path: string;
  label: string;
  xLabel: string;
  yLabel: string;
  points: AnalysisPoint[];
  minY?: number | null;
  maxY?: number | null;
  lastY?: number | null;
}

export interface AnalysisParseResult {
  projectPath: string;
  series: AnalysisSeries[];
  warnings: string[];
  generatedAt: string;
}

export interface AnalysisCacheRecord {
  projectPath: string;
  path: string;
  label: string;
  xLabel: string;
  yLabel: string;
  pointCount: number;
  minY?: number | null;
  maxY?: number | null;
  lastY?: number | null;
  generatedAt: string;
}

export type TrajectoryFormat =
  | "pdb"
  | "xyz"
  | "lammpsDump"
  | "xtc"
  | "trr"
  | "dcd"
  | "netcdf"
  | "gsd"
  | "unknown";

export type TrajectoryIndexStrategy = "textOffsets" | "metadataOnly" | "unsupported";

export interface TrajectoryFrameDescriptor {
  frameIndex: number;
  byteStart: number;
  byteEnd: number;
  atomCount?: number | null;
  timePs?: number | null;
  label: string;
}

export interface TrajectoryIndexRequest {
  projectPath: string;
  trajectoryPath: string;
  frameStride?: number | null;
  maxPreviewFrames?: number | null;
  writeIndex: boolean;
}

export interface TrajectoryIndex {
  projectPath: string;
  trajectoryPath: string;
  format: TrajectoryFormat;
  strategy: TrajectoryIndexStrategy;
  sizeBytes: number;
  frameCount?: number | null;
  sampledFrames: TrajectoryFrameDescriptor[];
  indexPath?: string | null;
  warnings: string[];
  generatedAt: string;
}

export interface TrajectoryChunkRequest {
  projectPath: string;
  trajectoryPath: string;
  frameIndices?: number[] | null;
  startFrame?: number | null;
  frameCount?: number | null;
  maxBytes?: number | null;
}

export interface TrajectoryFramePayload {
  frameIndex: number;
  label: string;
  format: TrajectoryFormat;
  contents: string;
  atomCount?: number | null;
  timePs?: number | null;
}

export interface TrajectoryChunk {
  projectPath: string;
  trajectoryPath: string;
  frames: TrajectoryFramePayload[];
  truncated: boolean;
  warnings: string[];
  generatedAt: string;
}

export type ReportFormat = "markdown" | "html" | "pdf";

export interface ReportExportRequest {
  projectPath: string;
  plan: SimulationPlan;
  task?: LocalTaskSnapshot | null;
  artifactIndex?: ArtifactIndex | null;
  format: ReportFormat;
}

export interface ExportedReport {
  path: string;
  format: ReportFormat;
  contents: string;
}
