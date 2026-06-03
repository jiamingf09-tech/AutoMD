import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "./lib/api";
import type {
  AnalysisKind,
  AnalysisCacheRecord,
  AnalysisParseResult,
  ArtifactIndex,
  ArtifactRecord,
  BatchExperimentPackage,
  BuildRecipe,
  BuildRecipeOptions,
  BuildWorkflowMode,
  BuildWorkflowResult,
  ContainerRecipe,
  DetectionStatus,
  EngineCapability,
  EngineInstallationRecord,
  EngineLogReport,
  EngineRunPackage,
  ExecutionMode,
  FailureAnalysis,
  GpuBackend,
  LocalRunMode,
  LocalTaskSnapshot,
  OutputSpec,
  ParameterMappingReport,
  ParameterMappingStatus,
  ProjectDomain,
  PluginKind,
  PluginRegistrySnapshot,
  ProjectTextFilePayload,
  ProjectSummary,
  ReportFormat,
  ExportedReport,
  RemoteExecutionPackage,
  RemoteJobSnapshot,
  RemoteProfile,
  RemoteWorkflowMode,
  RemoteWorkflowStepResult,
  RecipeExportResult,
  ResumePlan,
  RuntimeDiagnostics,
  RunArtifact,
  ScienceSidecarDiagnostics,
  SimulationPlan,
  SimulationStage,
  SimulationTask,
  StructurePreparationPackage,
  TaskRecord,
  TrajectoryAnalysisPackage,
  TrajectoryChunk,
  TrajectoryIndex,
  StructureImportResult,
  StructureSourceKind,
  ValidationReport,
  ValidationSeverity
} from "./types";

type TabId = "overview" | "projects" | "engines" | "workflow" | "run" | "remote" | "build" | "plugins" | "report";

const tabs: Array<{ id: TabId; label: string; description: string }> = [
  { id: "overview", label: "总览", description: "项目状态和体系预览" },
  { id: "projects", label: "项目", description: "项目创建和目录结构" },
  { id: "engines", label: "引擎", description: "检测、授权和平台能力" },
  { id: "workflow", label: "流程", description: "参数、阶段和分析模块" },
  { id: "run", label: "运行", description: "本地、容器和 HPC 调度" },
  { id: "remote", label: "远程", description: "SSH 和队列 profile" },
  { id: "build", label: "编译", description: "源码构建和容器 recipe" },
  { id: "plugins", label: "插件", description: "扩展 manifest 和能力" },
  { id: "report", label: "报告", description: "可复现实验输出" }
];

const engineLabel: Record<string, string> = {
  gromacs: "GROMACS",
  openmm: "OpenMM",
  ambertools: "AmberTools",
  lammps: "LAMMPS",
  cp2k: "CP2K",
  genesis: "GENESIS",
  hoomd: "HOOMD-blue",
  dl_poly: "DL_POLY",
  tinker: "Tinker",
  namd: "NAMD",
  amber_pmemd: "AMBER pmemd",
  charmm: "CHARMM",
  desmond: "Desmond",
  acemd: "ACEMD"
};

const statusText: Record<DetectionStatus, string> = {
  ready: "可用",
  missingInstall: "需安装",
  missingLicense: "需许可",
  platformUnsupported: "平台不支持",
  remoteRecommended: "建议远程"
};

const severityText: Record<ValidationSeverity, string> = {
  info: "信息",
  warning: "警告",
  error: "错误"
};

const executionModeText: Record<ExecutionMode, string> = {
  localProcess: "本地进程",
  condaEnvironment: "Conda 环境",
  container: "容器",
  wsl2: "WSL2",
  ssh: "SSH",
  slurm: "SLURM",
  pbs: "PBS",
  lsf: "LSF"
};

const gpuBackendText: Record<GpuBackend, string> = {
  cuda: "CUDA",
  rocm: "ROCm",
  openCl: "OpenCL",
  metal: "Metal",
  sycl: "SYCL",
  cpuOnly: "CPU"
};

const analysisText: Record<AnalysisKind, string> = {
  rmsd: "RMSD",
  rmsf: "RMSF",
  radiusOfGyration: "Rg",
  hydrogenBonds: "氢键",
  distances: "距离",
  angles: "角度",
  dihedrals: "二面角",
  contacts: "接触图",
  energyTerms: "能量项"
};

const outputSpecText: Record<keyof OutputSpec, string> = {
  generatedInputs: "生成输入",
  runLogs: "运行日志",
  checkpoints: "Checkpoint",
  trajectories: "轨迹",
  energy: "能量/状态",
  analysisTables: "分析表",
  reports: "报告"
};

const parameterMappingStatusText: Record<ParameterMappingStatus, string> = {
  mapped: "已映射",
  approximated: "近似映射",
  unsupported: "未支持",
  manualReview: "需复核"
};

const localRunModeText: Record<LocalRunMode, string> = {
  dryRun: "Dry run",
  mock: "Mock runner",
  real: "真实本地执行"
};

const remoteWorkflowModeText: Record<RemoteWorkflowMode, string> = {
  dryRun: "Dry run",
  writeFiles: "只写脚本",
  execute: "执行 ssh/rsync"
};

const buildWorkflowModeText: Record<BuildWorkflowMode, string> = {
  dryRun: "Dry run",
  writeFiles: "只写脚本",
  execute: "执行构建"
};

const failureCategoryText: Record<FailureAnalysis["category"], string> = {
  missingExecutable: "缺少可执行文件",
  missingInput: "缺少输入文件",
  missingTopology: "拓扑缺口",
  parameterMismatch: "坐标/拓扑不匹配",
  missingForceField: "缺少力场参数",
  licenseRequired: "需要许可",
  gpuUnavailable: "GPU 不可用",
  mpiFailure: "MPI 失败",
  numericalInstability: "数值不稳定",
  diskOrPermission: "磁盘或权限问题",
  schedulerFailure: "调度器失败",
  unknown: "未知失败"
};

const pluginKindText: Record<PluginKind, string> = {
  engineAdapter: "引擎适配器",
  analysisModule: "分析模块",
  remoteScheduler: "远程调度器",
  buildRecipe: "构建 recipe",
  reportTemplate: "报告模板"
};

function defaultBuildRecipeOptions(engineId: string): BuildRecipeOptions {
  return {
    engineId,
    enableMpi: true,
    enableGpu: true,
    gpuBackend: "cuda",
    enablePlumed: engineId === "gromacs",
    installPrefix: null
  };
}

function isNativeEditablePath(path: string) {
  return /^(generated|runs|remote|build-recipes|analysis|reports)\//.test(path)
    && /\.(mdp|mdin|conf|cfg|inp|in|key|txt|json|ya?ml|py|sh|slurm|pbs|lsf|md)$/i.test(path);
}

