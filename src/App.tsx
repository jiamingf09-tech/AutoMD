import appIconUrl from './assets/icon.png';
import { Fragment, useEffect, useMemo, useRef, useState } from "react";
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
  EngineDeployResult,
  EngineDeployStrategy,
  EngineInstallationRecord,
  EngineTarget,
  EngineLogReport,
  EngineRunPackage,
  ExecutionMode,
  FailureAnalysis,
  GpuBackend,
  LocalRunMode,
  LocalTaskSnapshot,
  ParameterMappingReport,
  ParameterMappingStatus,
  ProjectDomain,
  PluginKind,
  PluginAction,
  PluginConfigRequest,
  PluginImportRequest,
  PluginManifest,
  PluginRegistrySnapshot,
  PluginRunMode,
  PluginRunRequest,
  PluginRunResult,
  PluginTemplateRequest,
  ProjectTextFilePayload,
  ProjectSummary,
  ReportFormat,
  ExportedReport,
  RemoteAuthMethod,
  RemoteConnectionTest,
  RemoteExecutionPackage,
  RemoteHelperStatus,
  RemoteJobSnapshot,
  RemoteJobSubmission,
  RemoteProfile,
  RemoteSubmitPreflight,
  RemoteWorkflowMode,
  RemoteWorkflowStepResult,
  RecipeExportResult,
  ResumePlan,
  RuntimeDiagnostics,
  RunArtifact,
  ScienceSidecarDiagnostics,
  ScienceToolDiagnostic,
  SimulationPlan,
  SimulationStage,
  SimulationTask,
  ImportedStructureEntry,
  StructurePreparationPackage,
  StructureSummary,
  TaskRecord,
  TrajectoryAnalysisPackage,
  TrajectoryChunk,
  TrajectoryIndex,
  StructureImportResult,
  StructureSourceKind,
  ToolDiagnostic,
  ValidationReport,
  ValidationSeverity
} from "./types";

type TabId = "overview" | "workflow" | "run" | "remote" | "report" | "engines" | "plugins" | "pluginDetail" | "guide";

type NotificationSeverity = "error" | "warning" | "success" | "info";

type AppNotification = {
  id: string;
  severity: NotificationSeverity;
  title: string;
  message: string;
  /** Optional one-click fix the user can adopt (e.g. jump to the right page). */
  action?: { label: string; run: () => void };
  /** Show a "查看指引" link into the guide page. */
  guide?: boolean;
  /** Errors/warnings are tracked as unresolved "问题": closing minimizes (stays counted) instead of deleting. */
  persistent: boolean;
  /** Whether the toast is currently shown in the stack (minimized problems have visible=false). */
  visible: boolean;
  createdAt: number;
};

/** Fixed per-severity glyph + label (no colloquial wording). */
const NOTIFICATION_ICON: Record<NotificationSeverity, string> = {
  error: "❌",
  warning: "⚠️",
  info: "⏰",
  success: "✅"
};

type ThemeMode = "light" | "dark";

interface PerformancePreferences {
  cpuThreads: number;
  gpuDeviceId: string;
  gpuCount: number;
  memoryLimitGb: number;
  diskId: string;
}

type BackgroundTaskKind = "search" | "download" | "install" | "build" | "compile";
type BackgroundTaskStatus = "running" | "completed" | "failed";

interface BackgroundTask {
  id: string;
  label: string;
  kind: BackgroundTaskKind;
  status: BackgroundTaskStatus;
  progress: number;
  detail: string;
  startedAt: string;
  updatedAt: string;
}

const PERFORMANCE_PREF_KEY = "automd-performance-preferences";

function loadPerformancePreferences(): PerformancePreferences {
  if (typeof window === "undefined") {
    return { cpuThreads: 0, gpuDeviceId: "auto", gpuCount: 1, memoryLimitGb: 0, diskId: "auto" };
  }
  try {
    const parsed = JSON.parse(window.localStorage.getItem(PERFORMANCE_PREF_KEY) ?? "{}") as Partial<PerformancePreferences>;
    return {
      cpuThreads: Number(parsed.cpuThreads) || 0,
      gpuDeviceId: parsed.gpuDeviceId || "auto",
      gpuCount: Number.isFinite(Number(parsed.gpuCount)) ? Math.max(0, Number(parsed.gpuCount)) : 1,
      memoryLimitGb: Number(parsed.memoryLimitGb) || 0,
      diskId: parsed.diskId || "auto"
    };
  } catch {
    return { cpuThreads: 0, gpuDeviceId: "auto", gpuCount: 1, memoryLimitGb: 0, diskId: "auto" };
  }
}

function savePerformancePreferences(preferences: PerformancePreferences) {
  if (typeof window !== "undefined") {
    window.localStorage.setItem(PERFORMANCE_PREF_KEY, JSON.stringify(preferences));
  }
}

