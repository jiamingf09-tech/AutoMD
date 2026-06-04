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

type TabId = "overview" | "workflow" | "run" | "remote" | "report" | "engines" | "build" | "plugins" | "guide";

type ThemeMode = "light" | "dark";

/** Viewport width below which the layout starts to feel cramped. */
const MIN_COMFORTABLE_WIDTH = 1024;

/**
 * Passive, non-blocking banner shown when the window is narrower than the
 * comfortable layout width. Read-only: it never touches app state or APIs.
 */
function WindowSizeNotice() {
  const [width, setWidth] = useState<number>(() =>
    typeof window === "undefined" ? MIN_COMFORTABLE_WIDTH : window.innerWidth
  );
  const [dismissed, setDismissed] = useState(false);

  useEffect(() => {
    function onResize() {
      setWidth(window.innerWidth);
    }
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  if (width >= MIN_COMFORTABLE_WIDTH || dismissed) {
    return null;
  }

  return (
    <div className="window-size-notice" role="status">
      <span>当前窗口较窄，部分元素可能显示拥挤或错位，建议加宽窗口以获得最佳显示效果。</span>
      <button type="button" onClick={() => setDismissed(true)}>
        知道了
      </button>
    </div>
  );
}

/**
 * Destructive, two-stage project-deletion dialog. Renders a full-viewport
 * blurred red scrim over the whole app. The Cancel button is auto-focused so
 * the Enter key always defaults to the safe action; deleting requires an
 * explicit second confirmation.
 */
function DeleteProjectModal({
  project,
  stage,
  deleting,
  onCancel,
  onConfirm
}: {
  project: ProjectSummary;
  stage: "warn" | "confirm";
  deleting: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const cancelRef = useRef<HTMLButtonElement>(null);

  // Keep focus on the safe (Cancel) action — also re-focus when the stage
  // advances to the second confirmation, so Enter never deletes by accident.
  useEffect(() => {
    cancelRef.current?.focus();
  }, [stage]);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault();
        onCancel();
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onCancel]);

  return (
    <div className="modal-overlay" role="presentation" onMouseDown={onCancel}>
      <div
        className="modal-dialog modal-danger"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="delete-project-title"
        aria-describedby="delete-project-body"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <div className="modal-icon" aria-hidden="true">⚠</div>
        {stage === "warn" ? (
          <>
            <h3 id="delete-project-title">永久删除项目？</h3>
            <div id="delete-project-body" className="modal-body">
              <p>
                即将删除「<strong>{project.name}</strong>」。此操作<strong>不可撤销</strong>。
              </p>
              <p>
                项目目录将被整体永久删除，包括原始数据、中间数据和最终数据（inputs、generated、runs、trajectories、analysis、reports
                等全部内容）。
              </p>
              <p className="modal-path mono">{project.path}</p>
            </div>
          </>
        ) : (
          <>
            <h3 id="delete-project-title">二次确认</h3>
            <div id="delete-project-body" className="modal-body">
              <p>
                请再次确认：确定要<strong>永久删除</strong>「<strong>{project.name}</strong>」吗？删除后<strong>无法恢复</strong>。
              </p>
            </div>
          </>
        )}
        <div className="modal-actions">
          <button type="button" className="modal-cancel" ref={cancelRef} onClick={onCancel} disabled={deleting}>
            取消
          </button>
          <button type="button" className="modal-delete" onClick={onConfirm} disabled={deleting}>
            {stage === "warn" ? "删除" : deleting ? "删除中…" : "确认删除"}
          </button>
        </div>
      </div>
    </div>
  );
}

const tabs: Array<{ id: TabId; label: string; description: string }> = [
  { id: "overview", label: "项目", description: "创建、导入和结构视图" },
  { id: "workflow", label: "流程", description: "参数、阶段和分析模块" },
  { id: "run", label: "运行", description: "本地、容器和 HPC 调度" },
  { id: "remote", label: "远程", description: "SSH 和队列 profile" },
  { id: "report", label: "报告", description: "可复现实验输出" },
  { id: "engines", label: "引擎", description: "检测、授权和平台能力" },
  { id: "build", label: "编译", description: "源码构建和容器 recipe" },
  { id: "plugins", label: "插件", description: "扩展 manifest 和能力" }
];

const guideTab = {
  id: "guide" as const,
  label: "使用指引",
  description: "软件使用、配置和部署手册"
};

const engineGuideRows = [
  {
    id: "gromacs",
    category: "首选生物分子引擎",
    install: "推荐 Conda/Mamba 或源码 CMake。源码编译时按需启用 MPI、CUDA/ROCm/OpenCL、PLUMED。",
    configure: "在引擎页保存 gmx/gmx_mpi 路径、版本和授权状态；在流程页选择力场、溶剂、离子和阶段；在运行页生成 .mdp、topol、run 脚本。",
    notes: "最适合作为首版闭环：准备、最小化、NVT/NPT、生产、checkpoint resume 和基础分析。"
  },
  {
    id: "openmm",
    category: "Python/快速原型引擎",
    install: "推荐 Conda/Mamba 环境安装 openmm、pdbfixer、mdanalysis；GPU 后端由平台和包版本决定。",
    configure: "在引擎页保存 Python/OpenMM 环境路径；在“流程”页选择 timestep、temperature、pressure、checkpoint/report interval。",
    notes: "适合教学、快速验证和自定义 Python runner；复杂体系仍建议先用结构准备和参数检查。"
  },
  {
    id: "ambertools",
    category: "Amber 输入生态",
    install: "推荐 Conda/Mamba 安装 ambertools；商业 AMBER pmemd 不随软件分发。",
    configure: "配置 tleap、sander、cpptraj 可执行文件；使用结构准备页生成 tleap、mdin 和 cpptraj 分析输入。",
    notes: "AmberTools 可用于参数化、拓扑生成和分析；pmemd 需要用户已有许可。"
  },
  {
    id: "namd",
    category: "用户自备许可入口",
    install: "用户自行下载并按 NAMD 许可安装；AutoMD 只保存路径、检测版本和生成 .conf/运行入口。",
    configure: "在引擎页标记授权状态，保存 namd2/namd3 路径；在运行页检查 .conf、结构、拓扑和参数文件。",
    notes: "不要把 NAMD 二进制放进 AutoMD 发布包。Windows/macOS 不适合的场景可走远程 Linux。"
  },
  {
    id: "lammps",
    category: "材料和粗粒化扩展",
    install: "推荐源码 CMake 或容器；按模型启用 KSPACE、MOLECULE、GPU/KOKKOS、MPI 等包。",
    configure: "保存 lmp 可执行文件路径；保留原生 input 文件编辑；远程/HPC 运行通常比桌面更可靠。",
    notes: "材料体系参数差异大，GUI 只映射常用资源和阶段，复杂 input 以原生编辑为准。"
  },
  {
    id: "cp2k",
    category: "QM/MM 和材料计算",
    install: "推荐 toolchain/source build 或 HPC module；BLAS/LAPACK、MPI、libxc、ELPA、CUDA 支持需按集群环境决定。",
    configure: "保存 cp2k/CP2K module 信息；在“编译”页生成 recipe，在“远程”页生成 SLURM/PBS/LSF 脚本。",
    notes: "桌面一键编译风险高，建议优先 dry-run、写脚本、远程执行。"
  },
  {
    id: "genesis",
    category: "生物分子高性能扩展",
    install: "推荐源码编译或 HPC module；GPU/MPI 能力按平台检测结果提示。",
    configure: "保存 spdyn/atdyn 路径；在流程页使用阶段模板，复杂参数保留原生输入文件。",
    notes: "后续可扩展完整模板；当前适合作为检测、打包和远程运行入口。"
  },
  {
    id: "hoomd",
    category: "粒子模拟和材料扩展",
    install: "推荐 Conda/Python 环境；GPU 能力依赖 HOOMD-blue 版本和 CUDA/平台。",
    configure: "保存 Python 环境或 hoomd runner；使用插件/原生脚本承载复杂模型。",
    notes: "适合自定义脚本型工作流，GUI 应避免过度抽象模型细节。"
  },
  {
    id: "dl_poly",
    category: "经典 MD 扩展",
    install: "通常为源码或集群 module；MPI 构建更适合 HPC。",
    configure: "保存可执行文件和 module load；原生 CONTROL/FIELD/CONFIG 文件保留编辑入口。",
    notes: "作为后续材料/经典 MD 模板扩展。"
  },
  {
    id: "tinker",
    category: "极化力场扩展",
    install: "用户安装 Tinker/Tinker-HP；按平台保存可执行文件路径。",
    configure: "保存 dynamic/minimize/analyze 等入口；key 文件使用原生编辑器。",
    notes: "重点是路径检测、key 文件管理和运行/分析日志解析。"
  },
  {
    id: "amber_pmemd",
    category: "商业/受限引擎",
    install: "用户自行获取 AMBER 许可并安装；AutoMD 不下载、不分发。",
    configure: "只保存 pmemd/pmemd.cuda 路径和许可状态；运行前给出授权提示。",
    notes: "可复用 AmberTools 输入生态，但执行入口必须来自用户环境。"
  },
  {
    id: "charmm",
    category: "商业/受限引擎",
    install: "用户自行安装并完成许可授权。",
    configure: "保存 charmm 可执行文件路径；原生命令文件由用户检查。",
    notes: "AutoMD 只做检测、模板入口和日志/报告衔接。"
  },
  {
    id: "desmond",
    category: "商业/受限引擎",
    install: "用户在自己的 Schrodinger 授权环境中配置。",
    configure: "保存 launcher 或命令路径；不自动下载、不自动绕过许可证。",
    notes: "建议作为企业/实验室已有环境的适配入口。"
  },
  {
    id: "acemd",
    category: "商业/受限引擎",
    install: "用户自行安装并授权。",
    configure: "保存路径和许可状态；复杂参数保留原生配置文件。",
    notes: "适合后续高级适配器扩展。"
  }
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
  const workspaceRef = useRef<HTMLElement | null>(null);
  const [theme, setTheme] = useState<ThemeMode>(() => {
    if (typeof window === "undefined") return "light";
    const stored = window.localStorage.getItem("automd-theme");
    if (stored === "light" || stored === "dark") return stored;
    return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  });

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
    window.localStorage.setItem("automd-theme", theme);
  }, [theme]);

  useEffect(() => {
    workspaceRef.current?.scrollTo({ top: 0 });
  }, [activeTab]);
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
  const [deleteTarget, setDeleteTarget] = useState<ProjectSummary | null>(null);
  const [deleteStage, setDeleteStage] = useState<"warn" | "confirm">("warn");
  const [deletingProject, setDeletingProject] = useState(false);
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

  const readyCount = engines.filter((engine) => engine.detection.status === "ready").length;
  const activeView = activeTab === "guide"
    ? guideTab
    : tabs.find((tab) => tab.id === activeTab) ?? tabs[0];
  const activeProject = currentProject ?? projects[0] ?? null;
  const showProjectBanner = !["engines", "build", "plugins", "guide"].includes(activeTab);

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

  async function selectProject(project: ProjectSummary) {
    try {
      setCurrentProject(project);
      setProjects((items) => [project, ...items.filter((item) => item.id !== project.id)]);
      if (project.preferredEngineId) {
        setSelectedEngineId(project.preferredEngineId);
      }
      const generatedPlan = await api.generatePlan({
        projectId: project.id,
        name: `${project.name} workflow`,
        engineId: project.preferredEngineId ?? selectedEngineId,
        domain: project.domain
      });
      setPlan(generatedPlan);
      setTask(null);
      setRunPackage(null);
      setLocalSnapshot(null);
      await refreshCachedMetadata(project.path);
      const records = await api.listTaskRecords(project.id);
      setTaskRecords(records);
    } catch (caught) {
      reportError(caught);
    }
  }

  function openProjectFolder(path?: string | null) {
    if (!path) {
      setError("当前没有可打开的项目目录。");
      return;
    }
    void api.openPath(path).catch(reportError);
  }

  function requestDeleteProject(project: ProjectSummary) {
    setDeleteStage("warn");
    setDeleteTarget(project);
  }

  function cancelDeleteProject() {
    if (deletingProject) {
      return;
    }
    setDeleteTarget(null);
    setDeleteStage("warn");
  }

  async function confirmDeleteProject() {
    if (!deleteTarget || deletingProject) {
      return;
    }
    // First click on "删除" only advances to the second confirmation.
    if (deleteStage === "warn") {
      setDeleteStage("confirm");
      return;
    }
    const target = deleteTarget;
    setDeletingProject(true);
    try {
      await api.deleteProject(target.id);
      setProjects((items) => items.filter((item) => item.id !== target.id));
      if (currentProject?.id === target.id) {
        setCurrentProject(null);
        setPlan(null);
        setTask(null);
        setTaskRecords([]);
        setArtifactRecords([]);
        setAnalysisCacheRecords([]);
        setStructureImportResult(null);
      }
      setDeleteTarget(null);
      setDeleteStage("warn");
    } catch (caught) {
      reportError(caught);
    } finally {
      setDeletingProject(false);
    }
  }

  function openPluginFolder() {
    void api.openPluginFolder().catch(reportError);
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
    <>
      <WindowSizeNotice />
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
              className={`nav-item ${activeTab === tab.id ? "active" : ""} ${tab.id === "engines" ? "nav-separated" : ""}`}
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
          <button
            type="button"
            className={`guide-launch ${activeTab === "guide" ? "active" : ""}`}
            onClick={() => setActiveTab("guide")}
          >
            <span>使用指引</span>
            <small>软件配置、引擎、插件和部署</small>
          </button>
          <div className="sidebar-status-row">
            <span className="status-dot ready" />
            <span>{readyCount} 个本地能力已检测可用</span>
            <button
              type="button"
              className="theme-toggle"
              onClick={() => setTheme((current) => (current === "dark" ? "light" : "dark"))}
              aria-label={theme === "dark" ? "切换到浅色模式" : "切换到深色模式"}
              title={theme === "dark" ? "浅色模式" : "深色模式"}
            >
              {theme === "dark" ? "☀" : "🌙"}
            </button>
          </div>
        </div>
      </aside>

      <section className="workspace" ref={workspaceRef}>
        <header className="topbar">
          <div>
            <p className="eyebrow">跨平台生物分子 MD 首版</p>
            <h2>{activeView.label}</h2>
          </div>
          {activeTab === "guide" ? (
            <div className="topbar-actions">
              <button type="button" onClick={() => setActiveTab("overview")}>
                新建项目
              </button>
              <button type="button" className="primary" onClick={() => setActiveTab("engines")}>
                配置引擎
              </button>
            </div>
          ) : (
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
          )}
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

        {showProjectBanner ? (
          <CurrentProjectBanner
            project={activeProject}
            openProjectFolder={openProjectFolder}
          />
        ) : null}

        {activeTab === "overview" && (
          <ProjectPanel
            projects={projects}
            projectName={projectName}
            setProjectName={setProjectName}
            domain={domain}
            setDomain={setDomain}
            project={activeProject}
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
            plan={plan}
            createProject={createProject}
            importStructure={importStructure}
            selectProject={selectProject}
            requestDeleteProject={requestDeleteProject}
            openProjectFolder={openProjectFolder}
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
          <PluginsPanel pluginRegistry={pluginRegistry} openPluginFolder={openPluginFolder} />
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

        {activeTab === "guide" && (
          <GuidePanel
            engines={engines}
            pluginRegistry={pluginRegistry}
            setActiveTab={setActiveTab}
          />
        )}
      </section>
      </main>
      <AppStatusBar diagnostics={diagnostics} />
      {deleteTarget ? (
        <DeleteProjectModal
          project={deleteTarget}
          stage={deleteStage}
          deleting={deletingProject}
          onCancel={cancelDeleteProject}
          onConfirm={confirmDeleteProject}
        />
      ) : null}
    </>
  );
}

function GuidePanel({
  engines,
  pluginRegistry,
  setActiveTab
}: {
  engines: EngineCapability[];
  pluginRegistry: PluginRegistrySnapshot | null;
  setActiveTab: (tab: TabId) => void;
}) {
  const pluginKinds = Object.keys(pluginKindText) as PluginKind[];
  const exampleFlow: Array<{ step: string; action: string; details: string; done: string }> = [
    {
      step: "1. 创建示例项目",
      action: "进入“项目”页，创建项目，例如 Protein_Water_Demo。",
      details: "选择生物分子项目类型，项目目录建议放在空间充足的位置。导入本地 PDB/mmCIF 文件；如果还没有真实结构，可以先用已有小蛋白或短肽文件练习完整流程。",
      done: "项目列表出现新项目，结构摘要能显示原子数、残基数、链信息或导入文件路径。"
    },
    {
      step: "2. 准备结构",
      action: "在“项目/流程”相关区域检查结构准备包。",
      details: "确认缺失原子、氢原子、非标准残基、配体和水/离子处理。蛋白示例可使用 pH 7.0 加氢、Amber99SB-ILDN 或 CHARMM36 类蛋白力场、TIP3P 水模型、0.15 M NaCl，并中和体系。",
      done: "准备包能生成拓扑/结构输入；如果配体参数失败，先回到配体参数化而不是直接运行。"
    },
    {
      step: "3. 配置 GROMACS 或 OpenMM",
      action: "进入“引擎”页，选择要跑的引擎并保存路径。",
      details: "GROMACS 填 gmx 或 gmx_mpi；OpenMM 填 Python/Conda 环境。状态为 ready 才适合真实运行；缺失时先到“编译”页生成安装脚本，或先用 Mock runner 验证软件流程。",
      done: "引擎卡片显示可用版本、平台、GPU/MPI/PLUMED 能力和授权状态。"
    },
    {
      step: "4. 设置模拟流程",
      action: "进入“流程”页，逐段检查 EM、NVT、NPT、Production。",
      details: "入门示例可设置 EM 5000-50000 steps，NVT 100 ps，NPT 100 ps，Production 1 ns；温度 300 K，压力 1 bar，timestep 2 fs，约束 H-bonds，checkpoint/report interval 10-100 ps。",
      done: "Validation 没有 error；warning 要读完，尤其是力场、水模型、离子浓度、平台能力和输出频率。"
    },
    {
      step: "5. 先生成运行包",
      action: "进入“运行”页，先用 Dry run 或生成 run package。",
      details: "Dry run 只写输入文件、命令、脚本和目录，不启动引擎。检查 .mdp/.tpr/.top、OpenMM runner、run.sh、checkpoint 路径和输出文件布局。",
      done: "run directory、命令、输入文件和 artifact 预期清楚；这一步过了再进入真实执行。"
    },
    {
      step: "6. 运行本地或远程任务",
      action: "本地小体系可直接运行；集群任务进入“远程”页生成提交脚本。",
      details: "本地运行时关注日志、进度、checkpoint 和失败分类。HPC 运行时先 dry-run profile，确认 ssh、rsync、workdir、module load、队列、GPU 资源和 walltime。",
      done: "任务状态能从 preparing/running 进入 completed，或失败时能看到明确原因。"
    },
    {
      step: "7. 索引轨迹并分析",
      action: "回到“运行”页刷新 artifacts，索引轨迹，生成分析包。",
      details: "先索引 xtc/trr/dcd/pdb/xyz，再分块预览轨迹。常用分析包括 RMSD、RMSF、Rg、氢键、距离、角度、二面角、能量、温度、压力和接触图。",
      done: "分析结果有曲线、统计值或缓存记录；大轨迹不要一次性加载到前端。"
    },
    {
      step: "8. 导出报告",
      action: "进入“报告”页，选择 Markdown、HTML 或 PDF。",
      details: "报告应包含项目、环境、引擎版本、参数、运行命令、日志摘要、checkpoint、轨迹摘要、分析结果和可复现记录。正式项目建议保留原生参数文件。",
      done: "导出路径出现报告文件；报告能说明这次模拟怎样复现、哪里可能需要人工复核。"
    }
  ];

  const moduleRows: Array<{
    title: string;
    target: TabId;
    use: string;
    fill: string;
    check: string;
    next: string;
  }> = [
    {
      title: "项目",
      target: "overview",
      use: "创建项目、快速切换项目、打开项目文件夹、导入结构，并查看结构与轨迹视图。",
      fill: "填写项目名、领域、首选引擎；导入 PDB/mmCIF/SDF/MOL2/SMILES 或已有工程目录。项目索引里可以一键打开项目所在文件夹。",
      check: "当前项目固定条显示正确项目；结构导入后能看到 importedPath、原子/残基/链摘要，结构视图从空状态变为 Mol* 加载状态。",
      next: "导入后进入“流程”设置力场、溶剂、离子和阶段参数；还没配置引擎时先去“引擎”。"
    },
    {
      title: "引擎",
      target: "engines",
      use: "配置本机或用户授权环境中的 MD 引擎。这里决定软件能不能调用 GROMACS、OpenMM、AmberTools、NAMD 等。",
      fill: "填写可执行文件路径、版本、授权状态和检测记录。商业/受限引擎只登记用户已有路径，不在软件里下载。",
      check: "ready 表示可直接调用；需要安装表示先去 Build；需要许可证表示先完成外部授权；平台不支持时考虑 WSL2、容器或远程。",
      next: "引擎 ready 后回“流程”映射参数；缺工具去“编译”；Linux-only 或大任务去“远程”。"
    },
    {
      title: "流程",
      target: "workflow",
      use: "编辑 SimulationPlan：体系、力场、溶剂、离子、模拟阶段、输出和基础分析。这里是参数工作的中心。",
      fill: "设置力场、水模型、盒子尺寸、离子浓度、温度、压力、timestep、阶段时长、checkpoint 间隔和输出频率。复杂引擎参数保留原生文件编辑。",
      check: "看参数映射是否 mapped、approximated 或 unsupported。unsupported 不代表不能跑，但代表需要人工看原生输入文件。",
      next: "Validation 通过后去“运行”生成 run package；结构准备失败则回到项目/结构输入。"
    },
    {
      title: "运行",
      target: "run",
      use: "执行本地任务、Dry run、Mock runner、日志解析、取消任务、checkpoint resume、批量重复实验、轨迹索引和分析包生成。",
      fill: "选择本地运行模式，设置批量重复数量和 seed，必要时编辑原生参数文件，粘贴日志样本做解析。",
      check: "先确认 run package，再看任务状态、日志尾部、失败分类、checkpoint 和 artifact。真实执行前最好先 Dry run。",
      next: "本地完成后刷新 artifact 并分析；集群任务去“远程”；需要报告去“报告”。"
    },
    {
      title: "远程",
      target: "remote",
      use: "配置 SSH/HPC profile，生成同步、提交、查询、日志和回收脚本。适合大体系、GPU 队列和 Linux-only 引擎。",
      fill: "填写 host、user、port、workdir、scheduler、queue/partition、account、walltime、CPU/GPU、module load 和运行命令模板。",
      check: "先 Dry run，看 rsync、ssh、sbatch/qsub/bsub、状态查询和日志路径是否正确。workdir 必须有写权限。",
      next: "脚本确认后执行提交；任务完成后回收结果，再到“运行/报告”分析。"
    },
    {
      title: "编译",
      target: "build",
      use: "生成安装脚本、源码编译 recipe、容器 recipe 和构建日志。适合没有引擎、需要 MPI/GPU/PLUMED 或平台不支持时使用。",
      fill: "选择引擎和构建模式，设置 prefix、MPI、GPU 后端、PLUMED、容器工具和超时。默认先 dry-run 或只写脚本。",
      check: "读 build manifest，确认不会写入系统目录、不会绕过许可证、下载源可信、GPU/MPI 选项符合机器或集群环境。",
      next: "编译成功后回“引擎”保存新路径；失败时看日志分类和缺失依赖。"
    },
    {
      title: "插件",
      target: "plugins",
      use: "查看和管理扩展 manifest。插件可以增加引擎适配器、分析模块、远程调度器、构建 recipe 或报告模板。",
      fill: "把 .automd-plugin.json 放入插件目录，声明 id、name、kind、version、entrypoint、capabilities、license 和支持平台。",
      check: "查看 warning、entrypoint、sourcePath 和 capabilities。未知来源插件不要启用执行命令，先读 manifest。",
      next: "插件被识别后，对应能力会出现在引擎、分析、远程、编译或报告页面。"
    },
    {
      title: "报告",
      target: "report",
      use: "整理可复现实验记录，导出 Markdown、HTML 或 PDF。适合项目结束、阶段汇报或复现实验归档。",
      fill: "选择报告格式，刷新 artifact 和分析缓存，确认项目、参数、环境、命令、日志和图表都已进入报告。",
      check: "报告应能回答：输入是什么、用什么引擎和版本、参数是什么、命令如何执行、结果在哪里、哪些地方需要人工复核。",
      next: "导出后保存报告和项目目录；需要继续生产模拟时回“运行”用 checkpoint resume。"
    }
  ];

  return (
    <div className="guide-page">
      <section className="panel span-3 guide-hero">
        <div>
          <p className="eyebrow">AutoMD 软件使用手册</p>
          <h3>按页面完成分子动力学项目：导入结构、配置引擎、设置参数、运行、分析和导出报告。</h3>
        </div>
        <div className="guide-actions">
          <button type="button" className="primary" onClick={() => setActiveTab("overview")}>
            从新建项目开始
          </button>
          <button type="button" onClick={() => setActiveTab("engines")}>
            去配置引擎
          </button>
          <button type="button" onClick={() => setActiveTab("build")}>
            查看编译部署
          </button>
        </div>
      </section>

      <section className="panel span-3">
        <div className="panel-title-row">
          <div>
            <h3>完整示例：小型蛋白水溶液模拟</h3>
            <p className="muted">下面是一条可以照着走的完整路线。没有真实引擎时，也可以先用 Mock runner 验证软件操作闭环。</p>
          </div>
        </div>
        <div className="guide-flow-list">
          {exampleFlow.map((item) => (
            <article className="guide-flow-step" key={item.step}>
              <h4>{item.step}</h4>
              <dl className="compact-dl">
                <div><dt>在软件里做什么</dt><dd>{item.action}</dd></div>
                <div><dt>怎么填</dt><dd>{item.details}</dd></div>
                <div><dt>完成标志</dt><dd>{item.done}</dd></div>
              </dl>
            </article>
          ))}
        </div>
      </section>

      <section className="panel span-3">
        <div className="panel-title-row">
          <div>
            <h3>每个页面怎么用</h3>
            <p className="muted">先按页面职责找入口，再看“完成标志”。这样比到处找按钮更稳。</p>
          </div>
        </div>
        <div className="guide-module-list">
          {moduleRows.map((row) => (
            <article className="guide-module-row" key={row.title}>
              <div>
                <h4>{row.title}</h4>
                <button type="button" onClick={() => setActiveTab(row.target)}>
                  打开{row.title}页
                </button>
              </div>
              <dl className="compact-dl">
                <div><dt>用途</dt><dd>{row.use}</dd></div>
                <div><dt>需要填写/检查</dt><dd>{row.fill}</dd></div>
                <div><dt>完成标志</dt><dd>{row.check}</dd></div>
                <div><dt>下一步</dt><dd>{row.next}</dd></div>
              </dl>
            </article>
          ))}
        </div>
      </section>

      <section className="panel span-3">
        <div className="panel-title-row">
          <div>
            <h3>固定当前项目和底部状态栏</h3>
            <p className="muted">这两个区域不属于某一次参数设置，而是帮助你随时确认“现在操作的是哪个项目、当前机器适合怎么跑”。</p>
          </div>
        </div>
        <dl className="definition-list">
          <div><dt>当前项目</dt><dd>在项目、流程、运行、远程和报告页顶部固定显示。滚动页面时仍能看到项目名、状态、目录，并可以快速切换项目或打开项目文件夹。</dd></div>
          <div><dt>GPU 状态</dt><dd>软件启动时自动检测 CUDA、ROCm 或 macOS Metal 能力，并在窗口底部右侧显示红/绿指示灯。绿色表示可用；红色表示当前按 CPU fallback 使用。</dd></div>
          <div><dt>悬停提示</dt><dd>鼠标放到底部 GPU 状态上，会显示不可用原因，例如未检测到 GPU 工具、平台/引擎不支持，或预览环境无法访问硬件。</dd></div>
          <div><dt>结构视图</dt><dd>新项目默认为空，导入结构后才会加载 Mol*。如果结构路径无效或格式不支持，视图会保留错误提示而不是显示假的分子图。</dd></div>
        </dl>
      </section>

      <section className="panel span-3">
        <div className="panel-title-row">
          <div>
            <h3>引擎配置</h3>
            <p className="muted">先把引擎登记到“引擎”页；缺少依赖时去“编译”页生成安装或编译脚本；平台不合适时走远程。</p>
          </div>
          <button type="button" onClick={() => setActiveTab("engines")}>打开引擎页</button>
        </div>
        <div className="guide-engine-list">
          {engineGuideRows.map((row) => {
            const engine = engines.find((item) => item.id === row.id);
            return (
              <article className="guide-engine-row" key={row.id}>
                <div>
                  <h4>{engine?.name ?? engineLabel[row.id] ?? row.id}</h4>
                  <div className="chip-row">
                    <span>{row.category}</span>
                    {engine ? <span>{statusText[engine.detection.status]}</span> : <span>等待注册</span>}
                    {engine?.license.requiresUserLicense ? <span>用户自带许可</span> : <span>开源/自由获取优先</span>}
                  </div>
                </div>
                <dl className="compact-dl">
                  <div><dt>安装</dt><dd>{row.install}</dd></div>
                  <div><dt>配置</dt><dd>{row.configure}</dd></div>
                  <div><dt>注意</dt><dd>{row.notes}</dd></div>
                </dl>
              </article>
            );
          })}
        </div>
      </section>

      <section className="panel span-2">
        <div className="panel-title-row">
          <div>
            <h3>引擎安装、部署和编译</h3>
            <p className="muted">软件内的“一键部署”会先生成可检查脚本，不会默认静默改系统目录。</p>
          </div>
          <button type="button" onClick={() => setActiveTab("build")}>打开编译页</button>
        </div>
        <div className="guide-section">
          <h4>推荐操作顺序</h4>
          <ol className="guide-steps compact">
            <li>在“引擎”页先检测 PATH、Conda/Mamba、Docker/Podman、CUDA/ROCm/OpenCL、MPI 和 PLUMED。</li>
            <li>如果引擎缺失，在“编译”页选择引擎，生成容器 recipe、源码脚本和 build manifest。</li>
            <li>先 Dry run，确认命令、下载源、写入目录、权限、prefix、GPU/MPI/PLUMED 选项。</li>
            <li>选择“只写脚本”时，脚本会落盘；你可以拿到 WSL2、Linux 服务器或 HPC 登录节点上再运行。</li>
            <li>只有在本机环境明确可控时才选择“执行构建”。执行后看日志路径、失败分类和生成的可执行文件。</li>
          </ol>
          <h4>常见构建选项</h4>
          <dl className="definition-list">
            <div><dt>MPI</dt><dd>多节点或多进程任务启用。桌面单机测试可先关闭，HPC 建议启用。</dd></div>
            <div><dt>GPU</dt><dd>CUDA、ROCm、OpenCL、Metal、SYCL 能力按引擎和平台判断，不能简单等价。</dd></div>
            <div><dt>PLUMED</dt><dd>增强采样常见于 GROMACS/LAMMPS/CP2K 等，必须匹配引擎版本重新编译或动态链接。</dd></div>
            <div><dt>Prefix</dt><dd>优先使用用户目录、Conda 环境或容器路径。系统目录需要管理员权限，不建议默认写入。</dd></div>
            <div><dt>容器</dt><dd>开源引擎可生成 Docker/Podman recipe；商业/受限引擎只能在用户已有授权环境中配置路径。</dd></div>
          </dl>
        </div>
      </section>

      <section className="panel">
        <div className="panel-title-row">
          <div>
            <h3>平台策略</h3>
            <p className="muted">同一个按钮背后的执行环境要因平台而异。</p>
          </div>
        </div>
        <dl className="definition-list">
          <div><dt>Windows</dt><dd>原生引擎直接调用；Linux-only 引擎优先 WSL2、容器或远程 Linux。</dd></div>
          <div><dt>macOS</dt><dd>区分 Apple Silicon 和 Intel。Metal/GPU 支持只在引擎明确支持时显示。</dd></div>
          <div><dt>Linux</dt><dd>最适合本地或 HPC 执行。注意 CUDA/ROCm 驱动、MPI ABI 和 module 版本。</dd></div>
          <div><dt>HPC</dt><dd>不要在登录节点盲目编译。先生成脚本，再按集群政策提交或交给管理员环境。</dd></div>
        </dl>
      </section>

      <section className="panel span-3">
        <div className="panel-title-row">
          <div>
            <h3>远程/HPC 配置</h3>
            <p className="muted">先保存 profile，再 dry-run 脚本，最后提交。不要第一次就直接执行大任务。</p>
          </div>
          <button type="button" onClick={() => setActiveTab("remote")}>打开远程页</button>
        </div>
        <div className="guide-table">
          <div className="guide-table-head">字段</div>
          <div className="guide-table-head">怎么填</div>
          <div className="guide-table-head">检查点</div>
          <div><strong>Host</strong></div>
          <div>登录节点域名，例如 login.cluster.edu。建议先在终端确认 ssh 能免密或正确输入密码。</div>
          <div>连接失败先查网络、VPN、SSH key、known_hosts。</div>
          <div><strong>Scheduler</strong></div>
          <div>选择 SLURM、PBS 或 LSF。AutoMD 会按调度器生成提交、状态和回收脚本。</div>
          <div>队列字段、GPU 资源语法、account/project 名称通常需要按集群改。</div>
          <div><strong>Workdir</strong></div>
          <div>远程工作目录，例如 /scratch/$USER/automd。不要放在空间很小的 home 目录。</div>
          <div>确认有写权限，轨迹文件会很大。</div>
          <div><strong>Module load</strong></div>
          <div>填写 gcc/openmpi/cuda/gromacs/cp2k 等 module load 命令，每行一条。</div>
          <div>module 版本必须和编译时 ABI 匹配。</div>
          <div><strong>Run mode</strong></div>
          <div>Dry run 只预览；写脚本只落盘；Execute 才会 ssh/rsync/submit。</div>
          <div>第一次建议 Dry run 和写脚本，确认后再执行。</div>
        </div>
      </section>

      <section className="panel span-3">
        <div className="panel-title-row">
          <div>
            <h3>插件系统使用</h3>
            <p className="muted">插件用于扩展能力。安装前先看来源、入口命令和 warning。</p>
          </div>
          <button type="button" onClick={() => setActiveTab("plugins")}>打开插件页</button>
        </div>
        <div className="metric-grid plugin-metrics">
          {pluginKinds.map((kind) => (
            <Metric
              key={kind}
              label={pluginKindText[kind]}
              value={pluginRegistry?.manifests.filter((manifest) => manifest.kind === kind).length ?? 0}
            />
          ))}
        </div>
        <div className="guide-section">
          <h4>当前已识别插件</h4>
          {pluginRegistry?.manifests.length ? (
            <div className="guide-plugin-list">
              {pluginRegistry.manifests.map((manifest) => (
                <article className="guide-plugin-row" key={manifest.id}>
                  <div>
                    <h4>{manifest.name}</h4>
                    <div className="chip-row">
                      <span>{pluginKindText[manifest.kind]}</span>
                      <span>v{manifest.version}</span>
                      <span>{manifest.engineId ?? "通用"}</span>
                    </div>
                  </div>
                  <dl className="compact-dl">
                    <div><dt>ID</dt><dd className="mono">{manifest.id}</dd></div>
                    <div><dt>入口</dt><dd className="mono truncate">{manifest.entrypoint}</dd></div>
                    <div><dt>来源</dt><dd className="mono truncate">{manifest.sourcePath ?? "built-in"}</dd></div>
                    <div><dt>能力</dt><dd>{manifest.capabilities.join(", ") || "未声明"}</dd></div>
                    <div><dt>许可</dt><dd>{manifest.licensePolicy ?? "未声明特殊许可"}</dd></div>
                    <div><dt>警告</dt><dd>{manifest.warnings.join("; ") || "无"}</dd></div>
                  </dl>
                </article>
              ))}
            </div>
          ) : (
            <EmptyState title="尚未识别到插件" text="打开插件页确认插件目录，或放入 *.automd-plugin.json manifest 后重新扫描。" />
          )}
          <h4>安装插件</h4>
          <ol className="guide-steps compact">
            <li>把插件 manifest 放到插件目录，文件名建议以 <span className="mono">.automd-plugin.json</span> 结尾。</li>
            <li>打开插件页，确认 manifest 数量、类型和警告信息。</li>
            <li>插件提供的引擎适配器、分析模块、调度器或报告模板会进入对应页面。</li>
            <li>来自未知来源的插件不要直接启用执行能力。先检查命令、脚本路径和写入目录。</li>
          </ol>
          <h4>manifest 至少应该说明</h4>
          <div className="chip-row">
            <span>id</span>
            <span>name</span>
            <span>kind</span>
            <span>version</span>
            <span>entry/command</span>
            <span>capabilities</span>
            <span>supportedPlatforms</span>
            <span>license</span>
          </div>
          <p className="muted">
            插件目录由当前系统的应用数据目录动态生成，不会写死某个用户名或某台电脑的绝对路径。插件页会显示本机实际目录，也可以一键打开。
          </p>
          <p className="muted">
            本机当前插件目录：<span className="mono">{pluginRegistry?.pluginRoot ?? "尚未加载"}</span>
          </p>
        </div>
      </section>

      <section className="panel span-3">
        <div className="panel-title-row">
          <div>
            <h3>运行、分析和报告</h3>
            <p className="muted">建议每次真实执行前都先生成运行包；每次结束后都刷新 artifact，再做分析和报告。</p>
          </div>
          <button type="button" onClick={() => setActiveTab("run")}>打开运行页</button>
        </div>
        <dl className="definition-list">
          <div><dt>Dry run</dt><dd>只生成输入、命令和脚本，不启动进程。适合检查参数、路径和文件布局。</dd></div>
          <div><dt>Mock runner</dt><dd>内置模拟器，用于测试 GUI、日志刷新、checkpoint、artifact 和报告闭环。</dd></div>
          <div><dt>真实执行</dt><dd>调用用户配置的本地引擎。执行前确认路径、许可证、GPU/MPI、项目目录、输出频率和 checkpoint 间隔。</dd></div>
          <div><dt>Checkpoint</dt><dd>中断后优先找 checkpoint resume。不要直接删除 run directory，否则会丢失恢复依据。</dd></div>
          <div><dt>轨迹</dt><dd>先索引，再分块加载。大轨迹不要一次性加载；先抽样预览，再交给 MDAnalysis 生成分析包。</dd></div>
          <div><dt>分析</dt><dd>RMSD 看整体稳定性，RMSF 看残基波动，Rg 看紧密程度，氢键/距离/角度/二面角看局部事件，能量/温度/压力看运行质量。</dd></div>
          <div><dt>报告</dt><dd>报告应包含环境、参数、命令、日志、分析图表、artifact、checkpoint 和可复现记录。</dd></div>
        </dl>
      </section>

      <section className="panel span-3">
        <div className="panel-title-row">
          <div>
            <h3>故障处理顺序</h3>
            <p className="muted">先排环境，再排输入，最后排数值稳定性。</p>
          </div>
        </div>
        <ol className="guide-steps compact">
          <li>引擎显示缺失：回到引擎页保存可执行文件路径，或在“编译”页生成安装脚本。</li>
          <li>许可证缺失：只在用户已有授权环境中配置，不在软件内下载商业引擎。</li>
          <li>拓扑/力场失败：回到流程页检查非标准残基、配体参数、力场和水模型。</li>
          <li>GPU 不可用：确认驱动、CUDA/ROCm/OpenCL、容器 runtime、HPC 分区和引擎编译选项。</li>
          <li>远程失败：先检查 ssh、workdir、module load、队列名和调度器输出。</li>
          <li>数值发散：降低 timestep、加强最小化、检查约束、温压耦合和初始结构冲突。</li>
        </ol>
      </section>
    </div>
  );
}

function CurrentProjectBanner({
  project,
  openProjectFolder
}: {
  project: ProjectSummary | null;
  openProjectFolder: (path?: string | null) => void;
}) {
  return (
    <section className="current-project-sticky" aria-label="current project">
      <div className="current-project-main">
        <span className="status-dot ready" />
        <div>
          <small>当前项目</small>
          <strong>{project?.name ?? "尚未选择项目"}</strong>
        </div>
      </div>
      <div className="current-project-actions">
        <button type="button" onClick={() => openProjectFolder(project?.path)} disabled={!project}>
          打开文件夹
        </button>
      </div>
    </section>
  );
}

function AppStatusBar({ diagnostics }: { diagnostics: RuntimeDiagnostics | null }) {
  const gpu = diagnostics?.gpu;
  const title = gpu
    ? `${gpu.reason}\n${gpu.detail}\n检查时间：${new Date(gpu.checkedAt).toLocaleString()}`
    : "正在检测 GPU 状态";

  return (
    <footer className="app-statusbar">
      <span>AutoMD</span>
      <div className={`gpu-status ${gpu?.available ? "available" : "unavailable"}`} title={title}>
        <span className="gpu-status-dot" />
        <span>{gpu?.label ?? "GPU 状态检测中"}</span>
        {gpu ? <small>{gpu.mode === "gpu" ? "GPU 模式" : "CPU 模式"}</small> : null}
      </div>
    </footer>
  );
}

function ProjectPanel({
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
  plan,
  createProject,
  importStructure,
  selectProject,
  requestDeleteProject,
  openProjectFolder
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
  plan: SimulationPlan | null;
  createProject: () => void;
  importStructure: () => void;
  selectProject: (project: ProjectSummary) => void;
  requestDeleteProject: (project: ProjectSummary) => void;
  openProjectFolder: (path?: string | null) => void;
}) {
  return (
    <div className="content-grid project-grid">
      <section className="engine-reminder span-3" role="note">
        <strong>请先检查引擎配置</strong>
        <span>开始导入和运行前，建议先到“引擎”页确认 GROMACS、OpenMM 或其他目标引擎是否可用；缺失时再到“编译”页生成安装脚本。</span>
      </section>
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
        <h3>项目索引</h3>
        {projects.length === 0 ? (
          <EmptyState title="暂无项目" text="AutoMD 会为每个项目创建 inputs、generated、runs、trajectories、analysis、reports、remote 等目录。" />
        ) : (
          <div className="project-index-list">
            {projects.map((item) => (
              <div className={`project-index-row ${project?.id === item.id ? "active" : ""}`} key={item.id}>
                <button type="button" onClick={() => selectProject(item)}>
                  <strong>{item.name}</strong>
                  <small>{item.domain} / {item.status}</small>
                  <span className="mono truncate">{item.path}</span>
                </button>
                <button type="button" className="project-delete" onClick={() => requestDeleteProject(item)}>
                  删除项目
                </button>
              </div>
            ))}
          </div>
        )}
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
        <MoleculeViewport plan={plan} project={project} />
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

function PluginsPanel({
  pluginRegistry,
  openPluginFolder
}: {
  pluginRegistry: PluginRegistrySnapshot | null;
  openPluginFolder: () => void;
}) {
  if (!pluginRegistry) {
    return (
      <section className="panel">
        <div className="panel-title-row">
          <h3>插件目录</h3>
          <button type="button" onClick={openPluginFolder}>
            打开插件目录
          </button>
        </div>
        <EmptyState title="插件注册表尚未加载" text="AutoMD 会扫描当前系统应用数据目录中的 *.automd-plugin.json manifest。" />
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
        <div className="panel-title-row">
          <h3>插件目录</h3>
          <button type="button" onClick={openPluginFolder}>
            打开插件目录
          </button>
        </div>
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
          <p>{sourcePath ? plan?.system.name : "结构导入后就绪"}</p>
          {plan?.system.sourcePath ? <small className="mono">{plan.system.sourcePath}</small> : null}
        </div>
        <span className="viewer-badge">{viewerStatus}</span>
      </div>
      <div className={`molecule-canvas ${sourcePath ? "molstar-canvas" : ""}`} aria-label="molecular viewport">
        <div ref={hostRef} className="molstar-host" />
        {showPlaceholder ? (
          <div className="molecule-empty-state">
            <EmptyState
              title={sourcePath ? "结构暂时无法显示" : "等待结构导入"}
              text={sourcePath ? viewerStatus : "导入 PDB、mmCIF、SDF、MOL2、SMILES 或已有引擎工程后，这里会加载结构与轨迹视图。"}
            />
          </div>
        ) : null}
      </div>
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