function App() {
  const [activeTab, setActiveTab] = useState<TabId>("overview");
  const [engines, setEngines] = useState<EngineCapability[]>([]);
  const [engineInstallations, setEngineInstallations] = useState<EngineInstallationRecord[]>([]);
  const [engineInstallationDraft, setEngineInstallationDraft] = useState<EngineInstallationRecord>({
    engineId: "gromacs",
    location: "",
    version: null,
    authorizationStatus: "ready",
    checkedAt: new Date().toISOString()
  });
  const [diagnostics, setDiagnostics] = useState<RuntimeDiagnostics | null>(null);
  const [scienceDiagnostics, setScienceDiagnostics] = useState<ScienceSidecarDiagnostics | null>(null);
  const [preparationPackage, setPreparationPackage] = useState<StructurePreparationPackage | null>(null);
  const [pluginRegistry, setPluginRegistry] = useState<PluginRegistrySnapshot | null>(null);
  const [remoteProfiles, setRemoteProfiles] = useState<RemoteProfile[]>([]);
  const [selectedRemoteProfileId, setSelectedRemoteProfileId] = useState<string | null>(null);
  const [remotePackage, setRemotePackage] = useState<RemoteExecutionPackage | null>(null);
  const [remoteJobSnapshot, setRemoteJobSnapshot] = useState<RemoteJobSnapshot | null>(null);
  const [remoteWorkflowMode, setRemoteWorkflowMode] = useState<RemoteWorkflowMode>("dryRun");
  const [remoteWorkflowJobId, setRemoteWorkflowJobId] = useState("");
  const [remoteWorkflowTimeout, setRemoteWorkflowTimeout] = useState(120);
  const [remoteWorkflowResult, setRemoteWorkflowResult] = useState<RemoteWorkflowStepResult | null>(null);
  const [remoteProfileDraft, setRemoteProfileDraft] = useState<RemoteProfile>({
    id: "custom-hpc",
    name: "Custom HPC",
    host: "login.example.edu",
    scheduler: "slurm",
    workdir: "/scratch/$USER/automd",
    moduleLoad: ["module load gcc openmpi cuda", "module load gromacs"],
    defaultQueue: "gpu"
  });
  const [remoteSubmitOutput, setRemoteSubmitOutput] = useState("123456;cluster");
  const [remoteStatusOutput, setRemoteStatusOutput] = useState("JOBID PARTITION NAME USER ST TIME NODES NODELIST\n123456 gpu automd noir R 00:10 1 node01");
  const [remoteLogOutput, setRemoteLogOutput] = useState("step 5000 of 10000\nPerformance: 82.125 ns/day");
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [currentProject, setCurrentProject] = useState<ProjectSummary | null>(null);
  const [plan, setPlan] = useState<SimulationPlan | null>(null);
  const [validation, setValidation] = useState<ValidationReport | null>(null);
  const [parameterMappingReport, setParameterMappingReport] = useState<ParameterMappingReport | null>(null);
  const [task, setTask] = useState<SimulationTask | null>(null);
  const [taskRecords, setTaskRecords] = useState<TaskRecord[]>([]);
  const [slurmScript, setSlurmScript] = useState("");
  const [runPackage, setRunPackage] = useState<EngineRunPackage | null>(null);
  const [batchReplicateCount, setBatchReplicateCount] = useState(3);
  const [batchSeedStart, setBatchSeedStart] = useState(20260603);
  const [batchPackage, setBatchPackage] = useState<BatchExperimentPackage | null>(null);
  const [nativeFile, setNativeFile] = useState<ProjectTextFilePayload | null>(null);
  const [nativeFileDraft, setNativeFileDraft] = useState("");
  const [nativeFileMessage, setNativeFileMessage] = useState<string | null>(null);
  const [localRunMode, setLocalRunMode] = useState<LocalRunMode>("mock");
  const [localSnapshot, setLocalSnapshot] = useState<LocalTaskSnapshot | null>(null);
  const [artifactIndex, setArtifactIndex] = useState<ArtifactIndex | null>(null);
  const [artifactRecords, setArtifactRecords] = useState<ArtifactRecord[]>([]);
  const [analysisResult, setAnalysisResult] = useState<AnalysisParseResult | null>(null);
  const [analysisCacheRecords, setAnalysisCacheRecords] = useState<AnalysisCacheRecord[]>([]);
  const [trajectoryIndex, setTrajectoryIndex] = useState<TrajectoryIndex | null>(null);
  const [trajectoryChunk, setTrajectoryChunk] = useState<TrajectoryChunk | null>(null);
  const [trajectoryAnalysisPackage, setTrajectoryAnalysisPackage] = useState<TrajectoryAnalysisPackage | null>(null);
  const [exportedReport, setExportedReport] = useState<ExportedReport | null>(null);
  const [sampleLog, setSampleLog] = useState("step 2500 of 10000\nWriting checkpoint, step 2500\nPerformance: 82.125 ns/day");
  const [logReport, setLogReport] = useState<EngineLogReport | null>(null);
  const [sampleFailureAnalysis, setSampleFailureAnalysis] = useState<FailureAnalysis | null>(null);
  const [manualResumePlan, setManualResumePlan] = useState<ResumePlan | null>(null);
  const [containerRecipe, setContainerRecipe] = useState<ContainerRecipe | null>(null);
  const [buildRecipe, setBuildRecipe] = useState<BuildRecipe | null>(null);
  const [recipeExportResult, setRecipeExportResult] = useState<RecipeExportResult | null>(null);
  const [buildWorkflowMode, setBuildWorkflowMode] = useState<BuildWorkflowMode>("dryRun");
  const [buildWorkflowTimeout, setBuildWorkflowTimeout] = useState(600);
  const [buildWorkflowResult, setBuildWorkflowResult] = useState<BuildWorkflowResult | null>(null);
  const [projectName, setProjectName] = useState("Demo protein-ligand MD");
  const [domain, setDomain] = useState<ProjectDomain>("biomolecular");
  const [selectedEngineId, setSelectedEngineId] = useState("gromacs");
  const [importSourceKind, setImportSourceKind] = useState<StructureSourceKind>("pdb");
  const [importSourcePath, setImportSourcePath] = useState("");
  const [importSmiles, setImportSmiles] = useState("");
  const [importDisplayName, setImportDisplayName] = useState("");
  const [structureImportResult, setStructureImportResult] = useState<StructureImportResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void bootstrap();
  }, []);

  useEffect(() => {
    if (!plan) {
      setParameterMappingReport(null);
      return;
    }
    void api.validatePlan(plan).then(setValidation).catch(reportError);
    void api.mapEngineParameters({ plan, engineId: plan.engineId }).then(setParameterMappingReport).catch(reportError);
  }, [plan]);

  useEffect(() => {
    if (!localSnapshot || ["completed", "failed", "cancelled"].includes(localSnapshot.status)) {
      if (localSnapshot) {
        void refreshTaskRecords();
      }
      if (localSnapshot?.artifacts.length) {
        const index = {
          projectPath: currentProject?.path ?? "",
          runDirectory: localSnapshot.runDirectory,
          artifacts: localSnapshot.artifacts,
          generatedAt: new Date().toISOString()
        };
        setArtifactIndex(index);
        void refreshAnalysis(index);
      }
      return;
    }
    const interval = window.setInterval(() => {
      void api.getLocalTask(localSnapshot.id).then((snapshot) => {
        setLocalSnapshot(snapshot);
        void refreshTaskRecords();
      }).catch(reportError);
    }, 500);
    return () => window.clearInterval(interval);
  }, [localSnapshot?.id, localSnapshot?.status]);

  const selectedEngine = useMemo(
    () => engines.find((engine) => engine.id === selectedEngineId) ?? engines[0],
    [engines, selectedEngineId]
  );

  const openSourceCount = engines.filter((engine) => !engine.license.requiresUserLicense).length;
  const externalCount = engines.filter((engine) => engine.license.requiresUserLicense).length;
  const readyCount = engines.filter((engine) => engine.detection.status === "ready").length;

  async function bootstrap() {
    try {
      const [capabilities, installations, runtime, science, plugins, profiles, storedProjects, storedTasks] = await Promise.all([
        api.engineCapabilities(),
        api.listEngineInstallations(),
        api.runtimeDiagnostics(),
        api.scienceSidecarDiagnostics(),
        api.pluginManifests(),
        api.remoteProfiles(),
        api.listProjects(),
        api.listTaskRecords(null)
      ]);
      setEngines(capabilities);
      setEngineInstallations(installations);
      if (capabilities[0]) {
        setEngineInstallationDraft((current) => ({ ...current, engineId: capabilities[0].id }));
      }
      setDiagnostics(runtime);
      setScienceDiagnostics(science);
      setPluginRegistry(plugins);
      setRemoteProfiles(profiles);
      setSelectedRemoteProfileId((current) => current ?? profiles[0]?.id ?? null);
      setProjects(storedProjects);
      setTaskRecords(storedTasks);
      if (storedProjects[0]) {
        await refreshCachedMetadata(storedProjects[0].path);
      }
      if (capabilities.length > 0 && !capabilities.some((engine) => engine.id === selectedEngineId)) {
        setSelectedEngineId(capabilities[0].id);
      }
      if (!plan) {
        const initialPlan = await api.generatePlan({
          projectId: storedProjects[0]?.id ?? null,
          name: "Default biomolecular workflow",
          engineId: "gromacs",
          domain: "biomolecular"
        });
        setPlan(initialPlan);
      }
    } catch (caught) {
      reportError(caught);
    }
  }

  async function createProject() {
    try {
      const project = await api.createProject({
        name: projectName,
        domain,
        preferredEngineId: selectedEngineId
      });
      setCurrentProject(project);
      setProjects((items) => [project, ...items.filter((item) => item.id !== project.id)]);
      const generatedPlan = await api.generatePlan({
        projectId: project.id,
        name: `${project.name} workflow`,
        engineId: selectedEngineId,
        domain
      });
      setPlan(generatedPlan);
      setTask(null);
      setTaskRecords([]);
      setArtifactRecords([]);
      setAnalysisCacheRecords([]);
      setRunPackage(null);
      setBatchPackage(null);
      setPreparationPackage(null);
      setLocalSnapshot(null);
      setArtifactIndex(null);
      setAnalysisResult(null);
      setTrajectoryIndex(null);
      setTrajectoryChunk(null);
      setTrajectoryAnalysisPackage(null);
      setExportedReport(null);
      setManualResumePlan(null);
      setStructureImportResult(null);
      setActiveTab("workflow");
    } catch (caught) {
      reportError(caught);
    }
  }

  async function importStructure() {
    const activeProject = currentProject ?? projects[0] ?? null;
    if (!activeProject || !plan) {
      setError("需要先创建项目并生成 SimulationPlan，才能导入结构。");
      return;
    }
    try {
      const result = await api.importStructure({
        projectPath: activeProject.path,
        sourceKind: importSourceKind,
        sourcePath: importSourceKind === "smiles" ? null : importSourcePath || null,
        smiles: importSourceKind === "smiles" ? importSmiles : null,
        displayName: importDisplayName || null,
        overwrite: true
      });
      setStructureImportResult(result);
      setPlan((current) => current ? { ...current, system: result.system } : current);
      const index = await api.collectArtifactIndex({
        projectPath: activeProject.path,
        runDirectory: null
      });
      setArtifactIndex(index);
      await refreshAnalysis(index);
      setActiveTab("overview");
    } catch (caught) {
      reportError(caught);
    }
  }

  async function queueMockTask() {
    if (!plan) {
      return;
    }
    try {
      const activeProject = currentProject ?? projects[0] ?? null;
      const [queuedTask, script, preparedPackage] = await Promise.all([
        api.createMockTask(plan),
        api.slurmScript(plan),
        api.prepareRunPackage({
          plan,
          projectPath: activeProject?.path ?? null,
          writeToDisk: Boolean(activeProject)
        })
      ]);
      setTask(queuedTask);
      setSlurmScript(script);
      setRunPackage(preparedPackage);
      setBatchPackage(null);
      setNativeFile(null);
      setNativeFileDraft("");
      setNativeFileMessage(null);
      setActiveTab("run");
    } catch (caught) {
      reportError(caught);
    }
  }

  async function generateBatchExperiment() {
    if (!plan) {
      return;
    }
    const activeProject = currentProject ?? projects[0] ?? null;
    const replicateCount = Math.max(1, Math.min(64, Math.floor(batchReplicateCount || 1)));
    const seedStart = Math.max(0, Math.floor(batchSeedStart || 1));
    try {
      const preparedBatch = await api.prepareBatchExperiment({
        plan,
        projectPath: activeProject?.path ?? null,
        replicateCount,
        seedStart,
        writeToDisk: Boolean(activeProject)
      });
      setBatchReplicateCount(replicateCount);
      setBatchSeedStart(seedStart);
      setBatchPackage(preparedBatch);
      setNativeFile(null);
      setNativeFileDraft("");
      setNativeFileMessage(null);
      if (activeProject) {
        await refreshArtifacts();
      }
      setActiveTab("run");
    } catch (caught) {
      reportError(caught);
    }
  }

  async function openNativeFile(path: string, fallbackContents?: string, fallbackLanguage = "text") {
    const activeProject = currentProject ?? projects[0] ?? null;
    if (!isNativeEditablePath(path)) {
      setError("该文件类型不在原生参数编辑器的安全白名单内。");
      return;
    }
    if (!activeProject) {
      setNativeFile({
        path,
        language: fallbackLanguage,
        contents: fallbackContents ?? "",
        sizeBytes: fallbackContents?.length ?? 0,
        modifiedAt: null
      });
      setNativeFileDraft(fallbackContents ?? "");
      setNativeFileMessage("当前没有项目目录，显示的是 run package 预览内容。");
      return;
    }
    try {
      const payload = await api.readProjectTextFile({ projectPath: activeProject.path, path });
      setNativeFile(payload);
      setNativeFileDraft(payload.contents);
      setNativeFileMessage(`已读取 ${payload.path}`);
    } catch (caught) {
      if (fallbackContents !== undefined) {
        setNativeFile({
          path,
          language: fallbackLanguage,
          contents: fallbackContents,
          sizeBytes: fallbackContents.length,
          modifiedAt: null
        });
        setNativeFileDraft(fallbackContents);
        setNativeFileMessage("项目文件尚未写入磁盘，当前显示 run package 预览内容。");
        return;
      }
      reportError(caught);
    }
  }

  async function saveNativeFile() {
    const activeProject = currentProject ?? projects[0] ?? null;
    if (!activeProject || !nativeFile) {
      setError("需要先创建项目并打开可编辑文件，才能保存原生参数。");
      return;
    }
    try {
      const saved = await api.writeProjectTextFile({
        projectPath: activeProject.path,
        path: nativeFile.path,
        contents: nativeFileDraft
      });
      setNativeFile(saved);
      setNativeFileDraft(saved.contents);
      setNativeFileMessage(`已保存 ${saved.path}`);
      await refreshArtifacts();
    } catch (caught) {
      reportError(caught);
    }
  }

  async function generatePreparationPackage() {
    if (!plan) {
      return;
    }
    try {
      const activeProject = currentProject ?? projects[0] ?? null;
      const prepared = await api.prepareStructurePackage({
        plan,
        projectPath: activeProject?.path ?? null,
        writeToDisk: Boolean(activeProject)
      });
      setPreparationPackage(prepared);
      setActiveTab("workflow");
    } catch (caught) {
      reportError(caught);
    }
  }

  async function parseLogSample() {
    if (!plan) {
      return;
    }
    try {
      const [report, failure] = await Promise.all([
        api.parseEngineLog({
          engineId: plan.engineId,
          logContents: sampleLog
        }),
        api.classifyFailure({
          engineId: plan.engineId,
          logContents: sampleLog,
          exitCode: null
        })
      ]);
      setLogReport(report);
      setSampleFailureAnalysis(failure);
    } catch (caught) {
      reportError(caught);
    }
  }

  async function startLocalRun() {
    if (!plan) {
      return;
    }
    try {
      const activeProject = currentProject ?? projects[0] ?? null;
      const snapshot = await api.startLocalRun({
        plan,
        projectPath: activeProject?.path ?? null,
        mode: localRunMode,
        writePackage: Boolean(activeProject)
      });
      setLocalSnapshot(snapshot);
      setManualResumePlan(snapshot.resumePlan ?? null);
      await refreshTaskRecords();
      if (snapshot.artifacts.length) {
        const index = {
          projectPath: activeProject?.path ?? "",
          runDirectory: snapshot.runDirectory,
          artifacts: snapshot.artifacts,
          generatedAt: new Date().toISOString()
        };
        setArtifactIndex(index);
        await refreshAnalysis(index);
      }
      setActiveTab("run");
    } catch (caught) {
      reportError(caught);
    }
  }

  async function cancelLocalRun() {
    if (!localSnapshot) {
      return;
    }
    try {
      const snapshot = await api.cancelLocalTask(localSnapshot.id);
      setLocalSnapshot(snapshot);
      setManualResumePlan(snapshot.resumePlan ?? null);
      await refreshTaskRecords();
    } catch (caught) {
      reportError(caught);
    }
  }

  async function refreshTaskRecords() {
    try {
      const records = await api.listTaskRecords((currentProject ?? projects[0] ?? null)?.id ?? null);
      setTaskRecords(records);
    } catch (caught) {
      reportError(caught);
    }
  }

  async function refreshCachedMetadata(projectPath = (currentProject ?? projects[0] ?? null)?.path) {
    if (!projectPath) {
      setArtifactRecords([]);
      setAnalysisCacheRecords([]);
      return;
    }
    try {
      const [artifacts, analysisCache] = await Promise.all([
        api.listArtifactRecords(projectPath),
        api.listAnalysisCacheRecords(projectPath)
      ]);
      setArtifactRecords(artifacts);
      setAnalysisCacheRecords(analysisCache);
    } catch (caught) {
      reportError(caught);
    }
  }

  async function discoverResumePlan() {
    const activeProject = currentProject ?? projects[0] ?? null;
    const runDirectory = localSnapshot?.runDirectory ?? runPackage?.runDirectory ?? null;
    if (!activeProject || !plan || !runDirectory) {
      setError("需要先创建项目并生成 run package，才能扫描 checkpoint。");
      return;
    }
    try {
      const resumePlan = await api.discoverResumePlan({
        projectPath: activeProject.path,
        runDirectory,
        engineId: plan.engineId
      });
      setManualResumePlan(resumePlan);
      if (localSnapshot) {
        setLocalSnapshot({ ...localSnapshot, resumePlan });
      }
    } catch (caught) {
      reportError(caught);
    }
  }

  async function refreshArtifacts() {
    const activeProject = currentProject ?? projects[0] ?? null;
    if (!activeProject) {
      setError("需要先创建项目，才能扫描项目目录中的 artifacts。");
      return;
    }
    try {
      const index = await api.collectArtifactIndex({
        projectPath: activeProject.path,
        runDirectory: localSnapshot?.runDirectory ?? runPackage?.runDirectory ?? null
      });
      setArtifactIndex(index);
      setArtifactRecords(await api.listArtifactRecords(activeProject.path));
      await refreshAnalysis(index);
    } catch (caught) {
      reportError(caught);
    }
  }

  async function refreshAnalysis(index = artifactIndex) {
    const activeProject = currentProject ?? projects[0] ?? null;
    if (!activeProject) {
      return;
    }
    const artifactPaths = index?.artifacts
      .filter((artifact) => artifact.kind === "analysisTable")
      .map((artifact) => artifact.path) ?? null;
    try {
      const parsed = await api.parseAnalysisResults({
        projectPath: activeProject.path,
        artifactPaths,
        maxPoints: 800
      });
      setAnalysisResult(parsed);
      setAnalysisCacheRecords(await api.listAnalysisCacheRecords(activeProject.path));
    } catch (caught) {
      reportError(caught);
    }
  }

  async function indexTrajectory(trajectoryPath?: string) {
    const activeProject = currentProject ?? projects[0] ?? null;
    const path = trajectoryPath ?? artifactIndex?.artifacts.find((artifact) => artifact.kind === "trajectory")?.path;
    if (!activeProject || !path) {
      setError("需要先创建项目并产生轨迹 artifact，才能建立轨迹索引。");
      return;
    }
    try {
      const index = await api.indexTrajectory({
        projectPath: activeProject.path,
        trajectoryPath: path,
        frameStride: 1,
        maxPreviewFrames: 120,
        writeIndex: true
      });
      setTrajectoryIndex(index);
      const firstFrame = index.sampledFrames[0]?.frameIndex ?? 0;
      const chunk = await api.readTrajectoryChunk({
        projectPath: activeProject.path,
        trajectoryPath: path,
        startFrame: firstFrame,
        frameCount: 1,
        maxBytes: 750_000
      });
      setTrajectoryChunk(chunk);
      await refreshArtifacts();
    } catch (caught) {
      reportError(caught);
    }
  }

  async function previewTrajectoryFrame(frameIndex: number) {
    const activeProject = currentProject ?? projects[0] ?? null;
    if (!activeProject || !trajectoryIndex) {
      setError("需要先建立轨迹索引，才能读取指定帧。");
      return;
    }
    try {
      const chunk = await api.readTrajectoryChunk({
        projectPath: activeProject.path,
        trajectoryPath: trajectoryIndex.trajectoryPath,
        frameIndices: [frameIndex],
        maxBytes: 750_000
      });
      setTrajectoryChunk(chunk);
    } catch (caught) {
      reportError(caught);
    }
  }

  async function generateTrajectoryAnalysisPackage() {
    const activeProject = currentProject ?? projects[0] ?? null;
    if (!activeProject || !plan) {
      setError("需要先创建项目并生成 SimulationPlan，才能生成 MDAnalysis 分析包。");
      return;
    }
    const trajectoryPath = artifactIndex?.artifacts.find((artifact) => artifact.kind === "trajectory")?.path ?? null;
    try {
      const analysisPackage = await api.prepareTrajectoryAnalysisPackage({
        plan,
        projectPath: activeProject.path,
        topologyPath: plan.system.sourcePath ?? null,
        trajectoryPath,
        selection: "protein and name CA",
        writeToDisk: true
      });
      setTrajectoryAnalysisPackage(analysisPackage);
      await refreshArtifacts();
    } catch (caught) {
      reportError(caught);
    }
  }

  async function exportReport(format: ReportFormat) {
    const activeProject = currentProject ?? projects[0] ?? null;
    if (!activeProject || !plan) {
      setError("需要先创建项目并生成 SimulationPlan，才能导出报告。");
      return;
    }
    try {
      const report = await api.exportReport({
        projectPath: activeProject.path,
        plan,
        task: localSnapshot,
        artifactIndex,
        format
      });
      setExportedReport(report);
      await refreshArtifacts();
    } catch (caught) {
      reportError(caught);
    }
  }

  async function generateRecipes(engineId = selectedEngineId) {
    try {
      const options = defaultBuildRecipeOptions(engineId);
      const [container, build] = await Promise.all([
        api.containerRecipe(engineId),
        api.buildRecipe(options)
      ]);
      setContainerRecipe(container);
      setBuildRecipe(build);
      setBuildWorkflowResult(null);
      setActiveTab("build");
    } catch (caught) {
      reportError(caught);
    }
  }

  async function exportRecipes(engineId = selectedEngineId) {
    const activeProject = currentProject ?? projects[0] ?? null;
    if (!activeProject) {
      setError("需要先创建项目，才能把构建 recipe 导出到项目目录。");
      return;
    }
    try {
      const options = defaultBuildRecipeOptions(engineId);
      const [container, build, exported] = await Promise.all([
        api.containerRecipe(engineId),
        api.buildRecipe(options),
        api.exportRecipePackage({
          projectPath: activeProject.path,
          buildOptions: options,
          includeContainer: true,
          includeBuildScript: true
        })
      ]);
      setContainerRecipe(container);
      setBuildRecipe(build);
      setRecipeExportResult(exported);
      setBuildWorkflowResult(null);
      setActiveTab("build");
      await refreshArtifacts();
    } catch (caught) {
      reportError(caught);
    }
  }

  async function runBuildWizard(engineId = selectedEngineId) {
    const activeProject = currentProject ?? projects[0] ?? null;
    if (!activeProject) {
      setError("需要先创建项目，才能运行构建向导。");
      return;
    }
    try {
      const options = defaultBuildRecipeOptions(engineId);
      const [container, build, result] = await Promise.all([
        api.containerRecipe(engineId),
        api.buildRecipe(options),
        api.runBuildWorkflow({
          projectPath: activeProject.path,
          buildOptions: options,
          includeContainer: true,
          includeBuildScript: true,
          mode: buildWorkflowMode,
          timeoutSeconds: buildWorkflowTimeout
        })
      ]);
      setContainerRecipe(container);
      setBuildRecipe(build);
      setBuildWorkflowResult(result);
      if (result.filesWritten.length || result.logPath) {
        await refreshArtifacts();
      }
      setActiveTab("build");
    } catch (caught) {
      reportError(caught);
    }
  }

  async function generateRemotePackage(profileId = selectedRemoteProfileId) {
    const profile = remoteProfiles.find((item) => item.id === profileId) ?? remoteProfiles[0];
    const activeProject = currentProject ?? projects[0] ?? null;
    if (!plan || !profile) {
      setError("需要先生成 SimulationPlan 并选择远程 profile。");
      return;
    }
    try {
      const generated = await api.remoteExecutionPackage({
        plan,
        profile,
        localProjectPath: activeProject?.path ?? null,
        includeSubmit: true
      });
      setRemotePackage(generated);
      setRemoteJobSnapshot(null);
      setRemoteWorkflowResult(null);
      setSelectedRemoteProfileId(profile.id);
      setActiveTab("remote");
    } catch (caught) {
      reportError(caught);
    }
  }

  async function saveEngineInstallation(record: EngineInstallationRecord) {
    try {
      const saved = await api.saveEngineInstallation({
        ...record,
        checkedAt: new Date().toISOString()
      });
      setEngineInstallations((items) => [
        saved,
        ...items.filter((item) => !(item.engineId === saved.engineId && item.location === saved.location))
      ]);
      const capabilities = await api.engineCapabilities();
      setEngines(capabilities);
      setEngineInstallationDraft(saved);
    } catch (caught) {
      reportError(caught);
    }
  }

  async function deleteEngineInstallation(record: EngineInstallationRecord) {
    try {
      const deleted = await api.deleteEngineInstallation(record.engineId, record.location);
      if (!deleted) {
        setError("未找到要删除的引擎安装记录。");
        return;
      }
      setEngineInstallations((items) =>
        items.filter((item) => !(item.engineId === record.engineId && item.location === record.location))
      );
      const capabilities = await api.engineCapabilities();
      setEngines(capabilities);
    } catch (caught) {
      reportError(caught);
    }
  }

  async function saveRemoteProfile(profile: RemoteProfile) {
    try {
      const saved = await api.saveRemoteProfile(profile);
      setRemoteProfiles((items) => [saved, ...items.filter((item) => item.id !== saved.id)]);
      setSelectedRemoteProfileId(saved.id);
      setRemoteProfileDraft(saved);
      if (plan) {
        const activeProject = currentProject ?? projects[0] ?? null;
        const generated = await api.remoteExecutionPackage({
          plan,
          profile: saved,
          localProjectPath: activeProject?.path ?? null,
          includeSubmit: true
        });
        setRemotePackage(generated);
        setRemoteJobSnapshot(null);
        setRemoteWorkflowResult(null);
      }
    } catch (caught) {
      reportError(caught);
    }
  }

  async function deleteRemoteProfile(id: string) {
    try {
      const deleted = await api.deleteRemoteProfile(id);
      if (!deleted) {
        setError("该 profile 可能是内置模板，未从数据库删除。");
        return;
      }
      const profiles = await api.remoteProfiles();
      setRemoteProfiles(profiles);
      setSelectedRemoteProfileId(profiles[0]?.id ?? null);
      setRemotePackage(null);
      setRemoteJobSnapshot(null);
      setRemoteWorkflowResult(null);
    } catch (caught) {
      reportError(caught);
    }
  }

  async function parseRemoteStatus() {
    const activeEngine = remotePackage?.engineId ?? plan?.engineId;
    const activeScheduler = remotePackage?.scheduler ?? plan?.resources.executionMode;
    if (!activeEngine || !activeScheduler) {
      setError("需要先生成远程执行包，才能解析远程状态。");
      return;
    }
    try {
      const snapshot = await api.parseRemoteJobStatus({
        engineId: activeEngine,
        scheduler: activeScheduler,
        submitOutput: remoteSubmitOutput,
        statusOutput: remoteStatusOutput,
        logOutput: remoteLogOutput
      });
      setRemoteJobSnapshot(snapshot);
    } catch (caught) {
      reportError(caught);
    }
  }

  async function runRemoteStep(stepId: string) {
    const activeProject = currentProject ?? projects[0] ?? null;
    if (!activeProject || !remotePackage) {
      setError("需要先创建项目并生成远程执行包，才能运行远程步骤。");
      return;
    }
    try {
      const result = await api.runRemoteWorkflowStep({
        projectPath: activeProject.path,
        package: remotePackage,
        stepId,
        mode: remoteWorkflowMode,
        jobId: remoteWorkflowJobId || remoteJobSnapshot?.jobId || null,
        timeoutSeconds: remoteWorkflowTimeout
      });
      setRemoteWorkflowResult(result);
      if (result.snapshot) {
        setRemoteJobSnapshot(result.snapshot);
        if (result.snapshot.jobId) {
          setRemoteWorkflowJobId(result.snapshot.jobId);
        }
      }
      if (result.stepId === "submit") {
        setRemoteSubmitOutput(result.stdout || result.stderr || remoteSubmitOutput);
      } else if (result.stepId === "status") {
        setRemoteStatusOutput(result.stdout || result.stderr || remoteStatusOutput);
      } else if (result.stepId === "tail-log") {
        setRemoteLogOutput(result.stdout || result.stderr || remoteLogOutput);
      }
      if (result.filesWritten.length) {
        await refreshArtifacts();
      }
    } catch (caught) {
      reportError(caught);
    }
  }

  function updatePlan(updater: (current: SimulationPlan) => SimulationPlan) {
    setPlan((current) => (current ? updater(current) : current));
  }

  function updateStageParameter(stageId: string, key: string, value: string) {
    updatePlan((current) => ({
      ...current,
      stages: current.stages.map((stage) =>
        stage.id === stageId
          ? { ...stage, parameters: { ...stage.parameters, [key]: value } }
          : stage
      )
    }));
  }

  function toggleStage(stageId: string) {
    updatePlan((current) => ({
      ...current,
      stages: current.stages.map((stage) =>
        stage.id === stageId ? { ...stage, enabled: !stage.enabled } : stage
      )
    }));
  }

  function reportError(caught: unknown) {
    setError(caught instanceof Error ? caught.message : String(caught));
  }

  return (
    <main className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark">A</div>
          <div>
            <h1>AutoMD</h1>
            <p>MD workflow studio</p>
          </div>
        </div>
        <nav className="nav-list" aria-label="AutoMD sections">
          {tabs.map((tab) => (
            <button
              className={`nav-item ${activeTab === tab.id ? "active" : ""}`}
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              type="button"
            >
              <span>{tab.label}</span>
              <small>{tab.description}</small>
            </button>
          ))}
        </nav>
        <div className="sidebar-footer">
          <span className="status-dot ready" />
          <span>{readyCount} 个本地能力已检测可用</span>
        </div>
      </aside>

      <section className="workspace">
        <header className="topbar">
          <div>
            <p className="eyebrow">跨平台生物分子 MD 首版</p>
            <h2>{tabs.find((tab) => tab.id === activeTab)?.label}</h2>
          </div>
          <div className="topbar-actions">
            <select
              value={selectedEngineId}
              onChange={(event) => {
                const engineId = event.target.value;
                setSelectedEngineId(engineId);
                updatePlan((current) => ({ ...current, engineId }));
              }}
            >
              {engines.map((engine) => (
                <option value={engine.id} key={engine.id}>
                  {engine.name}
                </option>
              ))}
            </select>
            <button type="button" className="primary" onClick={queueMockTask}>
              生成运行计划
            </button>
          </div>
        </header>

        {error ? (
          <div className="error-banner">
            <strong>执行错误</strong>
            <span>{error}</span>
            <button type="button" onClick={() => setError(null)}>
              关闭
            </button>
          </div>
        ) : null}

        {activeTab === "overview" && (
          <OverviewPanel
            openSourceCount={openSourceCount}
            externalCount={externalCount}
            readyCount={readyCount}
            project={currentProject ?? projects[0] ?? null}
            plan={plan}
            validation={validation}
          />
        )}

        {activeTab === "projects" && (
          <ProjectsPanel
            projects={projects}
            projectName={projectName}
            setProjectName={setProjectName}
            domain={domain}
            setDomain={setDomain}
            project={currentProject ?? projects[0] ?? null}
            selectedEngineId={selectedEngineId}
            engines={engines}
            importSourceKind={importSourceKind}
            setImportSourceKind={setImportSourceKind}
            importSourcePath={importSourcePath}
            setImportSourcePath={setImportSourcePath}
            importSmiles={importSmiles}
            setImportSmiles={setImportSmiles}
            importDisplayName={importDisplayName}
            setImportDisplayName={setImportDisplayName}
            structureImportResult={structureImportResult}
            createProject={createProject}
            importStructure={importStructure}
            setCurrentProject={setCurrentProject}
          />
        )}

        {activeTab === "engines" && (
          <EnginesPanel
            engines={engines}
            selectedEngineId={selectedEngineId}
            setSelectedEngineId={setSelectedEngineId}
            engineInstallations={engineInstallations}
            engineInstallationDraft={engineInstallationDraft}
            setEngineInstallationDraft={setEngineInstallationDraft}
            saveEngineInstallation={saveEngineInstallation}
            deleteEngineInstallation={deleteEngineInstallation}
            generateRecipes={generateRecipes}
          />
        )}

        {activeTab === "workflow" && plan && (
          <WorkflowPanel
            plan={plan}
            validation={validation}
            parameterMappingReport={parameterMappingReport}
            scienceDiagnostics={scienceDiagnostics}
            preparationPackage={preparationPackage}
            updatePlan={updatePlan}
            updateStageParameter={updateStageParameter}
            toggleStage={toggleStage}
            generatePreparationPackage={generatePreparationPackage}
          />
        )}

        {activeTab === "run" && (
          <RunPanel
            plan={plan}
            task={task}
            validation={validation}
            slurmScript={slurmScript}
            runPackage={runPackage}
            batchReplicateCount={batchReplicateCount}
            setBatchReplicateCount={setBatchReplicateCount}
            batchSeedStart={batchSeedStart}
            setBatchSeedStart={setBatchSeedStart}
            batchPackage={batchPackage}
            nativeFile={nativeFile}
            nativeFileDraft={nativeFileDraft}
            nativeFileMessage={nativeFileMessage}
            setNativeFileDraft={setNativeFileDraft}
            sampleLog={sampleLog}
            setSampleLog={setSampleLog}
            logReport={logReport}
            sampleFailureAnalysis={sampleFailureAnalysis}
            localRunMode={localRunMode}
            setLocalRunMode={setLocalRunMode}
            localSnapshot={localSnapshot}
            taskRecords={taskRecords}
            resumePlan={localSnapshot?.resumePlan ?? manualResumePlan}
            artifactIndex={artifactIndex}
            analysisResult={analysisResult}
            trajectoryIndex={trajectoryIndex}
            trajectoryChunk={trajectoryChunk}
            trajectoryAnalysisPackage={trajectoryAnalysisPackage}
            refreshArtifacts={refreshArtifacts}
            refreshTaskRecords={refreshTaskRecords}
            indexTrajectory={indexTrajectory}
            previewTrajectoryFrame={previewTrajectoryFrame}
            generateTrajectoryAnalysisPackage={generateTrajectoryAnalysisPackage}
            selectedEngine={selectedEngine}
            queueMockTask={queueMockTask}
            generateBatchExperiment={generateBatchExperiment}
            openNativeFile={openNativeFile}
            saveNativeFile={saveNativeFile}
            parseLogSample={parseLogSample}
            startLocalRun={startLocalRun}
            cancelLocalRun={cancelLocalRun}
            discoverResumePlan={discoverResumePlan}
          />
        )}

        {activeTab === "remote" && (
          <RemotePanel
            diagnostics={diagnostics}
            plan={plan}
            remoteProfiles={remoteProfiles}
            selectedRemoteProfileId={selectedRemoteProfileId}
            setSelectedRemoteProfileId={setSelectedRemoteProfileId}
            remotePackage={remotePackage}
            remoteJobSnapshot={remoteJobSnapshot}
            remoteWorkflowMode={remoteWorkflowMode}
            setRemoteWorkflowMode={setRemoteWorkflowMode}
            remoteWorkflowJobId={remoteWorkflowJobId}
            setRemoteWorkflowJobId={setRemoteWorkflowJobId}
            remoteWorkflowTimeout={remoteWorkflowTimeout}
            setRemoteWorkflowTimeout={setRemoteWorkflowTimeout}
            remoteWorkflowResult={remoteWorkflowResult}
            remoteProfileDraft={remoteProfileDraft}
            setRemoteProfileDraft={setRemoteProfileDraft}
            saveRemoteProfile={saveRemoteProfile}
            deleteRemoteProfile={deleteRemoteProfile}
            remoteSubmitOutput={remoteSubmitOutput}
            setRemoteSubmitOutput={setRemoteSubmitOutput}
            remoteStatusOutput={remoteStatusOutput}
            setRemoteStatusOutput={setRemoteStatusOutput}
            remoteLogOutput={remoteLogOutput}
            setRemoteLogOutput={setRemoteLogOutput}
            parseRemoteStatus={parseRemoteStatus}
            runRemoteStep={runRemoteStep}
            updatePlan={updatePlan}
            generateRemotePackage={generateRemotePackage}
          />
        )}

        {activeTab === "build" && (
          <BuildPanel
            engines={engines}
            selectedEngineId={selectedEngineId}
            containerRecipe={containerRecipe}
            buildRecipe={buildRecipe}
            recipeExportResult={recipeExportResult}
            buildWorkflowMode={buildWorkflowMode}
            setBuildWorkflowMode={setBuildWorkflowMode}
            buildWorkflowTimeout={buildWorkflowTimeout}
            setBuildWorkflowTimeout={setBuildWorkflowTimeout}
            buildWorkflowResult={buildWorkflowResult}
            generateRecipes={generateRecipes}
            exportRecipes={exportRecipes}
            runBuildWizard={runBuildWizard}
          />
        )}

        {activeTab === "plugins" && (
          <PluginsPanel pluginRegistry={pluginRegistry} />
        )}

        {activeTab === "report" && (
          <ReportPanel
            project={currentProject ?? projects[0] ?? null}
            plan={plan}
            validation={validation}
            artifactIndex={artifactIndex}
            artifactRecords={artifactRecords}
            analysisResult={analysisResult}
            analysisCacheRecords={analysisCacheRecords}
            exportedReport={exportedReport}
            refreshArtifacts={refreshArtifacts}
            refreshAnalysis={refreshAnalysis}
            exportReport={exportReport}
          />
        )}
      </section>
    </main>
  );
}

