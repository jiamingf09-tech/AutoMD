import type {
  DetectionStatus,
  ExecutionMode,
  FailureAnalysis,
  GpuBackend,
  LocalRunMode,
  ParameterMappingStatus,
  RemoteHelperStatus,
  ValidationSeverity,
  BuildWorkflowMode
} from "../types";

export const engineLabel: Record<string, string> = {
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

export const statusText: Record<DetectionStatus, string> = {
  ready: "可用",
  missingInstall: "需安装",
  missingLicense: "需许可",
  platformUnsupported: "平台不支持",
  remoteRecommended: "建议远程",
  notApplicable: "不适用"
};

export const remoteHelperStateText: Record<RemoteHelperStatus["status"], string> = {
  missing: "未安装 helper",
  ready: "已安装",
  outdated: "版本过旧",
  unreachable: "远程不可达",
  permissionDenied: "权限不足"
};

export const executionModeText: Record<ExecutionMode, string> = {
  localProcess: "本地进程",
  condaEnvironment: "Conda 环境",
  container: "容器",
  wsl2: "WSL2",
  ssh: "SSH",
  slurm: "SLURM",
  pbs: "PBS",
  lsf: "LSF"
};

export const gpuBackendText: Record<GpuBackend, string> = {
  cuda: "CUDA",
  rocm: "ROCm",
  openCl: "OpenCL",
  metal: "Metal",
  sycl: "SYCL",
  cpuOnly: "CPU"
};

export const localRunModeText: Record<LocalRunMode, string> = {
  dryRun: "Dry run",
  mock: "Mock runner",
  real: "真实本地执行"
};

export const failureCategoryText: Record<FailureAnalysis["category"], string> = {
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

export const severityText: Record<ValidationSeverity, string> = {
  info: "信息",
  warning: "警告",
  error: "错误"
};

export const buildWorkflowModeText: Record<BuildWorkflowMode, string> = {
  dryRun: "Dry run",
  writeFiles: "只写脚本",
  execute: "执行构建"
};

export const parameterMappingStatusText: Record<ParameterMappingStatus, string> = {
  mapped: "已映射",
  approximated: "近似映射",
  unsupported: "未支持",
  manualReview: "需复核"
};

