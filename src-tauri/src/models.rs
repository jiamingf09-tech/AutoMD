use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EngineCategory {
    Biomolecular,
    Materials,
    Quantum,
    Hybrid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EngineMaturity {
    FirstClass,
    Supported,
    Preview,
    ExternalOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LicenseClass {
    OpenSource,
    FreeToolkit,
    RestrictedAcademic,
    Commercial,
    Mixed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DistributionPolicy {
    BundledAllowed,
    InstallerRecipe,
    UserInstallRequired,
    UserLicenseRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LicensePolicy {
    pub class: LicenseClass,
    pub distribution: DistributionPolicy,
    pub bundled_by_automd: bool,
    pub requires_user_license: bool,
    pub guidance: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Platform {
    Windows,
    Macos,
    Linux,
    Wsl2,
    RemoteLinux,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlatformSupport {
    pub native: Vec<Platform>,
    pub recommended_fallbacks: Vec<Platform>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum GpuBackend {
    Cuda,
    Rocm,
    OpenCl,
    Metal,
    Sycl,
    CpuOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionMode {
    LocalProcess,
    CondaEnvironment,
    Container,
    Wsl2,
    Ssh,
    Slurm,
    Pbs,
    Lsf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DetectionStatus {
    Ready,
    MissingInstall,
    MissingLicense,
    PlatformUnsupported,
    RemoteRecommended,
    NotApplicable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DetectionState {
    pub status: DetectionStatus,
    pub path: Option<String>,
    pub version: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EngineCapability {
    pub id: String,
    pub name: String,
    pub category: EngineCategory,
    pub maturity: EngineMaturity,
    pub license: LicensePolicy,
    pub platform_support: PlatformSupport,
    pub executable_names: Vec<String>,
    pub gpu_backends: Vec<GpuBackend>,
    pub execution_modes: Vec<ExecutionMode>,
    pub supported_inputs: Vec<String>,
    pub supported_outputs: Vec<String>,
    pub supported_stages: Vec<SimulationStageKind>,
    pub detection: DetectionState,
    pub docs_url: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EngineTargetKind {
    Local,
    Remote,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RemoteHelperState {
    Missing,
    Ready,
    Outdated,
    Unreachable,
    PermissionDenied,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteHelperStatus {
    pub profile_id: String,
    pub helper_version: Option<String>,
    pub status: RemoteHelperState,
    pub install_path: Option<String>,
    pub platform: Option<Platform>,
    pub arch: Option<String>,
    pub hostname: Option<String>,
    pub hardware_json: Option<String>,
    pub checked_at: DateTime<Utc>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineTarget {
    pub id: String,
    pub kind: EngineTargetKind,
    pub profile_id: Option<String>,
    pub label: String,
    pub detail: String,
    pub status: RemoteHelperState,
    pub platform: Option<Platform>,
    pub arch: Option<String>,
    pub hostname: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineInstallationRecord {
    pub target_kind: EngineTargetKind,
    pub target_id: String,
    pub target_label: String,
    pub engine_id: String,
    pub location: String,
    pub version: Option<String>,
    pub authorization_status: DetectionStatus,
    pub platform: Option<Platform>,
    pub arch: Option<String>,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PluginKind {
    EngineAdapter,
    AnalysisModule,
    RemoteScheduler,
    BuildRecipe,
    ReportTemplate,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PluginOrigin {
    BuiltIn,
    User,
}

fn default_plugin_origin() -> PluginOrigin {
    PluginOrigin::User
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PluginValidationStatus {
    Valid,
    Warning,
    Error,
}

fn default_plugin_validation_status() -> PluginValidationStatus {
    PluginValidationStatus::Valid
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PluginRunMode {
    Sandbox,
    Direct,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PluginRunStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginAction {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
}

fn json_null() -> serde_json::Value {
    serde_json::Value::Null
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub kind: PluginKind,
    pub entrypoint: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    pub engine_id: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub license_policy: Option<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub source_path: Option<String>,
    #[serde(default)]
    pub supported_platforms: Vec<String>,
    #[serde(default)]
    pub integration_targets: Vec<String>,
    #[serde(default)]
    pub actions: Vec<PluginAction>,
    #[serde(default = "json_null")]
    pub config_schema: serde_json::Value,
    #[serde(default = "json_null")]
    pub default_config: serde_json::Value,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default = "default_plugin_origin")]
    pub origin: PluginOrigin,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub install_path: Option<String>,
    #[serde(default = "default_plugin_validation_status")]
    pub validation_status: PluginValidationStatus,
    #[serde(default = "json_null")]
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginRegistrySnapshot {
    pub plugin_root: String,
    pub manifests: Vec<PluginManifest>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginStateRecord {
    pub plugin_id: String,
    pub enabled: bool,
    pub config: serde_json::Value,
    pub installed_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginRunRecord {
    pub id: Uuid,
    pub plugin_id: String,
    pub action_id: String,
    pub mode: PluginRunMode,
    pub status: PluginRunStatus,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub stdout_tail: Option<String>,
    pub stderr_tail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginImportRequest {
    pub source_path: String,
    pub overwrite: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginTemplateRequest {
    pub id: String,
    pub name: String,
    pub kind: PluginKind,
    pub target: Option<String>,
    pub language: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginConfigRequest {
    pub plugin_id: String,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginRunRequest {
    pub plugin_id: String,
    pub action_id: String,
    pub mode: PluginRunMode,
    pub confirmed_direct: bool,
    pub context: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginRunResult {
    pub record: PluginRunRecord,
    pub stdout: String,
    pub stderr: String,
    pub parsed_output: Option<serde_json::Value>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ProjectDomain {
    Biomolecular,
    Materials,
    Qmmm,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ProjectStatus {
    Draft,
    Ready,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub id: Uuid,
    pub name: String,
    pub domain: ProjectDomain,
    pub path: String,
    pub created_at: DateTime<Utc>,
    pub last_opened_at: Option<DateTime<Utc>>,
    pub preferred_engine_id: Option<String>,
    pub status: ProjectStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectRequest {
    pub name: String,
    pub domain: ProjectDomain,
    pub preferred_engine_id: Option<String>,
    pub project_root: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum StructureSourceKind {
    Pdb,
    Mmcif,
    Sdf,
    Mol2,
    Smiles,
    EngineProject,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemSpec {
    pub source_kind: StructureSourceKind,
    pub source_path: Option<String>,
    pub name: String,
    pub molecule_count: Option<u32>,
    pub has_ligand: bool,
    pub has_membrane: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructureImportRequest {
    pub project_path: String,
    pub source_kind: StructureSourceKind,
    pub source_path: Option<String>,
    pub smiles: Option<String>,
    pub display_name: Option<String>,
    pub overwrite: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructureSummary {
    pub atom_count: Option<u32>,
    pub residue_count: Option<u32>,
    pub chain_count: Option<u32>,
    pub molecule_count: Option<u32>,
    pub model_count: Option<u32>,
    pub format_note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructureImportResult {
    pub system: SystemSpec,
    pub imported_path: String,
    pub summary: StructureSummary,
    pub warnings: Vec<String>,
    pub imported_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedStructureEntry {
    pub id: String,
    pub name: String,
    pub source_path: Option<String>,
    pub imported_path: String,
    pub source_kind: StructureSourceKind,
    pub imported_at: DateTime<Utc>,
    pub summary: Option<StructureSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteImportedStructureRequest {
    pub project_path: String,
    pub imported_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructureFileRequest {
    pub project_path: String,
    pub source_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructureFilePayload {
    pub source_path: String,
    pub format: String,
    pub contents: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForceFieldSpec {
    pub protein: String,
    pub water_model: String,
    pub ligand: Option<String>,
    pub ions: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SolventSpec {
    pub model: String,
    pub box_shape: String,
    pub padding_nm: f32,
    pub ionic_strength_molar: f32,
    pub neutralize: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceSpec {
    pub execution_mode: ExecutionMode,
    pub cpu_threads: u16,
    pub gpu_count: u16,
    pub mpi_ranks: u16,
    pub walltime_hours: f32,
    pub remote_profile_id: Option<String>,
    pub queue: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SimulationStageKind {
    StructurePreparation,
    EnergyMinimization,
    NvtEquilibration,
    NptEquilibration,
    Production,
    Analysis,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulationStage {
    pub id: String,
    pub kind: SimulationStageKind,
    pub label: String,
    pub enabled: bool,
    pub parameters: BTreeMap<String, String>,
    pub expected_outputs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AnalysisKind {
    Rmsd,
    Rmsf,
    RadiusOfGyration,
    HydrogenBonds,
    Distances,
    Angles,
    Dihedrals,
    Contacts,
    EnergyTerms,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisModule {
    pub kind: AnalysisKind,
    pub enabled: bool,
    pub parameters: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OutputSpec {
    pub generated_inputs: Vec<String>,
    pub run_logs: Vec<String>,
    pub checkpoints: Vec<String>,
    pub trajectories: Vec<String>,
    pub energy: Vec<String>,
    pub analysis_tables: Vec<String>,
    pub reports: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulationPlan {
    pub id: Uuid,
    pub project_id: Option<Uuid>,
    pub name: String,
    pub engine_id: String,
    pub system: SystemSpec,
    pub force_field: ForceFieldSpec,
    pub solvent: SolventSpec,
    pub resources: ResourceSpec,
    pub stages: Vec<SimulationStage>,
    #[serde(default)]
    pub outputs: OutputSpec,
    pub analysis: Vec<AnalysisModule>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanRequest {
    pub project_id: Option<Uuid>,
    pub name: String,
    pub engine_id: String,
    pub domain: ProjectDomain,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ValidationSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationItem {
    pub severity: ValidationSeverity,
    pub field: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ValidationStatus {
    Valid,
    ValidWithWarnings,
    Invalid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationReport {
    pub status: ValidationStatus,
    pub items: Vec<ValidationItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParameterMappingRequest {
    pub plan: SimulationPlan,
    pub engine_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ParameterMappingStatus {
    Mapped,
    Approximated,
    Unsupported,
    ManualReview,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParameterMappingItem {
    pub stage_id: String,
    pub stage_label: String,
    pub normalized_key: String,
    pub normalized_value: String,
    pub engine_key: String,
    pub engine_value: String,
    pub target_file: String,
    pub status: ParameterMappingStatus,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParameterMappingReport {
    pub engine_id: String,
    pub plan_id: Uuid,
    pub items: Vec<ParameterMappingItem>,
    pub warnings: Vec<String>,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TaskStatus {
    Queued,
    Preparing,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulationTask {
    pub id: Uuid,
    pub plan_id: Uuid,
    pub engine_id: String,
    pub status: TaskStatus,
    pub current_stage: Option<SimulationStageKind>,
    pub progress_percent: f32,
    pub ns_per_day: Option<f32>,
    pub log_tail: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRecord {
    pub id: Uuid,
    pub project_id: Option<Uuid>,
    pub plan_id: Uuid,
    pub engine_id: String,
    pub status: TaskStatus,
    pub current_stage: Option<SimulationStageKind>,
    pub progress_percent: f32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// How AutoMD authenticates the SSH connection to a remote target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum RemoteAuthMethod {
    /// Use the system SSH agent / `~/.ssh/config` (no in-app credentials).
    #[default]
    Agent,
    /// Use an explicit private-key file (`identity_file`).
    Key,
    /// Username + password, fed to ssh via a PTY. Session-only, never persisted.
    Password,
}

fn default_ssh_port() -> u16 {
    22
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteProfile {
    pub id: String,
    pub name: String,
    pub host: String,
    #[serde(default)]
    pub username: String,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    #[serde(default)]
    pub auth_method: RemoteAuthMethod,
    #[serde(default)]
    pub identity_file: Option<String>,
    pub scheduler: ExecutionMode,
    pub workdir: String,
    pub module_load: Vec<String>,
    pub default_queue: Option<String>,
}

/// Result of `test_remote_connection`: a single, non-retrying probe that reports
/// who/where we connected as and what scheduler the login node exposes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteConnectionTest {
    pub ok: bool,
    pub user: Option<String>,
    pub host: String,
    pub os: Option<String>,
    pub arch: Option<String>,
    pub hostname: Option<String>,
    /// Detected scheduler on the target (slurm/pbs/lsf), or `Ssh` for none found.
    pub scheduler: Option<ExecutionMode>,
    /// True when the remote `uname` reports a Linux target (full support).
    pub linux: bool,
    pub message: String,
    pub checked_at: DateTime<Utc>,
}

/// A single hardware category in a [`RemoteHardwareReport`]. `summary` is a
/// short, best-effort one-liner; `detail` is the raw command output for that
/// category, shown verbatim in the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteHardwareSection {
    pub summary: String,
    pub detail: String,
}

/// Result of `query_remote_hardware`: a one-shot, non-persisted snapshot of a
/// remote host's CPU / memory / GPU / disk. Every call re-runs the probe over
/// SSH; the result is never written to disk or cached.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteHardwareReport {
    pub ok: bool,
    pub host: String,
    pub user: Option<String>,
    pub os: Option<String>,
    pub hostname: Option<String>,
    pub cpu: RemoteHardwareSection,
    pub memory: RemoteHardwareSection,
    pub gpu: RemoteHardwareSection,
    pub disk: RemoteHardwareSection,
    pub message: String,
    pub checked_at: DateTime<Utc>,
}

/// One line item in the remote-submit preflight checklist.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreflightCheck {
    pub id: String,
    pub label: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSubmitPreflight {
    pub checks: Vec<PreflightCheck>,
    pub all_ok: bool,
    /// True when failing checks are safe for the explicit advanced SSH-direct
    /// override: helper only for schedulers, helper/engine for SSH direct.
    pub can_override: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemotePreflightRequest {
    pub profile: RemoteProfile,
    pub plan: SimulationPlan,
    pub project_id: Option<String>,
    pub project_path: Option<String>,
    pub structure_id: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSubmitRequest {
    pub profile: RemoteProfile,
    pub plan: SimulationPlan,
    pub project_id: Option<String>,
    pub project_path: Option<String>,
    pub structure_id: Option<String>,
    pub password: Option<String>,
    #[serde(default)]
    pub allow_no_helper: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteJobSubmission {
    pub job_id: Option<String>,
    pub scheduler: ExecutionMode,
    pub submit_output: String,
    pub remote_run_dir: String,
    pub remote_workdir: String,
    pub files_uploaded: u32,
    pub warnings: Vec<String>,
    pub submitted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemotePollRequest {
    pub profile: RemoteProfile,
    pub job_id: Option<String>,
    pub scheduler: ExecutionMode,
    pub engine_id: String,
    pub remote_run_dir: String,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteFetchRequest {
    pub profile: RemoteProfile,
    pub remote_run_dir: String,
    pub local_project_path: String,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteFetchResult {
    pub files_downloaded: u32,
    pub local_dir: String,
    pub message: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteExecutionRequest {
    pub plan: SimulationPlan,
    pub profile: RemoteProfile,
    pub local_project_path: Option<String>,
    pub include_submit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteCommand {
    pub id: String,
    pub label: String,
    pub command: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteExecutionPackage {
    pub engine_id: String,
    pub scheduler: ExecutionMode,
    pub profile_id: String,
    pub remote_workdir: String,
    pub run_directory: String,
    pub files: Vec<GeneratedFile>,
    pub commands: Vec<RemoteCommand>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteStatusParseRequest {
    pub engine_id: String,
    pub scheduler: ExecutionMode,
    pub submit_output: Option<String>,
    pub status_output: Option<String>,
    pub log_output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteJobSnapshot {
    pub scheduler: ExecutionMode,
    pub job_id: Option<String>,
    pub status: TaskStatus,
    pub queue_state: Option<String>,
    pub reason: Option<String>,
    pub progress_percent: Option<f32>,
    pub ns_per_day: Option<f32>,
    pub current_step: Option<u64>,
    pub log_report: Option<EngineLogReport>,
    pub warnings: Vec<String>,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RemoteWorkflowMode {
    DryRun,
    WriteFiles,
    Execute,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteWorkflowStepRequest {
    pub project_path: String,
    pub package: RemoteExecutionPackage,
    pub step_id: String,
    pub mode: RemoteWorkflowMode,
    pub job_id: Option<String>,
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteWorkflowStepResult {
    pub step_id: String,
    pub label: String,
    pub command: String,
    pub mode: RemoteWorkflowMode,
    pub files_written: Vec<String>,
    pub status: TaskStatus,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub snapshot: Option<RemoteJobSnapshot>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<u128>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDiagnostic {
    pub id: String,
    pub label: String,
    pub command: String,
    pub status: DetectionStatus,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePickRequest {
    pub title: Option<String>,
    pub extensions: Vec<String>,
    /// Optional default directory to open the file dialog in.
    #[serde(default)]
    pub default_dir: Option<String>,
    /// When true, the file dialog should show hidden files (dot-files).
    #[serde(default)]
    pub show_hidden: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutableSearchRequest {
    pub commands: Vec<String>,
    pub extra_dirs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutableSearchResult {
    pub found: bool,
    pub command: Option<String>,
    pub path: Option<String>,
    pub version: Option<String>,
    pub checked_locations: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuAvailability {
    pub available: bool,
    pub mode: String,
    pub backend: Option<GpuBackend>,
    pub label: String,
    pub reason: String,
    pub detail: String,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CpuHardware {
    pub brand: String,
    pub architecture: String,
    pub logical_cores: u16,
    pub physical_cores: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryHardware {
    pub total_bytes: Option<u64>,
    pub available_bytes: Option<u64>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuDevice {
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub backend: Option<GpuBackend>,
    pub memory_bytes: Option<u64>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskVolume {
    pub id: String,
    pub mount_point: String,
    pub filesystem: String,
    pub total_bytes: Option<u64>,
    pub available_bytes: Option<u64>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareDiagnostics {
    pub cpu: CpuHardware,
    pub memory: MemoryHardware,
    pub gpus: Vec<GpuDevice>,
    pub disks: Vec<DiskVolume>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDiagnostics {
    pub os: String,
    pub arch: String,
    pub tools: Vec<ToolDiagnostic>,
    pub gpu: GpuAvailability,
    pub hardware: HardwareDiagnostics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScienceToolDiagnostic {
    pub id: String,
    pub label: String,
    pub import_name: Option<String>,
    pub command: Option<String>,
    pub status: DetectionStatus,
    pub version: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScienceSidecarDiagnostics {
    pub python_executable: Option<String>,
    pub tools: Vec<ScienceToolDiagnostic>,
    pub environment_recipe: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScienceToolInspectRequest {
    pub id: String,
    pub label: String,
    pub import_name: Option<String>,
    pub command: Option<String>,
    pub executable_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructurePreparationRequest {
    pub plan: SimulationPlan,
    pub project_path: Option<String>,
    pub write_to_disk: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructurePreparationPackage {
    pub plan_id: Uuid,
    pub generated_directory: String,
    pub commands: Vec<EngineCommand>,
    pub files: Vec<EngineRunFile>,
    pub warnings: Vec<String>,
    pub writable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryAnalysisRequest {
    pub plan: SimulationPlan,
    pub project_path: Option<String>,
    pub topology_path: Option<String>,
    pub trajectory_path: Option<String>,
    pub selection: String,
    pub write_to_disk: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryAnalysisPackage {
    pub plan_id: Uuid,
    pub generated_directory: String,
    pub commands: Vec<EngineCommand>,
    pub files: Vec<EngineRunFile>,
    pub expected_outputs: Vec<String>,
    pub warnings: Vec<String>,
    pub writable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedFile {
    pub path: String,
    pub language: String,
    pub contents: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerRecipe {
    pub engine_id: String,
    pub title: String,
    pub files: Vec<GeneratedFile>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildRecipeOptions {
    pub engine_id: String,
    pub enable_mpi: bool,
    pub enable_gpu: bool,
    pub gpu_backend: Option<GpuBackend>,
    pub enable_plumed: bool,
    pub install_prefix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildRecipe {
    pub engine_id: String,
    pub title: String,
    pub script: String,
    pub steps: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipeExportRequest {
    pub project_path: String,
    pub build_options: BuildRecipeOptions,
    pub include_container: bool,
    pub include_build_script: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipeExportResult {
    pub engine_id: String,
    pub directory: String,
    pub files: Vec<GeneratedFile>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum BuildWorkflowMode {
    DryRun,
    WriteFiles,
    Execute,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EngineDeployStrategy {
    Auto,
    Package,
    SourceBuild,
    RecipeOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildWorkflowRequest {
    pub project_path: String,
    pub build_options: BuildRecipeOptions,
    pub include_container: bool,
    pub include_build_script: bool,
    pub mode: BuildWorkflowMode,
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineDeployRequest {
    pub target_id: String,
    pub engine_id: String,
    pub strategy: EngineDeployStrategy,
    pub mode: BuildWorkflowMode,
    pub build_options: BuildRecipeOptions,
    pub project_path: Option<String>,
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildWorkflowResult {
    pub engine_id: String,
    pub directory: String,
    pub command: String,
    pub mode: BuildWorkflowMode,
    pub files_written: Vec<String>,
    pub status: TaskStatus,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub log_path: Option<String>,
    pub failure_analysis: Option<FailureAnalysis>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<u128>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineDeployResult {
    pub target_id: String,
    pub engine_id: String,
    pub strategy: EngineDeployStrategy,
    pub mode: BuildWorkflowMode,
    pub record: Option<EngineInstallationRecord>,
    pub build_result: Option<BuildWorkflowResult>,
    pub status: TaskStatus,
    pub stdout: String,
    pub stderr: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineRunRequest {
    pub plan: SimulationPlan,
    pub project_path: Option<String>,
    pub write_to_disk: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineCommand {
    pub stage_id: String,
    pub label: String,
    pub command: String,
    pub working_directory: String,
    pub expected_outputs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineRunFile {
    pub path: String,
    pub language: String,
    pub contents: String,
    pub written: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineRunPackage {
    pub engine_id: String,
    pub plan_id: Uuid,
    pub run_directory: String,
    pub commands: Vec<EngineCommand>,
    pub files: Vec<EngineRunFile>,
    pub warnings: Vec<String>,
    pub writable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchExperimentRequest {
    pub plan: SimulationPlan,
    pub project_path: Option<String>,
    pub replicate_count: u32,
    pub seed_start: u64,
    pub write_to_disk: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchReplicaPlan {
    pub replica_index: u32,
    pub seed: u64,
    pub plan: SimulationPlan,
    pub run_directory: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchExperimentPackage {
    pub engine_id: String,
    pub plan_id: Uuid,
    pub generated_directory: String,
    pub replicas: Vec<BatchReplicaPlan>,
    pub files: Vec<EngineRunFile>,
    pub commands: Vec<EngineCommand>,
    pub warnings: Vec<String>,
    pub writable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTextFileRequest {
    pub project_path: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTextFileWriteRequest {
    pub project_path: String,
    pub path: String,
    pub contents: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTextFilePayload {
    pub path: String,
    pub language: String,
    pub contents: String,
    pub size_bytes: u64,
    pub modified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineLogParseRequest {
    pub engine_id: String,
    pub log_contents: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EngineLogEventKind {
    Progress,
    Performance,
    Warning,
    Error,
    Checkpoint,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineLogEvent {
    pub kind: EngineLogEventKind,
    pub line_number: usize,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineLogReport {
    pub engine_id: String,
    pub progress_percent: Option<f32>,
    pub ns_per_day: Option<f32>,
    pub current_step: Option<u64>,
    pub events: Vec<EngineLogEvent>,
    pub fatal_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FailureCategory {
    MissingExecutable,
    MissingInput,
    MissingTopology,
    ParameterMismatch,
    MissingForceField,
    LicenseRequired,
    GpuUnavailable,
    MpiFailure,
    NumericalInstability,
    DiskOrPermission,
    SchedulerFailure,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FailureSuggestion {
    pub title: String,
    pub detail: String,
    pub action_label: String,
    pub command_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FailureAnalysis {
    pub engine_id: String,
    pub category: FailureCategory,
    pub severity: ValidationSeverity,
    pub message: String,
    pub suggestions: Vec<FailureSuggestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FailureAnalysisRequest {
    pub engine_id: String,
    pub log_contents: String,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointCandidate {
    pub path: String,
    pub size_bytes: u64,
    pub modified_at: Option<DateTime<Utc>>,
    pub stage_hint: Option<String>,
    pub command_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumePlanRequest {
    pub project_path: String,
    pub run_directory: String,
    pub engine_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumePlan {
    pub engine_id: String,
    pub run_directory: String,
    pub checkpoints: Vec<CheckpointCandidate>,
    pub recommended: Option<CheckpointCandidate>,
    pub resume_command: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LocalRunMode {
    DryRun,
    Mock,
    Real,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartLocalRunRequest {
    pub plan: SimulationPlan,
    pub project_path: Option<String>,
    pub mode: LocalRunMode,
    pub write_package: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalTaskSnapshot {
    pub id: Uuid,
    pub plan_id: Uuid,
    pub engine_id: String,
    pub mode: LocalRunMode,
    pub status: TaskStatus,
    pub run_directory: String,
    pub command: String,
    pub progress_percent: f32,
    pub ns_per_day: Option<f32>,
    pub current_step: Option<u64>,
    pub log_tail: Vec<String>,
    pub error_message: Option<String>,
    pub exit_code: Option<i32>,
    pub artifacts: Vec<RunArtifact>,
    pub report_path: Option<String>,
    pub failure_analysis: Option<FailureAnalysis>,
    pub resume_plan: Option<ResumePlan>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentVariableRecord {
    pub key: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunEnvironmentSnapshot {
    pub os: String,
    pub arch: String,
    pub current_dir: String,
    pub environment: Vec<EnvironmentVariableRecord>,
    pub tools: Vec<ToolDiagnostic>,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalRunManifest {
    pub task_id: Uuid,
    pub plan_id: Uuid,
    pub engine_id: String,
    pub mode: LocalRunMode,
    pub command: String,
    pub project_path: String,
    pub run_directory: String,
    pub environment: RunEnvironmentSnapshot,
    pub plan: SimulationPlan,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ArtifactKind {
    Input,
    GeneratedInput,
    RunLog,
    Checkpoint,
    Trajectory,
    Energy,
    AnalysisTable,
    Figure,
    Report,
    Metadata,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunArtifact {
    pub path: String,
    pub kind: ArtifactKind,
    pub size_bytes: u64,
    pub modified_at: Option<DateTime<Utc>>,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactIndexRequest {
    pub project_path: String,
    pub run_directory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactIndex {
    pub project_path: String,
    pub run_directory: Option<String>,
    pub artifacts: Vec<RunArtifact>,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactRecord {
    pub project_path: String,
    pub path: String,
    pub kind: ArtifactKind,
    pub size_bytes: u64,
    pub modified_at: Option<DateTime<Utc>>,
    pub summary: Option<String>,
    pub run_directory: Option<String>,
    pub indexed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisParseRequest {
    pub project_path: String,
    pub artifact_paths: Option<Vec<String>>,
    pub max_points: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisPoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisSeries {
    pub path: String,
    pub label: String,
    pub x_label: String,
    pub y_label: String,
    pub points: Vec<AnalysisPoint>,
    pub min_y: Option<f64>,
    pub max_y: Option<f64>,
    pub last_y: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisParseResult {
    pub project_path: String,
    pub series: Vec<AnalysisSeries>,
    pub warnings: Vec<String>,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisCacheRecord {
    pub project_path: String,
    pub path: String,
    pub label: String,
    pub x_label: String,
    pub y_label: String,
    pub point_count: usize,
    pub min_y: Option<f64>,
    pub max_y: Option<f64>,
    pub last_y: Option<f64>,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TrajectoryFormat {
    Pdb,
    Xyz,
    LammpsDump,
    Xtc,
    Trr,
    Dcd,
    Netcdf,
    Gsd,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TrajectoryIndexStrategy {
    TextOffsets,
    MetadataOnly,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryFrameDescriptor {
    pub frame_index: usize,
    pub byte_start: u64,
    pub byte_end: u64,
    pub atom_count: Option<u32>,
    pub time_ps: Option<f64>,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryIndexRequest {
    pub project_path: String,
    pub trajectory_path: String,
    pub frame_stride: Option<usize>,
    pub max_preview_frames: Option<usize>,
    pub write_index: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryIndex {
    pub project_path: String,
    pub trajectory_path: String,
    pub format: TrajectoryFormat,
    pub strategy: TrajectoryIndexStrategy,
    pub size_bytes: u64,
    pub frame_count: Option<usize>,
    /// Full frame byte-offset table for seek-based chunk reads (text trajectories).
    #[serde(default)]
    pub frames: Vec<TrajectoryFrameDescriptor>,
    pub sampled_frames: Vec<TrajectoryFrameDescriptor>,
    pub index_path: Option<String>,
    pub warnings: Vec<String>,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryChunkRequest {
    pub project_path: String,
    pub trajectory_path: String,
    pub frame_indices: Option<Vec<usize>>,
    pub start_frame: Option<usize>,
    pub frame_count: Option<usize>,
    pub max_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryFramePayload {
    pub frame_index: usize,
    pub label: String,
    pub format: TrajectoryFormat,
    pub contents: String,
    pub atom_count: Option<u32>,
    pub time_ps: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajectoryChunk {
    pub project_path: String,
    pub trajectory_path: String,
    pub frames: Vec<TrajectoryFramePayload>,
    pub truncated: bool,
    pub warnings: Vec<String>,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportExportRequest {
    pub project_path: String,
    pub plan: SimulationPlan,
    pub task: Option<LocalTaskSnapshot>,
    pub artifact_index: Option<ArtifactIndex>,
    pub format: ReportFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ReportFormat {
    Markdown,
    Html,
    Pdf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedReport {
    pub path: String,
    pub format: ReportFormat,
    pub contents: String,
}