function OverviewPanel({
  openSourceCount,
  externalCount,
  readyCount,
  project,
  plan,
  validation
}: {
  openSourceCount: number;
  externalCount: number;
  readyCount: number;
  project: ProjectSummary | null;
  plan: SimulationPlan | null;
  validation: ValidationReport | null;
}) {
  return (
    <div className="content-grid overview-grid">
      <section className="panel span-2">
        <MoleculeViewport plan={plan} project={project} />
      </section>
      <section className="panel">
        <h3>能力快照</h3>
        <div className="metric-grid">
          <Metric label="开源/自由工具" value={openSourceCount} />
          <Metric label="用户自带许可" value={externalCount} />
          <Metric label="本地可用" value={readyCount} />
          <Metric label="流程阶段" value={plan?.stages.length ?? 0} />
        </div>
      </section>
      <section className="panel">
        <h3>当前项目</h3>
        {project ? (
          <dl className="definition-list">
            <div><dt>名称</dt><dd>{project.name}</dd></div>
            <div><dt>领域</dt><dd>{project.domain}</dd></div>
            <div><dt>状态</dt><dd>{project.status}</dd></div>
            <div><dt>目录</dt><dd className="mono">{project.path}</dd></div>
          </dl>
        ) : (
          <EmptyState title="尚未创建项目" text="创建项目后会生成可复现实验目录和 SQLite 索引。" />
        )}
      </section>
      <section className="panel span-2">
        <h3>首版路线</h3>
        <div className="roadmap">
          {["M0 架构和 schema", "M1 GUI 骨架", "M2 GROMACS 闭环", "M3 多引擎", "M4 远程/HPC", "M5 编译与扩展"].map((item, index) => (
            <div className="roadmap-item" key={item}>
              <span>{index + 1}</span>
              <p>{item}</p>
            </div>
          ))}
        </div>
      </section>
      <section className="panel">
        <h3>校验</h3>
        <ValidationList validation={validation} />
      </section>
    </div>
  );
}