function clampNumber(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

function suggestedCpuThreads(diagnostics: RuntimeDiagnostics | null) {
  const logical = diagnostics?.hardware.cpu.logicalCores || 1;
  return clampNumber(Math.min(8, Math.max(1, logical - 1)), 1, logical);
}

function effectiveCpuThreads(preferences: PerformancePreferences, diagnostics: RuntimeDiagnostics | null) {
  const logical = diagnostics?.hardware.cpu.logicalCores || Math.max(1, preferences.cpuThreads || 1);
  return clampNumber(preferences.cpuThreads || suggestedCpuThreads(diagnostics), 1, logical);
}

function effectiveGpuCount(preferences: PerformancePreferences, diagnostics: RuntimeDiagnostics | null) {
  if (preferences.gpuDeviceId === "cpu") return 0;
  const availableGpus = diagnostics?.hardware.gpus.filter((gpu) => gpu.backend).length ?? 0;
  if (availableGpus <= 0) return 0;
  return clampNumber(preferences.gpuCount || 1, 0, availableGpus);
}

function applyPerformanceToPlan(plan: SimulationPlan, preferences: PerformancePreferences, diagnostics: RuntimeDiagnostics | null): SimulationPlan {
  return {
    ...plan,
    resources: {
      ...plan.resources,
      cpuThreads: effectiveCpuThreads(preferences, diagnostics),
      gpuCount: effectiveGpuCount(preferences, diagnostics)
    }
  };
}

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

function DeleteModal({ titleText, bodyText, pathText, twoStage, stage, deleting, onCancel, onConfirm }: { titleText: string; bodyText: string; pathText?: string; twoStage: boolean; stage: 'warn' | 'confirm'; deleting: boolean; onCancel: () => void; onConfirm: () => void; }) {
  const cancelRef = useRef<HTMLButtonElement>(null);
  useEffect(() => { cancelRef.current?.focus(); }, [stage]);
  useEffect(() => { function h(e: KeyboardEvent) { if (e.key === 'Escape') { e.preventDefault(); onCancel(); } } window.addEventListener('keydown', h); return () => window.removeEventListener('keydown', h); }, [onCancel]);
  const isSecond = twoStage && stage === 'confirm';
  return (
    <div className="modal-overlay modal-overlay-danger" role="presentation" onMouseDown={onCancel}>
      <div className="modal-dialog modal-danger" role="alertdialog" aria-modal="true" aria-labelledby="del-title" aria-describedby="del-body" onMouseDown={(e) => e.stopPropagation()}>
        <div className="modal-icon" aria-hidden="true">⚠</div>
        {isSecond ? (<><h3 id="del-title">二次确认</h3><div id="del-body" className="modal-body"><p>请再次确认：确定要<strong>永久删除</strong>「<strong>{titleText}</strong>」吗？删除后<strong>无法恢复</strong>。</p></div></>) : (<><h3 id="del-title">{twoStage ? '永久删除项目？' : '删除结构？'}</h3><div id="del-body" className="modal-body"><p>{bodyText}</p>{pathText ? <p className="modal-path mono">{pathText}</p> : null}</div></>)}
        <div className="modal-actions">
          <button type="button" className="modal-cancel" ref={cancelRef} onClick={onCancel} disabled={deleting}>取消</button>
          <button type="button" className="modal-delete" onClick={onConfirm} disabled={deleting}>{isSecond ? (deleting ? '删除中…' : '确认删除') : (twoStage ? '删除' : (deleting ? '删除中…' : '确认删除'))}</button>
        </div>
      </div>
    </div>
  );
}

function DeleteProjectModal({ project, stage, deleting, onCancel, onConfirm }: { project: ProjectSummary; stage: 'warn' | 'confirm'; deleting: boolean; onCancel: () => void; onConfirm: () => void; }) {
  return <DeleteModal titleText={project.name} bodyText={`即将删除「${project.name}」。此操作不可撤销。项目目录将被整体永久删除，包括所有数据（inputs、generated、runs、trajectories、analysis、reports 等）。`} pathText={project.path} twoStage={true} stage={stage} deleting={deleting} onCancel={onCancel} onConfirm={onConfirm} />;
}

function DeleteStructureModal({ structure, deleting, onCancel, onConfirm }: { structure: StructureEntry; deleting: boolean; onCancel: () => void; onConfirm: () => void; }) {
  return (
    <DeleteModal
      titleText={structure.name}
      bodyText={`即将删除结构「${structure.name}」。此操作会移除项目 inputs/ 中对应的导入文件，并从结构索引中删除。`}
      pathText={structure.importedPath}
      twoStage={false}
      stage="warn"
      deleting={deleting}
      onCancel={onCancel}
      onConfirm={onConfirm}
    />
  );
}

function DirectPluginRunModal({
  manifest,
  action,
  running,
  onCancel,
  onConfirm
}: {
  manifest: PluginManifest;
  action: PluginAction;
  running: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const cancelRef = useRef<HTMLButtonElement>(null);
  useEffect(() => { cancelRef.current?.focus(); }, []);
  useEffect(() => {
    function h(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault();
        onCancel();
      }
    }
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [onCancel]);

  return (
    <div className="modal-overlay modal-overlay-danger" role="presentation" onMouseDown={onCancel}>
      <div className="modal-dialog modal-danger" role="alertdialog" aria-modal="true" aria-labelledby="plugin-run-title" aria-describedby="plugin-run-body" onMouseDown={(event) => event.stopPropagation()}>
        <div className="modal-icon" aria-hidden="true">⚠</div>
        <h3 id="plugin-run-title">直接运行插件？</h3>
        <div id="plugin-run-body" className="modal-body">
          <p>
            请再次确认：确定要以<strong>直接运行模式</strong>执行「<strong>{manifest.name}</strong>」的「<strong>{action.label}</strong>」吗？
            这会跳过 AutoMD 的轻量沙盒限制。
          </p>
          <p className="modal-path mono">
            entrypoint={manifest.entrypoint}; command={action.command ?? "按入口类型推断"}; args={action.args.join(" ") || "无"}
          </p>
          <p>只有确认插件来源可信、入口脚本和写入目录都安全时才建议直接运行。</p>
        </div>
        <div className="modal-actions">
          <button type="button" className="modal-cancel" ref={cancelRef} onClick={onCancel} disabled={running}>取消</button>
          <button type="button" className="modal-delete" onClick={onConfirm} disabled={running}>{running ? "运行中…" : "确认直接运行"}</button>
        </div>
      </div>
    </div>
  );
}

// Ordered to match the actual beginner workflow (top → bottom): create a
// project & import a structure, configure, run, then view results. Advanced
// infrastructure tabs (remote / engines / plugins) follow the separator.
const tabs: Array<{ id: TabId; label: string; description: string; icon: string }> = [
  { id: "overview", label: "项目", description: "创建项目并导入结构", icon: "⌂" },
  { id: "workflow", label: "流程", description: "参数、阶段和分析模块", icon: "⇄" },
  { id: "run", label: "运行", description: "本地运行与任务监控", icon: "▶" },
  { id: "report", label: "报告", description: "可复现实验输出", icon: "▤" },
  { id: "remote", label: "远程", description: "SSH / HPC 集群执行", icon: "⇡" },
  { id: "engines", label: "引擎", description: "本机/远程部署与检测", icon: "⚙" },
  { id: "plugins", label: "插件", description: "扩展 manifest 和能力", icon: "◇" }
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
    configure: "保存 cp2k/CP2K module 信息；在“引擎”页的高级部署/编译区生成 recipe，在“远程”页生成 SLURM/PBS/LSF 脚本。",
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
  remoteRecommended: "建议远程",
  notApplicable: "不适用"
};

const remoteHelperStateText: Record<RemoteHelperStatus["status"], string> = {
  missing: "未安装 helper",
  ready: "已安装",
  outdated: "版本过旧",
  unreachable: "远程不可达",
  permissionDenied: "权限不足"
};

function defaultRemoteWorkdir(username: string): string {
  const user = username.trim();
  if (!user || user === "root") return "/root/automd";
  if (/^[A-Za-z0-9._-]+$/.test(user)) return `/home/${user}/automd`;
  return "~/automd";
}

function isAutoManagedRemoteWorkdir(workdir: string, username: string): boolean {
  const value = workdir.trim();
  return value === "/root/automd" || value === "~/automd" || value === defaultRemoteWorkdir(username);
}

function isEnginePlatformBlocked(engine: EngineCapability): boolean {
  return engine.detection.status === "notApplicable" || engine.detection.status === "platformUnsupported";
}

function enginePlatformMessage(engine: EngineCapability): string {
  return engine.detection.message
    || `${engine.name} 不支持当前平台；支持平台：${engine.platformSupport.native.join(", ")}。请改用受支持平台、WSL2、容器或远程/HPC。`;
}

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

interface StructureEntry {
  id: string;
  name: string;
  sourcePath: string | null;
  importedPath: string;
  sourceKind: StructureSourceKind;
  importedAt: string;
  summary?: StructureSummary | null;
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
  const [engineTargets, setEngineTargets] = useState<EngineTarget[]>([]);
  const [selectedEngineTargetId, setSelectedEngineTargetId] = useState("local");
  const [engineInstallations, setEngineInstallations] = useState<EngineInstallationRecord[]>([]);
  const [installableEngines, setInstallableEngines] = useState<string[]>(["gromacs", "openmm", "ambertools", "lammps", "cp2k", "hoomd"]);
  const [installableTools, setInstallableTools] = useState<string[]>(["mpirun", "plumed"]);
  const [engineInstallationDraft, setEngineInstallationDraft] = useState<EngineInstallationRecord>({
    targetKind: "local",
    targetId: "local",
    targetLabel: "本机",
    engineId: "gromacs",
    location: "",
    version: null,
    authorizationStatus: "ready",
    platform: null,
    arch: null,
    checkedAt: new Date().toISOString()
  });
  const [diagnostics, setDiagnostics] = useState<RuntimeDiagnostics | null>(null);
  const [performancePreferences, setPerformancePreferences] = useState<PerformancePreferences>(() => loadPerformancePreferences());
  const [backgroundTasks, setBackgroundTasks] = useState<BackgroundTask[]>([]);
  const [showBgTasks, setShowBgTasks] = useState(false);
  const [scienceDiagnostics, setScienceDiagnostics] = useState<ScienceSidecarDiagnostics | null>(null);
  const [preparationPackage, setPreparationPackage] = useState<StructurePreparationPackage | null>(null);
  const [pluginRegistry, setPluginRegistry] = useState<PluginRegistrySnapshot | null>(null);
  const [selectedPluginId, setSelectedPluginId] = useState<string | null>(null);
  const [pluginImportPath, setPluginImportPath] = useState("");
  const [pluginImportOverwrite, setPluginImportOverwrite] = useState(false);
  const [pluginTemplateDraft, setPluginTemplateDraft] = useState<PluginTemplateRequest>({
    id: "my-analysis-plugin",
    name: "My Analysis Plugin",
    kind: "analysisModule",
    target: "workflow",
    language: "python",
    description: "读取当前 AutoMD 上下文并生成一个示例分析 artifact。"
  });
  const [pluginConfigDrafts, setPluginConfigDrafts] = useState<Record<string, string>>({});
  const [pluginRunResult, setPluginRunResult] = useState<PluginRunResult | null>(null);
  const [directPluginRunTarget, setDirectPluginRunTarget] = useState<{ manifest: PluginManifest; action: PluginAction } | null>(null);
  const [pluginBusy, setPluginBusy] = useState(false);
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
    name: "我的 HPC / 服务器",
    host: "",
    username: "root",
    port: 22,
    authMethod: "password",
    identityFile: null,
    scheduler: "slurm",
    workdir: defaultRemoteWorkdir("root"),
    moduleLoad: [],
    defaultQueue: null
  });
  // In-app SSH connect → submit → monitor → fetch (session-only password).
  const [remotePassword, setRemotePassword] = useState("");
  const [remoteConnectionTest, setRemoteConnectionTest] = useState<RemoteConnectionTest | null>(null);
  const [remoteConnecting, setRemoteConnecting] = useState(false);
  const [remotePreflight, setRemotePreflight] = useState<RemoteSubmitPreflight | null>(null);
  const [remoteAllowNoHelper, setRemoteAllowNoHelper] = useState(false);
  const [remoteSubmission, setRemoteSubmission] = useState<RemoteJobSubmission | null>(null);
  const [remoteBusy, setRemoteBusy] = useState<null | "preflight" | "submit" | "poll" | "fetch">(null);
  const [remoteAutoPoll, setRemoteAutoPoll] = useState(true);
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
  const [engineDeployResult, setEngineDeployResult] = useState<EngineDeployResult | null>(null);
  const [projectName, setProjectName] = useState("Demo protein-ligand MD");
  const [domain, setDomain] = useState<ProjectDomain>("biomolecular");
  const [selectedEngineId, setSelectedEngineId] = useState("gromacs");
  const [importSourceKind, setImportSourceKind] = useState<StructureSourceKind>("pdb");
  const [importSourcePath, setImportSourcePath] = useState("");
  const [importSmiles, setImportSmiles] = useState("");
  const [importDisplayName, setImportDisplayName] = useState("");
  const [structureImportResult, setStructureImportResult] = useState<StructureImportResult | null>(null);
  const [structures, setStructures] = useState<StructureEntry[]>([]);
  const [activeStructureId, setActiveStructureId] = useState<string | null>(null);
  const [renamingStructureId, setRenamingStructureId] = useState<string | null>(null);
  const [renamingStructureDraft, setRenamingStructureDraft] = useState('');
  const [deleteStructureTarget, setDeleteStructureTarget] = useState<StructureEntry | null>(null);
  const [deletingStructure, setDeletingStructure] = useState(false);
  const [renamingProjectId, setRenamingProjectId] = useState<string | null>(null);
  const [renamingProjectDraft, setRenamingProjectDraft] = useState('');
  const [notifications, setNotifications] = useState<AppNotification[]>([]);
  const [flashProblems, setFlashProblems] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);

  useEffect(() => {
    void bootstrap();
  }, []);

  useEffect(() => {
    let active = true;
    void api.engineCapabilitiesForTarget(selectedEngineTargetId)
      .then((capabilities) => {
        if (!active) return;
        setEngines(capabilities);
        if (capabilities.length > 0 && !capabilities.some((engine) => engine.id === selectedEngineId)) {
          setSelectedEngineId(capabilities[0].id);
        }
      })
      .catch(reportError);
    return () => {
      active = false;
    };
  }, [selectedEngineTargetId]);

  useEffect(() => {
    if (engineTargets.length > 0 && !engineTargets.some((target) => target.id === selectedEngineTargetId)) {
      setSelectedEngineTargetId(engineTargets[0].id);
    }
  }, [engineTargets, selectedEngineTargetId]);

  // Native macOS menu -> frontend actions (only inside Tauri).
  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) {
      return;
    }
    let active = true;
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/event")
      .then(({ listen }) =>
        listen<string>("menu-action", (event) => {
          switch (event.payload) {
            case "settings":
              setSettingsOpen(true);
              break;
            case "new-project":
              setActiveTab("overview");
              break;
            case "open-project-folder":
              openProjectFolder((currentProject ?? projects[0] ?? null)?.path);
              break;
            case "toggle-theme":
              setTheme((current) => (current === "dark" ? "light" : "dark"));
              break;
            case "guide":
              setActiveTab("guide");
              break;
            default:
              break;
          }
        })
      )
      .then((fn) => {
        if (active) {
          unlisten = fn;
        } else {
          fn();
        }
      })
      .catch((error) => console.warn("menu-action listener failed", error));
    return () => {
      active = false;
      if (unlisten) {
        unlisten();
      }
    };
  }, [currentProject, projects]);

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
  const selectedEngineTarget = useMemo(
    () => engineTargets.find((target) => target.id === selectedEngineTargetId) ?? engineTargets[0] ?? null,
    [engineTargets, selectedEngineTargetId]
  );
  const selectedPlugin = pluginRegistry?.manifests.find((manifest) => manifest.id === selectedPluginId) ?? null;
  const enabledUserPlugins = pluginRegistry?.manifests.filter((manifest) => manifest.origin === "user" && manifest.enabled) ?? [];

  const readyCount = engines.filter((engine) => engine.detection.status === "ready").length;
  const activeView = activeTab === "guide"
    ? guideTab
    : activeTab === "pluginDetail"
      ? { id: "pluginDetail" as const, label: selectedPlugin?.name ?? "插件详情", description: "用户插件配置和运行" }
    : tabs.find((tab) => tab.id === activeTab) ?? tabs[0];
  const activeProject = currentProject ?? projects[0] ?? null;
  const activeStructure = structures.find((s) => s.id === activeStructureId) ?? null;
  const showProjectBanner = !["engines", "plugins", "pluginDetail", "guide"].includes(activeTab);
  const showStructureRequiredWarning = showProjectBanner && Boolean(activeProject) && !activeStructure;

  useEffect(() => {
    if (!diagnostics || performancePreferences.cpuThreads > 0) return;
    const next = { ...performancePreferences, cpuThreads: suggestedCpuThreads(diagnostics) };
    savePerformancePreferences(next);
    setPerformancePreferences(next);
    setPlan((current) => current ? applyPerformanceToPlan(current, next, diagnostics) : current);
  }, [diagnostics, performancePreferences]);

  function updatePerformancePreferences(patch: Partial<PerformancePreferences>) {
    const logical = diagnostics?.hardware.cpu.logicalCores || 1;
    const next = {
      ...performancePreferences,
      ...patch
    };
    next.cpuThreads = clampNumber(Math.round(next.cpuThreads || suggestedCpuThreads(diagnostics)), 1, logical);
    const availableGpuCount = diagnostics?.hardware.gpus.filter((gpu) => gpu.backend).length ?? 0;
    next.gpuCount = next.gpuDeviceId === "cpu" || availableGpuCount <= 0
      ? 0
      : clampNumber(Math.round(next.gpuCount || 1), 0, availableGpuCount);
    next.memoryLimitGb = Math.max(0, Number(next.memoryLimitGb) || 0);
    savePerformancePreferences(next);
    setPerformancePreferences(next);
    setPlan((current) => current ? applyPerformanceToPlan(current, next, diagnostics) : current);
  }

  function startBackgroundTask(label: string, kind: BackgroundTaskKind, detail = "准备中") {
    const now = new Date().toISOString();
    const id = typeof crypto !== "undefined" && "randomUUID" in crypto
      ? crypto.randomUUID()
      : `${Date.now()}-${Math.random().toString(16).slice(2)}`;
    setBackgroundTasks((items) => [
      { id, label, kind, status: "running", progress: 5, detail, startedAt: now, updatedAt: now },
      ...items.slice(0, 7)
    ]);
    return id;
  }

  function updateBackgroundTask(id: string, patch: Partial<Pick<BackgroundTask, "status" | "progress" | "detail">>) {
    setBackgroundTasks((items) =>
      items.map((task) =>
        task.id === id
          ? { ...task, ...patch, updatedAt: new Date().toISOString() }
          : task
      )
    );
  }

  function finishBackgroundTask(id: string, detail: string, status: BackgroundTaskStatus = "completed") {
    updateBackgroundTask(id, { status, progress: status === "completed" ? 100 : 0, detail });
    window.setTimeout(() => {
      setBackgroundTasks((items) => items.filter((task) => task.id !== id));
    }, 12000);
  }

  function requireActiveStructure(action = "继续分子动力学流程"): StructureEntry | null {
    if (!activeProject) {
      notifyError("需要先创建或选择项目，再导入结构。");
      setActiveTab("overview");
      return null;
    }
    if (!activeStructure || !plan?.system.sourcePath) {
      pushNotification({
        severity: "warning",
        title: "未选中结构",
        message: `请先在“项目”页导入并选中一个结构，再${action}。没有选中结构时，AutoMD 会拒绝生成或发送分子动力学运行指令。`,
        action: { label: "去选择结构", run: () => setActiveTab("overview") },
        guide: false
      });
      setActiveTab("overview");
      return null;
    }
    return activeStructure;
  }

  function patchRuntimeTool(toolId: string, patch: Partial<ToolDiagnostic>) {
    setDiagnostics((current) => current
      ? {
          ...current,
          tools: current.tools.map((tool) => tool.id === toolId ? { ...tool, ...patch } : tool)
        }
      : current
    );
  }

  function structureExtensions(kind: StructureSourceKind) {
    switch (kind) {
      case "pdb":
        return ["pdb", "ent"];
      case "mmcif":
        return ["cif", "mmcif"];
      case "sdf":
        return ["sdf"];
      case "mol2":
        return ["mol2"];
      case "engineProject":
        return ["gro", "top", "tpr", "inp", "in", "conf", "prmtop", "rst7", "pdb", "cif"];
      case "smiles":
        return ["smi", "smiles", "txt"];
      default:
        return [];
    }
  }

  function fileNameFromPath(path: string) {
    return path.split(/[\\/]/).pop() ?? path;
  }

  function importedStructureToEntry(entry: ImportedStructureEntry): StructureEntry {
    return {
      id: entry.id || entry.importedPath,
      name: entry.name,
      sourcePath: entry.sourcePath ?? null,
      importedPath: entry.importedPath,
      sourceKind: entry.sourceKind,
      importedAt: entry.importedAt,
      summary: entry.summary ?? null
    };
  }

  function systemFromStructure(entry: StructureEntry) {
    return {
      sourceKind: entry.sourceKind,
      sourcePath: entry.importedPath,
      name: entry.name,
      moleculeCount: entry.summary?.moleculeCount ?? entry.summary?.residueCount ?? null,
      hasLigand: false,
      hasMembrane: false,
      notes: entry.summary ? [entry.summary.formatNote] : []
    };
  }

  async function bootstrap() {
    try {
      const [capabilities, targets, installations, runtime, science, plugins, profiles, storedProjects, storedTasks] = await Promise.all([
        api.engineCapabilities(),
        api.engineTargets(),
        api.listEngineInstallations(),
        api.runtimeDiagnostics(),
        api.scienceSidecarDiagnostics(),
        api.pluginManifests(),
        api.remoteProfiles(),
        api.listProjects(),
        api.listTaskRecords(null)
      ]);
      setEngines(capabilities);
      setEngineTargets(targets);
      setEngineInstallations(installations);
      void api.listInstallableEngines().then(setInstallableEngines).catch(() => undefined);
      void api.listInstallableTools().then(setInstallableTools).catch(() => undefined);
      if (capabilities[0]) {
        setEngineInstallationDraft((current) => ({ ...current, engineId: capabilities[0].id }));
      }
      setDiagnostics(runtime);
      setScienceDiagnostics(science);
      setPluginRegistry(plugins);
      setRemoteProfiles(profiles);
      setSelectedRemoteProfileId((current) => current ?? profiles[0]?.id ?? null);
      setRemoteProfileDraft((current) => {
        const hydrated = profiles.find((profile) => profile.id === current.id) ?? profiles[0];
        return hydrated ?? current;
      });
      setProjects(storedProjects);
      setTaskRecords(storedTasks);
      let restoredStructures: StructureEntry[] = [];
      if (storedProjects[0]) {
        restoredStructures = await refreshCachedMetadata(storedProjects[0].path);
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
        const restoredPlan = restoredStructures[0]
          ? { ...initialPlan, system: systemFromStructure(restoredStructures[0]) }
          : initialPlan;
        setPlan(applyPerformanceToPlan(restoredPlan, performancePreferences, runtime));
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
      setPlan(applyPerformanceToPlan(generatedPlan, performancePreferences, diagnostics));
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
      setStructures([]);
      setActiveStructureId(null);
      notifySuccess(`项目「${project.name}」已创建，默认流程已生成。`, "项目已创建");
    } catch (caught) {
      reportError(caught);
    }
  }

  async function selectProject(project: ProjectSummary) {
    try {
      setCurrentProject(project);
      if (project.preferredEngineId) {
        setSelectedEngineId(project.preferredEngineId);
      }
      const generatedPlan = await api.generatePlan({
        projectId: project.id,
        name: `${project.name} workflow`,
        engineId: project.preferredEngineId ?? selectedEngineId,
        domain: project.domain
      });
      setPlan(applyPerformanceToPlan(generatedPlan, performancePreferences, diagnostics));
      setTask(null);
      setRunPackage(null);
      setLocalSnapshot(null);
      // Clear the previous project's derived state so switching projects never
      // shows stale run/report/trajectory data (refreshCachedMetadata below
      // reloads artifacts/analysis/structures for the newly selected project).
      setArtifactIndex(null);
      setAnalysisResult(null);
      setTrajectoryIndex(null);
      setTrajectoryChunk(null);
      setTrajectoryAnalysisPackage(null);
      setExportedReport(null);
      setBatchPackage(null);
      setPreparationPackage(null);
      setManualResumePlan(null);
      setStructureImportResult(null);
      const loadedStructures = await refreshCachedMetadata(project.path);
      if (loadedStructures[0]) {
        setPlan((current) => current ? { ...current, system: systemFromStructure(loadedStructures[0]) } : current);
      }
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
      notifySuccess(`项目「${target.name}」已永久删除。`, "已删除");
      setProjects((items) => items.filter((item) => item.id !== target.id));
      if (currentProject?.id === target.id) {
        setCurrentProject(null);
        setPlan(null);
        setTask(null);
        setTaskRecords([]);
        setArtifactRecords([]);
        setAnalysisCacheRecords([]);
        setStructureImportResult(null);
        setStructures([]); setActiveStructureId(null);
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

  async function refreshPluginRegistry() {
    try {
      const snapshot = await api.pluginManifests();
      setPluginRegistry(snapshot);
      if (selectedPluginId && !snapshot.manifests.some((manifest) => manifest.id === selectedPluginId)) {
        setSelectedPluginId(null);
        if (activeTab === "pluginDetail") {
          setActiveTab("plugins");
        }
      }
      notifySuccess("插件注册表已刷新。", "插件已刷新");
    } catch (caught) {
      reportError(caught);
    }
  }

  async function browsePluginManifest() {
    try {
      const picked = await api.pickFile({ title: "选择插件 manifest", extensions: ["json"] });
      if (picked) {
        setPluginImportPath(picked);
      }
    } catch (caught) {
      reportError(caught);
    }
  }

  async function importPlugin() {
    if (!pluginImportPath.trim()) {
      notifyError("请先填写插件目录或选择 .automd-plugin.json。");
      return;
    }
    setPluginBusy(true);
    try {
      const snapshot = await api.importPlugin({ sourcePath: pluginImportPath.trim(), overwrite: pluginImportOverwrite });
      setPluginRegistry(snapshot);
      const imported = snapshot.manifests.find((manifest) => manifest.sourcePath?.includes(pluginImportPath.trim()) || manifest.installPath?.includes(pluginImportPath.trim()));
      if (imported) {
        setSelectedPluginId(imported.id);
      }
      setPluginImportPath("");
      notifySuccess("插件已导入并启用。", "导入完成");
    } catch (caught) {
      reportError(caught);
    } finally {
      setPluginBusy(false);
    }
  }

  async function createPluginTemplate() {
    setPluginBusy(true);
    try {
      const snapshot = await api.createPluginTemplate(pluginTemplateDraft);
      setPluginRegistry(snapshot);
      const created = snapshot.manifests.find((manifest) => manifest.id === pluginTemplateDraft.id || manifest.name === pluginTemplateDraft.name);
      if (created) {
        setSelectedPluginId(created.id);
        setActiveTab("pluginDetail");
      }
      notifySuccess("插件模板已创建并启用。", "插件已创建");
    } catch (caught) {
      reportError(caught);
    } finally {
      setPluginBusy(false);
    }
  }

  async function setUserPluginEnabled(pluginId: string, enabled: boolean) {
    setPluginBusy(true);
    try {
      const snapshot = await api.setPluginEnabled(pluginId, enabled);
      setPluginRegistry(snapshot);
      notifySuccess(enabled ? "插件已启用。" : "插件已停用。", enabled ? "已启用" : "已停用");
      if (!enabled && selectedPluginId === pluginId && activeTab === "pluginDetail") {
        setActiveTab("plugins");
      }
    } catch (caught) {
      reportError(caught);
    } finally {
      setPluginBusy(false);
    }
  }

  async function deleteUserPlugin(pluginId: string) {
    setPluginBusy(true);
    try {
      const snapshot = await api.deletePlugin(pluginId);
      setPluginRegistry(snapshot);
      notifySuccess("用户插件已删除。", "插件已删除");
      if (selectedPluginId === pluginId) {
        setSelectedPluginId(null);
        setActiveTab("plugins");
      }
    } catch (caught) {
      reportError(caught);
    } finally {
      setPluginBusy(false);
    }
  }

  async function savePluginConfig(manifest: PluginManifest) {
    const draft = pluginConfigDrafts[manifest.id] ?? JSON.stringify(manifest.config ?? manifest.defaultConfig ?? {}, null, 2);
    try {
      const parsed = draft.trim() ? JSON.parse(draft) : null;
      const snapshot = await api.savePluginConfig({ pluginId: manifest.id, config: parsed });
      setPluginRegistry(snapshot);
      notifySuccess("插件配置已保存。", "配置已保存");
    } catch (caught) {
      reportError(caught instanceof SyntaxError ? new Error("插件配置必须是合法 JSON。") : caught);
    }
  }

  function pluginRunContext() {
    return {
      projectId: activeProject?.id ?? null,
      projectPath: activeProject?.path ?? null,
      structureId: activeStructure?.id ?? null,
      structurePath: activeStructure?.importedPath ?? null,
      plan,
      allowedOutputDirs: [
        activeProject?.path ? `${activeProject.path}/analysis` : null,
        activeProject?.path ? `${activeProject.path}/reports` : null,
        activeProject?.path ? `${activeProject.path}/generated` : null
      ].filter(Boolean)
    };
  }

  async function runPluginAction(manifest: PluginManifest, action: PluginAction, mode: PluginRunMode, confirmedDirect = false) {
    if (mode === "direct" && !confirmedDirect) {
      setDirectPluginRunTarget({ manifest, action });
      return;
    }
    setPluginBusy(true);
    try {
      const request: PluginRunRequest = {
        pluginId: manifest.id,
        actionId: action.id,
        mode,
        confirmedDirect,
        context: pluginRunContext()
      };
      const result = await api.runPluginAction(request);
      setPluginRunResult(result);
      if (result.record.status === "failed") {
        notifyError(result.stderr || "插件运行失败。");
      } else {
        notifySuccess("插件动作已运行完成。", "插件已运行");
      }
      void api.pluginManifests().then(setPluginRegistry).catch(() => undefined);
    } catch (caught) {
      reportError(caught);
    } finally {
      setPluginBusy(false);
      setDirectPluginRunTarget(null);
    }
  }

  function openPluginInstallFolder(pluginId: string) {
    void api.openPluginInstallFolder(pluginId).catch(reportError);
  }

  async function browseStructureFile() {
    const taskId = startBackgroundTask("选择结构文件", "search", "打开系统文件选择器");
    try {
      const selectedPath = await api.pickFile({
        title: "选择要导入的结构文件",
        extensions: structureExtensions(importSourceKind)
      });
      if (!selectedPath) {
        finishBackgroundTask(taskId, "已取消文件选择", "completed");
        return;
      }
      setImportSourcePath(selectedPath);
      if (!importDisplayName.trim()) {
        setImportDisplayName(fileNameFromPath(selectedPath).replace(/\.[^.]+$/, ""));
      }
      finishBackgroundTask(taskId, `已选择 ${fileNameFromPath(selectedPath)}`);
    } catch (caught) {
      finishBackgroundTask(taskId, "文件选择失败", "failed");
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
        overwrite: false
      });
      setStructureImportResult(result);
      setPlan((current) => current ? { ...current, system: result.system } : current);
      const newEntry: StructureEntry = {
        id: result.importedPath,
        name: result.system.name,
        sourcePath: importSourceKind === "smiles" ? importSmiles || null : importSourcePath || null,
        importedPath: result.importedPath,
        sourceKind: importSourceKind,
        importedAt: result.importedAt,
        summary: result.summary
      };
      setStructures((prev) => {
        const withoutSamePath = prev.filter((structure) => structure.importedPath !== newEntry.importedPath);
        return [newEntry, ...withoutSamePath];
      });
      setActiveStructureId(newEntry.id);
      setImportDisplayName("");
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

  function selectStructure(entry: StructureEntry) {
    setActiveStructureId(entry.id);
    setPlan((current) => current ? { ...current, system: systemFromStructure(entry) } : current);
  }
  function requestDeleteStructure(entry: StructureEntry) { setDeleteStructureTarget(entry); }
  function cancelDeleteStructure() { if (deletingStructure) return; setDeleteStructureTarget(null); }
  async function confirmDeleteStructure() {
    if (!deleteStructureTarget || deletingStructure) return;
    const target = deleteStructureTarget;
    const activeProject = currentProject ?? projects[0] ?? null;
    if (!activeProject) {
      setError("需要先选择项目，才能删除结构。");
      return;
    }
    setDeletingStructure(true);
    try {
      await api.deleteImportedStructure({
        projectPath: activeProject.path,
        importedPath: target.importedPath
      });
      const nextStructures = structures.filter((structure) => structure.importedPath !== target.importedPath);
      setStructures(nextStructures);
      if (activeStructureId === target.id) {
        const nextActive = nextStructures[0] ?? null;
        setActiveStructureId(nextActive?.id ?? null);
        setPlan((current) => {
          if (!current) {
            return current;
          }
          if (nextActive) {
            return { ...current, system: systemFromStructure(nextActive) };
          }
          return {
            ...current,
            system: {
              ...current.system,
              sourcePath: null,
              name: activeProject.name
            }
          };
        });
      }
      setDeleteStructureTarget(null);
      notifySuccess(`结构「${target.name}」已删除。`, "结构已删除");
    } catch (caught) {
      reportError(caught);
    } finally {
      setDeletingStructure(false);
    }
  }
  function startRenameStructure(entry: StructureEntry) { setRenamingStructureId(entry.id); setRenamingStructureDraft(entry.name); }
  function commitRenameStructure(id: string) {
    const t = renamingStructureDraft.trim();
    if (t) { setStructures((p) => p.map((s) => s.id === id ? { ...s, name: t } : s)); if (id === activeStructureId) setPlan((c) => c ? { ...c, system: { ...c.system, name: t } } : c); }
    setRenamingStructureId(null); setRenamingStructureDraft('');
  }
  function startRenameProject(proj: ProjectSummary) { setRenamingProjectId(proj.id); setRenamingProjectDraft(proj.name); }
  function commitRenameProject(id: string) {
    const t = renamingProjectDraft.trim();
    if (t) { setProjects((p) => p.map((x) => x.id === id ? { ...x, name: t } : x)); if (currentProject?.id === id) setCurrentProject((p) => p ? { ...p, name: t } : p); }
    setRenamingProjectId(null); setRenamingProjectDraft('');
  }

  async function autoFindTool(tool: ToolDiagnostic) {
    const taskId = startBackgroundTask(`自动查找 ${tool.label}`, "search", `正在查找 ${tool.command}`);
    try {
      updateBackgroundTask(taskId, { progress: 35, detail: "扫描 PATH 和常见安装目录" });
      const result = await api.findExecutable({ commands: [tool.command], extraDirs: [] });
      if (result.found && result.path) {
        patchRuntimeTool(tool.id, {
          status: "ready",
          detail: result.path
        });
        finishBackgroundTask(taskId, result.message);
      } else {
        finishBackgroundTask(taskId, result.message, "failed");
      }
    } catch (caught) {
      finishBackgroundTask(taskId, "自动查找失败", "failed");
      reportError(caught);
    }
  }

  async function manualFindTool(tool: ToolDiagnostic) {
    const taskId = startBackgroundTask(`手动选择 ${tool.label}`, "search", "等待用户选择可执行文件");
    try {
      const selectedPath = await api.pickFile({
        title: `选择 ${tool.label} 可执行文件`,
        extensions: []
      });
      if (!selectedPath) {
        finishBackgroundTask(taskId, "已取消手动选择");
        return;
      }
      patchRuntimeTool(tool.id, {
        status: "ready",
        detail: selectedPath
      });
      finishBackgroundTask(taskId, `已选择 ${fileNameFromPath(selectedPath)}`);
    } catch (caught) {
      finishBackgroundTask(taskId, "手动选择失败", "failed");
      reportError(caught);
    }
  }

  async function autoInstallTool(tool: ToolDiagnostic) {
    // Conda-installable tools (MPI, PLUMED) install for real, no compilation.
    if (installableTools.includes(tool.id)) {
      const taskId = startBackgroundTask(`安装 ${tool.label}`, "install", "通过 conda-forge 下载并安装（可能需要几分钟）");
      notifyInstalling(tool.label);
      try {
        updateBackgroundTask(taskId, { progress: 40, detail: "创建隔离环境并解析依赖…" });
        const path = await api.installTool(tool.id);
        patchRuntimeTool(tool.id, { status: "ready", detail: path });
        finishBackgroundTask(taskId, `${tool.label} 已安装：${fileNameFromPath(path)}`);
        notifySuccess(`${tool.label} 已通过 conda-forge 安装完成，可直接使用。`, "已安装");
      } catch (caught) {
        finishBackgroundTask(taskId, `${tool.label} 安装失败`, "failed");
        reportError(caught);
      }
      return;
    }
    // GPU drivers / Docker / cluster schedulers can't be installed by conda — guide.
    setActiveTab("guide");
    pushNotification({
      severity: "info",
      title: "请查看安装方式",
      message: `${tool.label} 不是 AutoMD 可以静默安装的 Conda 工具，通常涉及系统服务、虚拟机、GPU 驱动或 HPC 登录节点。已打开使用指引中的安装说明。`
    });
  }

  function patchScienceTool(toolId: string, patch: Partial<ScienceToolDiagnostic>) {
    setScienceDiagnostics((current) => current
      ? {
          ...current,
          tools: current.tools.map((tool) => tool.id === toolId ? { ...tool, ...patch } : tool)
        }
      : current
    );
  }

  async function autoFindScienceTool(tool: ScienceToolDiagnostic) {
    const taskId = startBackgroundTask(`自动查找 ${tool.label}`, "search", "刷新 AutoMD 科学环境和系统 Python 检测");
    try {
      updateBackgroundTask(taskId, { progress: 45, detail: "检查内置 automd-science、系统 python3 和 AmberTools 命令" });
      const diagnostics = await api.scienceSidecarDiagnostics();
      setScienceDiagnostics(diagnostics);
      const refreshed = diagnostics.tools.find((item) => item.id === tool.id);
      finishBackgroundTask(
        taskId,
        refreshed?.status === "ready" ? `${tool.label} 已可用` : `${tool.label} 仍未找到`,
        refreshed?.status === "ready" ? "completed" : "failed"
      );
    } catch (caught) {
      finishBackgroundTask(taskId, "自动查找失败", "failed");
      reportError(caught);
    }
  }

  async function manualFindScienceTool(tool: ScienceToolDiagnostic) {
    const isPythonModule = Boolean(tool.importName);
    const title = isPythonModule
      ? `选择可导入 ${tool.label} 的 Python 可执行文件`
      : `选择 ${tool.label} 可执行文件`;
    const taskId = startBackgroundTask(`手动选择 ${tool.label}`, "search", title);
    try {
      const selectedPath = await api.pickFile({ title, extensions: [] });
      if (!selectedPath) {
        finishBackgroundTask(taskId, "已取消手动选择");
        return;
      }
      updateBackgroundTask(taskId, { progress: 65, detail: isPythonModule ? "正在测试 Python import" : "正在检查可执行文件" });
      const inspected = await api.inspectScienceTool({
        id: tool.id,
        label: tool.label,
        importName: tool.importName ?? null,
        command: tool.command ?? null,
        executablePath: selectedPath
      });
      patchScienceTool(tool.id, inspected);
      finishBackgroundTask(
        taskId,
        inspected.status === "ready" ? `${tool.label} 已可用` : `${tool.label} 仍不可用`,
        inspected.status === "ready" ? "completed" : "failed"
      );
    } catch (caught) {
      finishBackgroundTask(taskId, "手动选择失败", "failed");
      reportError(caught);
    }
  }

  async function autoInstallScienceSidecar() {
    const taskId = startBackgroundTask("安装 Python 科学侧车", "install", "准备 conda-forge automd-science 环境");
    notifyInstalling("Python 科学侧车");
    try {
      updateBackgroundTask(taskId, { progress: 25, detail: "检查 Conda/Mamba；缺失时自动安装 Miniforge" });
      const diagnostics = await api.installScienceSidecar();
      setScienceDiagnostics(diagnostics);
      const readyCount = diagnostics.tools.filter((tool) => tool.status === "ready").length;
      finishBackgroundTask(taskId, `科学侧车安装完成：${readyCount}/${diagnostics.tools.length} 项可用`);
      notifySuccess("AutoMD 科学侧车已通过 conda-forge 安装完成。", "已安装");
    } catch (caught) {
      finishBackgroundTask(taskId, "科学侧车安装失败", "failed");
      reportError(caught);
    }
  }

  function warnUnsupportedEnginePlatform(engine: EngineCapability) {
    setSelectedEngineId(engine.id);
    pushNotification({
      severity: "warning",
      title: "平台不支持",
      message: enginePlatformMessage(engine),
      action: { label: "去远程页", run: () => setActiveTab("remote") },
      guide: true
    });
  }

  function warnRemoteHelperRequired(target: EngineTarget) {
    pushNotification({
      severity: "warning",
      title: "远程 helper 未就绪",
      message: `请先在远程页为 ${target.label} 安装或检测 AutoMD helper，然后再扫描、部署或编译远程引擎。`,
      action: { label: "去远程页", run: () => setActiveTab("remote") },
      guide: true
    });
  }

  async function autoFindEngine(engine: EngineCapability) {
    if (isEnginePlatformBlocked(engine)) {
      warnUnsupportedEnginePlatform(engine);
      return;
    }
    const target = selectedEngineTarget;
    if (target?.kind === "remote" && target.status !== "ready" && target.status !== "outdated") {
      warnRemoteHelperRequired(target);
      return;
    }
    const taskId = startBackgroundTask(
      `自动扫描 ${engine.name}`,
      "search",
      target?.kind === "remote" ? "通过远程 helper 扫描目标设备" : "扫描 PATH 和常见引擎目录"
    );
    try {
      setSelectedEngineId(engine.id);
      if (target?.kind === "remote") {
        updateBackgroundTask(taskId, { progress: 45, detail: target.detail });
        const capabilities = await api.scanEnginesOnTarget(target.id);
        setEngines(capabilities);
        const [targets, installations] = await Promise.all([
          api.engineTargets(),
          api.listEngineInstallations()
        ]);
        setEngineTargets(targets);
        setEngineInstallations(installations);
        finishBackgroundTask(taskId, `已扫描 ${target.label}`);
        return;
      }
      updateBackgroundTask(taskId, { progress: 40, detail: engine.executableNames.join(", ") });
      const result = await api.findExecutable({ commands: engine.executableNames, extraDirs: [] });
      if (!result.found || !result.path) {
        finishBackgroundTask(taskId, result.message, "failed");
        return;
      }
      await saveEngineInstallation({
        targetKind: "local",
        targetId: "local",
        targetLabel: "本机",
        engineId: engine.id,
        location: result.path,
        version: null,
        authorizationStatus: engine.license.requiresUserLicense ? "missingLicense" : "ready",
        platform: diagnostics?.os === "macos" ? "macos" : diagnostics?.os === "windows" ? "windows" : diagnostics?.os === "linux" ? "linux" : null,
        arch: diagnostics?.arch ?? null,
        checkedAt: new Date().toISOString()
      });
      finishBackgroundTask(taskId, `已找到 ${fileNameFromPath(result.path)}`);
    } catch (caught) {
      finishBackgroundTask(taskId, "引擎自动查找失败", "failed");
      reportError(caught);
    }
  }

  async function manualFindEngine(engine: EngineCapability) {
    if (isEnginePlatformBlocked(engine)) {
      warnUnsupportedEnginePlatform(engine);
      return;
    }
    const target = selectedEngineTarget;
    const taskId = startBackgroundTask(
      `手动登记 ${engine.name}`,
      "search",
      target?.kind === "remote" ? "登记远程目标上的可执行文件路径" : "等待用户选择引擎可执行文件"
    );
    try {
      setSelectedEngineId(engine.id);
      const selectedPath = target?.kind === "remote"
        ? window.prompt(`请输入 ${target.label} 上的 ${engine.name} 可执行文件路径，例如 /opt/${engine.id}/bin/${engine.executableNames[0] ?? engine.id}`)
        : await api.pickFile({
            title: `选择 ${engine.name} 可执行文件`,
            extensions: []
          });
      if (!selectedPath) {
        finishBackgroundTask(taskId, "已取消手动选择");
        return;
      }
      await saveEngineInstallation({
        targetKind: target?.kind ?? "local",
        targetId: target?.id ?? "local",
        targetLabel: target?.label ?? "本机",
        engineId: engine.id,
        location: selectedPath,
        version: null,
        authorizationStatus: engine.license.requiresUserLicense ? "missingLicense" : "ready",
        platform: target?.platform ?? null,
        arch: target?.arch ?? null,
        checkedAt: new Date().toISOString()
      });
      finishBackgroundTask(taskId, `已选择 ${fileNameFromPath(selectedPath)}`);
    } catch (caught) {
      finishBackgroundTask(taskId, "手动选择引擎失败", "failed");
      reportError(caught);
    }
  }

  async function autoInstallEngine(engine: EngineCapability) {
    if (isEnginePlatformBlocked(engine)) {
      warnUnsupportedEnginePlatform(engine);
      return;
    }
    const target = selectedEngineTarget;
    if (target?.kind === "remote" && target.status !== "ready" && target.status !== "outdated") {
      warnRemoteHelperRequired(target);
      return;
    }
    const activeProject = currentProject ?? projects[0] ?? null;
    const taskId = startBackgroundTask(`一键部署 ${engine.name}`, "install", `${target?.label ?? "本机"}：包管理安装或源码构建`);
    notifyInstalling(engine.name);
    try {
      setSelectedEngineId(engine.id);
      updateBackgroundTask(taskId, { progress: 25, detail: "解析部署策略…" });
      const result = await api.installOrBuildEngine({
        targetId: target?.id ?? "local",
        engineId: engine.id,
        strategy: "auto",
        mode: installableEngines.includes(engine.id) ? "execute" : buildWorkflowMode,
        buildOptions: defaultBuildRecipeOptions(engine.id),
        projectPath: activeProject?.path ?? null,
        timeoutSeconds: buildWorkflowTimeout
      });
      setEngineDeployResult(result);
      if (result.buildResult) {
        setBuildWorkflowResult(result.buildResult);
      }
      updateBackgroundTask(taskId, { progress: 85, detail: "刷新目标设备引擎状态…" });
      const [capabilities, targets, installations] = await Promise.all([
        api.engineCapabilitiesForTarget(target?.id ?? "local"),
        api.engineTargets(),
        api.listEngineInstallations()
      ]);
      setEngines(capabilities);
      setEngineTargets(targets);
      setEngineInstallations(installations);
      finishBackgroundTask(taskId, result.status === "failed" ? `${engine.name} 部署失败` : `${engine.name} 部署完成`, result.status === "failed" ? "failed" : "completed");
      if (result.record) {
        notifySuccess(`${engine.name} 已在 ${result.record.targetLabel} 登记为可用。`, "引擎已部署");
      }
    } catch (caught) {
      finishBackgroundTask(taskId, `${engine.name} 部署失败`, "failed");
      reportError(caught);
    }
  }

  async function prepareEngineBuild(engine: EngineCapability) {
    const taskId = startBackgroundTask(`生成 ${engine.name} 编译脚本`, "build", "准备源码拉取和一站式编译脚本");
    const activeProject = currentProject ?? projects[0] ?? null;
    try {
      setSelectedEngineId(engine.id);
      setBuildWorkflowMode("writeFiles");
      updateBackgroundTask(taskId, { progress: 30, detail: "生成容器 recipe 和源码编译 recipe" });
      const options = defaultBuildRecipeOptions(engine.id);
      const [container, build] = await Promise.all([
        api.containerRecipe(engine.id),
        api.buildRecipe(options)
      ]);
      setContainerRecipe(container);
      setBuildRecipe(build);
      setRecipeExportResult(null);

      if (activeProject) {
        updateBackgroundTask(taskId, { progress: 65, detail: "写入 build-recipes/ 一站式脚本" });
        const result = await api.runBuildWorkflow({
          projectPath: activeProject.path,
          buildOptions: options,
          includeContainer: true,
          includeBuildScript: true,
          mode: "writeFiles",
          timeoutSeconds: buildWorkflowTimeout
        });
        setBuildWorkflowResult(result);
        await refreshArtifacts();
        finishBackgroundTask(taskId, "已生成源码拉取、容器 recipe 和编译脚本；可在引擎卡片高级区执行。");
      } else {
        setBuildWorkflowResult(null);
        finishBackgroundTask(taskId, "已生成编译 recipe；创建项目后可写入脚本或执行构建。");
      }
      setActiveTab("engines");
    } catch (caught) {
      finishBackgroundTask(taskId, "自动安装/编译入口失败", "failed");
      reportError(caught);
    }
  }

  async function queueMockTask() {
    if (!plan) {
      return;
    }
    if (!requireActiveStructure("生成运行计划")) {
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
    if (!requireActiveStructure("生成批量重复实验包")) {
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
    if (!requireActiveStructure("生成结构准备包")) {
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
      if (failure && failure.severity !== "info") {
        notifyFailure(failure);
      }
    } catch (caught) {
      reportError(caught);
    }
  }

  async function startLocalRun() {
    if (!plan) {
      return;
    }
    if (!requireActiveStructure("启动本地任务")) {
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
      if (snapshot.failureAnalysis && snapshot.failureAnalysis.severity !== "info") {
        notifyFailure(snapshot.failureAnalysis);
      }
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

  async function refreshCachedMetadata(projectPath = (currentProject ?? projects[0] ?? null)?.path): Promise<StructureEntry[]> {
    if (!projectPath) {
      setArtifactRecords([]);
      setAnalysisCacheRecords([]);
      setStructures([]);
      setActiveStructureId(null);
      return [];
    }
    try {
      const [artifacts, analysisCache, importedStructures] = await Promise.all([
        api.listArtifactRecords(projectPath),
        api.listAnalysisCacheRecords(projectPath),
        api.listImportedStructures(projectPath)
      ]);
      const mappedStructures = importedStructures.map(importedStructureToEntry);
      setArtifactRecords(artifacts);
      setAnalysisCacheRecords(analysisCache);
      setStructures(mappedStructures);
      setActiveStructureId(mappedStructures[0]?.id ?? null);
      return mappedStructures;
    } catch (caught) {
      reportError(caught);
      return [];
    }
  }

  async function discoverResumePlan() {
    const activeProject = currentProject ?? projects[0] ?? null;
    const runDirectory = localSnapshot?.runDirectory ?? runPackage?.runDirectory ?? null;
    if (!activeProject || !plan || !runDirectory) {
      setError("需要先创建项目并生成 run package，才能扫描 checkpoint。");
      return;
    }
    if (!requireActiveStructure("扫描 checkpoint")) {
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
    if (!requireActiveStructure("生成 MDAnalysis 分析包")) {
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
    if (!requireActiveStructure("导出模拟报告")) {
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
      setEngineDeployResult(null);
      setActiveTab("engines");
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
      setEngineDeployResult(null);
      setActiveTab("engines");
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
    const engine = engines.find((item) => item.id === engineId);
    if (engine && isEnginePlatformBlocked(engine)) {
      warnUnsupportedEnginePlatform(engine);
      return;
    }
    const target = selectedEngineTarget;
    if (target?.kind === "remote" && target.status !== "ready" && target.status !== "outdated") {
      warnRemoteHelperRequired(target);
      return;
    }
    const taskId = startBackgroundTask(`构建向导 ${engineLabel[engineId] ?? engineId}`, "build", "准备构建 recipe");
    try {
      const options = defaultBuildRecipeOptions(engineId);
      updateBackgroundTask(taskId, { progress: 35, detail: "生成容器 recipe 和源码脚本" });
      const [container, build, result] = await Promise.all([
        api.containerRecipe(engineId),
        api.buildRecipe(options),
        api.installOrBuildEngine({
          targetId: selectedEngineTarget?.id ?? "local",
          engineId,
          strategy: buildWorkflowMode === "dryRun" || buildWorkflowMode === "writeFiles" ? "recipeOnly" : "sourceBuild",
          mode: buildWorkflowMode,
          buildOptions: options,
          projectPath: activeProject.path,
          timeoutSeconds: buildWorkflowTimeout
        })
      ]);
      setContainerRecipe(container);
      setBuildRecipe(build);
      setEngineDeployResult(result);
      setBuildWorkflowResult(result.buildResult ?? null);
      updateBackgroundTask(taskId, { progress: 85, detail: buildWorkflowModeText[result.mode] });
      if ((result.buildResult?.filesWritten.length ?? 0) || result.buildResult?.logPath) {
        await refreshArtifacts();
      }
      const [capabilities, targets, installations] = await Promise.all([
        api.engineCapabilitiesForTarget(selectedEngineTarget?.id ?? "local"),
        api.engineTargets(),
        api.listEngineInstallations()
      ]);
      setEngines(capabilities);
      setEngineTargets(targets);
      setEngineInstallations(installations);
      finishBackgroundTask(taskId, result.status === "failed" ? "构建向导失败，查看高级部署日志" : "构建向导完成");
      setActiveTab("engines");
    } catch (caught) {
      finishBackgroundTask(taskId, "构建向导失败", "failed");
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
    if (!requireActiveStructure("生成远程执行包")) {
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
        ...items.filter((item) => !(item.targetId === saved.targetId && item.engineId === saved.engineId && item.location === saved.location))
      ]);
      const capabilities = await api.engineCapabilitiesForTarget(saved.targetId);
      setEngines(capabilities);
      setEngineInstallationDraft(saved);
    } catch (caught) {
      reportError(caught);
    }
  }

  async function deleteEngineInstallation(record: EngineInstallationRecord) {
    try {
      const deleted = await api.deleteEngineInstallationForTarget(record.targetId, record.engineId, record.location);
      if (!deleted) {
        setError("未找到要删除的引擎安装记录。");
        return;
      }
      setEngineInstallations((items) =>
        items.filter((item) => !(item.targetId === record.targetId && item.engineId === record.engineId && item.location === record.location))
      );
      const capabilities = await api.engineCapabilitiesForTarget(record.targetId);
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
      if (plan && activeStructure && plan.system.sourcePath) {
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
      } else {
        setRemotePackage(null);
        setRemoteJobSnapshot(null);
        setRemoteWorkflowResult(null);
      }
      const targets = await api.engineTargets();
      setEngineTargets(targets);
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
      setRemoteProfileDraft((current) => profiles[0] ?? {
        ...current,
        id: "custom-hpc",
        name: "我的 HPC / 服务器",
        host: "",
        username: "root",
        port: 22,
        authMethod: "password",
        identityFile: null,
        scheduler: "slurm",
        workdir: defaultRemoteWorkdir("root"),
        moduleLoad: [],
        defaultQueue: null
      });
      setRemotePackage(null);
      setRemoteJobSnapshot(null);
      setRemoteWorkflowResult(null);
      const targets = await api.engineTargets();
      setEngineTargets(targets);
    } catch (caught) {
      reportError(caught);
    }
  }

  async function installRemoteHelperForProfile(profileId: string) {
    const profile = remoteProfiles.find((item) => item.id === profileId);
    const taskId = startBackgroundTask(`安装远程 helper`, "install", profile ? `${profile.name} · ${profile.host}` : profileId);
    try {
      updateBackgroundTask(taskId, { progress: 35, detail: "通过 SSH 写入 helper 脚本…" });
      const status = await api.installRemoteHelper(profileId);
      updateBackgroundTask(taskId, { progress: 80, detail: "刷新远程设备状态…" });
      const [targets, capabilities] = await Promise.all([
        api.engineTargets(),
        api.engineCapabilitiesForTarget(`remote:${profileId}`)
      ]);
      setEngineTargets(targets);
      if (selectedEngineTargetId === `remote:${profileId}`) {
        setEngines(capabilities);
      }
      finishBackgroundTask(taskId, status.status === "ready" ? "远程 helper 已安装" : "远程 helper 安装未完成", status.status === "ready" ? "completed" : "failed");
      if (status.status === "ready") {
        notifySuccess(`${profile?.name ?? profileId} helper 已就绪。`, "远程 helper 已安装");
      } else if (status.lastError) {
        setError(status.lastError);
      }
    } catch (caught) {
      finishBackgroundTask(taskId, "远程 helper 安装失败", "failed");
      reportError(caught);
    }
  }

  async function checkRemoteHelperForProfile(profileId: string) {
    const profile = remoteProfiles.find((item) => item.id === profileId);
    const taskId = startBackgroundTask(`检测远程 helper`, "search", profile ? `${profile.name} · ${profile.host}` : profileId);
    try {
      const status = await api.checkRemoteHelper(profileId);
      const [targets, capabilities] = await Promise.all([
        api.engineTargets(),
        api.engineCapabilitiesForTarget(`remote:${profileId}`)
      ]);
      setEngineTargets(targets);
      if (selectedEngineTargetId === `remote:${profileId}`) {
        setEngines(capabilities);
      }
      finishBackgroundTask(taskId, status.status === "ready" ? "远程 helper 已就绪" : "远程 helper 不可用", status.status === "ready" ? "completed" : "failed");
      if (status.lastError) {
        setError(status.lastError);
      }
    } catch (caught) {
      finishBackgroundTask(taskId, "远程 helper 检测失败", "failed");
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
    if (!requireActiveStructure("运行远程步骤")) {
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

  // --- In-app SSH: connect → (helper) → preflight → submit → monitor → fetch ---
  const remotePasswordArg = () =>
    remoteProfileDraft.authMethod === "password" ? remotePassword : null;

  async function testRemoteConnection() {
    if (!remoteProfileDraft.host.trim()) {
      setError("请先填写主机/IP，再测试连接。");
      return;
    }
    if (remoteProfileDraft.authMethod === "password" && !remotePassword) {
      setError("密码认证：请先输入密码再测试连接。");
      return;
    }
    setRemoteConnecting(true);
    setRemoteConnectionTest(null);
    try {
      const result = await api.testRemoteConnection(remoteProfileDraft, remotePasswordArg());
      setRemoteConnectionTest(result);
      if (result.ok) {
        const nextScheduler = result.scheduler ?? "ssh";
        if (nextScheduler !== remoteProfileDraft.scheduler) {
          setRemoteProfileDraft({ ...remoteProfileDraft, scheduler: nextScheduler });
        }
        removeNotificationsMatching((item) =>
          item.severity === "error" &&
          (item.message.includes("密码认证：请先输入密码再测试连接") || item.message.includes("请先填写主机/IP，再测试连接"))
        );
        notifySuccess(result.message, "已连接");
      } else {
        pushNotification({ severity: "error", title: "连接失败", message: result.message });
      }
    } catch (caught) {
      reportError(caught);
    } finally {
      setRemoteConnecting(false);
    }
  }

  async function runRemotePreflight() {
    if (!plan) {
      setError("需要先在「流程」生成 SimulationPlan，才能预检。");
      return;
    }
    const activeProject = currentProject ?? projects[0] ?? null;
    setRemoteBusy("preflight");
    setRemotePreflight(null);
    try {
      const result = await api.preflightRemoteSubmit({
        profile: remoteProfileDraft,
        plan,
        projectId: activeProject?.id ?? null,
        projectPath: activeProject?.path ?? null,
        structureId: activeStructureId,
        password: remotePasswordArg()
      });
      setRemotePreflight(result);
    } catch (caught) {
      reportError(caught);
    } finally {
      setRemoteBusy(null);
    }
  }

  async function submitRemoteJob() {
    if (!plan) {
      setError("需要先在「流程」生成 SimulationPlan。");
      return;
    }
    if (!requireActiveStructure("提交远程作业")) {
      return;
    }
    const activeProject = currentProject ?? projects[0] ?? null;
    if (!activeProject) {
      setError("请先选择当前项目。");
      return;
    }
    setRemoteBusy("submit");
    const taskId = startBackgroundTask("提交远程作业", "install", `${remoteProfileDraft.name} · ${remoteProfileDraft.host}`);
    notifyInstalling(`远程作业（${remoteProfileDraft.host}）`);
    try {
      updateBackgroundTask(taskId, { progress: 45, detail: "上传项目并提交到调度器…" });
      const submission = await api.submitRemoteJob({
        profile: remoteProfileDraft,
        plan,
        projectId: activeProject.id,
        projectPath: activeProject.path,
        structureId: activeStructureId,
        password: remotePasswordArg(),
        allowNoHelper: remoteAllowNoHelper
      });
      setRemoteSubmission(submission);
      setRemoteWorkflowJobId(submission.jobId ?? "");
      setRemoteJobSnapshot(null);
      finishBackgroundTask(taskId, submission.jobId ? `已提交：job ${submission.jobId}` : "已提交", "completed");
      notifySuccess(
        submission.jobId ? `作业已提交（job ${submission.jobId}），开始自动监控。` : "作业已提交。",
        "已提交"
      );
    } catch (caught) {
      finishBackgroundTask(taskId, "远程提交失败", "failed");
      reportError(caught);
    } finally {
      setRemoteBusy(null);
    }
  }

  async function pollRemoteJobNow() {
    if (!remoteSubmission || !plan) {
      return;
    }
    setRemoteBusy((busy) => (busy === null ? "poll" : busy));
    try {
      const snapshot = await api.pollRemoteJob({
        profile: remoteProfileDraft,
        jobId: remoteSubmission.jobId ?? remoteWorkflowJobId ?? null,
        scheduler: remoteSubmission.scheduler,
        engineId: plan.engineId,
        remoteRunDir: remoteSubmission.remoteRunDir,
        password: remotePasswordArg()
      });
      setRemoteJobSnapshot(snapshot);
    } catch (caught) {
      reportError(caught);
    } finally {
      setRemoteBusy((busy) => (busy === "poll" ? null : busy));
    }
  }

  async function cancelRemoteJob() {
    if (!remoteSubmission || !plan) {
      return;
    }
    try {
      const message = await api.cancelRemoteJob({
        profile: remoteProfileDraft,
        jobId: remoteSubmission.jobId ?? remoteWorkflowJobId ?? null,
        scheduler: remoteSubmission.scheduler,
        engineId: plan.engineId,
        remoteRunDir: remoteSubmission.remoteRunDir,
        password: remotePasswordArg()
      });
      notifySuccess(message, "已取消");
      void pollRemoteJobNow();
    } catch (caught) {
      reportError(caught);
    }
  }

  async function fetchRemoteResults() {
    if (!remoteSubmission) {
      return;
    }
    const activeProject = currentProject ?? projects[0] ?? null;
    if (!activeProject) {
      setError("请先选择当前项目。");
      return;
    }
    setRemoteBusy("fetch");
    const taskId = startBackgroundTask("回收远程结果", "search", remoteSubmission.remoteRunDir);
    try {
      const result = await api.fetchRemoteResults({
        profile: remoteProfileDraft,
        remoteRunDir: remoteSubmission.remoteRunDir,
        localProjectPath: activeProject.path,
        password: remotePasswordArg()
      });
      finishBackgroundTask(taskId, `已回收 ${result.filesDownloaded} 个文件`, "completed");
      notifySuccess(result.message, "已回收结果");
      await refreshArtifacts();
    } catch (caught) {
      finishBackgroundTask(taskId, "回收结果失败", "failed");
      reportError(caught);
    } finally {
      setRemoteBusy(null);
    }
  }

  // Auto-poll an active remote job every 8s until it reaches a terminal state.
  useEffect(() => {
    if (!remoteSubmission || !remoteAutoPoll) {
      return;
    }
    const terminal = remoteJobSnapshot
      ? ["completed", "failed", "cancelled"].includes(remoteJobSnapshot.status)
      : false;
    if (terminal) {
      return;
    }
    const timer = window.setInterval(() => {
      void pollRemoteJobNow();
    }, 8000);
    return () => window.clearInterval(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [remoteSubmission, remoteAutoPoll, remoteJobSnapshot?.status]);

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

  function pushNotification(input: Omit<AppNotification, "id" | "createdAt" | "persistent" | "visible">) {
    const id = typeof crypto !== "undefined" && "randomUUID" in crypto
      ? crypto.randomUUID()
      : `${Date.now()}-${Math.random().toString(16).slice(2)}`;
    // Errors/warnings are "问题": they persist (minimize on close, stay counted).
    const persistent = input.severity === "error" || input.severity === "warning";
    setNotifications((items) => [{ ...input, id, persistent, visible: true, createdAt: Date.now() }, ...items].slice(0, 8));
    if (!persistent) {
      window.setTimeout(() => removeNotification(id), 6000);
    }
    return id;
  }

  function removeNotification(id: string) {
    setNotifications((items) => items.filter((item) => item.id !== id));
  }

  function removeNotificationsMatching(predicate: (item: AppNotification) => boolean) {
    setNotifications((items) => items.filter((item) => !predicate(item)));
  }

  // Close (×): minimize a persistent problem (stays counted in the status bar); drop ephemeral notices.
  function dismissNotification(id: string) {
    setNotifications((items) =>
      items.flatMap((item) => {
        if (item.id !== id) return [item];
        return item.persistent ? [{ ...item, visible: false }] : [];
      })
    );
  }

  // "忽略问题": resolve and remove entirely (no longer counted).
  function ignoreNotification(id: string) {
    removeNotification(id);
  }

  // Status-bar click: re-pop minimized problems, or briefly flash the stack if all are already shown.
  function reviewProblems() {
    setNotifications((items) => {
      const hasHidden = items.some((item) => item.persistent && !item.visible);
      if (hasHidden) {
        return items.map((item) => (item.persistent ? { ...item, visible: true } : item));
      }
      return items;
    });
    if (!notifications.some((item) => item.persistent && !item.visible)) {
      setFlashProblems(true);
      window.setTimeout(() => setFlashProblems(false), 750);
    }
  }

  /** Map a raw error message to a one-click fix (jump the user to where they can resolve it). */
  function inferQuickFix(message: string): AppNotification["action"] | undefined {
    if (/创建项目|尚未[^，。]*项目|先[^，。]*项目|选择项目/.test(message)) {
      return { label: "去项目页", run: () => setActiveTab("overview") };
    }
    if (/引擎|可执行文件|executable|未检测到|不可用|未安装|安装/.test(message)) {
      return { label: "去引擎页", run: () => setActiveTab("engines") };
    }
    if (/编译|构建|recipe|build/i.test(message)) {
      return { label: "去引擎页", run: () => setActiveTab("engines") };
    }
    if (/远程|ssh|slurm|profile/i.test(message)) {
      return { label: "去远程页", run: () => setActiveTab("remote") };
    }
    return undefined;
  }

  function notifyError(message: string) {
    pushNotification({ severity: "error", title: "错误", message, action: inferQuickFix(message), guide: true });
  }

  function notifySuccess(message: string, title = "完成") {
    pushNotification({ severity: "success", title, message });
  }

  /** Bottom-right reminder shown the moment a background install starts. */
  function notifyInstalling(label: string) {
    pushNotification({
      severity: "info",
      title: "正在安装",
      message: `${label} 正在后台安装（可能需要几分钟）。安装期间软件可正常使用，进度见左下角「后台任务」。`
    });
  }

  /** Surface a diagnosed run/build failure as a toast with a category-aware one-click fix. */
  function notifyFailure(failure: FailureAnalysis) {
    const fixes: Partial<Record<FailureAnalysis["category"], { label: string; run: () => void }>> = {
      missingExecutable: { label: "去引擎页安装", run: () => setActiveTab("engines") },
      licenseRequired: { label: "去引擎页", run: () => setActiveTab("engines") },
      missingInput: { label: "去项目页导入", run: () => setActiveTab("overview") },
      missingTopology: { label: "去流程页", run: () => setActiveTab("workflow") },
      missingForceField: { label: "去流程页", run: () => setActiveTab("workflow") },
      parameterMismatch: { label: "去流程页", run: () => setActiveTab("workflow") },
      mpiFailure: { label: "去远程页", run: () => setActiveTab("remote") },
      schedulerFailure: { label: "去远程页", run: () => setActiveTab("remote") }
    };
    const suggestion = failure.suggestions[0];
    const detail = suggestion ? `${suggestion.title}：${suggestion.detail}` : failure.message;
    const isError = failure.severity === "error";
    pushNotification({
      severity: isError ? "error" : "warning",
      title: isError ? "错误" : "警告",
      message: `${failureCategoryText[failure.category]}：${detail}`,
      action: fixes[failure.category],
      guide: true
    });
  }

  // Compatibility shim: existing `setError("…")` calls now surface as error toasts;
  // `setError(null)` (old banner dismiss) becomes a no-op since toasts self-manage.
  function setError(message: string | null) {
    if (message) {
      notifyError(message);
    }
  }

  function reportError(caught: unknown) {
    notifyError(caught instanceof Error ? caught.message : String(caught));
  }

  return (
    <>
      <WindowSizeNotice />
      <main className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <img src={appIconUrl} className="brand-mark-img" alt="AutoMD logo" />
          <div>
            <h1>AutoMD</h1>
            <p>MD workflow studio</p>
          </div>
        </div>
        <nav className="nav-list" aria-label="AutoMD sections">
          {tabs.map((tab) => (
            <button
              className={`nav-item ${activeTab === tab.id ? "active" : ""} ${tab.id === "remote" ? "nav-separated" : ""}`}
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              type="button"
            >
              <span className="nav-icon" aria-hidden="true">{tab.icon}</span>
              <span className="nav-copy">
                <span>{tab.label}</span>
                <small>{tab.description}</small>
              </span>
            </button>
          ))}
          {enabledUserPlugins.length ? (
            <div className="nav-plugin-group" aria-label="用户插件">
              <span>用户插件</span>
              {enabledUserPlugins.map((plugin) => (
                <button
                  className={`nav-item nav-plugin ${activeTab === "pluginDetail" && selectedPluginId === plugin.id ? "active" : ""}`}
                  key={plugin.id}
                  onClick={() => {
                    setSelectedPluginId(plugin.id);
                    setActiveTab("pluginDetail");
                  }}
                  type="button"
                >
                  <span className="nav-icon" aria-hidden="true">✦</span>
                  <span className="nav-copy">
                    <span>{plugin.name}</span>
                    <small>{pluginKindText[plugin.kind]} · {plugin.integrationTargets.join(", ") || "通用"}</small>
                  </span>
                </button>
              ))}
            </div>
          ) : null}
        </nav>
        <div className="sidebar-footer">
          <button
            type="button"
            className={`guide-launch ${activeTab === "guide" ? "active" : ""}`}
            onClick={() => setActiveTab("guide")}
          >
            <span className="nav-icon" aria-hidden="true">?</span>
            <span className="nav-copy">
              <span>使用指引</span>
              <small>软件配置、引擎、插件和部署</small>
            </span>
          </button>
          <div className="sidebar-status-row">
            <button
              type="button"
              className="sidebar-icon-btn"
              onClick={() => setSettingsOpen(true)}
              aria-label="设置"
              title="设置"
            >
              ⚙
            </button>
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
            <h2>{activeView.label}</h2>
          </div>
          {activeTab !== "guide" ? (
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
          ) : null}
        </header>

        {showProjectBanner ? (
          <CurrentProjectBanner
            project={activeProject}
            activeStructure={activeStructure}
            openProjectFolder={openProjectFolder}
          />
        ) : null}

        {showStructureRequiredWarning ? (
          <section className="engine-reminder structure-required-warning" role="alert">
            <strong>未选中结构</strong>
            <span>
              当前项目还没有选中的结构。请先在“项目”页导入并选中一个 PDB/mmCIF/SDF/MOL2/SMILES
              或已有引擎工程文件；在此之前，AutoMD 不会生成或发送分子动力学运行指令。
            </span>
            <button type="button" onClick={() => setActiveTab("overview")}>
              去选择结构
            </button>
          </section>
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
            browseStructureFile={browseStructureFile}
            importStructure={importStructure}
            selectProject={selectProject}
            requestDeleteProject={requestDeleteProject}
            openProjectFolder={openProjectFolder}
            structures={structures}
            activeStructureId={activeStructureId}
            selectStructure={selectStructure}
            requestDeleteStructure={requestDeleteStructure}
            renamingStructureId={renamingStructureId}
            renamingStructureDraft={renamingStructureDraft}
            setRenamingStructureDraft={setRenamingStructureDraft}
            startRenameStructure={startRenameStructure}
            commitRenameStructure={commitRenameStructure}
            renamingProjectId={renamingProjectId}
            renamingProjectDraft={renamingProjectDraft}
            setRenamingProjectDraft={setRenamingProjectDraft}
            startRenameProject={startRenameProject}
            commitRenameProject={commitRenameProject}
          />
        )}

        {activeTab === "engines" && (
          <EnginesPanel
            engines={engines}
            engineTargets={engineTargets}
            selectedEngineTargetId={selectedEngineTargetId}
            setSelectedEngineTargetId={setSelectedEngineTargetId}
            selectedEngineId={selectedEngineId}
            setSelectedEngineId={setSelectedEngineId}
            engineInstallations={engineInstallations}
            engineInstallationDraft={engineInstallationDraft}
            setEngineInstallationDraft={setEngineInstallationDraft}
            saveEngineInstallation={saveEngineInstallation}
            deleteEngineInstallation={deleteEngineInstallation}
            generateRecipes={generateRecipes}
            autoFindEngine={autoFindEngine}
            manualFindEngine={manualFindEngine}
            autoInstallEngine={autoInstallEngine}
            installableEngines={installableEngines}
            containerRecipe={containerRecipe}
            buildRecipe={buildRecipe}
            recipeExportResult={recipeExportResult}
            buildWorkflowMode={buildWorkflowMode}
            setBuildWorkflowMode={setBuildWorkflowMode}
            buildWorkflowTimeout={buildWorkflowTimeout}
            setBuildWorkflowTimeout={setBuildWorkflowTimeout}
            buildWorkflowResult={buildWorkflowResult}
            engineDeployResult={engineDeployResult}
            exportRecipes={exportRecipes}
            runBuildWizard={runBuildWizard}
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
            autoFindScienceTool={autoFindScienceTool}
            manualFindScienceTool={manualFindScienceTool}
            autoInstallScienceSidecar={autoInstallScienceSidecar}
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
            plan={plan}
            diagnostics={diagnostics}
            remoteProfiles={remoteProfiles}
            selectedRemoteProfileId={selectedRemoteProfileId}
            setSelectedRemoteProfileId={setSelectedRemoteProfileId}
            remoteProfileDraft={remoteProfileDraft}
            setRemoteProfileDraft={setRemoteProfileDraft}
            remotePassword={remotePassword}
            setRemotePassword={setRemotePassword}
            remoteConnectionTest={remoteConnectionTest}
            remoteConnecting={remoteConnecting}
            testRemoteConnection={testRemoteConnection}
            saveRemoteProfile={saveRemoteProfile}
            deleteRemoteProfile={deleteRemoteProfile}
            engineTargets={engineTargets}
            installRemoteHelper={installRemoteHelperForProfile}
            checkRemoteHelper={checkRemoteHelperForProfile}
            projectName={(currentProject ?? projects[0] ?? null)?.name ?? null}
            structureName={activeStructure?.name ?? null}
            updatePlan={updatePlan}
            remotePreflight={remotePreflight}
            runRemotePreflight={runRemotePreflight}
            remoteAllowNoHelper={remoteAllowNoHelper}
            setRemoteAllowNoHelper={setRemoteAllowNoHelper}
            submitRemoteJob={submitRemoteJob}
            remoteSubmission={remoteSubmission}
            remoteBusy={remoteBusy}
            remoteJobSnapshot={remoteJobSnapshot}
            pollRemoteJobNow={pollRemoteJobNow}
            cancelRemoteJob={cancelRemoteJob}
            fetchRemoteResults={fetchRemoteResults}
            remoteAutoPoll={remoteAutoPoll}
            setRemoteAutoPoll={setRemoteAutoPoll}
            remoteWorkflowJobId={remoteWorkflowJobId}
            setRemoteWorkflowJobId={setRemoteWorkflowJobId}
            remotePackage={remotePackage}
            generateRemotePackage={generateRemotePackage}
            remoteWorkflowMode={remoteWorkflowMode}
            setRemoteWorkflowMode={setRemoteWorkflowMode}
            remoteWorkflowTimeout={remoteWorkflowTimeout}
            setRemoteWorkflowTimeout={setRemoteWorkflowTimeout}
            remoteWorkflowResult={remoteWorkflowResult}
            runRemoteStep={runRemoteStep}
            remoteSubmitOutput={remoteSubmitOutput}
            setRemoteSubmitOutput={setRemoteSubmitOutput}
            remoteStatusOutput={remoteStatusOutput}
            setRemoteStatusOutput={setRemoteStatusOutput}
            remoteLogOutput={remoteLogOutput}
            setRemoteLogOutput={setRemoteLogOutput}
            parseRemoteStatus={parseRemoteStatus}
            autoFindTool={autoFindTool}
            manualFindTool={manualFindTool}
            autoInstallTool={autoInstallTool}
            installableTools={installableTools}
          />
        )}

        {activeTab === "plugins" && (
          <PluginsPanel
            pluginRegistry={pluginRegistry}
            selectedPluginId={selectedPluginId}
            setSelectedPluginId={setSelectedPluginId}
            setActiveTab={setActiveTab}
            pluginImportPath={pluginImportPath}
            setPluginImportPath={setPluginImportPath}
            pluginImportOverwrite={pluginImportOverwrite}
            setPluginImportOverwrite={setPluginImportOverwrite}
            pluginTemplateDraft={pluginTemplateDraft}
            setPluginTemplateDraft={setPluginTemplateDraft}
            pluginConfigDrafts={pluginConfigDrafts}
            setPluginConfigDrafts={setPluginConfigDrafts}
            pluginRunResult={pluginRunResult}
            pluginBusy={pluginBusy}
            openPluginFolder={openPluginFolder}
            refreshPluginRegistry={refreshPluginRegistry}
            browsePluginManifest={browsePluginManifest}
            importPlugin={importPlugin}
            createPluginTemplate={createPluginTemplate}
            setUserPluginEnabled={setUserPluginEnabled}
            deleteUserPlugin={deleteUserPlugin}
            savePluginConfig={savePluginConfig}
            runPluginAction={runPluginAction}
            openPluginInstallFolder={openPluginInstallFolder}
          />
        )}

        {activeTab === "pluginDetail" && (
          <PluginDetailPage
            manifest={selectedPlugin}
            pluginConfigDrafts={pluginConfigDrafts}
            setPluginConfigDrafts={setPluginConfigDrafts}
            pluginRunResult={pluginRunResult}
            pluginBusy={pluginBusy}
            setActiveTab={setActiveTab}
            setUserPluginEnabled={setUserPluginEnabled}
            deleteUserPlugin={deleteUserPlugin}
            savePluginConfig={savePluginConfig}
            runPluginAction={runPluginAction}
            openPluginInstallFolder={openPluginInstallFolder}
          />
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
            setActiveTab={setActiveTab}
          />
        )}
      </section>
      </main>
      <AppStatusBar
        diagnostics={diagnostics}
        backgroundTasks={backgroundTasks}
        notifications={notifications}
        onReviewProblems={reviewProblems}
        bgTasksOpen={showBgTasks}
        onToggleBgTasks={() => setShowBgTasks((open) => !open)}
      />
      {showBgTasks ? (
        <BackgroundTaskPanel tasks={backgroundTasks} onClose={() => setShowBgTasks(false)} />
      ) : null}
      <NotificationStack
        notifications={notifications}
        flash={flashProblems}
        onDismiss={dismissNotification}
        onIgnore={ignoreNotification}
        onGuide={() => setActiveTab("guide")}
      />
      {settingsOpen ? (
        <SettingsModal
          theme={theme}
          setTheme={setTheme}
          diagnostics={diagnostics}
          performancePreferences={performancePreferences}
          updatePerformancePreferences={updatePerformancePreferences}
          plan={plan}
          onClose={() => setSettingsOpen(false)}
        />
      ) : null}
      {deleteTarget ? (
        <DeleteProjectModal
          project={deleteTarget}
          stage={deleteStage}
          deleting={deletingProject}
          onCancel={cancelDeleteProject}
          onConfirm={confirmDeleteProject}
        />
      ) : null}
      {deleteStructureTarget ? (
        <DeleteStructureModal
          structure={deleteStructureTarget}
          deleting={deletingStructure}
          onCancel={cancelDeleteStructure}
          onConfirm={confirmDeleteStructure}
        />
      ) : null}
      {directPluginRunTarget ? (
        <DirectPluginRunModal
          manifest={directPluginRunTarget.manifest}
          action={directPluginRunTarget.action}
          running={pluginBusy}
          onCancel={() => setDirectPluginRunTarget(null)}
          onConfirm={() => runPluginAction(directPluginRunTarget.manifest, directPluginRunTarget.action, "direct", true)}
        />
      ) : null}
    </>
  );
}

function GuidePanel({
  engines,
  setActiveTab
}: {
  engines: EngineCapability[];
  setActiveTab: (tab: TabId) => void;
}) {
  const guideContentRef = useRef<HTMLDivElement | null>(null);
  const guideOutlineRef = useRef<HTMLElement | null>(null);
  const [activeGuideSection, setActiveGuideSection] = useState("guide-quickstart");
  const guideSections = useMemo(() => [
    { id: "guide-quickstart", label: "快速开始" },
    { id: "guide-concepts", label: "先弄懂这些词" },
    { id: "guide-full-flow", label: "完整项目流程" },
    { id: "guide-parameters", label: "常用参数" },
    { id: "guide-science", label: "科学环境" },
    { id: "guide-pages", label: "每个页面怎么用" },
    { id: "guide-directories", label: "项目目录" },
    { id: "guide-reproducibility", label: "索引与复现记录" },
    { id: "guide-structure-import", label: "结构导入格式" },
    { id: "guide-run-features", label: "运行功能" },
    { id: "guide-status", label: "顶部与状态栏" },
    { id: "guide-performance", label: "性能配置" },
    { id: "guide-engines", label: "引擎配置" },
    { id: "guide-deploy-build", label: "安装部署编译" },
    { id: "guide-platform", label: "平台策略" },
    { id: "guide-remote", label: "远程/HPC" },
    { id: "guide-run-report", label: "分析和报告" },
    { id: "guide-plugins", label: "插件管理" },
    { id: "guide-failures", label: "故障处理" }
  ], []);

  const conceptRows: Array<{ term: string; meaning: string; where: string }> = [
    {
      term: "当前项目",
      meaning: "AutoMD 的工作目录。一个项目会保存原始输入、导入后的结构、生成的参数文件、运行日志、checkpoint、轨迹、分析结果和报告。",
      where: "在项目、流程、运行、远程和报告页顶部固定显示。确认这里的项目名再点击任何运行类按钮。"
    },
    {
      term: "左侧导航图标",
      meaning: "每个图标对应一个主要工作区：项目、流程、运行、报告、远程、引擎和插件。图标用于快速扫视，文字标签仍然是判断模块用途的主依据。",
      where: "按照从上到下的顺序使用：先项目，再流程，再运行和报告；远程、引擎、插件是配置和扩展区域。"
    },
    {
      term: "当前结构",
      meaning: "真正要拿去做模拟的分子结构。只有选中结构后，软件才允许生成结构准备文件、运行包、远程提交脚本或报告。",
      where: "在项目页的结构索引中选择。没有结构时，后续页面会警告并拒绝发送 MD 运行指令。"
    },
    {
      term: "结构准备与分析环境",
      meaning: "AutoMD 管理的 Python 科学环境，负责 PDBFixer/OpenMM/RDKit/Open Babel/AmberTools/MDAnalysis/MDTraj 这类工具。",
      where: "在流程页查看。当前引擎需要的工具显示可用或需安装，不需要的显示不适用。"
    },
    {
      term: "SimulationPlan",
      meaning: "GUI 中的统一模拟计划，包含体系、力场、水模型、离子、阶段长度、资源和分析模块。",
      where: "在流程页编辑。它不是 GROMACS .mdp 或 AMBER mdin，而是生成这些原生文件的上层计划。"
    },
    {
      term: "参数映射",
      meaning: "把 GUI 参数翻译成当前引擎的原生字段，例如 GROMACS .mdp、OpenMM runner、AMBER mdin、NAMD conf。",
      where: "在流程页的高级折叠区查看。普通用户先看参数检查；正式长模拟前再检查映射和生成文件。"
    },
    {
      term: "结构准备文件",
      meaning: "运行前生成的输入准备文件，例如修复结构脚本、environment.yml、tleap 输入、OpenMM 预处理脚本和配体参数化提示。",
      where: "在流程页生成。它不是最终结果，而是帮助你把结构变成可运行输入。"
    },
    {
      term: "运行包",
      meaning: "可以执行或提交的一组文件，包含引擎输入、命令脚本、日志路径、checkpoint 路径和 artifact 约定。",
      where: "在运行页生成。先 Dry run 检查，再真实执行。"
    },
    {
      term: "批量实验包",
      meaning: "为同一个计划生成多个 replica 和不同 seed 的重复实验文件，适合检查随机种子、初始速度或重复模拟的一致性。",
      where: "在运行页的“高级 / 更多”里设置 Replica 数和 Seed 起点。生成后只写 generated/batch 文件，不会直接启动模拟。"
    },
    {
      term: "Artifact",
      meaning: "运行产生或分析产生的文件索引，例如日志、轨迹、能量、分析表、报告和 checkpoint。",
      where: "运行完成后在运行页刷新，报告页会复用这些记录。"
    },
    {
      term: "Checkpoint",
      meaning: "中断后继续模拟的恢复点。真实生产模拟一定要设置合理 checkpoint 间隔。",
      where: "运行页查看和恢复。不要随手删除 run directory。"
    },
    {
      term: "Walltime",
      meaning: "给本地/远程任务预留的实际机器运行时间，不等于模拟时间。模拟时间用 ns/us/ms 表示。",
      where: "生产模拟长度在流程页用 ns；HPC walltime 在远程或资源设置里用小时。"
    },
    {
      term: "性能配置",
      meaning: "左下角设置里的 CPU、GPU、内存和磁盘偏好。它决定 AutoMD 生成本地运行、远程模板、安装/分析任务时优先使用多少资源。",
      where: "普通用户先用默认建议；要避免卡顿时减少 CPU 核心数，GPU 不确定时先选 CPU 模式或自动选择。"
    }
  ];

  const quickStartSteps: Array<{ title: string; desc: string; tab: TabId; cta: string }> = [
    { title: "创建项目", desc: "在项目页创建一个项目，确认顶部固定条显示的是这个项目。项目会自动建立 inputs、generated、runs、trajectories、analysis、reports 等目录。", tab: "overview", cta: "去新建" },
    { title: "配置引擎", desc: "到引擎页选择目标引擎。入门生物分子优先 GROMACS；快速教学或 Python 自定义优先 OpenMM；缺失时点自动查找、手动查找或一键安装。", tab: "engines", cta: "去安装" },
    { title: "导入结构", desc: "回项目页用浏览按钮选择 PDB、mmCIF、SDF、MOL2、SMILES 或已有工程，并在结构索引中确认当前结构已选中。", tab: "overview", cta: "去导入" },
    { title: "准备科学环境", desc: "流程页会显示当前引擎需要哪些 Python/AmberTools 工具。可用就继续，需安装就点一键安装，不适用的不用管。", tab: "workflow", cta: "去检查" },
    { title: "设置参数", desc: "在流程页设置力场、水模型、盒子 padding、盐浓度、温度、压力、阶段长度和生产模拟长度。测试先用 1 ns，正式再拉长。", tab: "workflow", cta: "去配置" },
    { title: "生成运行包", desc: "运行页先 Dry run，只生成输入、命令和脚本，不启动引擎。检查无误后再本地运行或转远程/HPC。", tab: "run", cta: "去运行" },
    { title: "分析轨迹", desc: "运行完成后刷新 artifact，索引轨迹，再生成 RMSD、RMSF、Rg、氢键、能量和温压等分析。", tab: "run", cta: "去分析" },
    { title: "导出报告", desc: "报告页导出 Markdown、HTML 或 PDF，保留输入、环境、参数、命令、日志、分析和复现记录。", tab: "report", cta: "去报告" }
  ];
  const exampleFlow: Array<{ step: string; action: string; details: string; done: string }> = [
    {
      step: "1. 新建项目并确认当前项目",
      action: "打开“项目”页，在“创建项目”里填写名称，例如 Protein_Water_Demo，领域选择“生物分子”，首选引擎选择 GROMACS 或 OpenMM，然后点击创建。",
      details: "项目名建议只包含英文、数字、下划线或短横线，方便在 HPC、脚本和报告里复现。项目目录建议放在空间充足的位置，轨迹文件可能很大。创建后不要急着去运行，先看页面顶部固定条是不是显示新项目。",
      done: "顶部固定条显示当前项目；项目索引里能看到这个项目；点击“打开文件夹”能打开项目目录。"
    },
    {
      step: "2. 导入并选中结构",
      action: "在“导入结构”卡片里选择输入类型，点击“浏览”选文件，再点击“导入到 inputs/”。",
      details: "PDB/mmCIF 适合蛋白、核酸和复合物；SDF/MOL2/SMILES 适合小分子；已有 GROMACS、OpenMM、AMBER、NAMD 等工程可以作为已有引擎工程导入。显示名称可以留空，软件会用文件名；导入后输入框会清空，方便继续导入第二个结构。",
      done: "结构索引中出现导入项，并且目标结构被选中。结构与轨迹视图不再只是空状态。"
    },
    {
      step: "3. 检查引擎和科学环境",
      action: "先到“引擎”页让目标引擎变成可用，再回“流程”页看结构准备与分析环境。",
      details: "GROMACS、OpenMM、AmberTools、LAMMPS、CP2K、HOOMD-blue 等开源工具优先用一键安装。NAMD、AMBER pmemd、CHARMM、Desmond、ACEMD 等受限或商业引擎必须使用你已有授权环境。流程页的科学环境用于结构修复、加氢、配体处理、OpenMM 快速验证和轨迹分析；当前引擎不需要的项目会显示“不适用”。",
      done: "目标引擎显示 ready 或可用；流程页里当前引擎必需的科学工具没有红色缺失项。"
    },
    {
      step: "4. 设置入门参数",
      action: "在“流程”页设置模拟参数。蛋白水溶液入门建议：蛋白力场 CHARMM36m 或 Amber 系列，水模型 TIP3P，padding 1.0 nm，盐浓度 0.15 M，温度 300 K，压力 1 bar。",
      details: "阶段建议先跑短测试：能量最小化 5000 到 50000 steps，NVT 100 ps，NPT 100 ps，Production 1 ns。测试稳定后再把生产模拟长度改成 10 ns、100 ns 或更长。生产模拟长度是模拟时间，单位是 ns；HPC walltime 才是机器排队/运行时间，单位通常是小时。",
      done: "参数检查显示通过或只有可理解的提示；没有选中结构时这里不应继续生成运行文件。"
    },
    {
      step: "5. 生成结构准备文件",
      action: "在“流程”页点击“生成结构准备文件”。",
      details: "这一步会写出预处理脚本、environment.yml、配体/AmberTools 提示和参数准备清单。它解决的是“结构能不能被准备成可运行输入”，不是正式开始模拟。如果这里失败，先处理缺失原子、非标准残基、配体参数、力场或工具缺失。",
      done: "结构准备文件列表不再为空；错误信息明确指出是缺工具、缺输入、参数化失败还是路径问题。"
    },
    {
      step: "6. Dry run 生成运行包",
      action: "进入“运行”页，先选择 Dry run 或生成 run package。",
      details: "Dry run 只写输入文件、命令和脚本，不启动引擎。检查 .mdp、.top、.tpr、OpenMM runner、NAMD conf、run.sh、checkpoint 路径、日志路径和轨迹路径。看不懂原生文件时，至少确认路径在当前项目目录内，命令里使用的是你配置的引擎。",
      done: "运行包生成成功，artifact 里能看到将要产生的日志、checkpoint、轨迹、分析和报告路径。"
    },
    {
      step: "7. 真实运行或提交远程任务",
      action: "小体系可以在“运行”页启动本地任务；大体系、Linux-only 引擎或 GPU 队列任务去“远程”页生成 HPC 脚本。",
      details: "本地任务要看日志尾部、进度、checkpoint、失败分类和底部后台任务状态。远程任务先检查 SSH、rsync、workdir、module load、队列名、GPU 资源和 walltime；第一次建议只写脚本或 Dry run，不要直接提交长任务。",
      done: "任务进入 completed，或失败时能看到具体原因。中断后优先用 checkpoint resume。"
    },
    {
      step: "8. 分析、检查质量并导出报告",
      action: "运行结束后刷新 artifact，索引轨迹，生成分析包，再到“报告”页导出。",
      details: "入门必须看 RMSD、温度、压力和能量是否稳定；RMSF 看柔性区域；Rg 看整体紧密程度；氢键、距离、角度和二面角用于解释局部事件。报告应包含结构、环境、引擎版本、参数、命令、日志、checkpoint、轨迹摘要、图表和人工复核点。",
      done: "报告文件已生成，别人拿到项目目录和报告能知道输入是什么、怎么跑、结果在哪里、如何复现。"
    }
  ];

  const parameterRows: Array<{ item: string; beginner: string; note: string }> = [
    { item: "蛋白力场", beginner: "蛋白体系优先 CHARMM36m 或 Amber 系列。", note: "配体、金属、膜蛋白或非标准残基可能需要额外参数。不要只因为下拉框里有选项就假设它适合所有分子。" },
    { item: "水模型", beginner: "入门水溶液常用 TIP3P。", note: "水模型最好和力场推荐搭配，不同引擎名字可能不同，参数映射会把 GUI 选项翻译成原生字段。" },
    { item: "盒子 padding", beginner: "蛋白到盒子边界先用 1.0 nm。", note: "太小容易和周期边界相互作用，太大会显著增加水分子和计算量。" },
    { item: "盐浓度", beginner: "常见生理盐浓度可用 0.15 M，并中和体系电荷。", note: "如果实验体系有特定离子条件，应按实验条件设置。" },
    { item: "温度/压力", beginner: "常见水溶液测试用 300 K 和 1 bar。", note: "NVT 控温，NPT 控温控压；膜体系、材料体系和特殊溶剂需要更谨慎。" },
    { item: "Timestep", beginner: "有氢键约束时常用 2 fs；不确定先用更保守设置。", note: "数值发散时先降低 timestep、加强最小化、检查初始冲突和约束。" },
    { item: "生产模拟长度", beginner: "测试先 1 ns；看稳定后再做 10 ns、100 ns 或更长。", note: "这是模拟时间，不是现实等待时间。现实等待时间由机器性能、体系大小和资源决定。" },
    { item: "输出频率", beginner: "测试可密一点，正式长模拟不要太密。", note: "输出太频繁会产生巨大轨迹和日志；太稀会错过事件。先按 10 到 100 ps 量级检查。" },
    { item: "Checkpoint 间隔", beginner: "长任务一定要开 checkpoint，间隔比输出频率略长也可以。", note: "HPC walltime 到期、断电或取消任务时，checkpoint 是恢复依据。" }
  ];

  const scienceRows: Array<{ tool: string; role: string; needed: string }> = [
    { tool: "OpenMM", role: "Python MD 引擎和快速验证环境。可直接跑小体系，也可用于生成/检查 Python runner。", needed: "选择 OpenMM 引擎时必需；只跑 GROMACS/LAMMPS/CP2K 时通常不适用或可选。" },
    { tool: "PDBFixer", role: "修复 PDB/mmCIF 中缺失原子、加氢、处理简单结构清理。", needed: "生物分子结构准备常用；材料体系或已有完整拓扑时可能不适用。" },
    { tool: "MDAnalysis", role: "读取轨迹并计算 RMSD、RMSF、Rg、距离、氢键等分析。", needed: "几乎所有需要分析轨迹的工作流都建议安装。" },
    { tool: "MDTraj", role: "轻量轨迹读取、几何分析和部分格式互转。", needed: "常用于轨迹预览和基础分析；某些材料模板只作为可选分析工具。" },
    { tool: "RDKit", role: "小分子读写、SMILES/SDF 处理、基础化学检查。", needed: "有配体、小分子或 SMILES 输入时需要；纯蛋白水溶液可选。" },
    { tool: "Open Babel", role: "分子格式转换和部分小分子预处理。", needed: "SDF/MOL2/SMILES 互转或配体流程常用；纯蛋白可选。" },
    { tool: "AmberTools tleap", role: "生成 Amber 拓扑、坐标和力场输入。", needed: "AmberTools/AMBER 生态必需；GROMACS/OpenMM 只在复用 Amber 输入时需要。" },
    { tool: "AmberTools antechamber/parmchk2", role: "配体电荷和 GAFF/参数检查入口。", needed: "小分子配体参数化时需要；没有配体时不适用。" },
    { tool: "AmberTools cpptraj", role: "Amber 轨迹后处理和分析。", needed: "AMBER 轨迹分析常用；其他引擎可选。" }
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
      fill: "填写项目名、领域、首选引擎；导入 PDB/mmCIF/SDF/MOL2/SMILES 或已有工程目录。文件路径可以手输，也可以点“浏览”打开系统文件管理器。",
      check: "当前项目固定条显示正确项目；结构导入后能看到 importedPath、原子/残基/链摘要，并且结构索引里有当前选中项。结构视图从空状态变为 Mol* 加载状态。",
      next: "导入后进入“流程”设置力场、溶剂、离子和阶段参数；还没配置引擎时先去“引擎”。"
    },
    {
      title: "流程",
      target: "workflow",
      use: "编辑 SimulationPlan：体系、力场、溶剂、离子、模拟阶段、输出和基础分析。这里是参数工作的中心。",
      fill: "设置力场、水模型、盒子尺寸、离子浓度、温度、压力、timestep、阶段时长、checkpoint 间隔和输出频率。复杂引擎参数保留原生文件编辑。",
      check: "看参数映射是否 mapped、approximated 或 unsupported。unsupported 不代表不能跑，但代表需要人工看原生输入文件。",
      next: "参数检查通过后去“运行”生成 run package；结构准备失败则回到项目/结构输入。"
    },
    {
      title: "运行",
      target: "run",
      use: "执行本地任务、Dry run、Mock runner、日志解析、取消任务、checkpoint resume、批量重复实验、轨迹索引和分析包生成。",
      fill: "选择本地运行模式；普通用户先用 Dry run 生成运行包。需要重复实验时，打开“高级 / 更多”，填写 Replica 数和 Seed 起点，点击“生成批量实验包”。必要时再编辑原生参数文件或粘贴日志样本做解析。",
      check: "先确认当前项目固定条里显示了当前结构，再生成 run package；没有选中结构时 AutoMD 会直接拒绝运行。之后再看任务状态、日志尾部、失败分类、checkpoint 和 artifact。真实执行前最好先 Dry run。",
      next: "本地完成后刷新 artifact 并分析；集群任务去“远程”；需要报告去“报告”。"
    },
    {
      title: "报告",
      target: "report",
      use: "整理可复现实验记录，导出 Markdown、HTML 或 PDF。适合项目结束、阶段汇报或复现实验归档。",
      fill: "选择报告格式，刷新 artifact 和分析缓存，确认项目、参数、环境、命令、日志和图表都已进入报告。",
      check: "报告应能回答：输入是什么、用什么引擎和版本、参数是什么、命令如何执行、结果在哪里、哪些地方需要人工复核。",
      next: "导出后保存报告和项目目录；需要继续生产模拟时回“运行”用 checkpoint resume。"
    },
    {
      title: "远程",
      target: "remote",
      use: "配置 SSH/HPC profile，生成同步、提交、查询、日志和回收脚本。适合大体系、GPU 队列和 Linux-only 引擎。",
      fill: "填写 host、user、port、workdir、scheduler、queue/partition、account、walltime、CPU/GPU、module load 和运行命令模板。",
      check: "先确认已选中结构，再 Dry run，看 rsync、ssh、sbatch/qsub/bsub、状态查询和日志路径是否正确。workdir 必须有写权限。",
      next: "脚本确认后执行提交；任务完成后回收结果，再到“运行/报告”分析。"
    },
    {
      title: "引擎",
      target: "engines",
      use: "配置本机或用户授权环境中的 MD 引擎。这里决定软件能不能调用 GROMACS、OpenMM、AmberTools、NAMD 等。",
      fill: "缺失时先点“自动查找”，找不到再点“手动查找”选择可执行文件；可通过 conda-forge 安装的引擎点“一键安装”会真实下载并安装。需要许可或复杂平台构建的引擎会生成编译脚本或要求手动配置授权路径。",
      check: "ready 表示可直接调用；需要安装表示先用一键部署或打开该引擎卡片的高级部署/编译；需要许可证表示先完成外部授权；平台不支持时考虑 WSL2、容器或远程。",
      next: "引擎 ready 后回“流程”映射参数；缺工具使用引擎卡片内的高级部署/编译；Linux-only 或大任务去“远程”。"
    },
    {
      title: "插件",
      target: "plugins",
      use: "查看和管理扩展 manifest。插件可以增加引擎适配器、分析模块、远程调度器、构建 recipe 或报告模板。",
      fill: "把 .automd-plugin.json 放入插件目录，声明 id、name、kind、version、entrypoint、capabilities、license 和支持平台。",
      check: "查看 warning、entrypoint、sourcePath 和 capabilities。未知来源插件不要启用执行命令，先读 manifest。",
      next: "插件被识别后，对应能力会出现在引擎、分析、远程、报告或引擎高级部署区。"
    }
  ];

  const projectDirectoryRows: Array<{ path: string; purpose: string; userAction: string }> = [
    { path: "inputs/", purpose: "保存导入的原始结构、配体和已有引擎工程文件。", userAction: "导入结构后先检查这里是否有对应文件；不要直接把大型轨迹塞进 inputs。" },
    { path: "generated/", purpose: "保存 AutoMD 生成的引擎输入、参数文件、Python 准备包和分析包。", userAction: "Dry run 后重点检查 generated/<engine>/；需要手工复核时从运行页打开原生文件编辑器。" },
    { path: "generated/prep/", purpose: "结构准备包，包含 prepare_structure.py、environment.yml、准备报告和配体参数化说明。", userAction: "结构准备失败时先看这里的 README 和 structure-prep-report.json。" },
    { path: "generated/analysis/", purpose: "MDAnalysis 分析包，包含 run_mdanalysis.py、environment.yml 和分析说明。", userAction: "轨迹出来后再生成；先确认 topology、trajectory 和 selection 能对应。" },
    { path: "generated/batch/", purpose: "批量 replica/seed 实验包。每个 replica 有独立计划和独立原生文件。", userAction: "重复实验前生成；不要把单次运行的文件手动复制成批量实验。" },
    { path: "runs/", purpose: "每次本地或远程运行的日志、命令、stdout/stderr、环境快照和任务状态。", userAction: "任务失败时先打开对应 run 目录的日志；checkpoint resume 也依赖这里。" },
    { path: "checkpoints/", purpose: "集中保存或索引可恢复的 checkpoint/restart 文件。", userAction: "取消任务或 walltime 到期后先找 checkpoint，不要删除。" },
    { path: "trajectories/", purpose: "保存轨迹文件和 .automd-index 分块索引。", userAction: "大轨迹先索引，再预览抽样帧；二进制轨迹通常交给 MDAnalysis 解码。" },
    { path: "analysis/", purpose: "保存 RMSD/RMSF/Rg/氢键/距离/能量等 CSV、XVG 或 JSON 结果。", userAction: "刷新 artifact 后图表会读取这里的小型数值表。" },
    { path: "reports/", purpose: "保存 Markdown、HTML 和 PDF 报告。", userAction: "项目阶段结束或要分享结果时导出；报告会引用 artifact 和环境快照。" },
    { path: "remote/", purpose: "保存 SSH/rsync、SLURM/PBS/LSF 或 SSH 直跑脚本。", userAction: "远程任务先写脚本审阅，再执行连接、同步、提交和回收。" },
    { path: "build-recipes/", purpose: "保存引擎部署、源码编译、容器 recipe 和构建日志。", userAction: "一键部署失败或平台特殊时，从这里查看脚本、stdout/stderr 和失败分析。" }
  ];

  const reproducibilityRows: Array<{ item: string; purpose: string; where: string; userAction: string }> = [
    { item: "SQLite 项目索引", purpose: "保存项目列表、当前项目摘要、远程 profile、引擎安装记录、任务历史、artifact 元数据和分析缓存。", where: "软件自己的应用数据目录，不是项目科学结果本身。", userAction: "普通用户不需要手动打开；如果项目文件夹还在，科学输入和输出仍以项目目录为准。" },
    { item: "engine_installations", purpose: "记住每个本机或远程目标设备上的引擎路径、版本、授权状态和检测时间。", where: "引擎页扫描、手动登记或一键部署后写入。", userAction: "换电脑、换远程机器或移动 Conda 环境后，重新自动扫描或手动登记。" },
    { item: "remote_profiles", purpose: "记住 SSH/HPC 主机、端口、用户名、调度器、workdir、队列和 module/setup 命令。", where: "远程页保存 profile 后写入。", userAction: "密码或 key 变化、workdir 变化、集群 module 变化时重新测试连接。" },
    { item: "local_tasks", purpose: "保存本地任务 id、计划 id、引擎、模式、状态、进度、运行目录、退出码、错误和日志尾部。", where: "运行页启动 Dry run、Mock 或真实本地执行时写入。", userAction: "任务失败时先看这里对应的任务卡，再打开 run 目录日志。" },
    { item: "automd-run-manifest.json", purpose: "每次 Dry run、Mock 或真实执行都会写出的可复现快照，包含 OS/arch、环境变量片段、运行工具、命令、计划和 run directory。", where: "runs/<engine-plan>/automd-run-manifest.json。", userAction: "不要把它当参数文件手改；报告和排错会引用它来说明当时到底怎么跑。" },
    { item: "artifact_records", purpose: "保存最近一次项目扫描得到的文件种类、路径、大小、修改时间和摘要。", where: "刷新 artifact 后写入 SQLite；真实文件仍在 inputs/generated/runs/trajectories/analysis/reports 等目录。", userAction: "如果列表和文件夹不一致，先点刷新 artifact；远程任务先 sync-down 再刷新。" },
    { item: "analysis_cache", purpose: "保存解析出来的图表序列摘要，例如 x/y 轴、点数、最小值、最大值和最后值。", where: "解析 .xvg 或 CSV 后写入 SQLite。", userAction: "图表为空时先确认 analysis 文件存在且有数值列，再重新解析。" },
    { item: "trajectory index manifest", purpose: "记录大轨迹的格式、抽样帧、字节范围、时间和警告，避免 UI 一次性读完整轨迹。", where: "trajectories/.automd-index/。", userAction: "文本轨迹可直接分块预览；XTC/TRR/DCD/NetCDF/GSD 通常先作为 metadata，再用 MDAnalysis 侧车分析。" },
    { item: "报告文件", purpose: "把当前计划、任务状态、运行命令、环境 manifest、artifact、分析图表和错误记录整理成 Markdown/HTML/PDF。", where: "reports/。", userAction: "导出前确认当前项目、当前结构、artifact 和分析缓存已经刷新。" }
  ];

  const structureInputRows: Array<{ format: string; use: string; caution: string }> = [
    { format: "PDB / mmCIF", use: "蛋白、核酸、蛋白-配体复合物、实验结构或 AlphaFold/建模结构。", caution: "常见问题是缺失原子、缺氢、非标准残基、altloc、断链、金属配位和配体没有参数；先用结构准备包修复，再检查力场支持。" },
    { format: "SDF / MOL2", use: "单个小分子、配体或辅因子输入。", caution: "这只是化学结构，不等于可运行拓扑。要确认质子化、手性、电荷、atom type、mol2/frcmod 或对应力场参数。" },
    { format: "SMILES", use: "快速登记配体或小分子草图。", caution: "SMILES 没有 3D 构象、质子化环境和力场参数；生产模拟前必须生成并复核 3D 构型和参数。" },
    { format: "已有引擎工程", use: "已经有 GROMACS/OpenMM/AMBER/NAMD/LAMMPS/CP2K 等原生项目时做统一索引。", caution: "AutoMD 会登记和生成 manifest，但不会自动证明原生输入科学有效；仍要看原生日志和参数文件。" },
    { format: "大型轨迹", use: "运行后分析和预览，不作为新结构导入入口。", caution: "XTC/TRR/DCD/NetCDF/GSD 会先作为 metadata-only artifact；需要帧内容时用轨迹索引或 MDAnalysis 包。" }
  ];

  const runFeatureRows: Array<{ feature: string; when: string; how: string; risk: string }> = [
    { feature: "Dry run", when: "第一次配置新项目、新引擎、新远程 profile 或新参数时。", how: "只生成输入、命令、脚本和路径约定，不启动计算。检查 generated、runs、remote 和 expected outputs。", risk: "Dry run 成功只说明文件能生成，不说明结构/力场一定科学正确。" },
    { feature: "Mock runner", when: "想测试 GUI 监控、日志刷新、进度条、artifact 和报告闭环时。", how: "运行内置模拟器，不调用真实引擎。适合验证 UI 和流程，不适合作为科学结果。", risk: "Mock completed 不是 MD 成功；不要把 mock 日志写进正式报告结论。" },
    { feature: "真实本地执行", when: "引擎 ready、结构已选中、参数检查通过、机器资源足够时。", how: "AutoMD 调用 generated/runs 中的 run 脚本，轮询状态、日志尾部、checkpoint 和失败分类。", risk: "可能长时间占用 CPU/GPU；生产前确认线程数、GPU、输出频率和 checkpoint。" },
    { feature: "原生文件编辑", when: "参数映射显示近似/需复核，或引擎需要 GUI 没覆盖的字段。", how: "在运行页打开 .mdp、.mdin、.conf、.inp、.key、.py、.sh、JSON/YAML 等项目内文本文件。", risk: "只允许编辑项目内安全路径；改完原生文件后要重新 Dry run 或确认命令仍引用正确文件。" },
    { feature: "日志解析", when: "本地/远程/手工运行失败，或要从已有日志提取 step、ns/day、checkpoint、warning、fatal error。", how: "把 log 尾部或完整小日志粘贴到运行页解析器。AutoMD 会分类并给建议。", risk: "只看最后一行可能漏掉真正首个错误；优先找第一条 fatal/error/warning。" },
    { feature: "Checkpoint resume", when: "任务中断、取消、walltime 到期、机器重启或远程连接断开后。", how: "刷新 artifact 或发现 resume plan，使用推荐 checkpoint 和 append/restart 命令恢复。", risk: "删除 run 目录、改 deffnm、移动 checkpoint 或换不兼容参数会导致不能续跑。" },
    { feature: "批量实验包", when: "需要多 seed、多 replica 检查随机性、收敛性或重复性。", how: "在运行页高级区设置 replica 数和 seed 起点，生成 generated/batch 和 run-batch.sh。", risk: "批量包生成后不会自动启动；每个 replica 的输出要分开看，不要只看平均值。" },
    { feature: "轨迹索引/分块预览", when: "轨迹太大，不适合一次加载到前端。", how: "先索引文本轨迹 PDB/XYZ/LAMMPS dump；二进制轨迹登记为 metadata，交给分析侧车。", risk: "预览帧只是抽样，不代表完整轨迹质量；正式分析要用完整轨迹和合理 selection。" },
    { feature: "报告导出", when: "完成一次测试、生产模拟或阶段性分析后。", how: "刷新 artifact 和分析缓存，导出 Markdown/HTML/PDF。", risk: "报告反映当前项目状态；导出前要确认当前项目和当前结构没有切错。" }
  ];

  const performanceRows: Array<{ setting: string; meaning: string; recommendation: string; warning: string }> = [
    { setting: "CPU 核心数", meaning: "限制本地任务、分析任务和模板资源字段使用的线程数。", recommendation: "保留 1 个逻辑核心给系统；不确定时用软件建议值。小测试 2-4 线程通常够用，大任务再按机器和引擎扩展。", warning: "核心数越多不一定越快，内存带宽、I/O、MPI/OpenMP 设置和引擎缩放效率都会影响性能。" },
    { setting: "GPU 选择", meaning: "选择自动 GPU、某个具体 GPU，或强制 CPU。", recommendation: "NVIDIA/CUDA、AMD/ROCm、Apple/Metal 是否能用取决于引擎。遇到模型错误和 GPU 错误分不清时，先用 CPU 复现小测试。", warning: "底部显示 GPU 可用不代表所有引擎都能用这个 GPU；GROMACS/OpenMM/LAMMPS/CP2K 的 GPU 后端不同。" },
    { setting: "GPU 数量", meaning: "告诉计划和远程模板预期使用几张可加速 GPU。", recommendation: "桌面单卡通常填 1；没有可用 GPU 或选择 CPU 时为 0；HPC 多 GPU 要和队列资源语法一致。", warning: "多 GPU 需要引擎、MPI/域分解、驱动和队列资源同时支持，否则可能比单 GPU 更慢或直接失败。" },
    { setting: "内存上限", meaning: "给后台分析、安装和任务提示一个软限制。0 表示自动。", recommendation: "普通用户保持 0；大轨迹分析时按机器内存留出系统余量，例如 32 GB 机器不要把上限设满。", warning: "这不是操作系统硬限制，真正内存占用仍由引擎、Python 分析和轨迹大小决定。" },
    { setting: "工作磁盘", meaning: "显示系统检测到的磁盘卷和可用空间，用于判断项目、轨迹和构建输出放在哪里。", recommendation: "轨迹和 build-recipes 放到空间充足的磁盘；HPC 远程优先 scratch/workdir，不要用很小的 home。", warning: "选择磁盘不会自动迁移已有项目目录；需要在项目页创建或打开正确位置。" },
    { setting: "检测到的 GPU 列表", meaning: "列出每个 GPU 的名称、厂商、后端和显存/详情。", recommendation: "多个 GPU 时选目标设备；不可用设备会标记不适用。", warning: "外接 GPU、虚拟机、容器和远程机器的 GPU 不一定会出现在本机设置里，远程 GPU 要在远程页/helper 检测。" },
    { setting: "外观主题", meaning: "浅色/深色显示偏好。", recommendation: "按阅读习惯选择；偏好会被记住。", warning: "主题只影响显示，不影响项目、参数或运行结果。" }
  ];

  const pluginGuideRows: Array<{ action: string; description: string; safety: string }> = [
    { action: "打开插件目录", description: "查看 AutoMD 当前用户插件根目录。目录路径由系统应用数据目录动态生成，不应写死为某个用户名。", safety: "跨电脑迁移时以软件显示的目录为准，不要复制别人机器上的绝对路径。" },
    { action: "导入插件", description: "选择插件目录或单个 .automd-plugin.json。目录导入会复制 manifest 和相对 entrypoint 到插件根目录。", safety: "id 冲突时默认拒绝；不能覆盖 built-in 插件。导入前先读 README 和 manifest。" },
    { action: "新建/快速创建插件", description: "选择模板：引擎适配器、分析模块、远程调度器、构建 recipe 或报告模板；填写名称、id、目标模块和入口语言。", safety: "生成的是可编辑模板，不代表科学逻辑已经正确；要用小项目和 mock 数据测试。" },
    { action: "启用/停用", description: "用户插件可停用，停用后左侧用户插件入口隐藏，但仍可在插件页重新启用。", safety: "built-in 插件只读，不允许删除或停用，避免破坏核心能力。" },
    { action: "删除插件", description: "只能删除非 built-in 用户插件，同时移除状态和安装目录。", safety: "删除前确认没有报告、流程或项目依赖该插件生成的文件。" },
    { action: "编辑配置", description: "按插件 configSchema 填写 JSON 配置，例如外部命令路径、默认 selection、报告模板选项。", safety: "配置错误会导致插件动作失败；保留默认配置作为回退。" },
    { action: "运行动作", description: "默认以轻量沙盒运行：固定 cwd、白名单环境变量、JSON stdin、受限输出目录。", safety: "直接运行会跳过部分限制，必须二次确认命令、cwd、权限和可能写入目录；未知插件不要直接运行。" },
    { action: "联动位置", description: "engineAdapter 出现在引擎页；analysisModule 出现在流程/运行分析；remoteScheduler 出现在远程；buildRecipe 出现在引擎高级部署；reportTemplate 出现在报告。", safety: "v1 不允许插件注入自定义 React 页面，所有插件都走通用详情页和声明式能力。" }
  ];

  const remoteModeRows: Array<{ mode: string; use: string; verify: string; fallback: string }> = [
    { mode: "SSH 直连", use: "没有 SLURM/PBS/LSF 的云服务器、工作站或个人 Linux 主机。", verify: "测试连接应显示 OS、架构、CPU、工作目录可写；提交时用 nohup/后台 PID 监控。", fallback: "如果掉线，重新连接后用状态查询和日志尾部恢复；失败时回收 runs/checkpoints/analysis/reports。" },
    { mode: "SLURM/PBS/LSF", use: "高校、研究所或企业 HPC 集群。", verify: "确认队列/partition、account、GPU 资源语法、module load、walltime 和 workdir。", fallback: "如果提交失败，先把脚本只写到 remote/，拿给集群管理员或在登录节点手动 sbatch/qsub/bsub。" },
    { mode: "远程 helper", use: "需要远程扫描硬件、扫描引擎、安装 helper、远程一键部署或远程构建时。", verify: "远程页显示 helper ready、版本匹配、hostname/platform/arch/hardwareJson 可读。", fallback: "helper 未安装时先在远程页安装；权限不足时换 workdir 或使用普通用户目录，不要在登录节点长时间编译。" },
    { mode: "只写脚本", use: "第一次接入新集群、没有权限直接执行、或需要人工审阅脚本时。", verify: "检查 sync-up、submit、status、log-tail、sync-down 每条命令。", fallback: "脚本可手动复制到远程执行；回传后仍可让 AutoMD 刷新 artifact。" },
    { mode: "执行模式", use: "SSH/rsync 已验证，profile 稳定，脚本内容已审阅。", verify: "底部后台任务显示上传、提交、查询或回收进度；任务卡显示 job id/PID。", fallback: "网络中断不等于远程任务停止。重新测试连接后查 job id/PID 和日志，再决定 cancel 或 fetch。" }
  ];

  const remoteScriptRows: Array<{ script: string; purpose: string; success: string; commonIssue: string }> = [
    { script: "sync-up", purpose: "在远程创建 workdir，并把当前项目需要的输入、生成文件和脚本上传过去。", success: "能看到 rsync 统计和远程目录创建成功。", commonIssue: "权限不足、workdir 展开失败、rsync 缺失、网络中断或上传了过大的旧结果目录。" },
    { script: "submit", purpose: "对 SLURM/PBS/LSF 调用 sbatch/qsub/bsub；对 SSH 直连写入后台脚本并返回 job id 或 PID。", success: "GUI 保存 job id/PID，任务进入 queued 或 running。", commonIssue: "队列名、account、GPU 资源语法、module load、walltime 或可执行文件路径不符合目标机器。" },
    { script: "status", purpose: "查询调度器队列或 SSH 进程状态，并把输出解析成 queued/running/completed/failed/cancelled。", success: "状态、当前阶段、日志尾部和失败分类会更新。", commonIssue: "job id 不存在、任务已结束但日志未同步、调度器命令在登录节点不可用。" },
    { script: "log-tail", purpose: "读取远程 run 目录里的引擎日志尾部，用来判断进度、warning、fatal error 和性能。", success: "能看到 step、ns/day、能量/温压输出或引擎完成标记。", commonIssue: "日志路径和 run directory 不一致，任务还没写日志，或权限不足。" },
    { script: "cancel", purpose: "取消调度器任务或 kill SSH 直连 PID。", success: "状态变为 cancelled，并保留已有日志和 checkpoint 供回收。", commonIssue: "任务已经结束、PID 复用、调度器权限不足，或使用了错误 profile。" },
    { script: "sync-down", purpose: "把 runs、checkpoints、trajectories、analysis 和 reports 等结果回收到本机项目。", success: "本机项目出现远程日志、checkpoint、轨迹和分析文件；刷新 artifact 后可见。", commonIssue: "轨迹过大超时、远程目录填错、rsync 中断、空间不足或只回收了部分结果。" }
  ];

  const failureRows: Array<{ category: string; why: string; fix: string; where: TabId }> = [
    { category: "缺少当前项目或当前结构", why: "运行包、远程提交和报告都需要知道输入来自哪个项目、哪一个结构。", fix: "回项目页创建/切换项目，导入结构，并在结构索引中选中目标结构。顶部固定条必须显示当前项目和当前结构。", where: "overview" },
    { category: "结构文件无法读取或格式不支持", why: "路径不在项目内、文件过大、扩展名不支持、PDB/mmCIF 内容不完整，或 SDF/MOL2/SMILES 不是可直接可视化的结构。", fix: "重新用浏览按钮导入；PDB/mmCIF 用于结构视图，配体文件先作为输入保存，再用结构准备/参数化流程处理。", where: "overview" },
    { category: "缺少可执行文件", why: "PATH、AutoMD 管理目录、Conda 环境或手动登记路径里找不到 gmx、python、tleap、lmp、cp2k 等入口。", fix: "到引擎页点自动扫描；找不到就手动登记；开源引擎用一键部署；商业/受限引擎必须配置用户已有安装路径。", where: "engines" },
    { category: "平台不支持", why: "某些引擎只支持 Linux，或当前 macOS/Windows 没有对应原生二进制/GPU 后端。", fix: "平台不支持的卡片按钮会禁用；改用远程 Linux、WSL2、容器或生成源码/集群脚本。", where: "engines" },
    { category: "许可证缺失", why: "NAMD、AMBER pmemd、CHARMM、Desmond、ACEMD 等不能由 AutoMD 下载或授权。", fix: "先在你的授权环境完成安装和许可，再回引擎页手动登记可执行文件和授权状态。", where: "engines" },
    { category: "Python 科学环境缺包", why: "PDBFixer、MDAnalysis、RDKit、Open Babel、AmberTools 或 OpenMM 模块不在当前 Python 环境。", fix: "在流程页点一键安装/修复科学环境；如果 Conda/Mamba 不可用，先让 AutoMD 安装 Miniforge 或手动选择 Python。", where: "workflow" },
    { category: "拓扑/力场缺口", why: "非标准残基、配体、金属、辅因子、膜、糖基化或 force-field atom type 没有对应参数。", fix: "回流程页检查力场/水模型；对配体生成并复核 mol2/frcmod/XML/topology；必要时用外部工具如 CHARMM-GUI、AmberTools、CGenFF 或实验室标准流程。", where: "workflow" },
    { category: "坐标和拓扑不匹配", why: "原子数、原子名、残基名、链、盒子或 include 文件与坐标不一致。", fix: "重新生成结构准备和运行包；不要混用旧 top/tpr/prmtop/psf 和新坐标；检查原生文件的 include 路径。", where: "workflow" },
    { category: "参数映射 unsupported/需复核", why: "统一 GUI 参数无法无损映射到某个引擎原生字段，或模板只能给建议。", fix: "展开流程页高级参数映射，打开生成的原生文件编辑；正式生产前用引擎自己的预处理命令验证。", where: "workflow" },
    { category: "GPU 不可用", why: "没有相关显卡、驱动缺失、CUDA/ROCm/OpenCL/Metal 与引擎不匹配，或引擎编译时没有启用 GPU 后端。", fix: "先看底部 GPU 状态；NVIDIA 才处理 CUDA，AMD Linux 才处理 ROCm，Apple 使用 Metal 但并非所有引擎支持；不确定先改 CPU 或远程/HPC。", where: "engines" },
    { category: "MPI 或多进程失败", why: "mpirun、MPI ABI、集群 module、进程数、hostfile 或 GPU-aware MPI 不匹配。", fix: "单机先关闭 MPI 跑小测试；HPC 上用集群推荐 module；源码构建时 MPI 编译器和运行时必须一致。", where: "engines" },
    { category: "数值发散 / NaN / LINCS / SHAKE", why: "初始结构冲突、最小化不足、timestep 过大、约束失败、温压耦合不合适或参数错误。", fix: "降低 timestep，增加最小化，先短 NVT/NPT，检查重叠原子、配体参数、质子化、盒子大小和温压设置。", where: "workflow" },
    { category: "磁盘或权限问题", why: "项目目录、远程 workdir、安装 prefix 或 build-recipes 不可写，磁盘空间不足，路径有空格或权限被系统拦截。", fix: "换到用户目录或 scratch；检查剩余空间；AutoMD 管理环境优先使用无空格目录 ~/.automd/engines；避免写系统目录。", where: "overview" },
    { category: "远程连接失败", why: "host/port/user/password/key 错误，VPN/防火墙阻断，known_hosts 变化，工作目录不可写，或 helper 未安装。", fix: "远程页先测试连接；修正 profile；安装/检查 helper；没有调度器的云主机选择 SSH 直连。", where: "remote" },
    { category: "调度器失败", why: "queue/partition、account、GPU 资源语法、walltime、module load 或 submit/status/cancel 命令不符合集群政策。", fix: "先只写脚本，检查 submit.slurm/pbs/lsf；拿第一条调度器错误修改 profile；必要时问管理员。", where: "remote" },
    { category: "rsync 上传/回收失败", why: "远程路径展开失败、权限不足、连接中断、rsync 未安装，或轨迹太大导致超时。", fix: "确认本地和远程都有 rsync；workdir 使用绝对路径或 $USER；先回收 runs/checkpoints/analysis/reports，再处理大轨迹。", where: "remote" },
    { category: "远程 helper 版本或权限问题", why: "远程 helper 未安装、版本过旧、安装目录不可写、Shell/PowerShell 不兼容，或 SSH 用户没有执行权限。", fix: "远程页重新检测/安装 helper；把 helper 装到用户可写 workdir；Windows 远程确认 OpenSSH Server 和 PowerShell 可用。", where: "remote" },
    { category: "构建失败", why: "缺 cmake/git/compiler、网络下载失败、prefix 不可写、GPU/MPI/PLUMED 选项不匹配或源码版本不支持。", fix: "在引擎高级部署里先 Dry run/只写脚本；查看 build-combined.log 第一条编译错误；调整 recipe 或改用 Conda/容器/远程。", where: "engines" },
    { category: "插件失败", why: "manifest 字段缺失、entrypoint 指向插件目录外、配置 JSON 不合法、权限不足或直接运行未确认。", fix: "回插件页查看 validation warning；修 manifest/config；默认用沙盒运行；未知插件不要直接运行。", where: "plugins" },
    { category: "运行 manifest 缺失", why: "运行包尚未生成、run 目录被删除、任务还没进入执行阶段，或旧版本项目没有 automd-run-manifest.json。", fix: "重新 Dry run 或启动任务生成 run 目录；不要手动删除 runs/<engine-plan>；刷新 artifact 后报告会重新引用 manifest。", where: "run" },
    { category: "Artifact 索引和文件夹不一致", why: "你移动/删除了项目文件，远程结果还没回收，或者 SQLite 缓存记录还是上一次扫描。", fix: "先打开项目文件夹确认真实文件，再在运行页刷新 artifact；远程任务先 sync-down，再刷新和导出报告。", where: "run" },
    { category: "分析图表为空或无法解析", why: "分析 CSV/XVG 不存在、列不是数字、selection 没选到原子、轨迹/拓扑不匹配，或二进制轨迹只登记了 metadata。", fix: "生成并运行 MDAnalysis 分析包；检查 topology、trajectory、selection；确认输出 CSV/XVG 有数值列，然后重新解析。", where: "run" },
    { category: "轨迹索引失败", why: "文件不是受支持文本轨迹、轨迹过大、帧格式不规则、maxBytes 太小，或 XTC/TRR/DCD/NetCDF/GSD 需要二进制解码器。", fix: "文本轨迹降低抽样或检查格式；二进制轨迹用 MDAnalysis 分析包生成小型 CSV；不要把完整大轨迹直接塞进前端预览。", where: "run" },
    { category: "原生文件编辑被拒绝", why: "路径不在项目允许目录内、文件超过大小上限、扩展名不是安全文本类型，或试图编辑轨迹/二进制文件。", fix: "只编辑 generated、runs、remote、build-recipes、analysis、reports 内的 .mdp/.mdin/.conf/.inp/.py/.sh/.json/.yaml/.md 等文本文件。", where: "run" },
    { category: "报告缺图或缺结果", why: "artifact 未刷新、分析 CSV/XVG 未生成、轨迹只登记了 metadata，或当前项目切错。", fix: "回运行页刷新 artifact、索引轨迹、生成分析包；确认当前项目固定条；再回报告页导出。", where: "run" }
  ];

  function syncGuideOutline(sectionId: string) {
    const outline = guideOutlineRef.current;
    const item = outline?.querySelector<HTMLElement>(`[data-outline-id="${sectionId}"]`);
    item?.scrollIntoView({ block: "nearest" });
  }

  function updateActiveGuideSection() {
    const content = guideContentRef.current;
    if (!content) return;
    const sections = guideSections
      .map((section) => document.getElementById(section.id))
      .filter((section): section is HTMLElement => Boolean(section));
    if (!sections.length) return;

    const contentRect = content.getBoundingClientRect();
    const bottomGap = content.scrollHeight - content.scrollTop - content.clientHeight;
    let nextId = sections[0].id;

    if (bottomGap < 24) {
      nextId = sections[sections.length - 1].id;
    } else {
      for (const section of sections) {
        const top = section.getBoundingClientRect().top - contentRect.top;
        if (top <= 110) {
          nextId = section.id;
        } else {
          break;
        }
      }
    }

    setActiveGuideSection((current) => {
      if (current !== nextId) {
        syncGuideOutline(nextId);
      }
      return current === nextId ? current : nextId;
    });
  }

  function scrollGuideTo(sectionId: string) {
    const target = document.getElementById(sectionId);
    if (!target) return;
    target.scrollIntoView({ behavior: "smooth", block: "start" });
    setActiveGuideSection(sectionId);
    syncGuideOutline(sectionId);
  }

  useEffect(() => {
    updateActiveGuideSection();
  }, [guideSections]);

  return (
    <div className="guide-layout">
      <aside className="guide-outline" ref={guideOutlineRef} aria-label="使用指引大纲">
        <div className="guide-outline-card">
          <div className="guide-outline-title">
            <strong>本页大纲</strong>
            <span>{guideSections.length} 个章节</span>
          </div>
          <nav className="guide-outline-list">
            {guideSections.map((section, index) => (
              <button
                type="button"
                key={section.id}
                data-outline-id={section.id}
                className={`guide-outline-item ${activeGuideSection === section.id ? "active" : ""}`}
                onClick={() => scrollGuideTo(section.id)}
              >
                <span>{index + 1}</span>
                <strong>{section.label}</strong>
              </button>
            ))}
          </nav>
        </div>
      </aside>

      <div className="guide-content" ref={guideContentRef} onScroll={updateActiveGuideSection}>
      <div className="guide-page">
      <section id="guide-quickstart" data-guide-section className="panel span-3 guide-quickstart">
        <div className="panel-title-row">
          <div>
            <h3>快速开始（8 步）</h3>
            <p className="muted">这是给第一次使用者的最短路线。每一步都能直接跳到对应页面，先跑短测试，再放大到正式模拟。</p>
          </div>
        </div>
        <ol className="quickstart-list">
          {quickStartSteps.map((step, index) => (
            <li className="quickstart-step" key={`${step.title}-${index}`}>
              <span className="quickstart-num">{index + 1}</span>
              <div className="quickstart-body">
                <strong>{step.title}</strong>
                <p>{step.desc}</p>
              </div>
              <button type="button" className="quickstart-cta" onClick={() => setActiveTab(step.tab)}>
                {step.cta}
              </button>
            </li>
          ))}
        </ol>
      </section>

      <section id="guide-concepts" data-guide-section className="panel span-3">
        <div className="panel-title-row">
          <div>
            <h3>先弄懂这些词</h3>
            <p className="muted">AutoMD 会把复杂的引擎文件藏到后台，但这些词决定你知道自己正在操作什么。</p>
          </div>
        </div>
        <dl className="definition-list guide-glossary">
          {conceptRows.map((row) => (
            <div key={row.term}>
              <dt>{row.term}</dt>
              <dd>{row.meaning} {row.where}</dd>
            </div>
          ))}
        </dl>
      </section>

      <section id="guide-full-flow" data-guide-section className="panel span-3">
        <div className="panel-title-row">
          <div>
            <h3>完整示例：小型蛋白水溶液模拟</h3>
            <p className="muted">下面是一条可以照着走的完整路线。目标是先跑一个 1 ns 的短测试，确认输入、引擎和分析闭环都正常。</p>
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

      <section id="guide-parameters" data-guide-section className="panel span-3">
        <div className="panel-title-row">
          <div>
            <h3>第一次模拟怎么填参数</h3>
            <p className="muted">这些不是万能科学结论，而是软件入门时比较稳的默认起点。正式课题要按体系和文献复核。</p>
          </div>
          <button type="button" onClick={() => setActiveTab("workflow")}>打开流程页</button>
        </div>
        <div className="guide-table">
          <div className="guide-table-head">项目</div>
          <div className="guide-table-head">入门填法</div>
          <div className="guide-table-head">什么时候要复核</div>
          {parameterRows.map((row) => (
            <Fragment key={row.item}>
              <div><strong>{row.item}</strong></div>
              <div>{row.beginner}</div>
              <div>{row.note}</div>
            </Fragment>
          ))}
        </div>
      </section>

      <section id="guide-science" data-guide-section className="panel span-3">
        <div className="panel-title-row">
          <div>
            <h3>结构准备与分析环境是什么</h3>
            <p className="muted">它不是让你学习 Python，而是 AutoMD 为结构准备和分析自动管理的一套科学工具。当前引擎不需要的项会显示“不适用”。</p>
          </div>
          <button type="button" onClick={() => setActiveTab("workflow")}>打开流程页</button>
        </div>
        <div className="guide-table">
          <div className="guide-table-head">工具</div>
          <div className="guide-table-head">作用</div>
          <div className="guide-table-head">什么时候需要</div>
          {scienceRows.map((row) => (
            <Fragment key={row.tool}>
              <div><strong>{row.tool}</strong></div>
              <div>{row.role}</div>
              <div>{row.needed}</div>
            </Fragment>
          ))}
        </div>
        <div className="guide-section">
          <h4>按钮应该怎么用</h4>
          <ol className="guide-steps compact">
            <li>先看状态：可用表示软件已经能调用；需安装表示当前引擎需要但没找到；不适用表示这个工具不参与当前引擎流程。</li>
            <li>点“自动查找”会扫描 PATH、AutoMD 管理目录和常见安装位置。</li>
            <li>点“手动查找”会打开系统文件选择器，让你选择 Python 或可执行文件。</li>
            <li>点“一键安装”会创建或修复 AutoMD 管理的 automd-science 环境，优先从 conda-forge 安装，不写入系统 Python。</li>
            <li>推荐环境的 environment.yml 只是高级预览，普通用户不需要手动复制。安装失败时再展开它，看缺的是包管理器、网络、权限还是具体包。</li>
          </ol>
        </div>
      </section>

      <section id="guide-pages" data-guide-section className="panel span-3">
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

      <section id="guide-directories" data-guide-section className="panel span-3">
        <div className="panel-title-row">
          <div>
            <h3>项目目录和文件都放在哪里</h3>
            <p className="muted">AutoMD 的核心原则是“软件状态可重建，科学文件留在项目目录”。不知道文件去哪了时，先按这个表查。</p>
          </div>
          <button type="button" onClick={() => setActiveTab("overview")}>打开项目页</button>
        </div>
        <div className="guide-table">
          <div className="guide-table-head">目录</div>
          <div className="guide-table-head">保存什么</div>
          <div className="guide-table-head">用户应该怎么用</div>
          {projectDirectoryRows.map((row) => (
            <Fragment key={row.path}>
              <div><strong className="mono">{row.path}</strong></div>
              <div>{row.purpose}</div>
              <div>{row.userAction}</div>
            </Fragment>
          ))}
        </div>
      </section>

      <section id="guide-reproducibility" data-guide-section className="panel span-3">
        <div className="panel-title-row">
          <div>
            <h3>索引、缓存和复现记录</h3>
            <p className="muted">这些记录解释“软件为什么知道某个引擎可用、某个结果在哪里、某次任务怎么跑”。它们服务于复现和排错，不要求用户手工维护。</p>
          </div>
        </div>
        <div className="guide-table guide-table-4">
          <div className="guide-table-head">记录</div>
          <div className="guide-table-head">作用</div>
          <div className="guide-table-head">保存位置</div>
          <div className="guide-table-head">用户怎么处理</div>
          {reproducibilityRows.map((row) => (
            <Fragment key={row.item}>
              <div><strong>{row.item}</strong></div>
              <div>{row.purpose}</div>
              <div>{row.where}</div>
              <div>{row.userAction}</div>
            </Fragment>
          ))}
        </div>
      </section>

      <section id="guide-structure-import" data-guide-section className="panel span-3">
        <div className="panel-title-row">
          <div>
            <h3>结构导入格式怎么选</h3>
            <p className="muted">导入成功只代表文件进入项目；真正能不能生产模拟，还取决于拓扑、力场、配体、质子化和结构准备。</p>
          </div>
          <button type="button" onClick={() => setActiveTab("overview")}>去导入结构</button>
        </div>
        <div className="guide-table">
          <div className="guide-table-head">格式</div>
          <div className="guide-table-head">适合什么</div>
          <div className="guide-table-head">必须注意</div>
          {structureInputRows.map((row) => (
            <Fragment key={row.format}>
              <div><strong>{row.format}</strong></div>
              <div>{row.use}</div>
              <div>{row.caution}</div>
            </Fragment>
          ))}
        </div>
      </section>

      <section id="guide-run-features" data-guide-section className="panel span-3">
        <div className="panel-title-row">
          <div>
            <h3>运行页每个功能怎么判断能不能用</h3>
            <p className="muted">运行页不是只有“开始”。它还负责生成包、复核原生文件、解析日志、恢复 checkpoint、索引轨迹和整理分析。</p>
          </div>
          <button type="button" onClick={() => setActiveTab("run")}>打开运行页</button>
        </div>
        <div className="guide-table guide-table-4">
          <div className="guide-table-head">功能</div>
          <div className="guide-table-head">什么时候用</div>
          <div className="guide-table-head">怎么操作</div>
          <div className="guide-table-head">风险/误区</div>
          {runFeatureRows.map((row) => (
            <Fragment key={row.feature}>
              <div><strong>{row.feature}</strong></div>
              <div>{row.when}</div>
              <div>{row.how}</div>
              <div>{row.risk}</div>
            </Fragment>
          ))}
        </div>
      </section>

      <section id="guide-status" data-guide-section className="panel span-3">
        <div className="panel-title-row">
          <div>
            <h3>固定当前项目和底部状态栏</h3>
            <p className="muted">这两个区域不属于某一次参数设置，而是帮助你随时确认“现在操作的是哪个项目、当前机器适合怎么跑”。</p>
          </div>
        </div>
        <dl className="definition-list">
          <div><dt>当前项目</dt><dd>在项目、流程、运行、远程和报告页顶部固定显示。滚动页面时仍能看到项目名、状态、目录，并可以快速切换项目或打开项目文件夹。</dd></div>
          <div><dt>GPU 状态</dt><dd>软件启动时会先识别本机显卡类型，再判断 CUDA、ROCm 或 macOS Metal 是否相关。只有 NVIDIA 才提示 CUDA/NVIDIA 需安装，只有支持 ROCm 的 AMD/Linux 环境才提示 ROCm 需安装；其他显卡会显示“不适用”。</dd></div>
          <div><dt>后台任务</dt><dd>自动查找、手动选择、自动安装、写脚本和编译时，底部 GPU 状态左侧会显示“后台任务 X 个”和平均进度。悬停可查看每个任务的名称、状态、百分比和当前步骤。</dd></div>
          <div><dt>悬停提示</dt><dd>鼠标放到底部 GPU 状态上，会显示不可用原因，例如未检测到 GPU 工具、平台/引擎不支持，或预览环境无法访问硬件。</dd></div>
          <div><dt>结构视图</dt><dd>新项目默认为空，导入结构后才会加载 Mol*。如果结构路径无效或格式不支持，视图会保留错误提示而不是显示假的分子图。</dd></div>
        </dl>
      </section>

      <section id="guide-performance" data-guide-section className="panel span-3">
        <div className="panel-title-row">
          <div>
            <h3>左下角设置和性能配置怎么用</h3>
            <p className="muted">设置不是给开发者看的。它帮助普通用户控制本机不要被全占用，也帮助 AutoMD 生成更合理的本地和远程资源字段。</p>
          </div>
        </div>
        <div className="guide-table guide-table-4">
          <div className="guide-table-head">设置项</div>
          <div className="guide-table-head">含义</div>
          <div className="guide-table-head">推荐填法</div>
          <div className="guide-table-head">注意事项</div>
          {performanceRows.map((row) => (
            <Fragment key={row.setting}>
              <div><strong>{row.setting}</strong></div>
              <div>{row.meaning}</div>
              <div>{row.recommendation}</div>
              <div>{row.warning}</div>
            </Fragment>
          ))}
        </div>
      </section>

      <section id="guide-engines" data-guide-section className="panel span-3">
        <div className="panel-title-row">
          <div>
            <h3>引擎配置</h3>
            <p className="muted">先把引擎登记到“引擎”页；缺少依赖时在对应引擎卡片里打开高级部署/编译生成安装或构建脚本；平台不合适时走远程。</p>
          </div>
          <button type="button" onClick={() => setActiveTab("engines")}>打开引擎页</button>
        </div>
        <div className="guide-section">
          <h4>不知道选哪个时先按这个规则</h4>
          <dl className="definition-list">
            <div><dt>蛋白/核酸/配体的常规生物分子模拟</dt><dd>优先 GROMACS。它适合完整闭环、速度快、资料多，也最适合作为 AutoMD 的默认入门路线。</dd></div>
            <div><dt>教学、快速验证、自定义 Python 逻辑</dt><dd>选 OpenMM。它对 Python 友好，适合先确认结构和参数思路，但大型生产任务仍要看硬件和体系大小。</dd></div>
            <div><dt>Amber 拓扑、配体参数、cpptraj 分析</dt><dd>装 AmberTools。它既可以独立做输入生态，也能给其他引擎准备配体和分析材料。</dd></div>
            <div><dt>材料、粗粒化或非生物分子模型</dt><dd>看 LAMMPS、CP2K、HOOMD-blue、DL_POLY。复杂模型通常需要保留原生 input 文件编辑。</dd></div>
            <div><dt>实验室已经有商业/受限引擎授权</dt><dd>使用 NAMD、AMBER pmemd、CHARMM、Desmond 或 ACEMD 入口。AutoMD 只保存路径和生成运行入口，不下载这些引擎。</dd></div>
            <div><dt>桌面电脑跑不动或平台不支持</dt><dd>不要硬装。去“远程”页配置 SSH/HPC，或在“引擎”页对应卡片的高级部署/编译中生成 Linux、容器或集群脚本。</dd></div>
          </dl>
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

      <section id="guide-deploy-build" data-guide-section className="panel span-2">
        <div className="panel-title-row">
          <div>
            <h3>引擎安装、部署和编译</h3>
            <p className="muted">能一键安装的会装到 AutoMD 管理的无空格目录；系统服务、GPU 驱动、容器虚拟机和 HPC 调度器会给出明确安装方式。</p>
          </div>
          <button type="button" onClick={() => setActiveTab("engines")}>打开引擎页</button>
        </div>
        <div className="guide-section">
          <h4>推荐操作顺序</h4>
          <ol className="guide-steps compact">
            <li>本机运行环境里，Conda、Mamba、MPI 和 PLUMED 这类 AutoMD 能管理的项目会显示“一键安装”。</li>
            <li>Docker、Podman、Apptainer、CUDA/ROCm 驱动、SLURM/PBS/LSF 这类系统或集群工具不会显示假安装按钮；缺失时点“查看安装方式”，已有替代工具时显示“不适用”。</li>
            <li>GROMACS、OpenMM、AmberTools、LAMMPS、CP2K 和 HOOMD-blue 的“一键安装”会实际执行：没有 Conda 时先下载 Miniforge，再用 conda-forge 创建隔离环境。</li>
            <li>Miniforge 和引擎环境会安装到 AutoMD 管理的无空格目录，例如 <span className="mono">~/.automd/engines</span>；这避免 macOS <span className="mono">Application Support</span> 空格导致安装器失败。</li>
            <li>需要源码/GPU/MPI/PLUMED 特殊构建时，先 Dry run，确认命令、下载源、写入目录、权限、prefix、GPU/MPI/PLUMED 选项。</li>
            <li>选择“只写脚本”时，脚本会落盘；你可以拿到 WSL2、Linux 服务器或 HPC 登录节点上再运行。</li>
            <li>只有在本机环境明确可控时才选择“执行构建”。执行后看日志路径、失败分类和生成的可执行文件。</li>
          </ol>
          <h4>常见构建选项</h4>
          <dl className="definition-list">
            <div><dt>MPI</dt><dd>多节点或多进程任务启用。桌面单机测试可先关闭，HPC 建议启用。</dd></div>
            <div><dt>GPU</dt><dd>CUDA、ROCm、OpenCL、Metal、SYCL 能力按显卡、引擎和平台共同判断。Apple/Intel/无独显机器不需要安装 CUDA 或 ROCm；NVIDIA 机器关注 CUDA，AMD Linux 机器关注 ROCm。</dd></div>
            <div><dt>PLUMED</dt><dd>增强采样常见于 GROMACS/LAMMPS/CP2K 等，必须匹配引擎版本重新编译或动态链接。</dd></div>
            <div><dt>Prefix</dt><dd>优先使用用户目录、Conda 环境或容器路径。系统目录需要管理员权限，不建议默认写入。</dd></div>
            <div><dt>容器</dt><dd>开源引擎可生成 Docker/Podman recipe；商业/受限引擎只能在用户已有授权环境中配置路径。</dd></div>
          </dl>
          <h4>自动安装覆盖范围</h4>
          <dl className="definition-list">
            <div><dt>Conda / Mamba</dt><dd>自动下载 Miniforge 并安装到 AutoMD 管理目录的 <span className="mono">engines/_tools/miniforge3</span>，不写系统目录。Mamba 会在这个内置 Miniforge 中额外安装 mamba 包。</dd></div>
            <div><dt>MPI / PLUMED</dt><dd>通过 conda-forge 创建 AutoMD 管理的隔离环境，安装 openmpi 或 plumed，并把生成的可执行文件路径回填到本机运行环境。</dd></div>
            <div><dt>开源引擎</dt><dd>GROMACS、AmberTools、LAMMPS、CP2K 这类 CLI 引擎会保存可执行文件路径；OpenMM 和 HOOMD-blue 是 Python 模块型引擎，会保存可导入模块的 Python 路径。</dd></div>
            <div><dt>容器工具</dt><dd>Docker Desktop、Podman、Apptainer 涉及系统服务、虚拟机或 Linux/HPC 环境。AutoMD 可以生成 recipe，但不会把系统级安装伪装成一键完成。</dd></div>
            <div><dt>GPU 驱动</dt><dd>CUDA/NVIDIA 驱动、ROCm/HIP 驱动必须匹配显卡、系统版本和内核/驱动。AutoMD 只在显卡相关时提示需安装；无关时显示“不适用”。</dd></div>
            <div><dt>HPC 调度器</dt><dd>SLURM/PBS/LSF 客户端通常由集群 module 或登录节点提供。桌面端缺失时应配置远程 profile，而不是在本机强行安装调度器。</dd></div>
            <div><dt>商业/受限引擎</dt><dd>NAMD、AMBER pmemd、CHARMM、Desmond、ACEMD 等需要用户已有许可或授权环境。AutoMD 只保存路径、检测授权状态并生成运行入口。</dd></div>
          </dl>
          <h4>不能一键安装的工具怎么处理</h4>
          <dl className="definition-list">
            <div><dt>Docker</dt><dd>macOS/Windows 安装 Docker Desktop，启动后等待状态变成 running；Linux 用发行版包管理器安装 Docker Engine，并确认当前用户有运行权限。AutoMD 只需要能找到 <span className="mono">docker</span> 命令。</dd></div>
            <div><dt>Podman</dt><dd>macOS/Windows 推荐 Podman Desktop 或 Homebrew/winget 安装；macOS 安装后还需要初始化并启动 podman machine。Linux 可用发行版包管理器安装。AutoMD 只检测 <span className="mono">podman</span> 命令，Docker 已可用时 Podman 会显示不适用。</dd></div>
            <div><dt>Apptainer</dt><dd>主要用于 Linux/HPC。macOS/Windows 桌面不建议本机安装；应在远程 Linux、WSL2、HPC 登录节点或管理员提供的 module 环境中使用。</dd></div>
            <div><dt>SLURM / PBS / LSF</dt><dd>这些不是普通桌面软件，而是集群调度系统。通常只在 HPC 登录节点有 <span className="mono">sbatch</span>、<span className="mono">qsub</span>、<span className="mono">bsub</span>。本机缺失不影响你在“远程”页配置 profile。</dd></div>
            <div><dt>CUDA / ROCm</dt><dd>只在显卡和平台相关时需要。Apple Silicon/Intel macOS 不需要 CUDA 或 ROCm；NVIDIA Linux/Windows 关注 CUDA，AMD Linux 关注 ROCm。</dd></div>
          </dl>
        </div>
      </section>

      <section id="guide-platform" data-guide-section className="panel">
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

      <section id="guide-remote" data-guide-section className="panel span-3">
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
        <div className="guide-section">
          <h4>远程模式怎么选</h4>
          <div className="guide-table guide-table-4">
            <div className="guide-table-head">模式</div>
            <div className="guide-table-head">适合场景</div>
            <div className="guide-table-head">怎么确认可用</div>
            <div className="guide-table-head">失败时怎么办</div>
            {remoteModeRows.map((row) => (
              <Fragment key={row.mode}>
                <div><strong>{row.mode}</strong></div>
                <div>{row.use}</div>
                <div>{row.verify}</div>
                <div>{row.fallback}</div>
              </Fragment>
            ))}
          </div>
        </div>
        <div className="guide-section">
          <h4>远程执行闭环</h4>
          <p className="muted">远程页不是只导出命令。它把连接、上传、提交、查状态、看日志、取消和回收串成同一套可审阅流程；高级用户仍可打开脚本手动执行。</p>
          <div className="guide-table guide-table-4">
            <div className="guide-table-head">步骤/脚本</div>
            <div className="guide-table-head">做什么</div>
            <div className="guide-table-head">成功标志</div>
            <div className="guide-table-head">常见问题</div>
            {remoteScriptRows.map((row) => (
              <Fragment key={row.script}>
                <div><strong>{row.script}</strong></div>
                <div>{row.purpose}</div>
                <div>{row.success}</div>
                <div>{row.commonIssue}</div>
              </Fragment>
            ))}
          </div>
        </div>
      </section>

      <section id="guide-run-report" data-guide-section className="panel span-3">
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
            <div><dt>预期输出</dt><dd>这是 AutoMD 用来识别文件的路径约定，例如 generated 输入、runs 日志、checkpoints、trajectories、analysis 和 reports。普通用户只需要在运行完成后看 artifact 列表，不需要在流程页手动改这些路径。</dd></div>
            <div><dt>报告</dt><dd>报告应包含环境、参数、命令、日志、分析图表、artifact、checkpoint 和可复现记录。</dd></div>
          </dl>
        </section>

      <section id="guide-plugins" data-guide-section className="panel span-3">
        <div className="panel-title-row">
          <div>
            <h3>插件管理和安全使用</h3>
            <p className="muted">插件是扩展能力，不是普通数据文件。先看 manifest、来源、权限和联动位置，再启用动作。</p>
          </div>
          <button type="button" onClick={() => setActiveTab("plugins")}>打开插件页</button>
        </div>
        <div className="guide-table">
          <div className="guide-table-head">操作</div>
          <div className="guide-table-head">说明</div>
          <div className="guide-table-head">安全检查</div>
          {pluginGuideRows.map((row) => (
            <Fragment key={row.action}>
              <div><strong>{row.action}</strong></div>
              <div>{row.description}</div>
              <div>{row.safety}</div>
            </Fragment>
          ))}
        </div>
      </section>

      <section id="guide-failures" data-guide-section className="panel span-3">
        <div className="panel-title-row">
          <div>
            <h3>常见报错、原因和解决方案</h3>
            <p className="muted">遇到错误时不要只看红色提示。先判断它属于环境、输入、参数、远程、构建还是结果整理，再去对应页面处理。</p>
          </div>
        </div>
        <div className="guide-section">
          <h4>推荐排查顺序</h4>
          <ol className="guide-steps compact">
            <li>先看顶部当前项目和当前结构，确认没有在错误项目里操作。</li>
            <li>再看引擎/科学环境是否可用，许可证和平台是否匹配。</li>
            <li>然后看结构、拓扑、力场、配体和原生参数文件。</li>
            <li>接着看资源：CPU/GPU/MPI、磁盘、权限、远程 workdir 和调度器。</li>
            <li>最后处理数值稳定性：timestep、最小化、约束、温压耦合和初始结构冲突。</li>
          </ol>
        </div>
        <div className="guide-table guide-table-4">
          <div className="guide-table-head">报错类别</div>
          <div className="guide-table-head">为什么会这样</div>
          <div className="guide-table-head">解决方案</div>
          <div className="guide-table-head">去哪里处理</div>
          {failureRows.map((row) => (
            <Fragment key={row.category}>
              <div><strong>{row.category}</strong></div>
              <div>{row.why}</div>
              <div>{row.fix}</div>
              <div><button type="button" onClick={() => setActiveTab(row.where)}>打开对应页面</button></div>
            </Fragment>
          ))}
        </div>
      </section>
      </div>
      </div>
    </div>
  );
}

function CurrentProjectBanner({
  project,
  activeStructure,
  openProjectFolder
}: {
  project: ProjectSummary | null;
  activeStructure: StructureEntry | null;
  openProjectFolder: (path?: string | null) => void;
}) {
  return (
    <section className="current-project-sticky" aria-label="current project">
      <div className="current-project-main">
        <span className="status-dot ready" />
        <div>
          <small>当前项目</small>
          <div className="current-project-name-row">
            <strong>{project?.name ?? "尚未选择项目"}</strong>
            {activeStructure ? (
              <span className="current-structure-badge">
                当前结构：{activeStructure.name}
              </span>
            ) : null}
          </div>
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

function AppStatusBar({
  diagnostics,
  backgroundTasks,
  notifications,
  onReviewProblems,
  bgTasksOpen,
  onToggleBgTasks
}: {
  diagnostics: RuntimeDiagnostics | null;
  backgroundTasks: BackgroundTask[];
  notifications: AppNotification[];
  onReviewProblems: () => void;
  bgTasksOpen: boolean;
  onToggleBgTasks: () => void;
}) {
  const gpu = diagnostics?.gpu;
  const problems = notifications.filter((item) => item.persistent);
  const hasError = problems.some((item) => item.severity === "error");
  const problemTitle = problems.length
    ? `${problems.map((item) => `${NOTIFICATION_ICON[item.severity]} ${item.message}`).join("\n")}\n\n点击重新显示或定位这些问题`
    : "";
  const runningTasks = backgroundTasks.filter((task) => task.status === "running");
  const taskProgress = runningTasks.length
    ? Math.round(runningTasks.reduce((sum, task) => sum + task.progress, 0) / runningTasks.length)
    : 0;
  const taskTitle = backgroundTasks.length
    ? backgroundTasks
        .map((task) => `${task.label}：${task.status}，${Math.round(task.progress)}%｜${task.detail}`)
        .join("\n")
    : "当前没有后台任务";
  const title = gpu
    ? `${gpu.reason}\n${gpu.detail}\n检查时间：${new Date(gpu.checkedAt).toLocaleString()}`
    : "正在检测 GPU 状态";

  return (
    <footer className="app-statusbar">
      <div className="statusbar-left">
        <span>AutoMD</span>
        {backgroundTasks.length ? (
          <button
            type="button"
            className={`background-task-status ${bgTasksOpen ? "active" : ""}`}
            title={taskTitle}
            onClick={onToggleBgTasks}
          >
            {runningTasks.length ? (
              <span className="task-spinner" />
            ) : (
              <span className="bgtask-done-dot" />
            )}
            <span>后台任务 {runningTasks.length || backgroundTasks.length} 个</span>
            {runningTasks.length ? <small>{taskProgress}%</small> : null}
          </button>
        ) : null}
      </div>
      <div className="statusbar-right">
        {problems.length ? (
          <button
            type="button"
            className={`statusbar-problems ${hasError ? "" : "warn-only"}`}
            title={problemTitle}
            onClick={onReviewProblems}
          >
            <span className="statusbar-problem-dot" />
            <span>{problems.length} 个未处理问题</span>
          </button>
        ) : null}
        <div className={`gpu-status ${gpu?.available ? "available" : "unavailable"}`} title={title}>
          <span className="gpu-status-dot" />
          <span>{gpu?.label ?? "GPU 状态检测中"}</span>
          {gpu && !gpu.label.includes("模式") ? (
            <small>{gpu.mode === "gpu" ? "GPU 模式" : "CPU 模式"}</small>
          ) : null}
        </div>
      </div>
    </footer>
  );
}

const BACKGROUND_TASK_STATUS_TEXT: Record<BackgroundTaskStatus, string> = {
  running: "进行中",
  completed: "已完成",
  failed: "失败"
};

/** Popover (above the status bar) listing the background-task queue + progress. */
function BackgroundTaskPanel({
  tasks,
  onClose
}: {
  tasks: BackgroundTask[];
  onClose: () => void;
}) {
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        onClose();
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  return (
    <div className="bgtask-popover" role="dialog" aria-label="后台任务">
      <div className="bgtask-head">
        <strong>后台任务</strong>
        <button type="button" className="toast-close" onClick={onClose} aria-label="关闭">×</button>
      </div>
      {tasks.length === 0 ? (
        <p className="bgtask-empty">当前没有后台任务。</p>
      ) : (
        <ul className="bgtask-list">
          {tasks.map((task) => (
            <li className={`bgtask-item ${task.status}`} key={task.id}>
              <div className="bgtask-item-head">
                <span className="bgtask-item-label">{task.label}</span>
                <span className="bgtask-item-status">
                  {BACKGROUND_TASK_STATUS_TEXT[task.status]}
                  {task.status === "running" ? ` · ${Math.round(task.progress)}%` : ""}
                </span>
              </div>
              {task.status === "running" ? (
                <div className="bgtask-bar">
                  <div className="bgtask-bar-fill" style={{ width: `${Math.min(100, Math.max(4, task.progress))}%` }} />
                </div>
              ) : null}
              <small>{task.detail}</small>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/**
 * macOS-style toast stack (bottom-right). Errors/warnings persist as tracked
 * "问题" (close minimizes them into the status bar); success/reminders auto-fade.
 * Fixed per-severity glyph + label, one-click fix, guide link, and 忽略问题.
 */
function NotificationStack({
  notifications,
  flash,
  onDismiss,
  onIgnore,
  onGuide
}: {
  notifications: AppNotification[];
  flash: boolean;
  onDismiss: (id: string) => void;
  onIgnore: (id: string) => void;
  onGuide: () => void;
}) {
  const visible = notifications.filter((item) => item.visible);
  if (!visible.length) {
    return null;
  }
  return (
    <div className={`toast-stack ${flash ? "flash" : ""}`} role="region" aria-label="通知">
      {visible.map((item) => (
        <div className={`toast toast-${item.severity}`} key={item.id} role="alert">
          <span className="toast-icon" aria-hidden="true">{NOTIFICATION_ICON[item.severity]}</span>
          <div className="toast-body">
            <strong>{item.title}</strong>
            <p>{item.message}</p>
            {item.action || item.guide || item.persistent ? (
              <div className="toast-actions">
                {item.action ? (
                  <button
                    type="button"
                    className="toast-fix"
                    onClick={() => {
                      item.action?.run();
                      onIgnore(item.id);
                    }}
                  >
                    {item.action.label}
                  </button>
                ) : null}
                {item.guide ? (
                  <button
                    type="button"
                    className="toast-link"
                    onClick={() => {
                      onGuide();
                      onDismiss(item.id);
                    }}
                  >
                    查看指引
                  </button>
                ) : null}
                {item.persistent ? (
                  <button type="button" className="toast-ignore" onClick={() => onIgnore(item.id)}>
                    忽略问题
                  </button>
                ) : null}
              </div>
            ) : null}
          </div>
          <button
            type="button"
            className="toast-close"
            onClick={() => onDismiss(item.id)}
            aria-label="关闭"
            title="关闭（仍计入未处理问题）"
          >
            ×
          </button>
        </div>
      ))}
    </div>
  );
}

function SettingsModal({
  theme,
  setTheme,
  diagnostics,
  performancePreferences,
  updatePerformancePreferences,
  plan,
  onClose
}: {
  theme: ThemeMode;
  setTheme: (value: ThemeMode) => void;
  diagnostics: RuntimeDiagnostics | null;
  performancePreferences: PerformancePreferences;
  updatePerformancePreferences: (patch: Partial<PerformancePreferences>) => void;
  plan: SimulationPlan | null;
  onClose: () => void;
}) {
  const logicalCores = Math.max(1, diagnostics?.hardware.cpu.logicalCores ?? performancePreferences.cpuThreads ?? 1);
  const physicalCores = diagnostics?.hardware.cpu.physicalCores ?? null;
  const cpuThreads = effectiveCpuThreads(performancePreferences, diagnostics);
  const gpuDevices = diagnostics?.hardware.gpus ?? [];
  const usableGpuDevices = gpuDevices.filter((gpu) => gpu.backend);
  const diskVolumes = diagnostics?.hardware.disks ?? [];
  const selectedDisk = diskVolumes.find((disk) => disk.id === performancePreferences.diskId) ?? diskVolumes[0] ?? null;
  const selectedGpu = gpuDevices.find((gpu) => gpu.id === performancePreferences.gpuDeviceId) ?? null;
  const gpuCount = effectiveGpuCount(performancePreferences, diagnostics);
  const memoryLimitLabel = performancePreferences.memoryLimitGb > 0
    ? `${performancePreferences.memoryLimitGb} GB`
    : "自动";

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  return (
    <div className="modal-overlay" role="presentation" onMouseDown={onClose}>
      <div
        className="modal-dialog settings-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="settings-title"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <div className="settings-head">
          <h3 id="settings-title">设置</h3>
          <button type="button" className="toast-close" onClick={onClose} aria-label="关闭设置">×</button>
        </div>
        <div className="settings-section">
          <label>
            外观主题
            <select value={theme} onChange={(event) => setTheme(event.target.value as ThemeMode)}>
              <option value="light">浅色</option>
              <option value="dark">深色</option>
            </select>
          </label>
          <p className="settings-hint">主题偏好会被记住，下次启动自动应用。</p>
        </div>
        <div className="settings-section">
          <div className="settings-section-title">
            <h4>性能配置</h4>
            <p className="settings-hint">这些设置会写入当前流程的资源参数；本地运行、远程模板和后续安装/分析任务都会优先参考它。</p>
          </div>
          {diagnostics ? (
            <>
              <div className="settings-hardware-grid">
                <div className="settings-card">
                  <span>CPU</span>
                  <strong>{diagnostics.hardware.cpu.brand || diagnostics.hardware.cpu.architecture}</strong>
                  <small>
                    {logicalCores} 逻辑核心
                    {physicalCores ? ` / ${physicalCores} 物理核心` : ""}
                  </small>
                </div>
                <div className="settings-card">
                  <span>内存</span>
                  <strong>{formatBytes(diagnostics.hardware.memory.totalBytes)}</strong>
                  <small>
                    {diagnostics.hardware.memory.availableBytes
                      ? `可用 ${formatBytes(diagnostics.hardware.memory.availableBytes)}`
                      : diagnostics.hardware.memory.detail}
                  </small>
                </div>
                <div className="settings-card">
                  <span>GPU</span>
                  <strong>{gpuDevices.length ? `${gpuDevices.length} 个设备` : "未检测到"}</strong>
                  <small>{usableGpuDevices.length ? `${usableGpuDevices.length} 个可用于加速` : diagnostics.gpu.reason}</small>
                </div>
                <div className="settings-card">
                  <span>磁盘</span>
                  <strong>{diskVolumes.length ? `${diskVolumes.length} 个卷` : "未检测到"}</strong>
                  <small>{selectedDisk ? `${selectedDisk.mountPoint} 可用 ${formatBytes(selectedDisk.availableBytes)}` : "等待系统诊断"}</small>
                </div>
              </div>
              <div className="settings-control-grid">
                <label className="settings-wide-control">
                  CPU 核心数
                  <div className="settings-range-row">
                    <input
                      type="range"
                      min="1"
                      max={logicalCores}
                      value={cpuThreads}
                      onChange={(event) => updatePerformancePreferences({ cpuThreads: Number(event.target.value) })}
                    />
                    <input
                      type="number"
                      min="1"
                      max={logicalCores}
                      value={cpuThreads}
                      onChange={(event) => updatePerformancePreferences({ cpuThreads: Number(event.target.value) })}
                    />
                  </div>
                  <small>建议不要全占用，至少给系统保留 1 个逻辑核心。当前计划会使用 {plan?.resources.cpuThreads ?? cpuThreads} 线程。</small>
                </label>
                <label>
                  GPU 选择
                  <select
                    value={performancePreferences.gpuDeviceId}
                    onChange={(event) => {
                      const value = event.target.value;
                      updatePerformancePreferences({
                        gpuDeviceId: value,
                        gpuCount: value === "cpu" ? 0 : usableGpuDevices.length > 0 ? Math.max(1, gpuCount) : 0
                      });
                    }}
                  >
                    <option value="auto">自动选择可用 GPU</option>
                    <option value="cpu">只用 CPU</option>
                    {gpuDevices.map((gpu) => (
                      <option key={gpu.id} value={gpu.id} disabled={!gpu.backend}>
                        {gpu.name} {gpu.backend ? `(${gpuBackendText[gpu.backend]})` : "(不适用)"}
                      </option>
                    ))}
                  </select>
                  <small>
                    {selectedGpu
                      ? `${selectedGpu.vendor} · ${selectedGpu.backend ? gpuBackendText[selectedGpu.backend] : "不适用"}`
                      : usableGpuDevices.length ? "自动会优先使用可加速设备。" : "未检测到可用 GPU 时会使用 CPU 模式。"}
                  </small>
                </label>
                <label>
                  GPU 数量
                  <input
                    type="number"
                    min="0"
                    max={usableGpuDevices.length}
                    value={gpuCount}
                    disabled={performancePreferences.gpuDeviceId === "cpu" || usableGpuDevices.length === 0}
                    onChange={(event) => updatePerformancePreferences({ gpuCount: Number(event.target.value) })}
                  />
                  <small>当前计划 GPU 数量：{plan?.resources.gpuCount ?? gpuCount}</small>
                </label>
                <label>
                  内存上限
                  <input
                    type="number"
                    min="0"
                    step="1"
                    value={performancePreferences.memoryLimitGb}
                    onChange={(event) => updatePerformancePreferences({ memoryLimitGb: Number(event.target.value) })}
                  />
                  <small>0 表示自动；当前为 {memoryLimitLabel}。用于后台分析和任务提示。</small>
                </label>
                <label>
                  工作磁盘
                  <select
                    value={performancePreferences.diskId}
                    onChange={(event) => updatePerformancePreferences({ diskId: event.target.value })}
                  >
                    <option value="auto">自动选择项目所在磁盘</option>
                    {diskVolumes.map((disk) => (
                      <option key={disk.id} value={disk.id}>
                        {disk.mountPoint} · 可用 {formatBytes(disk.availableBytes)}
                      </option>
                    ))}
                  </select>
                  <small>{selectedDisk ? `${selectedDisk.filesystem} · 总量 ${formatBytes(selectedDisk.totalBytes)}` : "项目仍默认写入项目目录。"}</small>
                </label>
              </div>
              <div className="settings-device-list">
                <h5>检测到的 GPU</h5>
                {gpuDevices.length ? gpuDevices.map((gpu) => (
                  <div className="settings-device-row" key={gpu.id}>
                    <div>
                      <strong>{gpu.name}</strong>
                      <small>{gpu.vendor} · {gpu.backend ? gpuBackendText[gpu.backend] : "不适用"} · {formatBytes(gpu.memoryBytes)}</small>
                    </div>
                    <span>{gpu.detail}</span>
                  </div>
                )) : (
                  <p className="settings-hint">未检测到独立或可加速 GPU。AutoMD 会继续使用 CPU 模式。</p>
                )}
              </div>
            </>
          ) : (
            <p className="settings-hint">正在读取系统硬件信息，稍后会显示 CPU、内存、GPU 和磁盘。</p>
          )}
        </div>
        <div className="settings-section">
          <dl className="settings-meta">
            <div><dt>应用</dt><dd>AutoMD</dd></div>
            <div><dt>版本</dt><dd>0.1.0</dd></div>
          </dl>
        </div>
        <div className="modal-actions">
          <button type="button" className="primary" onClick={onClose}>完成</button>
        </div>
      </div>
    </div>
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
  browseStructureFile,
  importStructure,
  selectProject,
  requestDeleteProject,
  openProjectFolder,
  structures,
  activeStructureId,
  selectStructure,
  requestDeleteStructure,
  renamingStructureId,
  renamingStructureDraft,
  setRenamingStructureDraft,
  startRenameStructure,
  commitRenameStructure,
  renamingProjectId,
  renamingProjectDraft,
  setRenamingProjectDraft,
  startRenameProject,
  commitRenameProject
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
  browseStructureFile: () => void;
  importStructure: () => void;
  selectProject: (project: ProjectSummary) => void;
  requestDeleteProject: (project: ProjectSummary) => void;
  openProjectFolder: (path?: string | null) => void;
  structures: StructureEntry[];
  activeStructureId: string | null;
  selectStructure: (entry: StructureEntry) => void;
  requestDeleteStructure: (entry: StructureEntry) => void;
  renamingStructureId: string | null;
  renamingStructureDraft: string;
  setRenamingStructureDraft: (v: string) => void;
  startRenameStructure: (entry: StructureEntry) => void;
  commitRenameStructure: (id: string) => void;
  renamingProjectId: string | null;
  renamingProjectDraft: string;
  setRenamingProjectDraft: (v: string) => void;
  startRenameProject: (project: ProjectSummary) => void;
  commitRenameProject: (id: string) => void;
}) {
  return (
    <div className="content-grid project-grid">
      <section className="engine-reminder span-3" role="note">
        <strong>请先检查引擎配置</strong>
        <span>开始导入和运行前，建议先到“引擎”页确认 GROMACS、OpenMM 或其他目标引擎是否可用；缺失时在对应引擎卡片中使用一键部署或高级部署/编译。</span>
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
                {renamingProjectId === item.id ? (
                  <input
                    className="index-rename-input"
                    value={renamingProjectDraft}
                    autoFocus
                    onChange={(e) => setRenamingProjectDraft(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") commitRenameProject(item.id);
                      if (e.key === "Escape") commitRenameProject(item.id);
                    }}
                    onBlur={() => commitRenameProject(item.id)}
                  />
                ) : (
                  <button type="button" onClick={() => selectProject(item)}>
                    <strong>{item.name}</strong>
                    <small>{item.domain} / {item.status}</small>
                    <span className="mono truncate">{item.path}</span>
                  </button>
                )}
                <div className="index-action-group">
                  <button
                    type="button"
                    className="index-rename-btn"
                    title="重命名"
                    onClick={() => startRenameProject(item)}
                  >
                    <PencilIcon />
                  </button>
                  <button
                    type="button"
                    className="project-delete index-delete-btn"
                    onClick={() => requestDeleteProject(item)}
                  >
                    删除项目
                  </button>
                </div>
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
            <div className="input-with-button">
              <input
                value={importSourcePath}
                placeholder="/path/to/system.pdb"
                onChange={(event) => setImportSourcePath(event.target.value)}
              />
              <button type="button" onClick={browseStructureFile}>
                浏览
              </button>
            </div>
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

        {/* ── Structure Index ──────────────────────────────── */}
        <div className="structure-index-divider" />
        <h3 className="structure-index-title" style={{ marginTop: '12px' }}>结构索引</h3>
        {structures.length === 0 ? (
          <EmptyState title="暂无结构" text="导入结构后将在此显示，可在不同结构间切换视图和动力学参数。" />
        ) : (
          <div className="structure-index-list">
            {structures.map((entry) => (
              <div
                className={`structure-index-row ${entry.id === activeStructureId ? "active" : ""}`}
                key={entry.id}
              >
                {renamingStructureId === entry.id ? (
                  <input
                    className="index-rename-input"
                    value={renamingStructureDraft}
                    autoFocus
                    onChange={(e) => setRenamingStructureDraft(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") commitRenameStructure(entry.id);
                      if (e.key === "Escape") commitRenameStructure(entry.id);
                    }}
                    onBlur={() => commitRenameStructure(entry.id)}
                  />
                ) : (
                  <button
                    type="button"
                    className="structure-index-btn"
                    onClick={() => selectStructure(entry)}
                  >
                    <span className="structure-kind-badge">{entry.sourceKind.toUpperCase()}</span>
                    <strong>{entry.name}</strong>
                    <small className="mono truncate">{entry.importedPath}</small>
                  </button>
                )}
                <div className="index-action-group">
                  <button
                    type="button"
                    className="index-rename-btn"
                    title="重命名"
                    onClick={() => startRenameStructure(entry)}
                  >
                    <PencilIcon />
                  </button>
                  <button
                    type="button"
                    className="index-delete-btn"
                    title="删除结构"
                    onClick={() => requestDeleteStructure(entry)}
                  >
                    ✕
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </section>
      <section className="panel span-2">
        <MoleculeViewport plan={plan} project={project} />
      </section>
    </div>
  );
}

function EnginesPanel({
  engines,
  engineTargets,
  selectedEngineTargetId,
  setSelectedEngineTargetId,
  selectedEngineId,
  setSelectedEngineId,
  engineInstallations,
  engineInstallationDraft,
  setEngineInstallationDraft,
  saveEngineInstallation,
  deleteEngineInstallation,
  generateRecipes,
  autoFindEngine,
  manualFindEngine,
  autoInstallEngine,
  installableEngines,
  containerRecipe,
  buildRecipe,
  recipeExportResult,
  buildWorkflowMode,
  setBuildWorkflowMode,
  buildWorkflowTimeout,
  setBuildWorkflowTimeout,
  buildWorkflowResult,
  engineDeployResult,
  exportRecipes,
  runBuildWizard
}: {
  engines: EngineCapability[];
  engineTargets: EngineTarget[];
  selectedEngineTargetId: string;
  setSelectedEngineTargetId: (targetId: string) => void;
  selectedEngineId: string;
  setSelectedEngineId: (engineId: string) => void;
  engineInstallations: EngineInstallationRecord[];
  engineInstallationDraft: EngineInstallationRecord;
  setEngineInstallationDraft: (record: EngineInstallationRecord) => void;
  saveEngineInstallation: (record: EngineInstallationRecord) => void;
  deleteEngineInstallation: (record: EngineInstallationRecord) => void;
  generateRecipes: (engineId?: string) => void;
  autoFindEngine: (engine: EngineCapability) => void;
  manualFindEngine: (engine: EngineCapability) => void;
  autoInstallEngine: (engine: EngineCapability) => void;
  installableEngines: string[];
  containerRecipe: ContainerRecipe | null;
  buildRecipe: BuildRecipe | null;
  recipeExportResult: RecipeExportResult | null;
  buildWorkflowMode: BuildWorkflowMode;
  setBuildWorkflowMode: (value: BuildWorkflowMode) => void;
  buildWorkflowTimeout: number;
  setBuildWorkflowTimeout: (value: number) => void;
  buildWorkflowResult: BuildWorkflowResult | null;
  engineDeployResult: EngineDeployResult | null;
  exportRecipes: (engineId?: string) => void;
  runBuildWizard: (engineId?: string) => void;
}) {
  const selectedEngine = engines.find((engine) => engine.id === selectedEngineId) ?? engines[0];
  const selectedTarget = engineTargets.find((target) => target.id === selectedEngineTargetId) ?? engineTargets[0] ?? {
    id: "local",
    kind: "local" as const,
    profileId: null,
    label: "本机",
    detail: "本机",
    status: "ready" as const,
    platform: null,
    arch: null,
    hostname: null
  };
  const selectedRecords = engineInstallations.filter(
    (record) => record.targetId === selectedTarget.id && record.engineId === selectedEngineId
  );
  const targetDraft = {
    ...engineInstallationDraft,
    targetKind: selectedTarget.kind,
    targetId: selectedTarget.id,
    targetLabel: selectedTarget.label,
    platform: selectedTarget.platform ?? null,
    arch: selectedTarget.arch ?? null
  };
  const helperBlocked = selectedTarget.kind === "remote" && selectedTarget.status !== "ready" && selectedTarget.status !== "outdated";
  return (
    <div className="content-grid">
      <section className="panel span-3">
        <div className="panel-title-row">
          <div>
            <h3>目标设备</h3>
            <p className="muted">先选择本机或远程 profile，再对该设备扫描、部署、编译和登记引擎。</p>
          </div>
          <button type="button" onClick={() => generateRecipes(selectedEngineId)}>
            预览当前引擎 recipe
          </button>
        </div>
        <div className="engine-target-switcher">
          {engineTargets.map((target) => (
            <button
              type="button"
              key={target.id}
              className={`engine-target-card ${target.id === selectedTarget.id ? "selected" : ""}`}
              onClick={() => {
                setSelectedEngineTargetId(target.id);
                setEngineInstallationDraft({
                  ...targetDraft,
                  targetKind: target.kind,
                  targetId: target.id,
                  targetLabel: target.label,
                  platform: target.platform ?? null,
                  arch: target.arch ?? null
                });
              }}
            >
              <span className={`status-dot ${target.status === "ready" ? "ready" : "warn"}`} />
              <strong>{target.label}</strong>
              <small>{target.detail}</small>
            </button>
          ))}
        </div>
        {helperBlocked ? (
          <div className="warning-inline">
            该远程设备的 AutoMD helper 未就绪。请先到“远程”页安装/检测 helper，然后再扫描或部署引擎。
          </div>
        ) : null}
      </section>

      <section className="panel span-3">
        <div className="panel-title-row">
          <div>
            <h3>引擎部署</h3>
            <p className="muted">当前目标：{selectedTarget.label}。一键部署会自动选择包管理安装、源码构建或 recipe-only。</p>
          </div>
        </div>
        <div className="engine-grid">
          {engines.map((engine) => {
            const platformBlocked = isEnginePlatformBlocked(engine);
            const platformBlockedTitle = platformBlocked ? enginePlatformMessage(engine) : undefined;
            const blocked = platformBlocked || helperBlocked;
            const deployLabel = installableEngines.includes(engine.id)
              ? "一键部署"
              : "高级部署/编译";
            return (
            <article
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
              <div className="engine-card-actions" onClick={(event) => event.stopPropagation()}>
                  <button
                    type="button"
                    title={platformBlockedTitle}
                    onClick={() => autoFindEngine(engine)}
                  >
                    自动扫描
                  </button>
                  <button
                    type="button"
                    title={platformBlockedTitle}
                    onClick={() => manualFindEngine(engine)}
                  >
                    手动登记
                  </button>
                  <button
                    type="button"
                    className={blocked ? "" : "primary"}
                    title={platformBlockedTitle}
                    onClick={() => autoInstallEngine(engine)}
                  >
                    {deployLabel}
                  </button>
                </div>
              <details className="engine-advanced" onClick={(event) => event.stopPropagation()}>
                <summary>高级部署/编译</summary>
                <div className="engine-advanced-grid">
                  <div className="button-row">
                    <button type="button" onClick={() => generateRecipes(engine.id)}>预览 recipe</button>
                    <button type="button" onClick={() => exportRecipes(engine.id)}>导出到项目</button>
                  </div>
                  <label>
                    构建模式
                    <select value={buildWorkflowMode} onChange={(event) => setBuildWorkflowMode(event.target.value as BuildWorkflowMode)}>
                      <option value="dryRun">Dry run：只预览命令</option>
                      <option value="writeFiles">只写脚本：写入 build-recipes/</option>
                      <option value="execute">执行：运行构建脚本</option>
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
                  <button type="button" className={blocked ? "fill" : "primary fill"} onClick={() => runBuildWizard(engine.id)}>
                    运行高级部署
                  </button>
                  {selectedEngineId === engine.id && recipeExportResult ? (
                    <div className="success-inline">已导出到 <span className="mono">{recipeExportResult.directory}</span></div>
                  ) : null}
                  {selectedEngineId === engine.id && engineDeployResult ? (
                    <div className="build-runner-result compact">
                      <dl className="definition-list">
                        <div><dt>策略</dt><dd>{engineDeployResult.strategy}</dd></div>
                        <div><dt>状态</dt><dd>{engineDeployResult.status}</dd></div>
                        <div><dt>登记</dt><dd>{engineDeployResult.record?.location ?? "未登记"}</dd></div>
                      </dl>
                      {engineDeployResult.warnings.length ? (
                        <div className="warning-stack">
                          {engineDeployResult.warnings.map((warning) => <p key={warning}>{warning}</p>)}
                        </div>
                      ) : null}
                    </div>
                  ) : null}
                </div>
              </details>
            </article>
          );})}
        </div>
      </section>
      <section className="panel">
        <h3>手动登记 / 授权记录</h3>
        <div className="engine-install-form">
          <label>
            目标设备
            <input value={selectedTarget.label} readOnly />
          </label>
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
        <button type="button" className="primary fill" onClick={() => saveEngineInstallation(targetDraft)}>
          保存安装记录
        </button>
        <p className="hint-text">
          受限/商业引擎只保存用户配置的路径和授权状态；AutoMD 不下载、不镜像、不分发这些二进制。
        </p>
      </section>
      <section className="panel span-2">
        <h3>{selectedTarget.label} · {selectedEngine?.name ?? selectedEngineId} 保存记录</h3>
        {selectedRecords.length ? (
          <div className="engine-install-list">
            {selectedRecords.map((record) => (
              <div className="engine-install-row" key={`${record.targetId}-${record.engineId}-${record.location}`}>
                <div>
                  <strong className="mono">{record.location}</strong>
                  <small>{record.targetLabel} · {record.version ?? "version unknown"} · {new Date(record.checkedAt).toLocaleString()}</small>
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
      <section className="panel span-3">
        <h3>{containerRecipe?.title ?? "当前 recipe 预览"}</h3>
        <CodeBlock value={containerRecipe?.files[0]?.contents ?? "在任意引擎卡片的高级部署/编译中点击“预览 recipe”。"} />
      </section>
      <section className="panel span-3">
        <h3>{buildRecipe?.title ?? "源码编译脚本"}</h3>
        {buildRecipe ? (
          <div className="split">
            <div>
              <h4>步骤</h4>
              <ol>
                {buildRecipe.steps.map((step) => <li key={step}>{step}</li>)}
              </ol>
              <h4>风险</h4>
              <ul>
                {buildRecipe.warnings.map((warning) => <li key={warning}>{warning}</li>)}
              </ul>
            </div>
            <CodeBlock value={buildRecipe.script} />
          </div>
        ) : (
          <EmptyState title="尚未生成脚本" text="高级部署区可生成源码编译脚本、容器 recipe 和构建日志。" />
        )}
        {buildWorkflowResult ? (
          <div className="build-runner-result">
            <dl className="definition-list">
              <div><dt>模式</dt><dd>{buildWorkflowModeText[buildWorkflowResult.mode]}</dd></div>
              <div><dt>状态</dt><dd>{buildWorkflowResult.status}</dd></div>
              <div><dt>退出码</dt><dd>{buildWorkflowResult.exitCode ?? "n/a"}</dd></div>
              <div><dt>日志</dt><dd className="mono">{buildWorkflowResult.logPath ?? "未生成"}</dd></div>
            </dl>
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
        ) : null}
      </section>
    </div>
  );
}

type ScienceToolUsage = {
  role: "required" | "optional" | "notApplicable";
  label: string;
  detail: string;
};

function scienceToolUsage(plan: SimulationPlan, toolId: string): ScienceToolUsage {
  const engineId = plan.engineId;
  const hasLigand = plan.system.hasLigand;
  const notApplicable = (detail: string): ScienceToolUsage => ({ role: "notApplicable", label: "本引擎不需要", detail });
  const required = (detail: string): ScienceToolUsage => ({ role: "required", label: "当前引擎需要", detail });
  const ligandOnly = (detail: string): ScienceToolUsage =>
    hasLigand ? required(detail) : notApplicable("当前体系没有配体，暂不需要配体参数化工具。");
  const analysis = (detail: string): ScienceToolUsage => ({ role: "optional", label: "分析/预处理可用", detail });

  if (engineId === "openmm") {
    if (["openmm", "pdbfixer", "mdanalysis", "mdtraj"].includes(toolId)) {
      return required("OpenMM 流程、结构修复或轨迹分析会用到。");
    }
    if (["rdkit", "openbabel"].includes(toolId)) {
      return ligandOnly("配体体系会用它处理小分子格式和参数化前准备。");
    }
    return notApplicable("OpenMM 路线不需要 AmberTools 命令行工具。");
  }

  if (engineId === "ambertools" || engineId === "amber_pmemd") {
    if (["tleap", "antechamber", "parmchk2", "cpptraj"].includes(toolId)) {
      return required("Amber/AmberTools 输入生成和分析需要这些命令。");
    }
    if (["rdkit", "openbabel"].includes(toolId)) {
      return ligandOnly("配体体系会用它做小分子格式转换或参数检查。");
    }
    if (["mdanalysis", "mdtraj"].includes(toolId)) {
      return analysis("可用于独立轨迹分析；AmberTools 自带 cpptraj 也可完成一部分分析。");
    }
    return notApplicable("AmberTools 路线不需要 OpenMM/PDBFixer 作为必需依赖。");
  }

  if (["gromacs", "namd", "charmm"].includes(engineId)) {
    if (["pdbfixer", "mdanalysis", "mdtraj"].includes(toolId)) {
      return required("结构准备、轨迹索引或分析会用到。");
    }
    if (["rdkit", "openbabel", "antechamber", "parmchk2", "tleap"].includes(toolId)) {
      return ligandOnly("配体体系需要额外的小分子处理或拓扑准备工具。");
    }
    if (toolId === "cpptraj") {
      return analysis("可用于部分轨迹/能量后处理，但不是当前引擎启动的必需项。");
    }
    return notApplicable("当前引擎不需要这个 Python/AmberTools 组件作为必需依赖。");
  }

  if (["lammps", "cp2k", "genesis", "hoomd", "dl_poly", "tinker"].includes(engineId)) {
    if (["mdanalysis", "mdtraj"].includes(toolId)) {
      return analysis("可用于部分通用轨迹分析；材料/QM/MM 模板通常还需要引擎自己的分析工具。");
    }
    return notApplicable("当前材料/QM/MM 路线不依赖这个生物分子科学侧车组件。");
  }

  return analysis("可用于结构准备或分析，但当前引擎模板不强制要求。");
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
  generatePreparationPackage,
  autoFindScienceTool,
  manualFindScienceTool,
  autoInstallScienceSidecar
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
  autoFindScienceTool: (tool: ScienceToolDiagnostic) => void;
  manualFindScienceTool: (tool: ScienceToolDiagnostic) => void;
  autoInstallScienceSidecar: () => void;
}) {
  const productionStage = plan.stages.find((stage) => stage.id === "production");
  const productionDurationNs = productionStage?.parameters.durationNs ?? "";
  const scienceTools = scienceDiagnostics?.tools.map((tool) => ({
    tool,
    usage: scienceToolUsage(plan, tool.id),
    status: scienceToolUsage(plan, tool.id).role === "notApplicable" ? "notApplicable" as DetectionStatus : tool.status
  })) ?? [];
  const neededScienceTools = scienceTools.filter((entry) => entry.usage.role !== "notApplicable");
  const readyScienceCount = neededScienceTools.filter((entry) => entry.status === "ready").length;

  return (
    <div className="flow-steps">
      <section className="panel flow-step">
        <div className="flow-step-head">
          <span className="step-number">1</span>
          <div>
            <h3>模拟参数</h3>
            <p className="muted">填写和科学体系直接相关的参数；集群 walltime 属于远程/资源设置，不等于模拟长度。</p>
          </div>
        </div>
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
            生产模拟长度 (ns)
            <input
              type="number"
              min="0.001"
              step="1"
              value={productionDurationNs}
              onChange={(event) => updateStageParameter("production", "durationNs", event.target.value)}
            />
          </label>
        </div>
      </section>

      <section className="panel flow-step">
        <div className="flow-step-head">
          <span className="step-number">2</span>
          <div>
            <h3>阶段参数</h3>
            <p className="muted">能量最小化 / 升温 / 平衡 / 生产各阶段的开关与参数，按顺序确认即可。</p>
          </div>
        </div>
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

      <section className="panel flow-step">
        <div className="flow-step-head">
          <span className="step-number">3</span>
          <div>
            <h3>分析模块</h3>
            <p className="muted">勾选需要的分析；运行结束后会自动纳入分析与报告。</p>
          </div>
        </div>
        <div className="toggle-list analysis-toggle-grid">
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

      <section className="panel flow-step">
        <div className="flow-step-head">
          <span className="step-number">4</span>
          <div>
            <h3>结构准备与分析环境</h3>
            <p className="muted">
              用于修复结构、加氢、配体处理、OpenMM 快速验证和轨迹分析。当前引擎不需要的项目会显示“不适用”。
            </p>
          </div>
          <button type="button" className="primary" onClick={generatePreparationPackage}>
            生成结构准备文件
          </button>
        </div>
        <div className="sidecar-grid">
          <div>
            <h4>环境检查</h4>
            {scienceDiagnostics ? (
              <div className="tool-list compact-tools sci-tools-grid">
                <div className="sidecar-summary">
                  <strong>{readyScienceCount}/{neededScienceTools.length} 项可用</strong>
                  <small>Python: {scienceDiagnostics.pythonExecutable ?? "未找到"}</small>
                  <button type="button" className="primary" onClick={autoInstallScienceSidecar}>
                    一键安装/修复科学环境
                  </button>
                </div>
                {scienceTools.map(({ tool, usage, status }) => {
                  const needsAction = status !== "ready" && status !== "notApplicable";
                  return (
                    <div className={`science-tool-card ${needsAction ? "needs-action" : ""}`} key={tool.id}>
                      <div className="science-tool-head">
                        <div className="science-tool-meta">
                          <strong>{tool.label}</strong>
                          <small>{usage.label} · {tool.importName ?? tool.command ?? tool.id}</small>
                        </div>
                        <StatusPill status={status} />
                      </div>
                      {needsAction ? (
                        <div className="science-tool-actions">
                          <button type="button" onClick={() => autoFindScienceTool(tool)}>自动查找</button>
                          <button type="button" onClick={() => manualFindScienceTool(tool)}>手动查找</button>
                          <button type="button" className="primary" onClick={autoInstallScienceSidecar}>一键安装</button>
                        </div>
                      ) : (
                        <p className={`science-tool-detail ${status === "notApplicable" ? "" : "mono"}`}>
                          {status === "notApplicable" ? usage.detail : tool.detail}
                        </p>
                      )}
                    </div>
                  );
                })}
              </div>
            ) : (
              <EmptyState title="等待诊断" text="启动后会检测 OpenMM、PDBFixer、MDAnalysis、RDKit、Open Babel 和 AmberTools。" />
            )}
          </div>
          <div className="sidecar-side">
          <div>
            <h4>一键环境</h4>
            <div className="sidecar-explain">
              <p>推荐环境就是 AutoMD 管理的 Python 环境，默认名为 automd-science。它会安装结构准备和分析常用包，不写入系统 Python。</p>
              <p>一般用户只需要点“一键安装/修复科学环境”；YAML 仅用于高级复现、HPC 或手动 Conda/Mamba 配置。</p>
              <details>
                <summary>查看 environment.yml 预览</summary>
                <CodeBlock value={scienceDiagnostics?.environmentRecipe ?? "等待侧车诊断。"} />
              </details>
            </div>
          </div>
          <div>
            <h4>结构准备文件</h4>
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
              <EmptyState title="尚未生成" text="点击后会写入结构修复/加氢脚本、环境文件和配体处理说明；这是运行前输入准备，不是模拟结果。" />
            )}
          </div>
          </div>
        </div>
      </section>

      <section className="panel">
        <h3>参数检查</h3>
        <ValidationList validation={validation} />
      </section>

      <details className="panel flow-advanced">
        <summary>高级：当前引擎原生参数预览</summary>
        <p className="muted">
          这不是另一套需要你重新填写的参数，而是把上面的 GUI 参数翻译成 {engineLabel[plan.engineId] ?? plan.engineId}
          会写入的原生字段。需复核表示模板能给出建议，但正式生产前应打开生成文件确认。
        </p>
        <ParameterMappingList report={parameterMappingReport} />
      </details>
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
    <div className="flow-steps">
      <section className="panel flow-step">
        <div className="flow-step-head">
          <span className="step-number">1</span>
          <div>
            <h3>启动前检查</h3>
            <p className="muted">确认引擎与参数校验通过，然后生成本地 run package。</p>
          </div>
        </div>
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

      <section className="panel flow-step">
        <div className="flow-step-head">
          <span className="step-number">2</span>
          <div>
            <h3>本地执行</h3>
            <p className="muted">先用 Mock runner 验证 GUI 监控链路，再切换真实本地执行。</p>
          </div>
        </div>
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

      <section className="panel flow-step">
        <div className="flow-step-head">
          <span className="step-number">3</span>
          <div>
            <h3>结果与产物</h3>
            <p className="muted">运行产物索引、轨迹预览与分析曲线；远程作业回收的结果也会出现在这里。</p>
          </div>
          <button type="button" onClick={refreshArtifacts}>刷新索引</button>
        </div>
        {artifactIndex?.artifacts.length ? (
          <ArtifactTable artifacts={artifactIndex.artifacts} />
        ) : (
          <EmptyState title="暂无 artifact 索引" text="任务完成后会自动索引日志、checkpoint、轨迹、分析表和报告，也可以手动刷新项目目录。" />
        )}
        <TrajectoryIndexPanel
          artifacts={artifactIndex?.artifacts ?? []}
          trajectoryIndex={trajectoryIndex}
          trajectoryChunk={trajectoryChunk}
          indexTrajectory={indexTrajectory}
          previewTrajectoryFrame={previewTrajectoryFrame}
        />
        <TrajectoryAnalysisPackagePanel
          analysisPackage={trajectoryAnalysisPackage}
          generateTrajectoryAnalysisPackage={generateTrajectoryAnalysisPackage}
        />
        <h4>分析曲线</h4>
        <AnalysisChartGrid analysisResult={analysisResult} />
      </section>

      <details className="panel flow-advanced">
        <summary>高级 / 更多：批量实验、生成文件与脚本、原生编辑、资源、历史、日志解析</summary>

        <h4>批量重复实验</h4>
        <div className="batch-controls">
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
          <button type="button" className="primary" onClick={generateBatchExperiment} disabled={!plan}>
            生成批量实验包
          </button>
        </div>
        <p className="hint-text">用于多 seed / 多 replica 的重复实验；生成后会写入 generated/batch，不会立即启动模拟。</p>
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

        <h4>当前任务记录</h4>
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

        <h4>GROMACS Run Package</h4>
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

        <h4>生成文件</h4>
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

        <div className="advanced-head-row">
          <h4>原生参数文件编辑器</h4>
          <button type="button" onClick={saveNativeFile} disabled={!nativeFile}>保存</button>
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

        <h4>SLURM 脚本</h4>
        <CodeBlock value={slurmScript || "生成运行计划后显示 sbatch 脚本。"} />

        <h4>资源摘要</h4>
        {plan ? (
          <dl className="definition-list">
            <div><dt>执行模式</dt><dd>{executionModeText[plan.resources.executionMode]}</dd></div>
            <div><dt>CPU</dt><dd>{plan.resources.cpuThreads}</dd></div>
            <div><dt>GPU</dt><dd>{plan.resources.gpuCount}</dd></div>
            <div><dt>MPI</dt><dd>{plan.resources.mpiRanks}</dd></div>
          </dl>
        ) : null}

        <div className="advanced-head-row">
          <h4>SQLite 任务历史</h4>
          <button type="button" onClick={refreshTaskRecords}>刷新</button>
        </div>
        <TaskRecordList records={taskRecords} />

        <h4>断点续算</h4>
        <ResumePlanCard resumePlan={resumePlan} onDiscover={discoverResumePlan} />

        <div className="advanced-head-row">
          <h4>GROMACS 日志解析（手动粘贴）</h4>
          <button type="button" onClick={parseLogSample}>解析日志</button>
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
      </details>
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
  plan,
  diagnostics,
  remoteProfiles,
  selectedRemoteProfileId,
  setSelectedRemoteProfileId,
  remoteProfileDraft,
  setRemoteProfileDraft,
  remotePassword,
  setRemotePassword,
  remoteConnectionTest,
  remoteConnecting,
  testRemoteConnection,
  saveRemoteProfile,
  deleteRemoteProfile,
  engineTargets,
  installRemoteHelper,
  checkRemoteHelper,
  projectName,
  structureName,
  updatePlan,
  remotePreflight,
  runRemotePreflight,
  remoteAllowNoHelper,
  setRemoteAllowNoHelper,
  submitRemoteJob,
  remoteSubmission,
  remoteBusy,
  remoteJobSnapshot,
  pollRemoteJobNow,
  cancelRemoteJob,
  fetchRemoteResults,
  remoteAutoPoll,
  setRemoteAutoPoll,
  remoteWorkflowJobId,
  setRemoteWorkflowJobId,
  remotePackage,
  generateRemotePackage,
  remoteWorkflowMode,
  setRemoteWorkflowMode,
  remoteWorkflowTimeout,
  setRemoteWorkflowTimeout,
  remoteWorkflowResult,
  runRemoteStep,
  remoteSubmitOutput,
  setRemoteSubmitOutput,
  remoteStatusOutput,
  setRemoteStatusOutput,
  remoteLogOutput,
  setRemoteLogOutput,
  parseRemoteStatus,
  autoFindTool,
  manualFindTool,
  autoInstallTool,
  installableTools
}: {
  plan: SimulationPlan | null;
  diagnostics: RuntimeDiagnostics | null;
  remoteProfiles: RemoteProfile[];
  selectedRemoteProfileId: string | null;
  setSelectedRemoteProfileId: (value: string | null) => void;
  remoteProfileDraft: RemoteProfile;
  setRemoteProfileDraft: (value: RemoteProfile) => void;
  remotePassword: string;
  setRemotePassword: (value: string) => void;
  remoteConnectionTest: RemoteConnectionTest | null;
  remoteConnecting: boolean;
  testRemoteConnection: () => void;
  saveRemoteProfile: (profile: RemoteProfile) => void;
  deleteRemoteProfile: (id: string) => void;
  engineTargets: EngineTarget[];
  installRemoteHelper: (profileId: string) => void;
  checkRemoteHelper: (profileId: string) => void;
  projectName: string | null;
  structureName: string | null;
  updatePlan: (updater: (current: SimulationPlan) => SimulationPlan) => void;
  remotePreflight: RemoteSubmitPreflight | null;
  runRemotePreflight: () => void;
  remoteAllowNoHelper: boolean;
  setRemoteAllowNoHelper: (value: boolean) => void;
  submitRemoteJob: () => void;
  remoteSubmission: RemoteJobSubmission | null;
  remoteBusy: null | "preflight" | "submit" | "poll" | "fetch";
  remoteJobSnapshot: RemoteJobSnapshot | null;
  pollRemoteJobNow: () => void;
  cancelRemoteJob: () => void;
  fetchRemoteResults: () => void;
  remoteAutoPoll: boolean;
  setRemoteAutoPoll: (value: boolean) => void;
  remoteWorkflowJobId: string;
  setRemoteWorkflowJobId: (value: string) => void;
  remotePackage: RemoteExecutionPackage | null;
  generateRemotePackage: (profileId?: string | null) => void;
  remoteWorkflowMode: RemoteWorkflowMode;
  setRemoteWorkflowMode: (value: RemoteWorkflowMode) => void;
  remoteWorkflowTimeout: number;
  setRemoteWorkflowTimeout: (value: number) => void;
  remoteWorkflowResult: RemoteWorkflowStepResult | null;
  runRemoteStep: (stepId: string) => void;
  remoteSubmitOutput: string;
  setRemoteSubmitOutput: (value: string) => void;
  remoteStatusOutput: string;
  setRemoteStatusOutput: (value: string) => void;
  remoteLogOutput: string;
  setRemoteLogOutput: (value: string) => void;
  parseRemoteStatus: () => void;
  autoFindTool: (tool: ToolDiagnostic) => void;
  manualFindTool: (tool: ToolDiagnostic) => void;
  autoInstallTool: (tool: ToolDiagnostic) => void;
  installableTools: string[];
}) {
  const draft = remoteProfileDraft;
  const update = (patch: Partial<RemoteProfile>) => setRemoteProfileDraft({ ...draft, ...patch });
  const connected = remoteConnectionTest?.ok ?? false;
  const draftSaved = remoteProfiles.some((profile) => profile.id === draft.id);
  const isTemplate = draft.id.endsWith("-template");
  const helperTarget = engineTargets.find((target) => target.id === `remote:${draft.id}`) ?? null;
  const helperState = helperTarget?.status ?? "missing";
  const helperReady = helperState === "ready" || helperState === "outdated";
  const submitReady = Boolean(remotePreflight?.allOk || (remotePreflight?.canOverride && remoteAllowNoHelper));
  const jobActive = Boolean(
    remoteJobSnapshot && !["completed", "failed", "cancelled"].includes(remoteJobSnapshot.status)
  );
  const [deleteProfileTarget, setDeleteProfileTarget] = useState<RemoteProfile | null>(null);
  const [deleteProfileStage, setDeleteProfileStage] = useState<"warn" | "confirm">("warn");

  return (
    <div className="remote-flow">
      {/* Step 1 — Connect */}
      <section className="panel flow-step">
        <div className="flow-step-head">
          <span className="step-number">1</span>
          <div>
            <h3>连接服务器 / HPC</h3>
            <p className="muted">
              填好连接信息后点「测试连接」。GPU 租用（AutoDL / RunPod）一般是 IP/域名 + 端口 + root + 密码；
              高校超算一般是 用户名@登录节点 + 密钥或密码。远程目标以 Linux 为主。
            </p>
          </div>
        </div>

        {remoteProfiles.length > 0 ? (
          <label className="profile-loader">
            载入已保存的连接
            <select
              value={draftSaved ? draft.id : ""}
              onChange={(event) => {
                const value = event.target.value;
                if (value === "") {
                  setSelectedRemoteProfileId(null);
                  setRemoteProfileDraft({
                    id: `custom-${Date.now()}`,
                    name: "",
                    host: "",
                    username: "root",
                    port: 22,
                    authMethod: "password",
                    identityFile: null,
                    scheduler: "slurm",
                    workdir: defaultRemoteWorkdir("root"),
                    moduleLoad: [],
                    defaultQueue: null,
                  });
                } else {
                  const picked = remoteProfiles.find((profile) => profile.id === value);
                  if (picked) {
                    setSelectedRemoteProfileId(picked.id);
                    setRemoteProfileDraft(picked);
                  }
                }
              }}
            >
              <option value="">— 新连接 —</option>
              {remoteProfiles.map((profile) => (
                <option value={profile.id} key={profile.id}>
                  {profile.name}（{profile.host || "未填主机"}）
                </option>
              ))}
            </select>
          </label>
        ) : null}

        <div className="connection-card">
          <div className="form-grid three">
            <label>
              名称
              <input value={draft.name} onChange={(event) => update({ name: event.target.value })} placeholder="我的 HPC" />
            </label>
            <label className="span-2">
              主机 / IP
              <input
                value={draft.host}
                onChange={(event) => update({ host: event.target.value })}
                placeholder="connect.region.seetacloud.com 或 123.45.67.89 或 login.cluster.edu"
              />
            </label>
            <label>
              端口
              <input
                type="number"
                min={1}
                max={65535}
                value={draft.port}
                onChange={(event) => update({ port: Number(event.target.value) || 22 })}
              />
            </label>
            <label>
              用户名
              <input
                value={draft.username}
                onChange={(event) => {
                  const username = event.target.value;
                  update({
                    username,
                    workdir: isAutoManagedRemoteWorkdir(draft.workdir, draft.username) ? defaultRemoteWorkdir(username) : draft.workdir
                  });
                }}
                placeholder="root / 你的账号"
              />
            </label>
            <label>
              认证方式
              <select value={draft.authMethod} onChange={(event) => update({ authMethod: event.target.value as RemoteAuthMethod })}>
                <option value="password">用户名 + 密码（本会话内）</option>
                <option value="key">SSH 私钥文件</option>
                <option value="agent">系统 SSH 配置 / agent（~/.ssh/config）</option>
              </select>
            </label>
          </div>

          {draft.authMethod === "password" ? (
            <label>
              密码（仅本次会话保存，不写入磁盘）
              <input
                type="password"
                value={remotePassword}
                onChange={(event) => setRemotePassword(event.target.value)}
                placeholder="实例/账号密码"
                autoComplete="off"
              />
            </label>
          ) : draft.authMethod === "key" ? (
            <label>
              私钥文件路径
              <div className="input-with-browse">
                <input
                  value={draft.identityFile ?? ""}
                  onChange={(event) => update({ identityFile: event.target.value || null })}
                  placeholder="~/.ssh/id_ed25519"
                />
                <button
                  type="button"
                  onClick={async () => {
                    try {
                      const picked = await api.pickFile({
                        title: "选择 SSH 私钥文件",
                        extensions: [],
                        defaultDir: "~/.ssh",
                        showHidden: true,
                      });
                      if (picked) {
                        update({ identityFile: picked });
                      }
                    } catch (caught) {
                      console.error("浏览私钥失败", caught);
                    }
                  }}
                >
                  浏览
                </button>
              </div>
            </label>
          ) : (
            <p className="hint-text">将使用系统 ssh 与你的 ~/.ssh/config / 密钥 / agent，无需在此填写凭据。</p>
          )}

          <div className="button-row">
            <button type="button" className="primary" onClick={testRemoteConnection} disabled={remoteConnecting}>
              {remoteConnecting ? "连接中…" : "测试连接"}
            </button>
            {draftSaved ? (
              <button type="button" className="danger-outline" onClick={() => setDeleteProfileTarget(draft)}>
                删除该 profile
              </button>
            ) : (
              <button type="button" onClick={() => saveRemoteProfile(draft)}>
                保存为 profile
              </button>
            )}
          </div>

          {deleteProfileTarget ? (
            <DeleteModal
              titleText={deleteProfileTarget.name || "未命名连接"}
              bodyText={`即将删除连接「${deleteProfileTarget.name || "未命名"}」（${deleteProfileTarget.host || "未填主机"}）。此操作不可撤销。`}
              twoStage={true}
              stage={deleteProfileStage}
              deleting={false}
              onCancel={() => { setDeleteProfileTarget(null); setDeleteProfileStage("warn"); }}
              onConfirm={() => {
                if (deleteProfileStage === "warn") {
                  setDeleteProfileStage("confirm");
                } else {
                  deleteRemoteProfile(deleteProfileTarget.id);
                  setDeleteProfileTarget(null);
                  setDeleteProfileStage("warn");
                }
              }}
            />
          ) : null}

          {remoteConnectionTest ? (
            <div className={`connection-result ${remoteConnectionTest.ok ? "ok" : "fail"}`}>
              <strong>{remoteConnectionTest.ok ? "✅ 已连接" : "❌ 连接失败"}</strong>
              <span>{remoteConnectionTest.message}</span>
            </div>
          ) : null}
        </div>
      </section>

      {/* Step 2 — Remote helper (main flow, not advanced) */}
      <section className={`panel flow-step ${connected ? "" : "flow-step-pending"}`}>
        <div className="flow-step-head">
          <span className="step-number">2</span>
          <div>
            <h3>远程助手</h3>
            <p className="muted">助手让软件能自动扫描引擎、远程安装和监控。连接成功后若未安装，这里直接装上即可。</p>
          </div>
        </div>
        {!connected ? (
          <EmptyState title="先完成第 1 步" text="测试连接成功后再安装远程助手。" />
        ) : !draftSaved ? (
          <div className="connection-result fail">
            <strong>请先保存为 profile</strong>
            <span>远程助手按已保存的连接（含端口/认证）工作，请在第 1 步点「保存为 profile」。</span>
          </div>
        ) : (
          <>
            <dl className="definition-list">
              <div><dt>状态</dt><dd>{remoteHelperStateText[helperState]}</dd></div>
              <div><dt>平台</dt><dd>{helperTarget?.platform ?? "未检测"}</dd></div>
              <div><dt>架构</dt><dd>{helperTarget?.arch ?? "未检测"}</dd></div>
            </dl>
            {helperReady ? (
              <div className="connection-result ok">
                <strong>✅ 助手已就绪</strong>
                <span>可在下一步确认引擎并提交作业。</span>
              </div>
            ) : (
              <p className="hint-text">未安装：点下方「安装远程助手」，AutoMD 会通过 SSH 写入并探测远程环境。</p>
            )}
            <div className="button-row">
              <button type="button" className={helperReady ? "" : "primary"} onClick={() => installRemoteHelper(draft.id)}>
                {helperReady ? "重新安装 / 更新助手" : "安装远程助手"}
              </button>
              <button type="button" onClick={() => checkRemoteHelper(draft.id)}>
                检测助手
              </button>
            </div>
          </>
        )}
      </section>

      {/* Step 3 — Confirm plan + engine */}
      <section className="panel flow-step">
        <div className="flow-step-head">
          <span className="step-number">3</span>
          <div>
            <h3>确认要跑的计划</h3>
            <p className="muted">远程作业会使用当前项目、结构与计划。缺哪一项就回「项目 / 流程」补上。</p>
          </div>
        </div>
        <dl className="definition-list">
          <div><dt>项目</dt><dd>{projectName ?? <span className="warn-text">未选择</span>}</dd></div>
          <div><dt>结构</dt><dd>{structureName ?? <span className="warn-text">未选择</span>}</dd></div>
          <div><dt>引擎</dt><dd>{plan?.engineId ?? "未生成计划"}</dd></div>
          <div><dt>体系</dt><dd>{plan?.system.name ?? "—"}</dd></div>
        </dl>
        {plan ? (
          <div className="form-grid three">
            <label>
              远程工作目录
              <input value={draft.workdir} onChange={(event) => update({ workdir: event.target.value })} placeholder="/home/用户名/automd 或 /scratch/$USER/automd" />
            </label>
            <label>
              调度器
              <select value={draft.scheduler} onChange={(event) => update({ scheduler: event.target.value as ExecutionMode })}>
                <option value="ssh">SSH 直接运行</option>
                <option value="slurm">SLURM</option>
                <option value="pbs">PBS</option>
                <option value="lsf">LSF</option>
              </select>
            </label>
            <label>
              队列
              <input
                value={plan.resources.queue ?? ""}
                placeholder="gpu / normal"
                onChange={(event) =>
                  updatePlan((current) => ({ ...current, resources: { ...current.resources, queue: event.target.value || null } }))
                }
              />
            </label>
          </div>
        ) : (
          <EmptyState title="尚无计划" text="先到「流程」页生成 SimulationPlan。" />
        )}
      </section>

      {/* Step 4 — Preflight + submit */}
      <section className="panel flow-step">
        <div className="flow-step-head">
          <span className="step-number">4</span>
          <div>
            <h3>预检并提交</h3>
            <p className="muted">提交前逐项核对：项目 / 结构 / 计划 / 引擎 / 助手 / 工作目录 / 调度器。全部通过才允许提交。</p>
          </div>
        </div>
        <div className="button-row">
          <button type="button" onClick={runRemotePreflight} disabled={remoteBusy === "preflight"}>
            {remoteBusy === "preflight" ? "预检中…" : "运行预检"}
          </button>
        </div>
        {remotePreflight ? (
          <ul className="preflight-list">
            {remotePreflight.checks.map((check) => (
              <li className={`preflight-check ${check.ok ? "ok" : "fail"}`} key={check.id}>
                <span className="preflight-mark">{check.ok ? "✓" : "✗"}</span>
                <div>
                  <strong>{check.label}</strong>
                  <small>{check.detail}</small>
                </div>
              </li>
            ))}
          </ul>
        ) : (
          <EmptyState title="尚未预检" text="点「运行预检」检查是否满足提交条件。" />
        )}
        {remotePreflight && !remotePreflight.allOk && remotePreflight.canOverride ? (
          <label className="check-row">
            <input type="checkbox" checked={remoteAllowNoHelper} onChange={() => setRemoteAllowNoHelper(!remoteAllowNoHelper)} />
            <span>高级：跳过远程助手/引擎登记，直接 SSH 提交（仅在你确认远程已装好所需引擎时）</span>
          </label>
        ) : null}
        <div className="button-row">
          <button
            type="button"
            className="primary"
            onClick={submitRemoteJob}
            disabled={!submitReady || remoteBusy === "submit"}
          >
            {remoteBusy === "submit" ? "提交中…" : "上传并提交作业"}
          </button>
          {!submitReady ? <span className="hint-text">预检通过后才能提交。</span> : null}
        </div>
      </section>

      {/* Step 5 — Monitor */}
      <section className="panel flow-step">
        <div className="flow-step-head">
          <span className="step-number">5</span>
          <div>
            <h3>监控</h3>
            <p className="muted">提交后自动每 8 秒拉取一次状态与日志，无需手动粘贴。</p>
          </div>
        </div>
        {remoteSubmission ? (
          <>
            <dl className="definition-list">
              <div><dt>Job ID</dt><dd className="mono">{remoteSubmission.jobId ?? "未解析"}</dd></div>
              <div><dt>远程目录</dt><dd className="mono">{remoteSubmission.remoteRunDir}</dd></div>
              <div><dt>上传文件</dt><dd>{remoteSubmission.filesUploaded}</dd></div>
            </dl>
            <div className="button-row">
              <label className="check-row inline">
                <input type="checkbox" checked={remoteAutoPoll} onChange={() => setRemoteAutoPoll(!remoteAutoPoll)} />
                <span>自动刷新</span>
              </label>
              <button type="button" onClick={pollRemoteJobNow} disabled={remoteBusy === "poll"}>
                {remoteBusy === "poll" ? "查询中…" : "刷新状态"}
              </button>
              <button type="button" onClick={cancelRemoteJob} disabled={!jobActive}>
                取消作业
              </button>
              <label className="job-id-edit">
                Job ID
                <input value={remoteWorkflowJobId} onChange={(event) => setRemoteWorkflowJobId(event.target.value)} placeholder={remoteSubmission.jobId ?? "<job-id>"} />
              </label>
            </div>
            {remoteJobSnapshot ? (
              <div className="remote-snapshot">
                {remoteJobSnapshot.progressPercent != null ? (
                  <div className="progress-shell">
                    <div className="progress-bar" style={{ width: `${remoteJobSnapshot.progressPercent}%` }} />
                  </div>
                ) : null}
                <dl className="definition-list">
                  <div><dt>状态</dt><dd>{remoteJobSnapshot.status}</dd></div>
                  <div><dt>队列态</dt><dd>{remoteJobSnapshot.queueState ?? "未检测"}</dd></div>
                  <div><dt>步数</dt><dd>{remoteJobSnapshot.currentStep ?? "未检测"}</dd></div>
                  <div><dt>性能</dt><dd>{remoteJobSnapshot.nsPerDay ? `${remoteJobSnapshot.nsPerDay.toFixed(3)} ns/day` : "未检测"}</dd></div>
                </dl>
                {remoteJobSnapshot.reason ? <p className="hint-text">{remoteJobSnapshot.reason}</p> : null}
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
              <p className="hint-text">正在等待第一次状态返回…</p>
            )}
          </>
        ) : (
          <EmptyState title="尚未提交" text="完成第 4 步提交后，这里会自动显示作业状态与进度。" />
        )}
      </section>

      {/* Step 6 — Fetch results */}
      <section className="panel flow-step">
        <div className="flow-step-head">
          <span className="step-number">6</span>
          <div>
            <h3>回收结果</h3>
            <p className="muted">把远程的 runs / 轨迹 / 分析 / 报告同步回本地项目，随后到「运行 / 报告」查看。</p>
          </div>
        </div>
        <div className="button-row">
          <button type="button" className="primary" onClick={fetchRemoteResults} disabled={!remoteSubmission || remoteBusy === "fetch"}>
            {remoteBusy === "fetch" ? "下载中…" : "下载结果到本地"}
          </button>
          {!remoteSubmission ? <span className="hint-text">提交作业后可用。</span> : null}
        </div>
      </section>

      {/* Advanced — command export + manual parse + extras (fallback) */}
      <details className="panel flow-advanced">
        <summary>高级 / 备用手段：导出命令、脚本、手动解析、本机工具</summary>

        <h4>自定义 profile（module load 等）</h4>
        <label className="span-all">
          Module / setup commands
          <textarea
            value={draft.moduleLoad.join("\n")}
            onChange={(event) => update({ moduleLoad: event.target.value.split("\n") })}
            rows={3}
            spellCheck={false}
          />
        </label>
        <div className="button-row">
          <button type="button" onClick={() => saveRemoteProfile(draft)}>保存 profile</button>
        </div>

        <h4>导出命令 / 脚本（手动跑）</h4>
        <div className="button-row">
          <button type="button" onClick={() => generateRemotePackage(draft.id)} disabled={!plan}>
            生成远程命令包
          </button>
        </div>
        {remotePackage ? (
          <>
            <div className="remote-runner-controls">
              <label>
                执行模式
                <select value={remoteWorkflowMode} onChange={(event) => setRemoteWorkflowMode(event.target.value as RemoteWorkflowMode)}>
                  <option value="dryRun">Dry run：只预览命令</option>
                  <option value="writeFiles">只写脚本：写入 remote/ 文件</option>
                  <option value="execute">执行：运行本地 ssh/rsync</option>
                </select>
              </label>
              <label>
                超时 (秒)
                <input type="number" min={1} max={3600} value={remoteWorkflowTimeout} onChange={(event) => setRemoteWorkflowTimeout(Number(event.target.value))} />
              </label>
            </div>
            <div className="remote-command-grid">
              {remotePackage.commands.map((command) => (
                <div className="remote-command-row" key={command.id}>
                  <div>
                    <strong>{command.label}</strong>
                    <span>{command.description}</span>
                  </div>
                  <code>{command.command}</code>
                  <button type="button" onClick={() => runRemoteStep(command.id)}>运行步骤</button>
                </div>
              ))}
            </div>
            <div className="command-list">
              {remotePackage.files.map((file) => (
                <details key={file.path}>
                  <summary>{file.path}</summary>
                  <CodeBlock value={file.contents} />
                </details>
              ))}
            </div>
            {remoteWorkflowResult ? (
              <details open>
                <summary>上次步骤结果：{remoteWorkflowResult.label}（{remoteWorkflowResult.status}）</summary>
                <CodeBlock value={remoteWorkflowResult.stdout || remoteWorkflowResult.stderr || "(empty)"} />
              </details>
            ) : null}
          </>
        ) : (
          <p className="hint-text">生成后会列出 ssh / rsync / 提交 / 状态 / 回收命令，供你复制到终端手动执行。</p>
        )}

        <h4>手动状态解析（离线 / 隔离网备用）</h4>
        <div className="remote-status-grid">
          <label>
            Submit 输出
            <textarea value={remoteSubmitOutput} onChange={(event) => setRemoteSubmitOutput(event.target.value)} rows={3} spellCheck={false} />
          </label>
          <label>
            队列状态输出
            <textarea value={remoteStatusOutput} onChange={(event) => setRemoteStatusOutput(event.target.value)} rows={3} spellCheck={false} />
          </label>
          <label>
            远程日志片段
            <textarea value={remoteLogOutput} onChange={(event) => setRemoteLogOutput(event.target.value)} rows={3} spellCheck={false} />
          </label>
        </div>
        <div className="button-row">
          <button type="button" onClick={parseRemoteStatus} disabled={!remotePackage}>解析状态</button>
        </div>

        <h4>本机 ssh / rsync 等工具</h4>
        <div className="tool-list local-runtime-tools">
          {diagnostics?.tools.map((tool) => {
            const showActions = tool.status === "missingInstall" || tool.status === "missingLicense";
            const canInstall = installableTools.includes(tool.id);
            return (
              <div className={`tool-row ${showActions ? "needs-action" : ""}`} key={tool.id}>
                <div>
                  <strong>{tool.label}</strong>
                  <small>{tool.command}</small>
                </div>
                <StatusPill status={tool.status} />
                {showActions ? (
                  <div className="tool-action-row">
                    <button type="button" onClick={() => autoFindTool(tool)}>自动查找</button>
                    <button type="button" onClick={() => manualFindTool(tool)}>手动查找</button>
                    <button type="button" className={canInstall ? "primary" : ""} onClick={() => autoInstallTool(tool)}>
                      {canInstall ? "一键安装" : "查看安装方式"}
                    </button>
                  </div>
                ) : (
                  <small className="mono">{tool.detail}</small>
                )}
              </div>
            );
          })}
        </div>
      </details>
    </div>
  );
}

function PluginsPanel({
  pluginRegistry,
  selectedPluginId,
  setSelectedPluginId,
  setActiveTab,
  pluginImportPath,
  setPluginImportPath,
  pluginImportOverwrite,
  setPluginImportOverwrite,
  pluginTemplateDraft,
  setPluginTemplateDraft,
  pluginConfigDrafts,
  setPluginConfigDrafts,
  pluginRunResult,
  pluginBusy,
  openPluginFolder,
  refreshPluginRegistry,
  browsePluginManifest,
  importPlugin,
  createPluginTemplate,
  setUserPluginEnabled,
  deleteUserPlugin,
  savePluginConfig,
  runPluginAction,
  openPluginInstallFolder
}: {
  pluginRegistry: PluginRegistrySnapshot | null;
  selectedPluginId: string | null;
  setSelectedPluginId: (id: string | null) => void;
  setActiveTab: (tab: TabId) => void;
  pluginImportPath: string;
  setPluginImportPath: (value: string) => void;
  pluginImportOverwrite: boolean;
  setPluginImportOverwrite: (value: boolean) => void;
  pluginTemplateDraft: PluginTemplateRequest;
  setPluginTemplateDraft: (value: PluginTemplateRequest) => void;
  pluginConfigDrafts: Record<string, string>;
  setPluginConfigDrafts: (value: Record<string, string>) => void;
  pluginRunResult: PluginRunResult | null;
  pluginBusy: boolean;
  openPluginFolder: () => void;
  refreshPluginRegistry: () => void;
  browsePluginManifest: () => void;
  importPlugin: () => void;
  createPluginTemplate: () => void;
  setUserPluginEnabled: (pluginId: string, enabled: boolean) => void;
  deleteUserPlugin: (pluginId: string) => void;
  savePluginConfig: (manifest: PluginManifest) => void;
  runPluginAction: (manifest: PluginManifest, action: PluginAction, mode: PluginRunMode) => void;
  openPluginInstallFolder: (pluginId: string) => void;
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

  const userPlugins = pluginRegistry.manifests.filter((manifest) => manifest.origin === "user");
  const builtinPlugins = pluginRegistry.manifests.filter((manifest) => manifest.origin === "builtIn");
  const selectedManifest = pluginRegistry.manifests.find((manifest) => manifest.id === selectedPluginId) ?? userPlugins[0] ?? null;

  return (
    <div className="content-grid">
      <section className="panel">
        <div className="panel-title-row">
          <h3>插件目录</h3>
          <div className="button-row">
            <button type="button" onClick={refreshPluginRegistry}>刷新扫描</button>
            <button type="button" onClick={openPluginFolder}>打开插件目录</button>
          </div>
        </div>
        <dl className="definition-list">
          <div><dt>路径</dt><dd className="mono">{pluginRegistry.pluginRoot}</dd></div>
          <div><dt>manifest</dt><dd>{pluginRegistry.manifests.length}</dd></div>
          <div><dt>用户插件</dt><dd>{userPlugins.length}</dd></div>
          <div><dt>启用</dt><dd>{userPlugins.filter((plugin) => plugin.enabled).length}</dd></div>
          <div><dt>外部警告</dt><dd>{pluginRegistry.warnings.length}</dd></div>
        </dl>
        {pluginRegistry.warnings.length ? (
          <div className="warning-stack">
            {pluginRegistry.warnings.map((warning) => <p key={warning}>{warning}</p>)}
          </div>
        ) : null}
      </section>

      <section className="panel span-2">
        <div className="panel-title-row">
          <div>
            <h3>导入 / 新建插件</h3>
            <p className="muted">导入已有插件目录或 manifest；也可以用高级模板快速创建一个用户插件并接入软件。</p>
          </div>
        </div>
        <div className="plugin-builder-grid">
          <div className="plugin-import-box">
            <h4>导入插件</h4>
            <label>
              插件目录或 manifest
              <div className="input-with-button">
                <input value={pluginImportPath} onChange={(event) => setPluginImportPath(event.target.value)} placeholder="/path/to/plugin 或 *.automd-plugin.json" />
                <button type="button" onClick={browsePluginManifest}>浏览</button>
              </div>
            </label>
            <label className="check-row">
              <input type="checkbox" checked={pluginImportOverwrite} onChange={(event) => setPluginImportOverwrite(event.target.checked)} />
              <span>允许覆盖同 ID 的用户插件（不会覆盖 built-in）</span>
            </label>
            <button type="button" className="primary" onClick={importPlugin} disabled={pluginBusy}>导入插件</button>
          </div>
          <details className="plugin-import-box" open>
            <summary>高级：快速创建插件</summary>
            <div className="form-grid two plugin-template-grid">
              <label>
                插件名称
                <input value={pluginTemplateDraft.name} onChange={(event) => setPluginTemplateDraft({ ...pluginTemplateDraft, name: event.target.value })} />
              </label>
              <label>
                插件 ID
                <input value={pluginTemplateDraft.id} onChange={(event) => setPluginTemplateDraft({ ...pluginTemplateDraft, id: event.target.value })} />
              </label>
              <label>
                类型
                <select value={pluginTemplateDraft.kind} onChange={(event) => setPluginTemplateDraft({ ...pluginTemplateDraft, kind: event.target.value as PluginKind })}>
                  {(Object.keys(pluginKindText) as PluginKind[]).map((kind) => <option key={kind} value={kind}>{pluginKindText[kind]}</option>)}
                </select>
              </label>
              <label>
                入口语言
                <select value={pluginTemplateDraft.language} onChange={(event) => setPluginTemplateDraft({ ...pluginTemplateDraft, language: event.target.value })}>
                  <option value="python">Python</option>
                  <option value="javascript">JavaScript / Node</option>
                  <option value="bash">Bash</option>
                </select>
              </label>
              <label>
                联动目标
                <input value={pluginTemplateDraft.target ?? ""} onChange={(event) => setPluginTemplateDraft({ ...pluginTemplateDraft, target: event.target.value })} placeholder="workflow / engines / report" />
              </label>
              <label>
                描述
                <input value={pluginTemplateDraft.description ?? ""} onChange={(event) => setPluginTemplateDraft({ ...pluginTemplateDraft, description: event.target.value })} />
              </label>
            </div>
            <button type="button" className="primary" onClick={createPluginTemplate} disabled={pluginBusy}>快速创建并启用</button>
          </details>
        </div>
      </section>

      <section className="panel span-3">
        <h3>插件列表</h3>
        <h4>用户插件</h4>
        {userPlugins.length ? (
          <div className="engine-grid plugin-grid">
            {userPlugins.map((manifest) => (
              <PluginCard
                key={manifest.id}
                manifest={manifest}
                selected={selectedManifest?.id === manifest.id}
                pluginBusy={pluginBusy}
                onSelect={() => setSelectedPluginId(manifest.id)}
                onOpenDetail={() => {
                  setSelectedPluginId(manifest.id);
                  setActiveTab("pluginDetail");
                }}
                onToggle={() => setUserPluginEnabled(manifest.id, !manifest.enabled)}
                onDelete={() => {
                  if (window.confirm(`确定删除用户插件「${manifest.name}」吗？此操作会移除插件目录和配置。`)) {
                    deleteUserPlugin(manifest.id);
                  }
                }}
                onOpenFolder={() => openPluginInstallFolder(manifest.id)}
              />
            ))}
          </div>
        ) : (
          <EmptyState title="暂无用户插件" text="可以导入插件目录，或用高级模板快速创建一个插件。" />
        )}
        <h4>内置插件</h4>
        <div className="engine-grid plugin-grid">
          {builtinPlugins.map((manifest) => (
            <PluginCard
              key={manifest.id}
              manifest={manifest}
              selected={selectedManifest?.id === manifest.id}
              pluginBusy={pluginBusy}
              onSelect={() => setSelectedPluginId(manifest.id)}
              onOpenDetail={() => setSelectedPluginId(manifest.id)}
              onToggle={() => undefined}
              onDelete={() => undefined}
              onOpenFolder={() => undefined}
            />
          ))}
        </div>
      </section>

      <section className="panel span-3">
        <PluginDetail
          manifest={selectedManifest}
          pluginConfigDrafts={pluginConfigDrafts}
          setPluginConfigDrafts={setPluginConfigDrafts}
          pluginRunResult={pluginRunResult}
          pluginBusy={pluginBusy}
          setUserPluginEnabled={setUserPluginEnabled}
          deleteUserPlugin={deleteUserPlugin}
          savePluginConfig={savePluginConfig}
          runPluginAction={runPluginAction}
          openPluginInstallFolder={openPluginInstallFolder}
        />
      </section>

      <section className="panel span-3">
        <details className="plugin-guide" open>
          <summary>插件构建与接入指引</summary>
          <div className="guide-section">
            <p className="muted">插件目录由当前系统的应用数据目录动态生成，不会写死某个用户名或某台电脑的绝对路径。插件页会显示本机实际目录，也可以一键打开。</p>
            <div className="guide-table">
              <div className="guide-table-head">字段</div>
              <div className="guide-table-head">用途</div>
              <div className="guide-table-head">注意</div>
              <div><strong>id / name / kind / version</strong></div><div>标识插件、显示名称、类型和版本。</div><div>ID 只能使用小写 ASCII、数字、短横线或下划线。</div>
              <div><strong>entrypoint</strong></div><div>插件入口脚本，相对插件目录。</div><div>沙盒模式禁止绝对路径和 .. 跳出目录。</div>
              <div><strong>actions</strong></div><div>声明可运行动作、命令和参数。</div><div>不写 action 时会生成默认动作。</div>
              <div><strong>integrationTargets</strong></div><div>声明联动页面，例如 workflow、engines、remote、build、report。</div><div>v1 只做声明式入口和统一详情页。</div>
              <div><strong>configSchema / defaultConfig</strong></div><div>说明配置结构和默认值。</div><div>当前用 JSON 编辑，保存到 SQLite。</div>
              <div><strong>permissions</strong></div><div>声明读取项目、写 sandbox、直接运行等权限。</div><div>直接运行必须二次确认。</div>
            </div>
            <p className="muted">沙盒运行会通过 JSON stdin 接收当前项目、当前结构、SimulationPlan 和允许输出目录；stdout 可返回 JSON：<span className="mono">{"{ artifacts: [], warnings: [], logs: [] }"}</span>。</p>
          </div>
        </details>
      </section>
    </div>
  );
}

function PluginCard({
  manifest,
  selected,
  pluginBusy,
  onSelect,
  onOpenDetail,
  onToggle,
  onDelete,
  onOpenFolder
}: {
  manifest: PluginManifest;
  selected: boolean;
  pluginBusy: boolean;
  onSelect: () => void;
  onOpenDetail: () => void;
  onToggle: () => void;
  onDelete: () => void;
  onOpenFolder: () => void;
}) {
  const isBuiltIn = manifest.origin === "builtIn";
  return (
    <article className={`engine-card plugin-card ${selected ? "selected" : ""}`} onClick={onSelect}>
      <div className="engine-card-head">
        <strong>{manifest.name}</strong>
        <span className={`status-pill ${manifest.enabled ? "ready" : "missingInstall"}`}>{isBuiltIn ? "built-in" : manifest.enabled ? "已启用" : "已停用"}</span>
      </div>
      <dl className="compact-dl">
        <div><dt>ID</dt><dd className="mono">{manifest.id}</dd></div>
        <div><dt>类型</dt><dd>{pluginKindText[manifest.kind]}</dd></div>
        <div><dt>入口</dt><dd className="mono truncate">{manifest.entrypoint}</dd></div>
        <div><dt>联动</dt><dd>{manifest.integrationTargets.join(", ") || "通用详情"}</dd></div>
        <div><dt>来源</dt><dd className="mono truncate">{manifest.installPath ?? manifest.sourcePath ?? "built-in"}</dd></div>
      </dl>
      <div className="chip-row">
        {manifest.capabilities.slice(0, 6).map((capability) => <span key={capability}>{capability}</span>)}
        {manifest.validationStatus !== "valid" ? <span>{manifest.validationStatus}</span> : null}
      </div>
      {manifest.warnings.length ? (
        <div className="warning-stack compact-warning">
          {manifest.warnings.slice(0, 2).map((warning) => <p key={warning}>{warning}</p>)}
        </div>
      ) : null}
      <div className="plugin-card-actions" onClick={(event) => event.stopPropagation()}>
        <button type="button" onClick={onOpenDetail}>详情</button>
        {!isBuiltIn ? <button type="button" onClick={onOpenFolder}>打开目录</button> : null}
        {!isBuiltIn ? <button type="button" onClick={onToggle} disabled={pluginBusy}>{manifest.enabled ? "停用" : "启用"}</button> : null}
        {!isBuiltIn ? <button type="button" className="danger-lite" onClick={onDelete} disabled={pluginBusy}>删除</button> : <button type="button" disabled>内置只读</button>}
      </div>
    </article>
  );
}

function PluginDetailPage({
  manifest,
  pluginConfigDrafts,
  setPluginConfigDrafts,
  pluginRunResult,
  pluginBusy,
  setActiveTab,
  setUserPluginEnabled,
  deleteUserPlugin,
  savePluginConfig,
  runPluginAction,
  openPluginInstallFolder
}: {
  manifest: PluginManifest | null;
  pluginConfigDrafts: Record<string, string>;
  setPluginConfigDrafts: (value: Record<string, string>) => void;
  pluginRunResult: PluginRunResult | null;
  pluginBusy: boolean;
  setActiveTab: (tab: TabId) => void;
  setUserPluginEnabled: (pluginId: string, enabled: boolean) => void;
  deleteUserPlugin: (pluginId: string) => void;
  savePluginConfig: (manifest: PluginManifest) => void;
  runPluginAction: (manifest: PluginManifest, action: PluginAction, mode: PluginRunMode) => void;
  openPluginInstallFolder: (pluginId: string) => void;
}) {
  return (
    <div className="content-grid">
      <section className="panel span-3">
        <div className="panel-title-row">
          <div>
            <h3>{manifest?.name ?? "插件详情"}</h3>
            <p className="muted">统一插件详情页：配置、能力、联动位置和安全运行都在这里完成。</p>
          </div>
          <button type="button" onClick={() => setActiveTab("plugins")}>返回插件页</button>
        </div>
        <PluginDetail
          manifest={manifest}
          pluginConfigDrafts={pluginConfigDrafts}
          setPluginConfigDrafts={setPluginConfigDrafts}
          pluginRunResult={pluginRunResult}
          pluginBusy={pluginBusy}
          setUserPluginEnabled={setUserPluginEnabled}
          deleteUserPlugin={deleteUserPlugin}
          savePluginConfig={savePluginConfig}
          runPluginAction={runPluginAction}
          openPluginInstallFolder={openPluginInstallFolder}
        />
      </section>
    </div>
  );
}

function PluginDetail({
  manifest,
  pluginConfigDrafts,
  setPluginConfigDrafts,
  pluginRunResult,
  pluginBusy,
  setUserPluginEnabled,
  deleteUserPlugin,
  savePluginConfig,
  runPluginAction,
  openPluginInstallFolder
}: {
  manifest: PluginManifest | null;
  pluginConfigDrafts: Record<string, string>;
  setPluginConfigDrafts: (value: Record<string, string>) => void;
  pluginRunResult: PluginRunResult | null;
  pluginBusy: boolean;
  setUserPluginEnabled: (pluginId: string, enabled: boolean) => void;
  deleteUserPlugin: (pluginId: string) => void;
  savePluginConfig: (manifest: PluginManifest) => void;
  runPluginAction: (manifest: PluginManifest, action: PluginAction, mode: PluginRunMode) => void;
  openPluginInstallFolder: (pluginId: string) => void;
}) {
  if (!manifest) {
    return <EmptyState title="未选择插件" text="从插件列表或左侧用户插件入口选择一个插件。" />;
  }
  const isBuiltIn = manifest.origin === "builtIn";
  const configText = pluginConfigDrafts[manifest.id] ?? JSON.stringify(manifest.config ?? manifest.defaultConfig ?? {}, null, 2);
  return (
    <div className="plugin-detail">
      <div className="plugin-detail-grid">
        <dl className="definition-list">
          <div><dt>ID</dt><dd className="mono">{manifest.id}</dd></div>
          <div><dt>类型</dt><dd>{pluginKindText[manifest.kind]}</dd></div>
          <div><dt>来源</dt><dd>{isBuiltIn ? "built-in" : "user"}</dd></div>
          <div><dt>状态</dt><dd>{manifest.enabled ? "已启用" : "已停用"}</dd></div>
          <div><dt>入口</dt><dd className="mono">{manifest.entrypoint}</dd></div>
          <div><dt>安装目录</dt><dd className="mono">{manifest.installPath ?? manifest.sourcePath ?? "内置能力"}</dd></div>
          <div><dt>联动页面</dt><dd>{manifest.integrationTargets.join(", ") || "通用详情页"}</dd></div>
          <div><dt>平台</dt><dd>{manifest.supportedPlatforms.join(", ") || "未声明"}</dd></div>
        </dl>
        <div className="plugin-detail-side">
          <p>{manifest.description ?? "该插件未提供描述。"}</p>
          <div className="chip-row">
            {manifest.capabilities.map((capability) => <span key={capability}>{capability}</span>)}
          </div>
          {manifest.permissions.length ? <p className="muted">权限声明：{manifest.permissions.join(", ")}</p> : null}
          {manifest.warnings.length ? (
            <div className="warning-stack compact-warning">
              {manifest.warnings.map((warning) => <p key={warning}>{warning}</p>)}
            </div>
          ) : null}
          <div className="button-row">
            {!isBuiltIn ? <button type="button" onClick={() => openPluginInstallFolder(manifest.id)}>打开目录</button> : null}
            {!isBuiltIn ? <button type="button" onClick={() => setUserPluginEnabled(manifest.id, !manifest.enabled)} disabled={pluginBusy}>{manifest.enabled ? "停用" : "启用"}</button> : null}
            {!isBuiltIn ? <button type="button" className="danger-lite" onClick={() => {
              if (window.confirm(`确定删除用户插件「${manifest.name}」吗？`)) deleteUserPlugin(manifest.id);
            }} disabled={pluginBusy}>删除插件</button> : <button type="button" disabled>内置插件只读</button>}
          </div>
        </div>
      </div>
      {!isBuiltIn ? (
        <div className="plugin-config-row">
          <label>
            插件配置 JSON
            <textarea
              value={configText}
              onChange={(event) => setPluginConfigDrafts({ ...pluginConfigDrafts, [manifest.id]: event.target.value })}
              rows={8}
            />
          </label>
          <button type="button" className="primary" onClick={() => savePluginConfig(manifest)} disabled={pluginBusy}>保存配置</button>
        </div>
      ) : null}
      <div className="plugin-actions-panel">
        <h4>插件动作</h4>
        {!isBuiltIn && manifest.actions.length ? (
          <div className="plugin-action-list">
            {manifest.actions.map((action) => (
              <div className="plugin-action-row" key={action.id}>
                <div>
                  <strong>{action.label}</strong>
                  <small>{action.description ?? action.id}</small>
                  <code>{action.command ?? "按 entrypoint 推断"} {action.args.join(" ")}</code>
                </div>
                <div className="button-row">
                  <button type="button" className="primary" onClick={() => runPluginAction(manifest, action, "sandbox")} disabled={pluginBusy || !manifest.enabled}>沙盒运行</button>
                  <button type="button" className="danger-lite" onClick={() => runPluginAction(manifest, action, "direct")} disabled={pluginBusy || !manifest.enabled}>直接运行</button>
                </div>
              </div>
            ))}
          </div>
        ) : (
          <EmptyState title={isBuiltIn ? "内置能力不可运行" : "暂无动作"} text={isBuiltIn ? "built-in 插件由 AutoMD 内部模块调用，不通过用户插件 runner 执行。" : "在 manifest.actions 中声明动作后可在这里运行。"} />
        )}
        {pluginRunResult ? (
          <div className="plugin-run-result">
            <h4>最近一次插件运行</h4>
            <dl className="definition-list">
              <div><dt>插件</dt><dd>{pluginRunResult.record.pluginId}</dd></div>
              <div><dt>动作</dt><dd>{pluginRunResult.record.actionId}</dd></div>
              <div><dt>模式</dt><dd>{pluginRunResult.record.mode}</dd></div>
              <div><dt>状态</dt><dd>{pluginRunResult.record.status}</dd></div>
            </dl>
            <CodeBlock value={`STDOUT\n${pluginRunResult.stdout || "(empty)"}\n\nSTDERR\n${pluginRunResult.stderr || "(empty)"}`} />
          </div>
        ) : null}
      </div>
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

function formatBytes(value?: number | null) {
  if (value == null || !Number.isFinite(value)) {
    return "未知";
  }
  const units = ["B", "KB", "MB", "GB", "TB", "PB"];
  let normalized = value;
  let unitIndex = 0;
  while (normalized >= 1024 && unitIndex < units.length - 1) {
    normalized /= 1024;
    unitIndex += 1;
  }
  const precision = unitIndex === 0 ? 0 : normalized >= 100 ? 0 : 1;
  return `${normalized.toFixed(precision)} ${units[unitIndex]}`;
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

function PencilIcon() {
  // Crisp stroke pencil — the bare ✎ glyph rendered thin/faint and looked broken.
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M12 20h9" />
      <path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4 12.5-12.5z" />
    </svg>
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
    return <EmptyState title="等待检查" text="创建项目或修改参数后，AutoMD 会自动检查是否缺少必须处理的问题。" />;
  }
  const summaryText: Record<ValidationReport["status"], { title: string; text: string }> = {
    valid: {
      title: "参数检查通过",
      text: "暂未发现必须处理的问题，可以继续生成结构准备文件或运行包。"
    },
    validWithWarnings: {
      title: "有提示需要阅读",
      text: "可以继续，但建议先看下面的 warning，确认它们符合你的体系和引擎选择。"
    },
    invalid: {
      title: "需要先修正参数",
      text: "存在 error 时不要运行；先按下面的字段和说明修改参数或结构输入。"
    }
  };
  const summary = summaryText[validation.status];
  return (
    <div className="validation-list">
      <div className={`validation-summary ${validation.status}`}>
        <strong>{summary.title}</strong>
        <span>{validation.items.length ? `${validation.items.length} 条提示` : summary.text}</span>
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