function ProjectsPanel({
  projects,
  projectName,
  setProjectName,
  domain,
  setDomain,
  project,
  selectedEngineId,
  engines,
  importSourceKind,
  setImportSourceKind,
  importSourcePath,
  setImportSourcePath,
  importSmiles,
  setImportSmiles,
  importDisplayName,
  setImportDisplayName,
  structureImportResult,
  createProject,
  importStructure,
  setCurrentProject
}: {
  projects: ProjectSummary[];
  projectName: string;
  setProjectName: (value: string) => void;
  domain: ProjectDomain;
  setDomain: (value: ProjectDomain) => void;
  project: ProjectSummary | null;
  selectedEngineId: string;
  engines: EngineCapability[];
  importSourceKind: StructureSourceKind;
  setImportSourceKind: (value: StructureSourceKind) => void;
  importSourcePath: string;
  setImportSourcePath: (value: string) => void;
  importSmiles: string;
  setImportSmiles: (value: string) => void;
  importDisplayName: string;
  setImportDisplayName: (value: string) => void;
  structureImportResult: StructureImportResult | null;
  createProject: () => void;
  importStructure: () => void;
  setCurrentProject: (project: ProjectSummary) => void;
}) {
  return (
    <div className="content-grid">
      <section className="panel">
        <h3>创建项目</h3>
        <label>
          项目名称
          <input value={projectName} onChange={(event) => setProjectName(event.target.value)} />
        </label>
        <label>
          领域
          <select value={domain} onChange={(event) => setDomain(event.target.value as ProjectDomain)}>
            <option value="biomolecular">生物分子</option>
            <option value="materials">材料体系</option>
            <option value="qmmm">QM/MM</option>
          </select>
        </label>
        <label>
          首选引擎
          <input value={engineLabel[selectedEngineId] ?? selectedEngineId} readOnly />
        </label>
        <button type="button" className="primary fill" onClick={createProject}>
          创建并生成默认流程
        </button>
      </section>
      <section className="panel">
        <h3>导入结构</h3>
        <label>
          当前项目
          <input value={project?.name ?? "先创建或选择项目"} readOnly />
        </label>
        <label>
          输入类型
          <select value={importSourceKind} onChange={(event) => setImportSourceKind(event.target.value as StructureSourceKind)}>
            <option value="pdb">PDB</option>
            <option value="mmcif">mmCIF</option>
            <option value="sdf">SDF</option>
            <option value="mol2">MOL2</option>
            <option value="smiles">SMILES</option>
            <option value="engineProject">已有引擎工程</option>
          </select>
        </label>
        {importSourceKind === "smiles" ? (
          <label>
            SMILES
            <textarea value={importSmiles} onChange={(event) => setImportSmiles(event.target.value)} />
          </label>
        ) : (
          <label>
            文件路径
            <input
              value={importSourcePath}
              placeholder="/path/to/system.pdb"
              onChange={(event) => setImportSourcePath(event.target.value)}
            />
          </label>
        )}
        <label>
          显示名称
          <input
            value={importDisplayName}
            placeholder="留空则使用文件名"
            onChange={(event) => setImportDisplayName(event.target.value)}
          />
        </label>
        <button type="button" className="primary fill" onClick={importStructure}>
          导入到 inputs/
        </button>
        {structureImportResult ? (
          <div className="structure-summary">
            <dl className="definition-list">
              <div><dt>路径</dt><dd className="mono">{structureImportResult.importedPath}</dd></div>
              <div><dt>原子</dt><dd>{structureImportResult.summary.atomCount ?? "n/a"}</dd></div>
              <div><dt>残基/分子</dt><dd>{structureImportResult.summary.residueCount ?? structureImportResult.summary.moleculeCount ?? "n/a"}</dd></div>
              <div><dt>链</dt><dd>{structureImportResult.summary.chainCount ?? "n/a"}</dd></div>
            </dl>
            {structureImportResult.warnings.length ? (
              <div className="warning-stack">
                {structureImportResult.warnings.map((warning) => <p key={warning}>{warning}</p>)}
              </div>
            ) : null}
          </div>
        ) : null}
      </section>
      <section className="panel span-2">
        <h3>项目索引</h3>
        {projects.length === 0 ? (
          <EmptyState title="暂无项目" text="AutoMD 会为每个项目创建 inputs、generated、runs、trajectories、analysis、reports、remote 等目录。" />
        ) : (
          <div className="table">
            <div className="table-head four">
              <span>名称</span><span>领域</span><span>引擎</span><span>目录</span>
            </div>
            {projects.map((project) => (
              <button className="table-row four" type="button" key={project.id} onClick={() => setCurrentProject(project)}>
                <span>{project.name}</span>
                <span>{project.domain}</span>
                <span>{engines.find((engine) => engine.id === project.preferredEngineId)?.name ?? "未指定"}</span>
                <span className="mono truncate">{project.path}</span>
              </button>
            ))}
          </div>
        )}
      </section>
    </div>
  );
}

function EnginesPanel({
  engines,
  selectedEngineId,
  setSelectedEngineId,
  engineInstallations,
  engineInstallationDraft,
  setEngineInstallationDraft,
  saveEngineInstallation,
  deleteEngineInstallation,
  generateRecipes
}: {
  engines: EngineCapability[];
  selectedEngineId: string;
  setSelectedEngineId: (engineId: string) => void;
  engineInstallations: EngineInstallationRecord[];
  engineInstallationDraft: EngineInstallationRecord;
  setEngineInstallationDraft: (record: EngineInstallationRecord) => void;
  saveEngineInstallation: (record: EngineInstallationRecord) => void;
  deleteEngineInstallation: (record: EngineInstallationRecord) => void;
  generateRecipes: (engineId?: string) => void;
}) {
  const selectedEngine = engines.find((engine) => engine.id === selectedEngineId) ?? engines[0];
  const selectedRecords = engineInstallations.filter((record) => record.engineId === selectedEngineId);
  return (
    <div className="content-grid">
      <section className="panel span-3">
        <div className="panel-title-row">
          <h3>引擎能力矩阵</h3>
          <button type="button" onClick={() => generateRecipes(selectedEngineId)}>
            生成当前引擎 recipe
          </button>
        </div>
        <div className="engine-grid">
          {engines.map((engine) => (
            <button
              type="button"
              key={engine.id}
              className={`engine-card ${selectedEngineId === engine.id ? "selected" : ""}`}
              onClick={() => {
                setSelectedEngineId(engine.id);
                setEngineInstallationDraft({ ...engineInstallationDraft, engineId: engine.id });
              }}
            >
              <div className="engine-card-head">
                <strong>{engine.name}</strong>
                <StatusPill status={engine.detection.status} />
              </div>
              <p>{engine.license.guidance}</p>
              <div className="chip-row">
                <span>{engine.category}</span>
                <span>{engine.maturity}</span>
                <span>{engine.license.requiresUserLicense ? "用户许可" : "开源优先"}</span>
              </div>
              <dl className="compact-dl">
                <div><dt>入口</dt><dd>{engine.executableNames.join(", ")}</dd></div>
                <div><dt>GPU</dt><dd>{engine.gpuBackends.map((gpu) => gpuBackendText[gpu]).join(", ")}</dd></div>
                <div><dt>平台</dt><dd>{engine.platformSupport.native.join(", ")}</dd></div>
                <div><dt>路径</dt><dd className="mono">{engine.detection.path ?? "未配置"}</dd></div>
              </dl>
            </button>
          ))}
        </div>
      </section>
      <section className="panel">
        <h3>手动安装 / 授权记录</h3>
        <div className="engine-install-form">
          <label>
            引擎
            <select
              value={engineInstallationDraft.engineId}
              onChange={(event) =>
                setEngineInstallationDraft({ ...engineInstallationDraft, engineId: event.target.value })
              }
            >
              {engines.map((engine) => (
                <option key={engine.id} value={engine.id}>{engine.name}</option>
              ))}
            </select>
          </label>
          <label>
            路径 / 模块入口
            <input
              value={engineInstallationDraft.location}
              onChange={(event) =>
                setEngineInstallationDraft({ ...engineInstallationDraft, location: event.target.value })
              }
              placeholder="C:\\gromacs\\bin\\gmx.exe, /opt/namd/namd3, python3::openmm"
            />
          </label>
          <label>
            版本
            <input
              value={engineInstallationDraft.version ?? ""}
              onChange={(event) =>
                setEngineInstallationDraft({ ...engineInstallationDraft, version: event.target.value || null })
              }
              placeholder={selectedEngine?.detection.version ?? "optional"}
            />
          </label>
          <label>
            授权状态
            <select
              value={engineInstallationDraft.authorizationStatus}
              onChange={(event) =>
                setEngineInstallationDraft({
                  ...engineInstallationDraft,
                  authorizationStatus: event.target.value as DetectionStatus
                })
              }
            >
              <option value="ready">可直接使用</option>
              <option value="missingInstall">需要安装检查</option>
              <option value="missingLicense">需要许可证/授权</option>
              <option value="platformUnsupported">平台不支持</option>
              <option value="remoteRecommended">建议远程运行</option>
            </select>
          </label>
        </div>
        <button type="button" className="primary fill" onClick={() => saveEngineInstallation(engineInstallationDraft)}>
          保存安装记录
        </button>
        <p className="hint-text">
          受限/商业引擎只保存用户配置的路径和授权状态；AutoMD 不下载、不镜像、不分发这些二进制。
        </p>
      </section>
      <section className="panel span-2">
        <h3>{selectedEngine?.name ?? selectedEngineId} 保存记录</h3>
        {selectedRecords.length ? (
          <div className="engine-install-list">
            {selectedRecords.map((record) => (
              <div className="engine-install-row" key={`${record.engineId}-${record.location}`}>
                <div>
                  <strong className="mono">{record.location}</strong>
                  <small>{record.version ?? "version unknown"} · {new Date(record.checkedAt).toLocaleString()}</small>
                </div>
                <StatusPill status={record.authorizationStatus} />
                <button type="button" onClick={() => deleteEngineInstallation(record)}>删除</button>
              </div>
            ))}
          </div>
        ) : (
          <EmptyState title="暂无保存记录" text="保存路径后，AutoMD 会在下一次能力检测中优先显示用户配置。" />
        )}
      </section>
    </div>
  );
}

function WorkflowPanel({
  plan,
  validation,
  parameterMappingReport,
  scienceDiagnostics,
  preparationPackage,
  updatePlan,
  updateStageParameter,
  toggleStage,
  generatePreparationPackage
}: {
  plan: SimulationPlan;
  validation: ValidationReport | null;
  parameterMappingReport: ParameterMappingReport | null;
  scienceDiagnostics: ScienceSidecarDiagnostics | null;
  preparationPackage: StructurePreparationPackage | null;
  updatePlan: (updater: (current: SimulationPlan) => SimulationPlan) => void;
  updateStageParameter: (stageId: string, key: string, value: string) => void;
  toggleStage: (stageId: string) => void;
  generatePreparationPackage: () => void;
}) {
  return (
    <div className="content-grid">
      <section className="panel span-2">
        <h3>SimulationPlan</h3>
        <div className="form-grid three">
          <label>
            体系名
            <input
              value={plan.system.name}
              onChange={(event) =>
                updatePlan((current) => ({
                  ...current,
                  system: { ...current.system, name: event.target.value }
                }))
              }
            />
          </label>
          <label>
            蛋白力场
            <select
              value={plan.forceField.protein}
              onChange={(event) =>
                updatePlan((current) => ({
                  ...current,
                  forceField: { ...current.forceField, protein: event.target.value }
                }))
              }
            >
              <option>CHARMM36m</option>
              <option>AMBER ff19SB</option>
              <option>OPLS-AA/M</option>
              <option>user-defined</option>
            </select>
          </label>
          <label>
            水模型
            <select
              value={plan.forceField.waterModel}
              onChange={(event) =>
                updatePlan((current) => ({
                  ...current,
                  forceField: { ...current.forceField, waterModel: event.target.value }
                }))
              }
            >
              <option>TIP3P</option>
              <option>TIP4P-Ew</option>
              <option>SPC/E</option>
              <option>OPC</option>
            </select>
          </label>
          <label>
            水盒 padding (nm)
            <input
              type="number"
              min="0.1"
              step="0.1"
              value={plan.solvent.paddingNm}
              onChange={(event) =>
                updatePlan((current) => ({
                  ...current,
                  solvent: { ...current.solvent, paddingNm: Number(event.target.value) }
                }))
              }
            />
          </label>
          <label>
            盐浓度 (M)
            <input
              type="number"
              min="0"
              step="0.01"
              value={plan.solvent.ionicStrengthMolar}
              onChange={(event) =>
                updatePlan((current) => ({
                  ...current,
                  solvent: { ...current.solvent, ionicStrengthMolar: Number(event.target.value) }
                }))
              }
            />
          </label>
          <label>
            生产时长 (h)
            <input
              type="number"
              min="1"
              step="1"
              value={plan.resources.walltimeHours}
              onChange={(event) =>
                updatePlan((current) => ({
                  ...current,
                  resources: { ...current.resources, walltimeHours: Number(event.target.value) }
                }))
              }
            />
          </label>
        </div>
      </section>
      <section className="panel">
        <h3>分析模块</h3>
        <div className="toggle-list">
          {plan.analysis.map((module) => (
            <label className="check-row" key={module.kind}>
              <input
                type="checkbox"
                checked={module.enabled}
                onChange={() =>
                  updatePlan((current) => ({
                    ...current,
                    analysis: current.analysis.map((item) =>
                      item.kind === module.kind ? { ...item, enabled: !item.enabled } : item
                    )
                  }))
                }
              />
              <span>{analysisText[module.kind]}</span>
            </label>
          ))}
        </div>
      </section>
      <section className="panel span-3">
        <h3>预期输出</h3>
        <div className="output-spec-grid">
          {(Object.entries(plan.outputs) as Array<[keyof OutputSpec, string[]]>).map(([key, paths]) => (
            <div className="output-spec-group" key={key}>
              <strong>{outputSpecText[key]}</strong>
              <div className="chip-row outputs">
                {paths.map((path) => (
                  <span className="mono" key={path}>{path}</span>
                ))}
              </div>
            </div>
          ))}
        </div>
      </section>
      <section className="panel span-3">
        <h3>多引擎参数映射</h3>
        <ParameterMappingList report={parameterMappingReport} />
      </section>
      <section className="panel span-3">
        <div className="panel-title-row">
          <h3>Python 科学侧车</h3>
          <button type="button" className="primary" onClick={generatePreparationPackage}>
            生成结构准备包
          </button>
        </div>
        <div className="sidecar-grid">
          <div>
            <h4>依赖诊断</h4>
            {scienceDiagnostics ? (
              <div className="tool-list compact-tools">
                {scienceDiagnostics.tools.map((tool) => (
                  <div className="tool-row" key={tool.id}>
                    <div>
                      <strong>{tool.label}</strong>
                      <small>{tool.importName ?? tool.command ?? tool.id}</small>
                    </div>
                    <StatusPill status={tool.status} />
                  </div>
                ))}
              </div>
            ) : (
              <EmptyState title="等待诊断" text="启动后会检测 OpenMM、PDBFixer、MDAnalysis、RDKit、Open Babel 和 AmberTools。" />
            )}
          </div>
          <div>
            <h4>推荐环境</h4>
            <CodeBlock value={scienceDiagnostics?.environmentRecipe ?? "等待侧车诊断。"} />
          </div>
          <div>
            <h4>准备包</h4>
            {preparationPackage ? (
              <div className="run-package">
                <dl className="definition-list">
                  <div><dt>目录</dt><dd className="mono">{preparationPackage.generatedDirectory}</dd></div>
                  <div><dt>文件</dt><dd>{preparationPackage.files.length}</dd></div>
                  <div><dt>命令</dt><dd>{preparationPackage.commands.length}</dd></div>
                  <div><dt>写入</dt><dd>{preparationPackage.files.some((file) => file.written) ? "是" : "预览"}</dd></div>
                </dl>
                {preparationPackage.warnings.length ? (
                  <div className="warning-stack">
                    {preparationPackage.warnings.map((warning) => <p key={warning}>{warning}</p>)}
                  </div>
                ) : null}
                <div className="file-list">
                  {preparationPackage.files.map((file) => (
                    <div className="file-row" key={file.path}>
                      <span className="mono">{file.path}</span>
                      <small>{file.language}</small>
                    </div>
                  ))}
                </div>
              </div>
            ) : (
              <EmptyState title="尚未生成准备包" text="准备包会写入 PDBFixer/OpenMM 脚本、environment.yml 和配体参数化指引。" />
            )}
          </div>
        </div>
      </section>
      <section className="panel span-3">
        <h3>阶段参数</h3>
        <div className="stage-list">
          {plan.stages.map((stage) => (
            <StageEditor
              stage={stage}
              key={stage.id}
              updateStageParameter={updateStageParameter}
              toggleStage={toggleStage}
            />
          ))}
        </div>
      </section>
      <section className="panel span-3">
        <h3>校验结果</h3>
        <ValidationList validation={validation} />
      </section>
    </div>
  );
}

function StageEditor({
  stage,
  updateStageParameter,
  toggleStage
}: {
  stage: SimulationStage;
  updateStageParameter: (stageId: string, key: string, value: string) => void;
  toggleStage: (stageId: string) => void;
}) {
  return (
    <div className={`stage-row ${stage.enabled ? "" : "disabled"}`}>
      <label className="stage-check">
        <input type="checkbox" checked={stage.enabled} onChange={() => toggleStage(stage.id)} />
        <strong>{stage.label}</strong>
      </label>
      <div className="stage-params">
        {Object.entries(stage.parameters).map(([key, value]) => (
          <label key={key}>
            {key}
            <input value={value} onChange={(event) => updateStageParameter(stage.id, key, event.target.value)} />
          </label>
        ))}
      </div>
      <div className="chip-row outputs">
        {stage.expectedOutputs.map((output) => (
          <span key={output}>{output}</span>
        ))}
      </div>
    </div>
  );
}

function RunPanel({
  plan,
  task,
  validation,
  slurmScript,
  runPackage,
  batchReplicateCount,
  setBatchReplicateCount,
  batchSeedStart,
  setBatchSeedStart,
  batchPackage,
  nativeFile,
  nativeFileDraft,
  nativeFileMessage,
  setNativeFileDraft,
  sampleLog,
  setSampleLog,
  logReport,
  sampleFailureAnalysis,
  localRunMode,
  setLocalRunMode,
  localSnapshot,
  taskRecords,
  resumePlan,
  artifactIndex,
  analysisResult,
  trajectoryIndex,
  trajectoryChunk,
  trajectoryAnalysisPackage,
  refreshArtifacts,
  refreshTaskRecords,
  indexTrajectory,
  previewTrajectoryFrame,
  generateTrajectoryAnalysisPackage,
  selectedEngine,
  queueMockTask,
  generateBatchExperiment,
  openNativeFile,
  saveNativeFile,
  parseLogSample,
  startLocalRun,
  cancelLocalRun,
  discoverResumePlan
}: {
  plan: SimulationPlan | null;
  task: SimulationTask | null;
  validation: ValidationReport | null;
  slurmScript: string;
  runPackage: EngineRunPackage | null;
  batchReplicateCount: number;
  setBatchReplicateCount: (value: number) => void;
  batchSeedStart: number;
  setBatchSeedStart: (value: number) => void;
  batchPackage: BatchExperimentPackage | null;
  nativeFile: ProjectTextFilePayload | null;
  nativeFileDraft: string;
  nativeFileMessage: string | null;
  setNativeFileDraft: (value: string) => void;
  sampleLog: string;
  setSampleLog: (value: string) => void;
  logReport: EngineLogReport | null;
  sampleFailureAnalysis: FailureAnalysis | null;
  localRunMode: LocalRunMode;
  setLocalRunMode: (value: LocalRunMode) => void;
  localSnapshot: LocalTaskSnapshot | null;
  taskRecords: TaskRecord[];
  resumePlan: ResumePlan | null;
  artifactIndex: ArtifactIndex | null;
  analysisResult: AnalysisParseResult | null;
  trajectoryIndex: TrajectoryIndex | null;
  trajectoryChunk: TrajectoryChunk | null;
  trajectoryAnalysisPackage: TrajectoryAnalysisPackage | null;
  refreshArtifacts: () => void;
  refreshTaskRecords: () => void;
  indexTrajectory: (trajectoryPath?: string) => void;
  previewTrajectoryFrame: (frameIndex: number) => void;
  generateTrajectoryAnalysisPackage: () => void;
  selectedEngine?: EngineCapability;
  queueMockTask: () => void;
  generateBatchExperiment: () => void;
  openNativeFile: (path: string, fallbackContents?: string, fallbackLanguage?: string) => void;
  saveNativeFile: () => void;
  parseLogSample: () => void;
  startLocalRun: () => void;
  cancelLocalRun: () => void;
  discoverResumePlan: () => void;
}) {
  const localTaskActive = Boolean(localSnapshot && !["completed", "failed", "cancelled"].includes(localSnapshot.status));
  const generatedFiles = [...(runPackage?.files ?? []), ...(batchPackage?.files ?? [])];
  return (
    <div className="content-grid">
      <section className="panel">
        <h3>启动前检查</h3>
        {selectedEngine ? (
          <dl className="definition-list">
            <div><dt>引擎</dt><dd>{selectedEngine.name}</dd></div>
            <div><dt>授权</dt><dd>{selectedEngine.license.requiresUserLicense ? "需要用户许可确认" : "开源/自由工具"}</dd></div>
            <div><dt>检测</dt><dd><StatusPill status={selectedEngine.detection.status} /></dd></div>
            <div><dt>路径</dt><dd className="mono">{selectedEngine.detection.path ?? "未检测到"}</dd></div>
          </dl>
        ) : null}
        <ValidationList validation={validation} />
        <button type="button" className="primary fill" onClick={queueMockTask}>
          生成 run package
        </button>
      </section>
      <section className="panel">
        <h3>批量重复实验</h3>
        <div className="field-grid two">
          <label>
            Replica 数
            <input
              type="number"
              min={1}
              max={64}
              value={batchReplicateCount}
              onChange={(event) => setBatchReplicateCount(Number(event.target.value))}
            />
          </label>
          <label>
            Seed 起点
            <input
              type="number"
              min={0}
              value={batchSeedStart}
              onChange={(event) => setBatchSeedStart(Number(event.target.value))}
            />
          </label>
        </div>
        <button type="button" className="primary fill" onClick={generateBatchExperiment} disabled={!plan}>
          生成 batch package
        </button>
        {batchPackage ? (
          <div className="run-package">
            <dl className="definition-list">
              <div><dt>目录</dt><dd className="mono">{batchPackage.generatedDirectory}</dd></div>
              <div><dt>Replicas</dt><dd>{batchPackage.replicas.length}</dd></div>
              <div><dt>文件数</dt><dd>{batchPackage.files.length}</dd></div>
              <div><dt>写入磁盘</dt><dd>{batchPackage.files.some((file) => file.written) ? "是" : "否"}</dd></div>
            </dl>
            <div className="replica-list">
              {batchPackage.replicas.map((replica) => (
                <div className="replica-row" key={replica.plan.id}>
                  <strong>#{String(replica.replicaIndex).padStart(2, "0")}</strong>
                  <span>seed {replica.seed}</span>
                  <span className="mono">{replica.runDirectory}</span>
                </div>
              ))}
            </div>
            <details>
              <summary>Batch 命令</summary>
              <div className="command-list">
                {batchPackage.commands.map((command) => (
                  <details key={command.stageId}>
                    <summary>{command.label}</summary>
                    <CodeBlock value={command.command} />
                  </details>
                ))}
              </div>
            </details>
          </div>
        ) : (
          <EmptyState title="尚未生成 batch" text="用于多 seed / 多 replica 的重复实验；生成后会写入 generated/batch 并复用当前引擎适配器。" />
        )}
      </section>
      <section className="panel">
        <h3>本地执行</h3>
        <label>
          运行模式
          <select value={localRunMode} onChange={(event) => setLocalRunMode(event.target.value as LocalRunMode)}>
            <option value="dryRun">Dry run：只写入/校验，不启动进程</option>
            <option value="mock">Mock runner：快速模拟完整生命周期</option>
            <option value="real">真实本地执行：启动 run-gromacs.sh</option>
          </select>
        </label>
        <div className="button-row">
          <button type="button" className="primary" onClick={startLocalRun}>
            启动本地任务
          </button>
          <button type="button" onClick={cancelLocalRun} disabled={!localTaskActive}>
            取消任务
          </button>
        </div>
        {localSnapshot ? (
          <>
            <div className="progress-shell">
              <div className="progress-bar" style={{ width: `${localSnapshot.progressPercent}%` }} />
            </div>
            <dl className="definition-list">
              <div><dt>模式</dt><dd>{localRunModeText[localSnapshot.mode]}</dd></div>
              <div><dt>状态</dt><dd>{localSnapshot.status}</dd></div>
              <div><dt>步数</dt><dd>{localSnapshot.currentStep ?? "未检测"}</dd></div>
              <div><dt>性能</dt><dd>{localSnapshot.nsPerDay ? `${localSnapshot.nsPerDay.toFixed(3)} ns/day` : "未检测"}</dd></div>
              <div><dt>命令</dt><dd className="mono">{localSnapshot.command || "无"}</dd></div>
            </dl>
            {localSnapshot.errorMessage ? (
              <div className="error-inline">{localSnapshot.errorMessage}</div>
            ) : null}
            <FailureAnalysisCard analysis={localSnapshot.failureAnalysis ?? null} />
            <pre className="log-tail">{localSnapshot.logTail.join("\n")}</pre>
          </>
        ) : (
          <EmptyState title="暂无本地任务" text="推荐先用 Mock runner 验证 GUI 监控链路，再切换真实本地执行。" />
        )}
      </section>
      <section className="panel">
        <ResumePlanCard resumePlan={resumePlan} onDiscover={discoverResumePlan} />
      </section>
      <section className="panel">
        <div className="panel-title-row">
          <h3>SQLite 任务历史</h3>
          <button type="button" onClick={refreshTaskRecords}>
            刷新
          </button>
        </div>
        <TaskRecordList records={taskRecords} />
      </section>
      <section className="panel">
        <h3>任务记录</h3>
        {task ? (
          <>
            <div className="progress-shell">
              <div className="progress-bar" style={{ width: `${task.progressPercent}%` }} />
            </div>
            <dl className="definition-list">
              <div><dt>任务</dt><dd className="mono">{task.id}</dd></div>
              <div><dt>状态</dt><dd>{task.status}</dd></div>
              <div><dt>阶段</dt><dd>{task.currentStage ?? "未开始"}</dd></div>
            </dl>
            <pre className="log-tail">{task.logTail.join("\n")}</pre>
          </>
        ) : (
          <EmptyState title="暂无任务" text="生成运行计划后会创建可恢复的任务记录。" />
        )}
      </section>
      <section className="panel span-3">
        <div className="panel-title-row">
          <h3>Artifacts</h3>
          <button type="button" onClick={refreshArtifacts}>
            刷新索引
          </button>
        </div>
        {artifactIndex?.artifacts.length ? (
          <ArtifactTable artifacts={artifactIndex.artifacts} />
        ) : (
          <EmptyState title="暂无 artifact 索引" text="任务完成后会自动索引日志、checkpoint、轨迹、分析表和报告，也可以手动刷新项目目录。" />
        )}
      </section>
      <section className="panel span-3">
        <TrajectoryIndexPanel
          artifacts={artifactIndex?.artifacts ?? []}
          trajectoryIndex={trajectoryIndex}
          trajectoryChunk={trajectoryChunk}
          indexTrajectory={indexTrajectory}
          previewTrajectoryFrame={previewTrajectoryFrame}
        />
      </section>
      <section className="panel span-3">
        <TrajectoryAnalysisPackagePanel
          analysisPackage={trajectoryAnalysisPackage}
          generateTrajectoryAnalysisPackage={generateTrajectoryAnalysisPackage}
        />
      </section>
      <section className="panel span-3">
        <h3>分析曲线</h3>
        <AnalysisChartGrid analysisResult={analysisResult} />
      </section>
      <section className="panel span-2">
        <h3>GROMACS Run Package</h3>
        {runPackage ? (
          <div className="run-package">
            <dl className="definition-list">
              <div><dt>目录</dt><dd className="mono">{runPackage.runDirectory}</dd></div>
              <div><dt>文件数</dt><dd>{runPackage.files.length}</dd></div>
              <div><dt>命令数</dt><dd>{runPackage.commands.length}</dd></div>
              <div><dt>写入磁盘</dt><dd>{runPackage.files.some((file) => file.written) ? "是" : "否"}</dd></div>
            </dl>
            {runPackage.warnings.length > 0 ? (
              <div className="warning-stack">
                {runPackage.warnings.map((warning) => (
                  <p key={warning}>{warning}</p>
                ))}
              </div>
            ) : null}
            <div className="command-list">
              {runPackage.commands.map((command) => (
                <details key={command.stageId}>
                  <summary>{command.label}</summary>
                  <CodeBlock value={command.command} />
                </details>
              ))}
            </div>
          </div>
        ) : (
          <EmptyState title="尚未生成 run package" text="点击队列化后会生成 GROMACS .mdp、命令序列和运行脚本。" />
        )}
      </section>
      <section className="panel">
        <h3>生成文件</h3>
        {generatedFiles.length > 0 ? (
          <div className="file-list">
            {generatedFiles.map((file) => (
              <div className="file-row" key={file.path}>
                <span className="mono">{file.path}</span>
                <small>{file.language}</small>
                <small>{file.written ? "written" : "preview"}</small>
                {isNativeEditablePath(file.path) ? (
                  <button
                    type="button"
                    onClick={() => openNativeFile(file.path, file.contents, file.language)}
                  >
                    编辑
                  </button>
                ) : null}
              </div>
            ))}
          </div>
        ) : (
          <EmptyState title="等待生成" text="文件会按 project/generated、runs、analysis 分区保存。" />
        )}
      </section>
      <section className="panel span-2">
        <div className="panel-title-row">
          <h3>原生参数文件编辑器</h3>
          <button type="button" onClick={saveNativeFile} disabled={!nativeFile}>
            保存
          </button>
        </div>
        {nativeFile ? (
          <>
            <dl className="definition-list">
              <div><dt>文件</dt><dd className="mono">{nativeFile.path}</dd></div>
              <div><dt>语言</dt><dd>{nativeFile.language}</dd></div>
              <div><dt>大小</dt><dd>{nativeFile.sizeBytes} bytes</dd></div>
            </dl>
            {nativeFileMessage ? <div className="success-inline">{nativeFileMessage}</div> : null}
            <textarea
              className="native-editor"
              value={nativeFileDraft}
              spellCheck={false}
              onChange={(event) => setNativeFileDraft(event.target.value)}
            />
          </>
        ) : (
          <EmptyState title="尚未打开文件" text="在生成文件列表中选择 .mdp、.mdin、.conf、LAMMPS input 等原生文本文件进行编辑。" />
        )}
      </section>
      <section className="panel span-2">
        <h3>SLURM 脚本</h3>
        <CodeBlock value={slurmScript || "生成运行计划后显示 sbatch 脚本。"} />
      </section>
      <section className="panel">
        <h3>资源摘要</h3>
        {plan ? (
          <dl className="definition-list">
            <div><dt>执行模式</dt><dd>{executionModeText[plan.resources.executionMode]}</dd></div>
            <div><dt>CPU</dt><dd>{plan.resources.cpuThreads}</dd></div>
            <div><dt>GPU</dt><dd>{plan.resources.gpuCount}</dd></div>
            <div><dt>MPI</dt><dd>{plan.resources.mpiRanks}</dd></div>
          </dl>
        ) : null}
      </section>
      <section className="panel span-3">
        <div className="panel-title-row">
          <h3>GROMACS 日志解析</h3>
          <button type="button" onClick={parseLogSample}>
            解析日志
          </button>
        </div>
        <div className="split">
          <label>
            日志片段
            <textarea value={sampleLog} onChange={(event) => setSampleLog(event.target.value)} />
          </label>
          <div>
            {logReport ? (
              <dl className="definition-list">
                <div><dt>性能</dt><dd>{logReport.nsPerDay ? `${logReport.nsPerDay.toFixed(3)} ns/day` : "未检测"}</dd></div>
                <div><dt>当前步数</dt><dd>{logReport.currentStep ?? "未检测"}</dd></div>
                <div><dt>进度</dt><dd>{logReport.progressPercent ? `${logReport.progressPercent.toFixed(1)}%` : "未检测"}</dd></div>
                <div><dt>错误</dt><dd>{logReport.fatalError ?? "无"}</dd></div>
              </dl>
            ) : (
              <EmptyState title="未解析" text="粘贴 GROMACS log 后可提取 step、checkpoint、WARNING、fatal error 和 ns/day。" />
            )}
            {logReport?.events.length ? (
              <div className="event-list">
                {logReport.events.map((event) => (
                  <div className={`event-row ${event.kind}`} key={`${event.kind}-${event.lineNumber}-${event.message}`}>
                    <span>{event.kind}</span>
                    <small>line {event.lineNumber}</small>
                    <p>{event.message}</p>
                  </div>
                ))}
              </div>
            ) : null}
            <FailureAnalysisCard analysis={sampleFailureAnalysis} />
          </div>
        </div>
      </section>
    </div>
  );
}

function FailureAnalysisCard({ analysis }: { analysis: FailureAnalysis | null }) {
  if (!analysis) {
    return null;
  }
  return (
    <div className={`diagnostic-card ${analysis.severity}`}>
      <div className="diagnostic-header">
        <span>{failureCategoryText[analysis.category]}</span>
        <small>{severityText[analysis.severity]}</small>
      </div>
      <p>{analysis.message}</p>
      {analysis.suggestions.length ? (
        <div className="suggestion-list">
          {analysis.suggestions.map((suggestion) => (
            <div className="suggestion-item" key={`${suggestion.title}-${suggestion.actionLabel}`}>
              <strong>{suggestion.title}</strong>
              <span>{suggestion.detail}</span>
              {suggestion.commandHint ? <code>{suggestion.commandHint}</code> : null}
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function TaskRecordList({ records }: { records: TaskRecord[] }) {
  if (!records.length) {
    return (
      <EmptyState
        title="暂无持久化任务"
        text="启动本地任务后，AutoMD 会把 task id、engine、状态和进度写入 SQLite。"
      />
    );
  }

  return (
    <div className="task-record-list">
      {records.slice(0, 8).map((record) => (
        <div className="task-record-row" key={record.id}>
          <div>
            <strong>{engineLabel[record.engineId] ?? record.engineId}</strong>
            <span className="mono">{record.id}</span>
          </div>
          <span className={`task-status ${record.status}`}>{record.status}</span>
          <span>{record.progressPercent.toFixed(1)}%</span>
          <small>{new Date(record.updatedAt).toLocaleString()}</small>
        </div>
      ))}
    </div>
  );
}

function ResumePlanCard({
  resumePlan,
  onDiscover
}: {
  resumePlan: ResumePlan | null;
  onDiscover: () => void;
}) {
  return (
    <>
      <div className="panel-title-row">
        <h3>Checkpoint / Restart</h3>
        <button type="button" onClick={onDiscover}>
          扫描 checkpoint
        </button>
      </div>
      {resumePlan ? (
        <div className="resume-plan">
          <dl className="definition-list">
            <div><dt>引擎</dt><dd>{engineLabel[resumePlan.engineId] ?? resumePlan.engineId}</dd></div>
            <div><dt>Run 目录</dt><dd className="mono">{resumePlan.runDirectory}</dd></div>
            <div><dt>Checkpoint</dt><dd>{resumePlan.checkpoints.length}</dd></div>
          </dl>
          {resumePlan.resumeCommand ? (
            <div className="resume-command">
              <span>推荐恢复命令</span>
              <CodeBlock value={resumePlan.resumeCommand} />
            </div>
          ) : null}
          {resumePlan.warnings.length ? (
            <div className="warning-stack">
              {resumePlan.warnings.map((warning) => (
                <p key={warning}>{warning}</p>
              ))}
            </div>
          ) : null}
          {resumePlan.checkpoints.length ? (
            <div className="checkpoint-list">
              {resumePlan.checkpoints.map((checkpoint) => (
                <div className="checkpoint-row" key={checkpoint.path}>
                  <span className="mono truncate">{checkpoint.path}</span>
                  <small>{checkpoint.stageHint ?? "stage unknown"}</small>
                  <small>{formatBytes(checkpoint.sizeBytes)}</small>
                </div>
              ))}
            </div>
          ) : (
            <EmptyState title="未找到 checkpoint" text="真实或 mock 任务生成 .cpt 后会在这里显示可恢复命令。" />
          )}
        </div>
      ) : (
        <EmptyState title="等待扫描" text="任务结束会自动扫描，也可以手动读取 run 目录和 project/checkpoints。" />
      )}
    </>
  );
}

function RemotePanel({
  diagnostics,
  plan,
  remoteProfiles,
  selectedRemoteProfileId,
  setSelectedRemoteProfileId,
  remotePackage,
  remoteJobSnapshot,
  remoteWorkflowMode,
  setRemoteWorkflowMode,
  remoteWorkflowJobId,
  setRemoteWorkflowJobId,
  remoteWorkflowTimeout,
  setRemoteWorkflowTimeout,
  remoteWorkflowResult,
  remoteProfileDraft,
  setRemoteProfileDraft,
  saveRemoteProfile,
  deleteRemoteProfile,
  remoteSubmitOutput,
  setRemoteSubmitOutput,
  remoteStatusOutput,
  setRemoteStatusOutput,
  remoteLogOutput,
  setRemoteLogOutput,
  parseRemoteStatus,
  runRemoteStep,
  updatePlan,
  generateRemotePackage
}: {
  diagnostics: RuntimeDiagnostics | null;
  plan: SimulationPlan | null;
  remoteProfiles: RemoteProfile[];
  selectedRemoteProfileId: string | null;
  setSelectedRemoteProfileId: (value: string) => void;
  remotePackage: RemoteExecutionPackage | null;
  remoteJobSnapshot: RemoteJobSnapshot | null;
  remoteWorkflowMode: RemoteWorkflowMode;
  setRemoteWorkflowMode: (value: RemoteWorkflowMode) => void;
  remoteWorkflowJobId: string;
  setRemoteWorkflowJobId: (value: string) => void;
  remoteWorkflowTimeout: number;
  setRemoteWorkflowTimeout: (value: number) => void;
  remoteWorkflowResult: RemoteWorkflowStepResult | null;
  remoteProfileDraft: RemoteProfile;
  setRemoteProfileDraft: (value: RemoteProfile) => void;
  saveRemoteProfile: (profile: RemoteProfile) => void;
  deleteRemoteProfile: (id: string) => void;
  remoteSubmitOutput: string;
  setRemoteSubmitOutput: (value: string) => void;
  remoteStatusOutput: string;
  setRemoteStatusOutput: (value: string) => void;
  remoteLogOutput: string;
  setRemoteLogOutput: (value: string) => void;
  parseRemoteStatus: () => void;
  runRemoteStep: (stepId: string) => void;
  updatePlan: (updater: (current: SimulationPlan) => SimulationPlan) => void;
  generateRemotePackage: (profileId?: string | null) => void;
}) {
  const selectedProfile = remoteProfiles.find((profile) => profile.id === selectedRemoteProfileId) ?? remoteProfiles[0];
  const selectedIsTemplate = selectedProfile?.id.endsWith("-template") ?? true;
  return (
    <div className="content-grid">
      <section className="panel">
        <h3>本机运行环境</h3>
        <dl className="definition-list">
          <div><dt>OS</dt><dd>{diagnostics?.os ?? "unknown"}</dd></div>
          <div><dt>Arch</dt><dd>{diagnostics?.arch ?? "unknown"}</dd></div>
        </dl>
        <div className="tool-list">
          {diagnostics?.tools.map((tool) => (
            <div className="tool-row" key={tool.id}>
              <span>{tool.label}</span>
              <StatusPill status={tool.status} />
              <small className="mono">{tool.detail}</small>
            </div>
          ))}
        </div>
      </section>
      <section className="panel">
        <h3>远程 profile 模板</h3>
        <div className="form-grid">
          <label>
            Profile
            <select
              value={selectedProfile?.id ?? ""}
              onChange={(event) => {
                setSelectedRemoteProfileId(event.target.value);
                void generateRemotePackage(event.target.value);
              }}
            >
              {remoteProfiles.map((profile) => (
                <option value={profile.id} key={profile.id}>
                  {profile.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            调度器
            <select
              value={plan?.resources.executionMode ?? "localProcess"}
              onChange={(event) =>
                updatePlan((current) => ({
                  ...current,
                  resources: { ...current.resources, executionMode: event.target.value as ExecutionMode }
                }))
              }
            >
              <option value="localProcess">本地进程</option>
              <option value="ssh">SSH</option>
              <option value="slurm">SLURM</option>
              <option value="pbs">PBS</option>
              <option value="lsf">LSF</option>
              <option value="container">容器</option>
            </select>
          </label>
          <label>
            远程主机
            <input value={selectedProfile?.host ?? ""} readOnly />
          </label>
          <label>
            工作目录
            <input value={selectedProfile?.workdir ?? ""} readOnly />
          </label>
          <label>
            队列
            <input
              value={plan?.resources.queue ?? ""}
              placeholder="gpu, normal, short"
              onChange={(event) =>
                updatePlan((current) => ({
                  ...current,
                  resources: { ...current.resources, queue: event.target.value || null }
                }))
              }
            />
          </label>
          <label>
            CPU threads
            <input
              type="number"
              min="1"
              value={plan?.resources.cpuThreads ?? 1}
              onChange={(event) =>
                updatePlan((current) => ({
                  ...current,
                  resources: { ...current.resources, cpuThreads: Number(event.target.value) }
                }))
              }
            />
          </label>
        </div>
        <button type="button" className="primary fill" onClick={() => generateRemotePackage(selectedProfile?.id)}>
          生成远程执行包
        </button>
        <div className="script-surface">
          {(selectedProfile?.moduleLoad ?? []).map((line) => (
            <p className="mono" key={line}>{line}</p>
          ))}
          <p className="mono">workdir={remotePackage?.remoteWorkdir ?? selectedProfile?.workdir ?? "未生成"}</p>
          <p className="mono">sync=rsync --partial --append-verify</p>
        </div>
      </section>
      <section className="panel span-3">
        <div className="panel-title-row">
          <h3>自定义远程 Profile</h3>
          <div className="button-row compact">
            <button type="button" onClick={() => selectedProfile && setRemoteProfileDraft(selectedProfile)}>
              从当前填充
            </button>
            <button type="button" className="primary" onClick={() => saveRemoteProfile(remoteProfileDraft)}>
              保存 profile
            </button>
            <button
              type="button"
              onClick={() => selectedProfile && deleteRemoteProfile(selectedProfile.id)}
              disabled={!selectedProfile || selectedIsTemplate}
            >
              删除已保存
            </button>
          </div>
        </div>
        <div className="remote-profile-form">
          <label>
            ID
            <input
              value={remoteProfileDraft.id}
              onChange={(event) => setRemoteProfileDraft({ ...remoteProfileDraft, id: event.target.value })}
              placeholder="custom-slurm-gpu"
            />
          </label>
          <label>
            名称
            <input
              value={remoteProfileDraft.name}
              onChange={(event) => setRemoteProfileDraft({ ...remoteProfileDraft, name: event.target.value })}
              placeholder="Lab SLURM GPU"
            />
          </label>
          <label>
            主机
            <input
              value={remoteProfileDraft.host}
              onChange={(event) => setRemoteProfileDraft({ ...remoteProfileDraft, host: event.target.value })}
              placeholder="login.cluster.edu"
            />
          </label>
          <label>
            调度器
            <select
              value={remoteProfileDraft.scheduler}
              onChange={(event) =>
                setRemoteProfileDraft({ ...remoteProfileDraft, scheduler: event.target.value as ExecutionMode })
              }
            >
              <option value="ssh">SSH</option>
              <option value="slurm">SLURM</option>
              <option value="pbs">PBS</option>
              <option value="lsf">LSF</option>
            </select>
          </label>
          <label>
            工作目录
            <input
              value={remoteProfileDraft.workdir}
              onChange={(event) => setRemoteProfileDraft({ ...remoteProfileDraft, workdir: event.target.value })}
              placeholder="/scratch/$USER/automd"
            />
          </label>
          <label>
            默认队列
            <input
              value={remoteProfileDraft.defaultQueue ?? ""}
              onChange={(event) =>
                setRemoteProfileDraft({ ...remoteProfileDraft, defaultQueue: event.target.value || null })
              }
              placeholder="gpu"
            />
          </label>
          <label className="span-all">
            Module / setup commands
            <textarea
              value={remoteProfileDraft.moduleLoad.join("\n")}
              onChange={(event) =>
                setRemoteProfileDraft({
                  ...remoteProfileDraft,
                  moduleLoad: event.target.value.split("\n")
                })
              }
              rows={4}
              spellCheck={false}
            />
          </label>
        </div>
      </section>
      <section className="panel span-3">
        <div className="panel-title-row">
          <h3>远程命令</h3>
          <button type="button" onClick={() => generateRemotePackage(selectedProfile?.id)}>
            刷新
          </button>
        </div>
        {remotePackage ? (
          <>
            <dl className="definition-list">
              <div><dt>调度器</dt><dd>{executionModeText[remotePackage.scheduler]}</dd></div>
              <div><dt>Run 目录</dt><dd className="mono">{remotePackage.runDirectory}</dd></div>
              <div><dt>远程目录</dt><dd className="mono">{remotePackage.remoteWorkdir}</dd></div>
            </dl>
            <div className="remote-runner-controls">
              <label>
                执行模式
                <select value={remoteWorkflowMode} onChange={(event) => setRemoteWorkflowMode(event.target.value as RemoteWorkflowMode)}>
                  <option value="dryRun">Dry run：只预览命令</option>
                  <option value="writeFiles">只写脚本：写入 remote/ 文件，不连接</option>
                  <option value="execute">执行：运行本地 ssh/rsync 命令</option>
                </select>
              </label>
              <label>
                Job id / PID
                <input
                  value={remoteWorkflowJobId}
                  onChange={(event) => setRemoteWorkflowJobId(event.target.value)}
                  placeholder={remoteJobSnapshot?.jobId ?? "<job-id>"}
                />
              </label>
              <label>
                超时 (秒)
                <input
                  type="number"
                  min={1}
                  max={3600}
                  value={remoteWorkflowTimeout}
                  onChange={(event) => setRemoteWorkflowTimeout(Number(event.target.value))}
                />
              </label>
            </div>
            {remotePackage.warnings.length ? (
              <div className="warning-stack">
                {remotePackage.warnings.map((warning) => <p key={warning}>{warning}</p>)}
              </div>
            ) : null}
            <div className="remote-command-grid">
              {remotePackage.commands.map((command) => (
                <div className="remote-command-row" key={command.id}>
                  <div>
                    <strong>{command.label}</strong>
                    <span>{command.description}</span>
                  </div>
                  <code>{command.command}</code>
                  <button type="button" onClick={() => runRemoteStep(command.id)}>
                    运行步骤
                  </button>
                </div>
              ))}
            </div>
            {remoteWorkflowResult ? (
              <div className="remote-runner-result">
                <dl className="definition-list">
                  <div><dt>步骤</dt><dd>{remoteWorkflowResult.label}</dd></div>
                  <div><dt>模式</dt><dd>{remoteWorkflowModeText[remoteWorkflowResult.mode]}</dd></div>
                  <div><dt>状态</dt><dd>{remoteWorkflowResult.status}</dd></div>
                  <div><dt>退出码</dt><dd>{remoteWorkflowResult.exitCode ?? "n/a"}</dd></div>
                  <div><dt>写入文件</dt><dd>{remoteWorkflowResult.filesWritten.length}</dd></div>
                  <div><dt>耗时</dt><dd>{remoteWorkflowResult.durationMs ?? 0} ms</dd></div>
                </dl>
                {remoteWorkflowResult.warnings.length ? (
                  <div className="warning-stack">
                    {remoteWorkflowResult.warnings.map((warning) => <p key={warning}>{warning}</p>)}
                  </div>
                ) : null}
                <details>
                  <summary>执行命令</summary>
                  <CodeBlock value={remoteWorkflowResult.command} />
                </details>
                <details open={Boolean(remoteWorkflowResult.stdout)}>
                  <summary>stdout</summary>
                  <CodeBlock value={remoteWorkflowResult.stdout || "(empty)"} />
                </details>
                <details open={Boolean(remoteWorkflowResult.stderr)}>
                  <summary>stderr</summary>
                  <CodeBlock value={remoteWorkflowResult.stderr || "(empty)"} />
                </details>
              </div>
            ) : null}
          </>
        ) : (
          <EmptyState title="未生成远程包" text="选择 profile 后生成 SSH/rsync、提交、状态、取消和回收命令。" />
        )}
      </section>
      <section className="panel span-3">
        <h3>远程脚本</h3>
        {remotePackage ? (
          <div className="command-list">
            {remotePackage.files.map((file) => (
              <details key={file.path} open={file.path.includes("submit") || file.path.includes("run-ssh")}>
                <summary>{file.path}</summary>
                <CodeBlock value={file.contents} />
              </details>
            ))}
          </div>
        ) : (
          <EmptyState title="等待生成" text="脚本会包含调度器 directives、module load、运行命令和同步脚本。" />
        )}
      </section>
      <section className="panel span-3">
        <div className="panel-title-row">
          <h3>远程状态解析</h3>
          <button type="button" onClick={parseRemoteStatus} disabled={!remotePackage}>
            解析状态
          </button>
        </div>
        <div className="remote-status-grid">
          <label>
            Submit output
            <textarea
              value={remoteSubmitOutput}
              onChange={(event) => setRemoteSubmitOutput(event.target.value)}
              rows={4}
              spellCheck={false}
            />
          </label>
          <label>
            Scheduler status output
            <textarea
              value={remoteStatusOutput}
              onChange={(event) => setRemoteStatusOutput(event.target.value)}
              rows={4}
              spellCheck={false}
            />
          </label>
          <label>
            Remote log tail
            <textarea
              value={remoteLogOutput}
              onChange={(event) => setRemoteLogOutput(event.target.value)}
              rows={4}
              spellCheck={false}
            />
          </label>
        </div>
        {remoteJobSnapshot ? (
          <div className="remote-snapshot">
            <dl className="definition-list">
              <div><dt>Job id</dt><dd className="mono">{remoteJobSnapshot.jobId ?? "未提取"}</dd></div>
              <div><dt>状态</dt><dd>{remoteJobSnapshot.status}</dd></div>
              <div><dt>队列状态</dt><dd>{remoteJobSnapshot.queueState ?? "未检测"}</dd></div>
              <div><dt>当前步数</dt><dd>{remoteJobSnapshot.currentStep ?? "未检测"}</dd></div>
              <div><dt>性能</dt><dd>{remoteJobSnapshot.nsPerDay ? `${remoteJobSnapshot.nsPerDay.toFixed(3)} ns/day` : "未检测"}</dd></div>
              <div><dt>进度</dt><dd>{remoteJobSnapshot.progressPercent ? `${remoteJobSnapshot.progressPercent.toFixed(1)}%` : "未检测"}</dd></div>
            </dl>
            {remoteJobSnapshot.reason ? <p className="hint-text">{remoteJobSnapshot.reason}</p> : null}
            {remoteJobSnapshot.warnings.length ? (
              <div className="warning-stack">
                {remoteJobSnapshot.warnings.map((warning) => <p key={warning}>{warning}</p>)}
              </div>
            ) : null}
            {remoteJobSnapshot.logReport?.events.length ? (
              <div className="event-list compact-events">
                {remoteJobSnapshot.logReport.events.slice(0, 8).map((event) => (
                  <div className={`event-row ${event.kind}`} key={`${event.lineNumber}-${event.message}`}>
                    <span>{event.kind}</span>
                    <p>{event.message}</p>
                  </div>
                ))}
              </div>
            ) : null}
          </div>
        ) : (
          <EmptyState title="等待解析" text="粘贴 sbatch/qsub/bsub 返回、队列查询和远程日志片段后，AutoMD 会生成统一远程作业快照。" />
        )}
      </section>
    </div>
  );
}

function BuildPanel({
  engines,
  selectedEngineId,
  containerRecipe,
  buildRecipe,
  recipeExportResult,
  buildWorkflowMode,
  setBuildWorkflowMode,
  buildWorkflowTimeout,
  setBuildWorkflowTimeout,
  buildWorkflowResult,
  generateRecipes,
  exportRecipes,
  runBuildWizard
}: {
  engines: EngineCapability[];
  selectedEngineId: string;
  containerRecipe: ContainerRecipe | null;
  buildRecipe: BuildRecipe | null;
  recipeExportResult: RecipeExportResult | null;
  buildWorkflowMode: BuildWorkflowMode;
  setBuildWorkflowMode: (value: BuildWorkflowMode) => void;
  buildWorkflowTimeout: number;
  setBuildWorkflowTimeout: (value: number) => void;
  buildWorkflowResult: BuildWorkflowResult | null;
  generateRecipes: (engineId?: string) => void;
  exportRecipes: (engineId?: string) => void;
  runBuildWizard: (engineId?: string) => void;
}) {
  return (
    <div className="content-grid">
      <section className="panel">
        <h3>构建目标</h3>
        <div className="recipe-buttons">
          {engines.map((engine) => (
            <button
              type="button"
              key={engine.id}
              className={engine.id === selectedEngineId ? "selected-lite" : ""}
              onClick={() => generateRecipes(engine.id)}
            >
              {engine.name}
            </button>
          ))}
        </div>
        <div className="button-row">
          <button type="button" onClick={() => generateRecipes(selectedEngineId)}>
            预览 recipe
          </button>
          <button type="button" onClick={() => exportRecipes(selectedEngineId)}>
            导出到项目
          </button>
        </div>
        <div className="build-runner-controls">
          <label>
            构建模式
            <select value={buildWorkflowMode} onChange={(event) => setBuildWorkflowMode(event.target.value as BuildWorkflowMode)}>
              <option value="dryRun">Dry run：只预览构建命令</option>
              <option value="writeFiles">只写脚本：写入 build-recipes/</option>
              <option value="execute">执行：运行本地构建脚本</option>
            </select>
          </label>
          <label>
            超时 (秒)
            <input
              type="number"
              min={1}
              max={86400}
              value={buildWorkflowTimeout}
              onChange={(event) => setBuildWorkflowTimeout(Number(event.target.value))}
            />
          </label>
          <button type="button" className="primary fill" onClick={() => runBuildWizard(selectedEngineId)}>
            运行构建向导
          </button>
        </div>
        {recipeExportResult ? (
          <div className="success-inline">
            已导出到 <span className="mono">{recipeExportResult.directory}</span>
          </div>
        ) : null}
      </section>
      {recipeExportResult ? (
        <section className="panel">
          <h3>导出文件</h3>
          <div className="file-list">
            {recipeExportResult.files.map((file) => (
              <span className="mono" key={file.path}>
                {file.path}
              </span>
            ))}
          </div>
          {recipeExportResult.warnings.length ? (
            <ul>
              {recipeExportResult.warnings.map((warning) => (
                <li key={warning}>{warning}</li>
              ))}
            </ul>
          ) : null}
        </section>
      ) : null}
      {buildWorkflowResult ? (
        <section className="panel span-3">
          <h3>构建向导结果</h3>
          <div className="build-runner-result">
            <dl className="definition-list">
              <div><dt>引擎</dt><dd>{buildWorkflowResult.engineId}</dd></div>
              <div><dt>模式</dt><dd>{buildWorkflowModeText[buildWorkflowResult.mode]}</dd></div>
              <div><dt>状态</dt><dd>{buildWorkflowResult.status}</dd></div>
              <div><dt>退出码</dt><dd>{buildWorkflowResult.exitCode ?? "n/a"}</dd></div>
              <div><dt>日志</dt><dd className="mono">{buildWorkflowResult.logPath ?? "未生成"}</dd></div>
              <div><dt>耗时</dt><dd>{buildWorkflowResult.durationMs ?? 0} ms</dd></div>
            </dl>
            {buildWorkflowResult.warnings.length ? (
              <div className="warning-stack">
                {buildWorkflowResult.warnings.map((warning) => <p key={warning}>{warning}</p>)}
              </div>
            ) : null}
            <FailureAnalysisCard analysis={buildWorkflowResult.failureAnalysis ?? null} />
            <details>
              <summary>构建命令</summary>
              <CodeBlock value={buildWorkflowResult.command} />
            </details>
            <details open={Boolean(buildWorkflowResult.stdout)}>
              <summary>stdout</summary>
              <CodeBlock value={buildWorkflowResult.stdout || "(empty)"} />
            </details>
            <details open={Boolean(buildWorkflowResult.stderr)}>
              <summary>stderr</summary>
              <CodeBlock value={buildWorkflowResult.stderr || "(empty)"} />
            </details>
          </div>
        </section>
      ) : null}
      <section className="panel span-2">
        <h3>{containerRecipe?.title ?? "容器 recipe"}</h3>
        <CodeBlock value={containerRecipe?.files[0]?.contents ?? "选择引擎后生成 Containerfile。"} />
      </section>
      <section className="panel span-3">
        <h3>{buildRecipe?.title ?? "源码编译脚本"}</h3>
        {buildRecipe ? (
          <div className="split">
            <div>
              <h4>步骤</h4>
              <ol>
                {buildRecipe.steps.map((step) => (
                  <li key={step}>{step}</li>
                ))}
              </ol>
              <h4>风险</h4>
              <ul>
                {buildRecipe.warnings.map((warning) => (
                  <li key={warning}>{warning}</li>
                ))}
              </ul>
            </div>
            <CodeBlock value={buildRecipe.script} />
          </div>
        ) : (
          <EmptyState title="尚未生成脚本" text="开源引擎生成安装/编译 recipe；受限引擎仅生成本地授权环境接入清单。" />
        )}
      </section>
    </div>
  );
}

function PluginsPanel({ pluginRegistry }: { pluginRegistry: PluginRegistrySnapshot | null }) {
  if (!pluginRegistry) {
    return (
      <section className="panel">
        <EmptyState title="插件注册表尚未加载" text="AutoMD 会扫描 app plugins 目录中的 *.automd-plugin.json manifest。" />
      </section>
    );
  }

  const kindCounts = pluginRegistry.manifests.reduce<Record<string, number>>((counts, manifest) => {
    counts[manifest.kind] = (counts[manifest.kind] ?? 0) + 1;
    return counts;
  }, {});

  return (
    <div className="content-grid">
      <section className="panel">
        <h3>插件目录</h3>
        <dl className="definition-list">
          <div><dt>路径</dt><dd className="mono">{pluginRegistry.pluginRoot}</dd></div>
          <div><dt>manifest</dt><dd>{pluginRegistry.manifests.length}</dd></div>
          <div><dt>外部警告</dt><dd>{pluginRegistry.warnings.length}</dd></div>
        </dl>
        {pluginRegistry.warnings.length ? (
          <div className="warning-stack">
            {pluginRegistry.warnings.map((warning) => <p key={warning}>{warning}</p>)}
          </div>
        ) : null}
      </section>
      <section className="panel span-2">
        <h3>扩展能力</h3>
        <div className="metric-grid plugin-metrics">
          {(Object.keys(pluginKindText) as PluginKind[]).map((kind) => (
            <Metric key={kind} label={pluginKindText[kind]} value={kindCounts[kind] ?? 0} />
          ))}
        </div>
      </section>
      <section className="panel span-3">
        <h3>Manifest Registry</h3>
        <div className="engine-grid plugin-grid">
          {pluginRegistry.manifests.map((manifest) => (
            <article className="engine-card plugin-card" key={manifest.id}>
              <div className="engine-card-head">
                <strong>{manifest.name}</strong>
                <span className="status-pill ready">{pluginKindText[manifest.kind]}</span>
              </div>
              <dl className="compact-dl">
                <div><dt>ID</dt><dd className="mono">{manifest.id}</dd></div>
                <div><dt>版本</dt><dd>{manifest.version}</dd></div>
                <div><dt>入口</dt><dd className="mono truncate">{manifest.entrypoint}</dd></div>
                <div><dt>引擎</dt><dd>{manifest.engineId ?? "n/a"}</dd></div>
                <div><dt>来源</dt><dd className="mono truncate">{manifest.sourcePath ?? "built-in"}</dd></div>
              </dl>
              {manifest.licensePolicy ? (
                <p>License: {manifest.licensePolicy}</p>
              ) : null}
              <div className="chip-row">
                {manifest.capabilities.map((capability) => (
                  <span key={capability}>{capability}</span>
                ))}
              </div>
              {manifest.warnings.length ? (
                <div className="warning-stack compact-warning">
                  {manifest.warnings.map((warning) => <p key={warning}>{warning}</p>)}
                </div>
              ) : null}
            </article>
          ))}
        </div>
      </section>
    </div>
  );
}

function ArtifactTable({ artifacts }: { artifacts: RunArtifact[] }) {
  return (
    <div className="artifact-table">
      <div className="artifact-head">
        <span>类型</span>
        <span>路径</span>
        <span>大小</span>
        <span>摘要</span>
      </div>
      {artifacts.map((artifact) => (
        <div className="artifact-row" key={`${artifact.kind}-${artifact.path}`}>
          <span>{artifact.kind}</span>
          <span className="mono truncate">{artifact.path}</span>
          <span>{formatBytes(artifact.sizeBytes)}</span>
          <span>{artifact.summary ?? " "}</span>
        </div>
      ))}
    </div>
  );
}

function TrajectoryIndexPanel({
  artifacts,
  trajectoryIndex,
  trajectoryChunk,
  indexTrajectory,
  previewTrajectoryFrame
}: {
  artifacts: RunArtifact[];
  trajectoryIndex: TrajectoryIndex | null;
  trajectoryChunk: TrajectoryChunk | null;
  indexTrajectory: (trajectoryPath?: string) => void;
  previewTrajectoryFrame: (frameIndex: number) => void;
}) {
  const trajectories = artifacts.filter((artifact) => artifact.kind === "trajectory");

  return (
    <div className="trajectory-panel">
      <div className="panel-title-row">
        <h3>轨迹索引与分块预览</h3>
        <button type="button" onClick={() => indexTrajectory()} disabled={!trajectories.length}>
          索引首个轨迹
        </button>
      </div>
      {trajectories.length ? (
        <div className="trajectory-layout">
          <div className="trajectory-list">
            {trajectories.map((artifact) => (
              <button type="button" key={artifact.path} onClick={() => indexTrajectory(artifact.path)}>
                <span className="mono">{artifact.path}</span>
                <small>{formatBytes(artifact.sizeBytes)} · {artifact.summary ?? "等待索引"}</small>
              </button>
            ))}
          </div>
          <div className="trajectory-summary">
            {trajectoryIndex ? (
              <>
                <dl className="definition-list">
                  <div><dt>格式</dt><dd>{trajectoryIndex.format}</dd></div>
                  <div><dt>策略</dt><dd>{trajectoryIndex.strategy === "textOffsets" ? "文本 offset 索引" : "metadata-only"}</dd></div>
                  <div><dt>帧数</dt><dd>{trajectoryIndex.frameCount ?? "未解码"}</dd></div>
                  <div><dt>索引</dt><dd className="mono">{trajectoryIndex.indexPath ?? "未写入"}</dd></div>
                </dl>
                {trajectoryIndex.warnings.length ? (
                  <div className="warning-stack">
                    {trajectoryIndex.warnings.map((warning) => <p key={warning}>{warning}</p>)}
                  </div>
                ) : null}
                {trajectoryIndex.sampledFrames.length ? (
                  <div className="frame-chip-row">
                    {trajectoryIndex.sampledFrames.slice(0, 24).map((frame) => (
                      <button type="button" key={frame.frameIndex} onClick={() => previewTrajectoryFrame(frame.frameIndex)}>
                        #{frame.frameIndex}
                        <small>{frame.atomCount ? `${frame.atomCount} atoms` : frame.label}</small>
                      </button>
                    ))}
                  </div>
                ) : (
                  <EmptyState
                    title="暂无可预览帧"
                    text="二进制 XTC/TRR/DCD/GSD 当前只登记 metadata，帧解码会交给后续 MDAnalysis/Mol* 后台路径。"
                  />
                )}
              </>
            ) : (
              <EmptyState title="等待索引" text="选择轨迹后会生成 frame offset manifest，并按需读取小块帧内容。" />
            )}
          </div>
          <div className="trajectory-preview">
            {trajectoryChunk?.frames.length ? (
              <>
                <div className="analysis-card-head">
                  <div>
                    <strong>{trajectoryChunk.frames[0].label}</strong>
                    <span className="mono">{trajectoryChunk.trajectoryPath}</span>
                  </div>
                  <small>{trajectoryChunk.truncated ? "已截断" : "完整 chunk"}</small>
                </div>
                <CodeBlock value={trajectoryChunk.frames.map((frame) => frame.contents).join("\n")} />
                {trajectoryChunk.warnings.length ? (
                  <div className="warning-stack">
                    {trajectoryChunk.warnings.map((warning) => <p key={warning}>{warning}</p>)}
                  </div>
                ) : null}
              </>
            ) : (
              <EmptyState title="暂无 chunk" text="文本轨迹可以读取指定帧，避免一次性把大文件送进 UI。" />
            )}
          </div>
        </div>
      ) : (
        <EmptyState title="暂无轨迹 artifact" text="产生 trajectories/*.xtc、*.dcd、*.pdb、*.xyz 或 LAMMPS dump 后，这里会建立后台索引。" />
      )}
    </div>
  );
}

function TrajectoryAnalysisPackagePanel({
  analysisPackage,
  generateTrajectoryAnalysisPackage
}: {
  analysisPackage: TrajectoryAnalysisPackage | null;
  generateTrajectoryAnalysisPackage: () => void;
}) {
  return (
    <div className="analysis-package-panel">
      <div className="panel-title-row">
        <h3>MDAnalysis 分析侧车</h3>
        <button type="button" onClick={generateTrajectoryAnalysisPackage}>
          生成分析包
        </button>
      </div>
      {analysisPackage ? (
        <div className="analysis-package-grid">
          <div>
            <dl className="definition-list">
              <div><dt>目录</dt><dd className="mono">{analysisPackage.generatedDirectory}</dd></div>
              <div><dt>文件</dt><dd>{analysisPackage.files.length}</dd></div>
              <div><dt>命令</dt><dd>{analysisPackage.commands.length}</dd></div>
              <div><dt>写入磁盘</dt><dd>{analysisPackage.files.some((file) => file.written) ? "是" : "否"}</dd></div>
            </dl>
            {analysisPackage.warnings.length ? (
              <div className="warning-stack">
                {analysisPackage.warnings.map((warning) => <p key={warning}>{warning}</p>)}
              </div>
            ) : null}
          </div>
          <div>
            <h4>输出</h4>
            <div className="chip-row">
              {analysisPackage.expectedOutputs.map((output) => (
                <span key={output}>{output}</span>
              ))}
            </div>
          </div>
          <div className="command-list">
            {analysisPackage.commands.map((command) => (
              <details key={command.stageId}>
                <summary>{command.label}</summary>
                <CodeBlock value={command.command} />
              </details>
            ))}
          </div>
        </div>
      ) : (
        <EmptyState
          title="等待生成"
          text="生成后会写入 generated/analysis/run_mdanalysis.py，并约定输出 RMSD、RMSF、Rg、氢键和接触计数 CSV。"
        />
      )}
    </div>
  );
}

function formatBytes(value: number) {
  if (value < 1024) {
    return `${value} B`;
  }
  if (value < 1024 * 1024) {
    return `${(value / 1024).toFixed(1)} KB`;
  }
  return `${(value / 1024 / 1024).toFixed(1)} MB`;
}

function AnalysisChartGrid({ analysisResult }: { analysisResult: AnalysisParseResult | null }) {
  if (!analysisResult?.series.length) {
    return (
      <EmptyState
        title="暂无分析曲线"
        text="任务产生 analysis/*.xvg 或 CSV 后，AutoMD 会解析为 RMSD、Rg、能量、温度等曲线。"
      />
    );
  }

  return (
    <div className="analysis-grid">
      {analysisResult.series.map((series) => (
        <AnalysisChart series={series} key={`${series.path}-${series.label}`} />
      ))}
      {analysisResult.warnings.length ? (
        <div className="warning-stack span-all">
          {analysisResult.warnings.map((warning) => <p key={warning}>{warning}</p>)}
        </div>
      ) : null}
    </div>
  );
}

function AnalysisChart({ series }: { series: AnalysisParseResult["series"][number] }) {
  const points = series.points;
  const xValues = points.map((point) => point.x);
  const yValues = points.map((point) => point.y);
  const minX = Math.min(...xValues);
  const maxX = Math.max(...xValues);
  const minY = Math.min(...yValues);
  const maxY = Math.max(...yValues);
  const width = 360;
  const height = 160;
  const pad = 24;
  const xSpan = maxX - minX || 1;
  const ySpan = maxY - minY || 1;
  const polyline = points
    .map((point) => {
      const x = pad + ((point.x - minX) / xSpan) * (width - pad * 2);
      const y = height - pad - ((point.y - minY) / ySpan) * (height - pad * 2);
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");

  return (
    <div className="analysis-card">
      <div className="analysis-card-head">
        <div>
          <strong>{series.label}</strong>
          <span className="mono">{series.path}</span>
        </div>
        <small>{series.points.length} points</small>
      </div>
      <svg viewBox={`0 0 ${width} ${height}`} className="analysis-chart" role="img" aria-label={series.label}>
        <line x1={pad} y1={height - pad} x2={width - pad} y2={height - pad} />
        <line x1={pad} y1={pad} x2={pad} y2={height - pad} />
        <polyline points={polyline} />
      </svg>
      <dl className="analysis-stats">
        <div><dt>{series.xLabel}</dt><dd>{formatNumber(minX)} - {formatNumber(maxX)}</dd></div>
        <div><dt>min</dt><dd>{formatNumber(series.minY ?? minY)}</dd></div>
        <div><dt>max</dt><dd>{formatNumber(series.maxY ?? maxY)}</dd></div>
        <div><dt>last</dt><dd>{formatNumber(series.lastY ?? yValues[yValues.length - 1])} {series.yLabel}</dd></div>
      </dl>
    </div>
  );
}

function formatNumber(value: number) {
  if (!Number.isFinite(value)) {
    return "n/a";
  }
  if (Math.abs(value) >= 1000 || Math.abs(value) < 0.01) {
    return value.toExponential(2);
  }
  return value.toFixed(3).replace(/\.?0+$/, "");
}

function ReportPanel({
  project,
  plan,
  validation,
  artifactIndex,
  artifactRecords,
  analysisResult,
  analysisCacheRecords,
  exportedReport,
  refreshArtifacts,
  refreshAnalysis,
  exportReport
}: {
  project: ProjectSummary | null;
  plan: SimulationPlan | null;
  validation: ValidationReport | null;
  artifactIndex: ArtifactIndex | null;
  artifactRecords: ArtifactRecord[];
  analysisResult: AnalysisParseResult | null;
  analysisCacheRecords: AnalysisCacheRecord[];
  exportedReport: ExportedReport | null;
  refreshArtifacts: () => void;
  refreshAnalysis: () => void;
  exportReport: (format: ReportFormat) => void;
}) {
  const report = useMemo(() => {
    if (!plan) {
      return "# AutoMD Report\n\nNo simulation plan generated yet.\n";
    }
    return `# AutoMD Simulation Report

## Project
- Name: ${project?.name ?? "unsaved project"}
- Engine: ${plan.engineId}
- Created: ${plan.createdAt}

## System
- Source: ${plan.system.sourceKind}
- Force field: ${plan.forceField.protein}
- Water: ${plan.forceField.waterModel}
- Solvent padding: ${plan.solvent.paddingNm} nm
- Ionic strength: ${plan.solvent.ionicStrengthMolar} M

## Stages
${plan.stages.map((stage) => `- ${stage.enabled ? "[x]" : "[ ]"} ${stage.label}`).join("\n")}

## Validation
- Status: ${validation?.status ?? "unknown"}

## Artifacts
${artifactIndex?.artifacts.map((artifact) => `- ${artifact.kind}: ${artifact.path}`).join("\n") ?? "- No artifacts indexed"}

## Analysis
${analysisResult?.series.map((series) => `- ${series.label}: ${series.points.length} points, last=${series.lastY ?? "n/a"} ${series.yLabel}`).join("\n") ?? "- No analysis series parsed"}
`;
  }, [plan, project, validation, artifactIndex, analysisResult]);

  return (
    <div className="content-grid">
      <section className="panel span-2">
        <div className="panel-title-row">
          <h3>Markdown 报告草稿</h3>
          <div className="button-row compact">
            <button type="button" onClick={() => void navigator.clipboard?.writeText(report)}>
              复制
            </button>
            <button type="button" onClick={() => exportReport("markdown")}>
              导出 MD
            </button>
            <button type="button" onClick={() => exportReport("html")}>
              导出 HTML
            </button>
            <button type="button" onClick={() => exportReport("pdf")}>
              导出 PDF
            </button>
          </div>
        </div>
        {exportedReport ? (
          <div className="success-inline">
            已导出 {exportedReport.format}: <span className="mono">{exportedReport.path}</span>
          </div>
        ) : null}
        <CodeBlock value={exportedReport?.contents ?? report} />
      </section>
      <section className="panel">
        <div className="panel-title-row">
          <h3>Artifact 索引</h3>
          <button type="button" onClick={refreshArtifacts}>
            刷新
          </button>
        </div>
        {artifactIndex?.artifacts.length ? (
          <ArtifactTable artifacts={artifactIndex.artifacts} />
        ) : (
          <EmptyState title="尚无索引" text="运行任务完成后会自动索引，也可以在创建项目后手动刷新。" />
        )}
      </section>
      <section className="panel">
        <h3>SQLite 缓存</h3>
        <CacheSummary artifactRecords={artifactRecords} analysisCacheRecords={analysisCacheRecords} />
      </section>
      <section className="panel span-3">
        <div className="panel-title-row">
          <h3>分析图表</h3>
          <button type="button" onClick={() => refreshAnalysis()}>
            解析分析结果
          </button>
        </div>
        <AnalysisChartGrid analysisResult={analysisResult} />
      </section>
    </div>
  );
}

function CacheSummary({
  artifactRecords,
  analysisCacheRecords
}: {
  artifactRecords: ArtifactRecord[];
  analysisCacheRecords: AnalysisCacheRecord[];
}) {
  if (!artifactRecords.length && !analysisCacheRecords.length) {
    return <EmptyState title="暂无缓存记录" text="刷新 artifact 或解析分析结果后，SQLite 会保存摘要记录。" />;
  }

  return (
    <div className="cache-summary">
      <div>
        <strong>{artifactRecords.length}</strong>
        <span>artifact metadata rows</span>
      </div>
      <div>
        <strong>{analysisCacheRecords.length}</strong>
        <span>analysis series cached</span>
      </div>
      <div className="file-list">
        {artifactRecords.slice(0, 4).map((record) => (
          <span className="mono" key={`${record.path}-${record.indexedAt}`}>
            {record.kind}: {record.path}
          </span>
        ))}
        {analysisCacheRecords.slice(0, 4).map((record) => (
          <span className="mono" key={`${record.path}-${record.label}`}>
            {record.label}: {record.pointCount} pts, last={record.lastY ?? "n/a"} {record.yLabel}
          </span>
        ))}
      </div>
    </div>
  );
}

function MoleculeViewport({ plan, project }: { plan: SimulationPlan | null; project: ProjectSummary | null }) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const [viewerStatus, setViewerStatus] = useState("等待结构导入");
  const sourcePath = plan?.system.sourcePath ?? null;

  useEffect(() => {
    const host = hostRef.current;
    if (!host || !sourcePath || !project?.path) {
      setViewerStatus(sourcePath ? "需要项目路径以加载结构" : "等待结构导入");
      return;
    }

    let disposed = false;
    let viewer: any | null = null;
    host.innerHTML = "";
    setViewerStatus("正在初始化 Mol*");

    void (async () => {
      try {
        const [{ Viewer }, payload] = await Promise.all([
          import("molstar/lib/apps/viewer/app"),
          api.readStructureFile({
            projectPath: project.path,
            sourcePath
          })
        ]);
        if (disposed) {
          return;
        }
        viewer = await Viewer.create(host, {
          layoutIsExpanded: false,
          layoutShowControls: false,
          layoutShowSequence: false,
          layoutShowLog: false,
          layoutShowLeftPanel: false,
          collapseRightPanel: true,
          viewportShowExpand: false,
          viewportShowSelectionMode: false,
          viewportShowAnimation: true,
          viewportShowTrajectoryControls: true
        });
        if (disposed) {
          viewer.dispose();
          return;
        }
        await viewer.loadStructureFromData(payload.contents, payload.format as never, {
          dataLabel: payload.sourcePath
        });
        if (!disposed) {
          setViewerStatus(`Mol* loaded ${payload.sourcePath} (${formatBytes(payload.sizeBytes)})`);
        }
      } catch (caught) {
        if (!disposed) {
          setViewerStatus(`Mol* 加载失败：${caught instanceof Error ? caught.message : String(caught)}`);
        }
      }
    })();

    return () => {
      disposed = true;
      viewer?.dispose();
      host.innerHTML = "";
    };
  }, [project?.path, sourcePath]);

  const showPlaceholder = !sourcePath || viewerStatus.startsWith("Mol* 加载失败") || viewerStatus.startsWith("需要项目路径");

  return (
    <div className="molecule-panel">
      <div className="viewer-header">
        <div>
          <h3>结构与轨迹视图</h3>
          <p>{plan?.system.name ?? "导入结构后显示 Mol* 视图"}</p>
          {plan?.system.sourcePath ? <small className="mono">{plan.system.sourcePath}</small> : null}
        </div>
        <span className="viewer-badge">{viewerStatus}</span>
      </div>
      <div className={`molecule-canvas ${sourcePath ? "molstar-canvas" : ""}`} aria-label="molecular viewport">
        <div ref={hostRef} className="molstar-host" />
        {showPlaceholder ? <MoleculePlaceholder /> : null}
      </div>
    </div>
  );
}

function MoleculePlaceholder() {
  return (
    <div className="molecule-placeholder" aria-hidden="true">
      {Array.from({ length: 18 }).map((_, index) => (
        <span
          className={`atom atom-${index % 4}`}
          key={index}
          style={{
            left: `${12 + ((index * 19) % 76)}%`,
            top: `${16 + ((index * 31) % 66)}%`,
            transform: `scale(${0.72 + (index % 5) * 0.08})`
          }}
        />
      ))}
      <svg viewBox="0 0 600 260" role="presentation">
        <path d="M48 180 C118 48 182 58 236 118 S344 236 430 126 514 72 558 112" />
        <path d="M82 122 C156 222 224 224 300 148 S426 58 532 180" />
        <path d="M118 84 C210 20 344 44 480 84" />
      </svg>
    </div>
  );
}

function Metric({ label, value }: { label: string; value: number }) {
  return (
    <div className="metric">
      <strong>{value}</strong>
      <span>{label}</span>
    </div>
  );
}

function StatusPill({ status }: { status: DetectionStatus }) {
  return <span className={`status-pill ${status}`}>{statusText[status]}</span>;
}

function ParameterMappingList({ report }: { report: ParameterMappingReport | null }) {
  if (!report) {
    return <EmptyState title="等待映射" text="生成或修改 SimulationPlan 后自动刷新。" />;
  }

  const visibleItems = report.items.slice(0, 18);
  return (
    <div className="parameter-mapping">
      <div className="parameter-mapping-summary">
        <strong>{engineLabel[report.engineId] ?? report.engineId}</strong>
        <span>{report.items.length} 条参数映射</span>
        <small>{new Date(report.generatedAt).toLocaleString()}</small>
      </div>
      {report.warnings.length ? (
        <div className="warning-stack">
          {report.warnings.map((warning) => <p key={warning}>{warning}</p>)}
        </div>
      ) : null}
      <div className="parameter-mapping-table">
        <div className="parameter-mapping-head">
          <span>阶段</span>
          <span>GUI 参数</span>
          <span>原生字段</span>
          <span>文件</span>
          <span>状态</span>
        </div>
        {visibleItems.map((item, index) => (
          <div
            className={`parameter-mapping-row ${item.status}`}
            key={`${item.stageId}-${item.normalizedKey}-${item.engineKey}-${index}`}
          >
            <strong>{item.stageLabel}</strong>
            <div>
              <span className="mono">{item.normalizedKey}</span>
              <small>{item.normalizedValue}</small>
            </div>
            <div>
              <span className="mono">{item.engineKey}</span>
              <small>{item.engineValue}</small>
            </div>
            <span className="mono file-target">{item.targetFile}</span>
            <div className="mapping-status-cell">
              <MappingStatusPill status={item.status} />
              {item.notes.length ? <small>{item.notes.join(" ")}</small> : null}
            </div>
          </div>
        ))}
      </div>
      {report.items.length > visibleItems.length ? (
        <p className="muted-note">还有 {report.items.length - visibleItems.length} 条映射未展开，可在原生参数编辑器中查看对应文件。</p>
      ) : null}
    </div>
  );
}

function MappingStatusPill({ status }: { status: ParameterMappingStatus }) {
  return <span className={`mapping-pill ${status}`}>{parameterMappingStatusText[status]}</span>;
}

function ValidationList({ validation }: { validation: ValidationReport | null }) {
  if (!validation) {
    return <EmptyState title="等待校验" text="生成或修改 SimulationPlan 后自动刷新。" />;
  }
  return (
    <div className="validation-list">
      <div className={`validation-summary ${validation.status}`}>
        <strong>{validation.status}</strong>
        <span>{validation.items.length} 条消息</span>
      </div>
      {validation.items.map((item, index) => (
        <div className={`validation-item ${item.severity}`} key={`${item.field}-${index}`}>
          <span>{severityText[item.severity]}</span>
          <strong>{item.field}</strong>
          <p>{item.message}</p>
        </div>
      ))}
    </div>
  );
}

function EmptyState({ title, text }: { title: string; text: string }) {
  return (
    <div className="empty-state">
      <strong>{title}</strong>
      <p>{text}</p>
    </div>
  );
}

function CodeBlock({ value }: { value: string }) {
  return <pre className="code-block">{value}</pre>;
}

export default App;
