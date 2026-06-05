import type {
  AnalysisParseRequest,
  AnalysisParseResult,
  AnalysisCacheRecord,
  BatchExperimentPackage,
  BatchExperimentRequest,
  BuildRecipe,
  BuildRecipeOptions,
  BuildWorkflowRequest,
  BuildWorkflowResult,
  ContainerRecipe,
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
  PluginManifest,
  PluginRegistrySnapshot,
  ProjectTextFilePayload,
  ProjectTextFileRequest,
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

const now = () => new Date().toISOString();
const randomId = () =>
  globalThis.crypto?.randomUUID?.() ?? `mock-${Math.random().toString(36).slice(2)}`;

export const mockEngines: EngineCapability[] = [
  {
    id: "gromacs",
    name: "GROMACS",
    category: "biomolecular",
    maturity: "firstClass",
    license: {
      class: "openSource",
      distribution: "installerRecipe",
      bundledByAutomd: false,
      requiresUserLicense: false,
      guidance: "开源优先集成，提供安装、容器和编译 recipe。"
    },
    platformSupport: {
      native: ["windows", "macos", "linux"],
      recommendedFallbacks: ["wsl2", "remoteLinux"]
    },
    executableNames: ["gmx", "gmx_mpi"],
    gpuBackends: ["cuda", "openCl", "sycl", "cpuOnly"],
    executionModes: ["localProcess", "container", "ssh", "slurm"],
    supportedInputs: ["pdb", "gro", "top", "mdp"],
    supportedOutputs: ["xtc", "trr", "edr", "log"],
    supportedStages: [
      "structurePreparation",
      "energyMinimization",
      "nvtEquilibration",
      "nptEquilibration",
      "production",
      "analysis"
    ],
    detection: {
      status: "missingInstall",
      message: "Web 预览模式未访问本机 PATH。"
    },
    docsUrl: "https://manual.gromacs.org/documentation/current/",
    notes: ["首版完整闭环目标。"]
  },
  {
    id: "openmm",
    name: "OpenMM",
    category: "biomolecular",
    maturity: "firstClass",
    license: {
      class: "openSource",
      distribution: "installerRecipe",
      bundledByAutomd: false,
      requiresUserLicense: false,
      guidance: "通过 Python 科学侧车环境安装。"
    },
    platformSupport: {
      native: ["windows", "macos", "linux"],
      recommendedFallbacks: ["remoteLinux"]
    },
    executableNames: ["python module: openmm"],
    gpuBackends: ["cuda", "openCl", "cpuOnly"],
    executionModes: ["condaEnvironment", "localProcess", "container", "ssh"],
    supportedInputs: ["pdb", "xml", "sdf"],
    supportedOutputs: ["dcd", "pdb", "log"],
    supportedStages: [
      "structurePreparation",
      "energyMinimization",
      "nvtEquilibration",
      "nptEquilibration",
      "production",
      "analysis"
    ],
    detection: {
      status: "missingInstall",
      message: "Web 预览模式未访问 Python 环境。"
    },
    docsUrl: "https://openmm.org/documentation",
    notes: ["跨平台脚本后端。"]
  },
  {
    id: "ambertools",
    name: "AmberTools",
    category: "biomolecular",
    maturity: "supported",
    license: {
      class: "freeToolkit",
      distribution: "installerRecipe",
      bundledByAutomd: false,
      requiresUserLicense: false,
      guidance: "可自由获取；通过 tleap/sander/cpptraj 生成和运行 AMBER 输入生态。"
    },
    platformSupport: {
      native: ["windows", "macos", "linux"],
      recommendedFallbacks: ["wsl2", "remoteLinux"]
    },
    executableNames: ["tleap", "sander", "cpptraj"],
    gpuBackends: ["cpuOnly"],
    executionModes: ["condaEnvironment", "localProcess", "container", "ssh", "slurm"],
    supportedInputs: ["pdb", "mol2", "frcmod", "prmtop", "inpcrd", "mdin"],
    supportedOutputs: ["prmtop", "inpcrd", "nc", "mdout", "rst7"],
    supportedStages: [
      "structurePreparation",
      "energyMinimization",
      "nvtEquilibration",
      "nptEquilibration",
      "production",
      "analysis"
    ],
    detection: {
      status: "missingInstall",
      message: "Web 预览模式未访问本机 AmberTools 环境。"
    },
    docsUrl: "https://ambermd.org/AmberTools.php",
    notes: ["支持生成 tleap/mdin/cpptraj 模板。"]
  },
  {
    id: "namd",
    name: "NAMD",
    category: "biomolecular",
    maturity: "externalOnly",
    license: {
      class: "restrictedAcademic",
      distribution: "userLicenseRequired",
      bundledByAutomd: false,
      requiresUserLicense: true,
      guidance: "用户自行下载并确认许可后配置路径。"
    },
    platformSupport: {
      native: ["windows", "macos", "linux"],
      recommendedFallbacks: ["remoteLinux"]
    },
    executableNames: ["namd3", "namd2"],
    gpuBackends: ["cuda", "cpuOnly"],
    executionModes: ["localProcess", "ssh", "slurm"],
    supportedInputs: ["pdb", "psf", "conf"],
    supportedOutputs: ["dcd", "xst", "log"],
    supportedStages: [
      "structurePreparation",
      "energyMinimization",
      "nvtEquilibration",
      "nptEquilibration",
      "production",
      "analysis"
    ],
    detection: {
      status: "missingLicense",
      message: "受限模块，需要用户自带许可。"
    },
    docsUrl: "https://www.ks.uiuc.edu/Research/namd/",
    notes: ["仅提供适配器和授权向导。"]
  }
];

export const mockEngineInstallations: EngineInstallationRecord[] = [
  {
    targetKind: "local",
    targetId: "local",
    targetLabel: "本机",
    engineId: "namd",
    location: "/opt/user-licensed/namd3",
    version: "NAMD 3.0 user configured",
    authorizationStatus: "missingLicense",
    platform: "linux",
    arch: "x86_64",
    checkedAt: now()
  }
];

export const mockDiagnostics: RuntimeDiagnostics = {
  os: "web-preview",
  arch: "browser",
  gpu: {
    available: false,
    mode: "cpuFallback",
    backend: null,
    label: "GPU 不可用：CPU 模式",
    reason: "Web 预览模式无法访问本机 GPU。",
    detail: "预览环境按 CPU fallback 展示；桌面应用启动时会检测 CUDA、ROCm 或 macOS Metal。",
    checkedAt: now()
  },
  hardware: {
    cpu: {
      brand: "Web preview CPU",
      architecture: "browser",
      logicalCores: 8,
      physicalCores: null
    },
    memory: {
      totalBytes: 16 * 1024 ** 3,
      availableBytes: null,
      detail: "Web 预览模式使用示例内存。"
    },
    gpus: [
      {
        id: "cpu",
        name: "CPU 模式",
        vendor: "None",
        backend: null,
        memoryBytes: null,
        detail: "Web 预览模式无法访问真实 GPU。"
      }
    ],
    disks: [
      {
        id: "disk0",
        mountPoint: "/mock",
        filesystem: "mockfs",
        totalBytes: 512 * 1024 ** 3,
        availableBytes: 256 * 1024 ** 3,
        detail: "Web 预览磁盘。"
      }
    ]
  },
  tools: [
    { id: "conda", label: "Conda", command: "conda", status: "missingInstall", detail: "Web 预览模式" },
    { id: "docker", label: "Docker", command: "docker", status: "missingInstall", detail: "Web 预览模式" },
    { id: "ssh", label: "SSH", command: "ssh", status: "missingInstall", detail: "Web 预览模式" },
    { id: "sbatch", label: "SLURM sbatch", command: "sbatch", status: "missingInstall", detail: "Web 预览模式" },
    { id: "nvidia-smi", label: "CUDA / NVIDIA", command: "nvidia-smi", status: "notApplicable", detail: "Web 预览模式无法确认 NVIDIA GPU，桌面应用会按本机显卡判断。" },
    { id: "rocminfo", label: "ROCm", command: "rocminfo", status: "notApplicable", detail: "Web 预览模式无法确认 AMD GPU，桌面应用会按本机显卡判断。" }
  ]
};

export const mockScienceSidecarDiagnostics: ScienceSidecarDiagnostics = {
  pythonExecutable: "python3",
  tools: [
    { id: "openmm", label: "OpenMM", importName: "openmm", command: null, status: "missingInstall", version: null, detail: "Web 预览模式未访问 Python 环境。" },
    { id: "pdbfixer", label: "PDBFixer", importName: "pdbfixer", command: null, status: "missingInstall", version: null, detail: "Web 预览模式未访问 Python 环境。" },
    { id: "mdanalysis", label: "MDAnalysis", importName: "MDAnalysis", command: null, status: "missingInstall", version: null, detail: "Web 预览模式未访问 Python 环境。" },
    { id: "rdkit", label: "RDKit", importName: "rdkit", command: null, status: "missingInstall", version: null, detail: "Web 预览模式未访问 Python 环境。" },
    { id: "openbabel", label: "Open Babel Python", importName: "openbabel", command: null, status: "missingInstall", version: null, detail: "Web 预览模式未访问 Python 环境。" },
    { id: "tleap", label: "AmberTools tleap", importName: null, command: "tleap", status: "missingInstall", version: null, detail: "Web 预览模式未访问 PATH。" },
    { id: "antechamber", label: "AmberTools antechamber", importName: null, command: "antechamber", status: "missingInstall", version: null, detail: "Web 预览模式未访问 PATH。" },
    { id: "parmchk2", label: "AmberTools parmchk2", importName: null, command: "parmchk2", status: "missingInstall", version: null, detail: "Web 预览模式未访问 PATH。" },
    { id: "cpptraj", label: "AmberTools cpptraj", importName: null, command: "cpptraj", status: "missingInstall", version: null, detail: "Web 预览模式未访问 PATH。" }
  ],
  environmentRecipe: "name: automd-science\nchannels:\n  - conda-forge\ndependencies:\n  - python=3.11\n  - openmm\n  - pdbfixer\n  - mdanalysis\n  - rdkit\n  - openbabel\n  - ambertools\n",
  warnings: ["Web 预览模式不会检测本机 Python 科学环境。"]
};

export const mockRemoteProfiles: RemoteProfile[] = [
  {
    id: "slurm-gpu-template",
    name: "SLURM GPU cluster",
    host: "login.cluster.example",
    scheduler: "slurm",
    workdir: "/scratch/$USER/automd",
    moduleLoad: ["module load gcc openmpi cuda", "module load gromacs plumed"],
    defaultQueue: "gpu"
  },
  {
    id: "ssh-workstation-template",
    name: "SSH workstation",
    host: "workstation.example",
    scheduler: "ssh",
    workdir: "/data/automd",
    moduleLoad: ["source ~/.bashrc"],
    defaultQueue: null
  }
];

function pluginManifest(partial: Partial<PluginManifest> & Pick<PluginManifest, "id" | "name" | "kind" | "entrypoint" | "capabilities">): PluginManifest {
  return {
    version: "0.1.0",
    description: null,
    author: null,
    homepage: null,
    engineId: null,
    licensePolicy: null,
    warnings: [],
    sourcePath: null,
    supportedPlatforms: ["windows", "macos", "linux"],
    integrationTargets: [],
    actions: [],
    configSchema: null,
    defaultConfig: null,
    permissions: [],
    origin: "builtIn",
    enabled: true,
    installPath: null,
    validationStatus: "valid",
    config: null,
    ...partial
  };
}

export const mockPluginRegistry: PluginRegistrySnapshot = {
  pluginRoot: "/mock/AutoMD/plugins",
  manifests: [
    pluginManifest({
      id: "automd-core-engines",
      name: "AutoMD Core Engine Adapters",
      kind: "engineAdapter",
      entrypoint: "builtin://engine_adapters",
      engineId: "gromacs/openmm/ambertools/namd",
      capabilities: ["prepare", "run", "parse_progress", "classify_failure", "resume"],
      integrationTargets: ["engines", "run"]
    }),
    pluginManifest({
      id: "automd-core-analysis",
      name: "AutoMD Core Analysis Parsers",
      kind: "analysisModule",
      entrypoint: "builtin://analysis",
      capabilities: ["xvg", "csv", "chart_series"],
      integrationTargets: ["workflow", "run"]
    }),
    pluginManifest({
      id: "automd-core-schedulers",
      name: "AutoMD Core Remote Schedulers",
      kind: "remoteScheduler",
      entrypoint: "builtin://recipes/remote",
      capabilities: ["ssh", "slurm", "pbs", "lsf", "rsync"],
      integrationTargets: ["remote"]
    }),
    pluginManifest({
      id: "automd-core-build-recipes",
      name: "AutoMD Core Build Recipes",
      kind: "buildRecipe",
      entrypoint: "builtin://recipes/build",
      capabilities: ["container", "source_build", "plumed", "mpi", "gpu"],
      integrationTargets: ["build"]
    }),
    pluginManifest({
      id: "automd-core-report",
      name: "AutoMD Core Report Templates",
      kind: "reportTemplate",
      entrypoint: "builtin://artifacts/report",
      capabilities: ["markdown", "html", "pdf", "reproducibility_bundle"],
      integrationTargets: ["report"]
    }),
    pluginManifest({
      id: "demo-rmsd-plugin",
      name: "Demo RMSD Plugin",
      kind: "analysisModule",
      entrypoint: "entrypoint.py",
      description: "示例用户插件，用于展示左侧用户插件入口和沙盒运行。",
      capabilities: ["trajectory", "rmsd", "run"],
      integrationTargets: ["workflow", "run"],
      actions: [{ id: "default", label: "运行默认动作", description: "读取当前项目上下文并返回示例 artifact。", command: "python3", args: ["$PLUGIN_DIR/entrypoint.py"], timeoutSeconds: 30 }],
      permissions: ["projectRead", "sandboxWrite"],
      origin: "user",
      sourcePath: "/mock/AutoMD/plugins/demo-rmsd-plugin/demo-rmsd-plugin.automd-plugin.json",
      installPath: "/mock/AutoMD/plugins/demo-rmsd-plugin",
      config: { stride: 10 },
      defaultConfig: { stride: 10 },
      warnings: ["Web 预览示例插件。"]
    })
  ],
  warnings: []
};

export function mockCreateProject(request: CreateProjectRequest): ProjectSummary {
  const id = randomId();
  return {
    id,
    name: request.name,
    domain: request.domain,
    path: `/mock/AutoMD/projects/${request.name}-${id}`,
    createdAt: now(),
    lastOpenedAt: null,
    preferredEngineId: request.preferredEngineId ?? null,
    status: "draft"
  };
}

export function mockPlan(request: PlanRequest): SimulationPlan {
  return {
    id: randomId(),
    projectId: request.projectId ?? null,
    name: request.name,
    engineId: request.engineId,
    system: {
      sourceKind: "pdb",
      sourcePath: null,
      name: "protein-ligand-system",
      moleculeCount: null,
      hasLigand: true,
      hasMembrane: false,
      notes: ["导入结构后将自动更新体系摘要。"]
    },
    forceField: {
      protein: "CHARMM36m",
      waterModel: "TIP3P",
      ligand: "GAFF2 or CGenFF",
      ions: "Joung-Cheatham"
    },
    solvent: {
      model: "explicit",
      boxShape: "dodecahedron",
      paddingNm: 1,
      ionicStrengthMolar: 0.15,
      neutralize: true
    },
    resources: {
      executionMode: "localProcess",
      cpuThreads: 8,
      gpuCount: 1,
      mpiRanks: 1,
      walltimeHours: 24,
      remoteProfileId: null,
      queue: null
    },
    stages: [
      {
        id: "prepare",
        kind: "structurePreparation",
        label: "结构准备",
        enabled: true,
        parameters: { repairMissingAtoms: "true", addHydrogens: "true", parameterizeLigands: "true" },
        expectedOutputs: ["prepared_structure", "topology"]
      },
      {
        id: "em",
        kind: "energyMinimization",
        label: "能量最小化",
        enabled: true,
        parameters: { integrator: "steepest-descent", maxSteps: "50000", emtol: "1000" },
        expectedOutputs: ["minimized_structure", "energy_log"]
      },
      {
        id: "nvt",
        kind: "nvtEquilibration",
        label: "NVT 平衡",
        enabled: true,
        parameters: { durationPs: "100", temperatureK: "300", restraints: "heavy-atoms" },
        expectedOutputs: ["nvt_checkpoint", "temperature_trace"]
      },
      {
        id: "npt",
        kind: "nptEquilibration",
        label: "NPT 平衡",
        enabled: true,
        parameters: { durationPs: "1000", pressureBar: "1.0", temperatureK: "300" },
        expectedOutputs: ["npt_checkpoint", "pressure_trace", "density_trace"]
      },
      {
        id: "production",
        kind: "production",
        label: "生产模拟",
        enabled: true,
        parameters: { durationNs: "100", timestepFs: "2", checkpointEveryPs: "100" },
        expectedOutputs: ["trajectory", "checkpoint", "energy"]
      },
      {
        id: "analysis",
        kind: "analysis",
        label: "自动分析",
        enabled: true,
        parameters: { stride: "10", generateReport: "true" },
        expectedOutputs: ["analysis_tables", "figures", "report"]
      }
    ],
    outputs: {
      generatedInputs: ["generated/<engine>/automd-plan.json", "generated/<engine>/*"],
      runLogs: ["runs/<engine-plan>/*.log"],
      checkpoints: ["runs/<engine-plan>/*.{cpt,chk,rst7,restart.*}", "checkpoints/*"],
      trajectories: ["trajectories/*.{xtc,trr,dcd,nc,pdb,xyz,lammpstrj,dump,gsd}"],
      energy: ["runs/<engine-plan>/*.{edr,out,log}", "analysis/openmm_state.csv"],
      analysisTables: ["analysis/*.xvg", "analysis/*.csv", "analysis/*.json"],
      reports: ["reports/automd-report.md", "reports/automd-report.html", "reports/automd-report.pdf"]
    },
    analysis: [
      { kind: "rmsd", enabled: true, parameters: {} },
      { kind: "rmsf", enabled: true, parameters: {} },
      { kind: "radiusOfGyration", enabled: true, parameters: {} },
      { kind: "hydrogenBonds", enabled: true, parameters: {} },
      { kind: "distances", enabled: true, parameters: { atoms: "selection[0],selection[1]" } },
      { kind: "angles", enabled: true, parameters: { atoms: "selection[0],selection[1],selection[2]" } },
      { kind: "dihedrals", enabled: true, parameters: { atoms: "selection[0],selection[1],selection[2],selection[3]" } },
      { kind: "energyTerms", enabled: true, parameters: {} },
      { kind: "contacts", enabled: true, parameters: {} }
    ],
    createdAt: now()
  };
}

export function mockValidate(plan: SimulationPlan): ValidationReport {
  return {
    status: plan.stages.some((stage) => stage.enabled) ? "valid" : "invalid",
    items: plan.stages.some((stage) => stage.enabled)
      ? []
      : [{ severity: "error", field: "stages", message: "至少需要启用一个模拟阶段。" }]
  };
}

export function mockParameterMapping(request: ParameterMappingRequest): ParameterMappingReport {
  const plan = request.plan;
  const engineId = request.engineId || plan.engineId;
  const production = plan.stages.find((stage) => stage.id === "production");
  const nvt = plan.stages.find((stage) => stage.id === "nvt");
  const durationNs = Number(production?.parameters.durationNs ?? 100);
  const timestepFs = Number(production?.parameters.timestepFs ?? 2);
  const checkpointPs = Number(production?.parameters.checkpointEveryPs ?? 100);
  const steps = Math.max(1, Math.round((durationNs * 1_000_000) / Math.max(timestepFs, 0.001)));
  const interval = Math.max(1, Math.round((checkpointPs * 1000) / Math.max(timestepFs, 0.001)));
  const targetFile = engineId === "openmm"
    ? "generated/openmm/run_openmm.py"
    : engineId === "ambertools"
      ? "generated/ambertools/prod.mdin"
      : engineId === "namd"
        ? "generated/namd/automd.conf"
        : "generated/gromacs/md.mdp";

  return {
    engineId,
    planId: plan.id,
    generatedAt: now(),
    warnings: ["Web 预览模式使用模拟参数映射；Tauri 运行时由 Rust 映射器生成完整报告。"],
    items: [
      {
        stageId: "production",
        stageLabel: production?.label ?? "生产模拟",
        normalizedKey: "durationNs",
        normalizedValue: `${durationNs} ns`,
        engineKey: engineId === "ambertools" ? "nstlim" : engineId === "openmm" ? "total_steps" : engineId === "namd" ? "numsteps / run" : "nsteps",
        engineValue: String(steps),
        targetFile,
        status: "mapped",
        notes: [`由 ${durationNs} ns 和 ${timestepFs} fs 步长换算。`]
      },
      {
        stageId: "production",
        stageLabel: production?.label ?? "生产模拟",
        normalizedKey: "checkpointEveryPs",
        normalizedValue: `${checkpointPs} ps`,
        engineKey: engineId === "openmm" ? "report_interval" : engineId === "gromacs" ? "nstcheckpoint" : "restart/checkpoint interval",
        engineValue: String(interval),
        targetFile,
        status: engineId === "gromacs" || engineId === "openmm" ? "mapped" : "manualReview",
        notes: ["checkpoint/report 间隔按步数显示。"]
      },
      {
        stageId: "nvt",
        stageLabel: nvt?.label ?? "NVT 平衡",
        normalizedKey: "temperatureK",
        normalizedValue: `${nvt?.parameters.temperatureK ?? "300"} K`,
        engineKey: engineId === "openmm" ? "LangevinMiddleIntegrator temperature" : "native temperature target",
        engineValue: nvt?.parameters.temperatureK ?? "300",
        targetFile,
        status: "mapped",
        notes: ["温度参数映射到引擎热浴/积分器模板。"]
      }
    ]
  };
}

export function mockTask(plan: SimulationPlan): SimulationTask {
  return {
    id: randomId(),
    planId: plan.id,
    engineId: plan.engineId,
    status: "queued",
    currentStage: "structurePreparation",
    progressPercent: 0,
    nsPerDay: null,
    logTail: ["AutoMD task queued.", "Web preview uses mock execution."],
    createdAt: now()
  };
}

export function mockStructureImport(request: StructureImportRequest): StructureImportResult {
  const name = request.displayName?.trim() || request.sourcePath?.split(/[\\/]/).pop()?.replace(/\.[^.]+$/, "") || "imported-system";
  const slug = name.toLowerCase().replace(/[^a-z0-9_-]+/g, "-").replace(/^-+|-+$/g, "") || "imported-system";
  const extension = request.sourceKind === "mmcif"
    ? "cif"
    : request.sourceKind === "smiles"
      ? "smi"
      : request.sourceKind === "engineProject"
        ? "manifest.txt"
        : request.sourceKind;
  const isLigand = ["sdf", "mol2", "smiles"].includes(request.sourceKind);
  const summary = request.sourceKind === "pdb" || request.sourceKind === "mmcif"
    ? { atomCount: 1280, residueCount: 96, chainCount: 2, moleculeCount: 96, modelCount: 1, formatNote: `${request.sourceKind.toUpperCase()} mock summary` }
    : { atomCount: null, residueCount: null, chainCount: null, moleculeCount: 1, modelCount: null, formatNote: `${request.sourceKind} mock summary` };
  return {
    importedPath: `inputs/${slug}.${extension}`,
    summary,
    warnings: isLigand ? ["小分子输入已保存；真实 MD 前仍需要配体参数化和拓扑生成。"] : [],
    importedAt: now(),
    system: {
      sourceKind: request.sourceKind,
      sourcePath: `inputs/${slug}.${extension}`,
      name,
      moleculeCount: summary.moleculeCount,
      hasLigand: isLigand,
      hasMembrane: false,
      notes: [summary.formatNote]
    }
  };
}

export function mockStructureFile(request: StructureFileRequest): StructureFilePayload {
  const contents = [
    "ATOM      1  N   ALA A   1      -0.525   1.362   0.000  1.00 10.00           N",
    "ATOM      2  CA  ALA A   1       0.000   0.000   0.000  1.00 10.00           C",
    "ATOM      3  C   ALA A   1       1.520   0.000   0.000  1.00 10.00           C",
    "ATOM      4  O   ALA A   1       2.088  -1.044   0.000  1.00 10.00           O",
    "TER",
    "END"
  ].join("\n");
  return {
    sourcePath: request.sourcePath,
    format: request.sourcePath.toLowerCase().endsWith(".cif") || request.sourcePath.toLowerCase().endsWith(".mmcif") ? "mmcif" : "pdb",
    contents,
    sizeBytes: contents.length
  };
}

export function mockSlurm(plan: SimulationPlan): string {
  return `#!/usr/bin/env bash
#SBATCH --job-name=${plan.name}
#SBATCH --ntasks=${plan.resources.mpiRanks}
#SBATCH --cpus-per-task=${plan.resources.cpuThreads}
#SBATCH --time=${Math.ceil(plan.resources.walltimeHours)}:00:00

automd-run --plan automd-plan.json --engine ${plan.engineId}
`;
}

function mockGeneratedSlug(engineId: string): string {
  if (engineId === "openmm") return "openmm";
  if (engineId === "ambertools") return "ambertools";
  if (engineId === "namd") return "namd";
  if (
    [
      "lammps",
      "cp2k",
      "genesis",
      "hoomd",
      "dl_poly",
      "tinker",
      "amber_pmemd",
      "charmm",
      "desmond",
      "acemd"
    ].includes(engineId)
  ) return engineId;
  return "gromacs";
}

function mockRunScriptName(engineId: string): string {
  if (engineId === "openmm") return "run-openmm.sh";
  if (engineId === "ambertools") return "run-ambertools.sh";
  if (engineId === "namd") return "run-namd.sh";
  if (engineId === "lammps") return "run-lammps.sh";
  if (engineId === "cp2k") return "run-cp2k.sh";
  if (engineId === "genesis") return "run-genesis.sh";
  if (engineId === "hoomd") return "run-hoomd.sh";
  if (engineId === "dl_poly") return "run-dl-poly.sh";
  if (engineId === "tinker") return "run-tinker.sh";
  if (engineId === "amber_pmemd") return "run-amber-pmemd.sh";
  if (engineId === "charmm") return "run-charmm.sh";
  if (engineId === "desmond") return "run-desmond.sh";
  if (engineId === "acemd") return "run-acemd.sh";
  return "run-gromacs.sh";
}

export function mockRemoteExecutionPackage(request: RemoteExecutionRequest): RemoteExecutionPackage {
  const runDirectory = `runs/${request.plan.engineId}-${request.plan.id}`;
  const remoteWorkdir = `${request.profile.workdir}/${request.plan.name.replace(/[^A-Za-z0-9_-]+/g, "-")}-${request.plan.id}`;
  const schedulerFile = request.profile.scheduler === "pbs"
    ? "remote/submit.pbs"
    : request.profile.scheduler === "lsf"
      ? "remote/submit.lsf"
      : request.profile.scheduler === "slurm"
        ? "remote/submit.slurm"
        : "remote/run-ssh.sh";
  const submit = request.profile.scheduler === "slurm"
    ? `ssh ${request.profile.host} 'cd ${remoteWorkdir} && sbatch --parsable ${schedulerFile}'`
    : request.profile.scheduler === "pbs"
      ? `ssh ${request.profile.host} 'cd ${remoteWorkdir} && qsub ${schedulerFile}'`
      : request.profile.scheduler === "lsf"
        ? `ssh ${request.profile.host} 'cd ${remoteWorkdir} && bsub < ${schedulerFile}'`
        : `ssh ${request.profile.host} 'cd ${remoteWorkdir} && nohup bash ${schedulerFile} > logs/automd-ssh.out 2> logs/automd-ssh.err & echo $!'`;
  return {
    engineId: request.plan.engineId,
    scheduler: request.profile.scheduler,
    profileId: request.profile.id,
    remoteWorkdir,
    runDirectory,
    files: [
      {
        path: schedulerFile,
        language: request.profile.scheduler,
        contents: `${request.profile.moduleLoad.join("\n")}\ncd ${remoteWorkdir}\nbash ${runDirectory}/${mockRunScriptName(request.plan.engineId)}\n`
      },
      {
        path: "remote/sync-up.sh",
        language: "bash",
        contents: `rsync -az --delete --partial --append-verify ${request.localProjectPath ?? "."}/ ${request.profile.host}:${remoteWorkdir}/\n`
      },
      {
        path: "remote/sync-down.sh",
        language: "bash",
        contents: `rsync -az --partial --append-verify ${request.profile.host}:${remoteWorkdir}/runs/ ${request.localProjectPath ?? "."}/runs/\n`
      }
    ],
    commands: [
      {
        id: "sync-up",
        label: "同步到远程",
        command: `rsync -az --delete --partial --append-verify ${request.localProjectPath ?? "."}/ ${request.profile.host}:${remoteWorkdir}/`,
        description: "Web 预览模式远程同步命令。"
      },
      {
        id: "submit",
        label: "提交任务",
        command: submit,
        description: "Web 预览模式提交命令。"
      },
      {
        id: "status",
        label: "查询状态",
        command: request.profile.scheduler === "slurm"
          ? `ssh ${request.profile.host} 'squeue -j <job-id>'`
          : request.profile.scheduler === "pbs"
            ? `ssh ${request.profile.host} 'qstat <job-id>'`
            : request.profile.scheduler === "lsf"
              ? `ssh ${request.profile.host} 'bjobs <job-id>'`
              : `ssh ${request.profile.host} 'ps -p <pid> -o pid,etime,cmd'`,
        description: "查询调度器或远程进程状态。"
      },
      {
        id: "cancel",
        label: "取消任务",
        command: request.profile.scheduler === "slurm"
          ? `ssh ${request.profile.host} 'scancel <job-id>'`
          : request.profile.scheduler === "pbs"
            ? `ssh ${request.profile.host} 'qdel <job-id>'`
            : request.profile.scheduler === "lsf"
              ? `ssh ${request.profile.host} 'bkill <job-id>'`
              : `ssh ${request.profile.host} 'kill <pid>'`,
        description: "取消调度器任务或远程进程。"
      },
      {
        id: "tail-log",
        label: "读取远程日志",
        command: `ssh ${request.profile.host} 'cd ${remoteWorkdir} && tail -n 200 logs/*.out logs/*.err runs/*/*.log 2>/dev/null || true'`,
        description: "读取远程日志尾部供 GUI 解析。"
      },
      {
        id: "sync-down",
        label: "回收结果",
        command: `rsync -az --partial --append-verify ${request.profile.host}:${remoteWorkdir}/analysis/ ${request.localProjectPath ?? "."}/analysis/`,
        description: "回收分析结果。"
      }
    ],
    warnings: ["Web 预览模式不会连接远程主机；请在 Tauri 桌面模式中生成最终脚本。"]
  };
}

export function mockRemoteJobSnapshot(request: RemoteStatusParseRequest): RemoteJobSnapshot {
  const jobId = request.submitOutput?.match(/\d+/)?.[0] ?? "123456";
  const statusText = `${request.statusOutput ?? ""} ${request.logOutput ?? ""}`.toUpperCase();
  const status: RemoteJobSnapshot["status"] = statusText.includes("FAILED") || statusText.includes("EXIT")
    ? "failed"
    : statusText.includes("COMPLETED") || statusText.includes("DONE")
      ? "completed"
      : statusText.includes("RUN") || statusText.includes(" R ")
        ? "running"
        : "queued";
  return {
    scheduler: request.scheduler,
    jobId,
    status,
    queueState: status === "running" ? "RUNNING" : status.toUpperCase(),
    reason: null,
    progressPercent: request.logOutput?.includes("step") ? 50 : null,
    nsPerDay: request.logOutput?.includes("Performance") ? 82.125 : null,
    currentStep: request.logOutput?.includes("step") ? 5000 : null,
    logReport: null,
    warnings: [],
    generatedAt: now()
  };
}

export function mockRemoteWorkflowStep(request: RemoteWorkflowStepRequest): RemoteWorkflowStepResult {
  const command = request.package.commands.find((item) => item.id === request.stepId) ?? request.package.commands[0];
  const stdout = request.stepId === "submit"
    ? "123456;cluster\n"
    : request.stepId === "status"
      ? "JOBID PARTITION NAME USER ST TIME NODES NODELIST\n123456 gpu automd noir R 00:10 1 node01\n"
      : request.stepId === "tail-log"
        ? "step 5000 of 10000\nPerformance: 82.125 ns/day\n"
        : "mock remote workflow step completed\n";
  const snapshot = ["submit", "status", "tail-log"].includes(request.stepId)
    ? mockRemoteJobSnapshot({
      engineId: request.package.engineId,
      scheduler: request.package.scheduler,
      submitOutput: request.stepId === "submit" ? stdout : null,
      statusOutput: request.stepId === "status" ? stdout : null,
      logOutput: request.stepId === "tail-log" ? stdout : null
    })
    : null;
  return {
    stepId: request.stepId,
    label: command?.label ?? request.stepId,
    command: (command?.command ?? "")
      .split("<job-id>").join(request.jobId ?? "123456")
      .split("<pid>").join(request.jobId ?? "123456"),
    mode: request.mode,
    filesWritten: request.mode === "dryRun" ? [] : request.package.files.map((file) => file.path),
    status: request.mode === "execute" ? "completed" : "completed",
    exitCode: request.mode === "execute" ? 0 : null,
    stdout: request.mode === "execute" ? stdout : "",
    stderr: "",
    snapshot,
    startedAt: now(),
    finishedAt: now(),
    durationMs: request.mode === "execute" ? 120 : 0,
    warnings: request.mode === "execute"
      ? ["Web preview mock did not contact a remote host."]
      : ["Preview mode did not execute ssh/rsync."]
  };
}

export function mockContainerRecipe(engineId: string): ContainerRecipe {
  return {
    engineId,
    title: `${engineId} container recipe`,
    files: [
      {
        path: `containers/${engineId}.Containerfile`,
        language: "dockerfile",
        contents: `FROM ubuntu:24.04\nWORKDIR /work\n# Install ${engineId} according to its license and upstream docs.\n`
      }
    ],
    notes: ["Web 预览模式生成通用模板。"]
  };
}

export function mockBuildRecipe(options: BuildRecipeOptions): BuildRecipe {
  return {
    engineId: options.engineId,
    title: `${options.engineId} build recipe`,
    script: `#!/usr/bin/env bash\nset -euo pipefail\nmkdir -p "${options.installPrefix ?? "$HOME/.local/automd"}"\n`,
    steps: ["确认许可证和平台支持。", "下载源码。", "编译并登记路径。"],
    warnings: ["Web 预览模式生成通用脚本。"]
  };
}

export function mockRecipeExportResult(request: RecipeExportRequest): RecipeExportResult {
  const engineId = request.buildOptions.engineId;
  const directory = `build-recipes/${engineId}`;
  return {
    engineId,
    directory,
    files: [
      {
        path: `${directory}/containers/${engineId}.Containerfile`,
        language: "dockerfile",
        contents: `FROM ubuntu:24.04\nWORKDIR /work\n# Install ${engineId}.\n`
      },
      {
        path: `${directory}/build-${engineId}.sh`,
        language: "bash",
        contents: "#!/usr/bin/env bash\nset -euo pipefail\n"
      },
      {
        path: `${directory}/README.md`,
        language: "markdown",
        contents: `# ${engineId} build recipe\n`
      }
    ],
    warnings: ["Web 预览模式不会写入真实项目目录。"]
  };
}

export function mockBuildWorkflow(request: BuildWorkflowRequest): BuildWorkflowResult {
  const engineId = request.buildOptions.engineId;
  const directory = `build-recipes/${engineId}`;
  const files = mockRecipeExportResult({
    projectPath: request.projectPath,
    buildOptions: request.buildOptions,
    includeContainer: request.includeContainer,
    includeBuildScript: request.includeBuildScript
  }).files;
  return {
    engineId,
    directory,
    command: `bash ${directory}/build-${engineId}.sh`,
    mode: request.mode,
    filesWritten: request.mode === "dryRun" ? [] : files.map((file) => file.path),
    status: "completed",
    exitCode: request.mode === "execute" ? 0 : null,
    stdout: request.mode === "execute" ? `Use this placeholder to compile ${engineId}\n` : "",
    stderr: "",
    logPath: request.mode === "execute" ? `${directory}/logs/build-combined.log` : null,
    failureAnalysis: null,
    startedAt: now(),
    finishedAt: now(),
    durationMs: request.mode === "execute" ? 100 : 0,
    warnings: request.mode === "execute"
      ? ["Web preview mock did not run a real compiler."]
      : ["Preview mode did not execute a compiler process."]
  };
}

export function mockRunPackage(request: EngineRunRequest): EngineRunPackage {
  const runDirectory = `runs/${request.plan.engineId}-${request.plan.id}`;
  if (request.plan.engineId === "openmm") {
    return {
      engineId: request.plan.engineId,
      planId: request.plan.id,
      runDirectory,
      writable: Boolean(request.projectPath),
      warnings: ["Web 预览模式生成 OpenMM mock run package。"],
      commands: [
        {
          stageId: "openmm-env",
          label: "检测 OpenMM Python 环境",
          command: "python -c \"import openmm; print(openmm.version.version)\"",
          workingDirectory: ".",
          expectedOutputs: []
        },
        {
          stageId: "openmm-run",
          label: "运行 OpenMM workflow",
          command: `python generated/openmm/run_openmm.py --plan generated/openmm/automd-plan.json --out ${runDirectory}`,
          workingDirectory: ".",
          expectedOutputs: [`${runDirectory}/openmm.chk`, "trajectories/openmm.dcd", "analysis/openmm_state.csv"]
        }
      ],
      files: [
        {
          path: "generated/openmm/run_openmm.py",
          language: "python",
          contents: "from openmm.app import *\n# AutoMD OpenMM runner preview\n",
          written: request.writeToDisk
        },
        {
          path: `${runDirectory}/run-openmm.sh`,
          language: "bash",
          contents: "#!/usr/bin/env bash\nset -euo pipefail\npython generated/openmm/run_openmm.py --plan generated/openmm/automd-plan.json\n",
          written: request.writeToDisk
        }
      ]
    };
  }
  if (request.plan.engineId === "ambertools") {
    return {
      engineId: request.plan.engineId,
      planId: request.plan.id,
      runDirectory,
      writable: Boolean(request.projectPath),
      warnings: ["Web 预览模式生成 AmberTools mock run package；配体仍需 mol2/frcmod 参数。"],
      commands: [
        {
          stageId: "ambertools-env",
          label: "检测 AmberTools 命令行工具",
          command: "tleap -h >/dev/null 2>&1 && sander -h >/dev/null 2>&1 && cpptraj -h >/dev/null 2>&1",
          workingDirectory: ".",
          expectedOutputs: []
        },
        {
          stageId: "ambertools-tleap",
          label: "生成 AMBER topology/restart",
          command: "tleap -f generated/ambertools/tleap.in",
          workingDirectory: ".",
          expectedOutputs: ["generated/ambertools/system.prmtop", "generated/ambertools/system.inpcrd"]
        },
        {
          stageId: "ambertools-prod",
          label: "sander 生产模拟",
          command: `sander -O -i generated/ambertools/prod.mdin -o ${runDirectory}/prod.out -p generated/ambertools/system.prmtop -c ${runDirectory}/equil.rst7 -r ${runDirectory}/prod.rst7 -x trajectories/ambertools-prod.nc`,
          workingDirectory: ".",
          expectedOutputs: ["trajectories/ambertools-prod.nc", `${runDirectory}/prod.rst7`]
        },
        {
          stageId: "ambertools-analysis",
          label: "cpptraj 基础 RMSD/Rg 分析",
          command: "cpptraj -i generated/ambertools/cpptraj.in",
          workingDirectory: ".",
          expectedOutputs: ["analysis/amber_rmsd.xvg", "analysis/amber_rg.xvg"]
        }
      ],
      files: [
        {
          path: "generated/ambertools/tleap.in",
          language: "amber",
          contents: "source leaprc.protein.ff19SB\nsource leaprc.water.tip3p\nsystem = loadpdb inputs/system.pdb\nsaveamberparm system generated/ambertools/system.prmtop generated/ambertools/system.inpcrd\n",
          written: request.writeToDisk
        },
        {
          path: "generated/ambertools/prod.mdin",
          language: "amber",
          contents: "Production AutoMD system\n&cntrl\n  imin=0, nstlim=500000, dt=0.002,\n/\n",
          written: request.writeToDisk
        },
        {
          path: "generated/ambertools/cpptraj.in",
          language: "amber",
          contents: "parm generated/ambertools/system.prmtop\ntrajin trajectories/ambertools-prod.nc\nrms first out analysis/amber_rmsd.xvg\n",
          written: request.writeToDisk
        },
        {
          path: `${runDirectory}/run-ambertools.sh`,
          language: "bash",
          contents: "#!/usr/bin/env bash\nset -euo pipefail\ntleap -f generated/ambertools/tleap.in\n",
          written: request.writeToDisk
        }
      ]
    };
  }
  if (request.plan.engineId === "namd") {
    return {
      engineId: request.plan.engineId,
      planId: request.plan.id,
      runDirectory,
      writable: Boolean(request.projectPath),
      warnings: [
        "NAMD 是用户自带许可/安装的外部模块；AutoMD 不下载、不分发 NAMD 二进制文件。",
        "当前模板需要用户提供 PSF/PDB/CHARMM 参数文件。"
      ],
      commands: [
        {
          stageId: "namd-env",
          label: "检测用户安装的 NAMD",
          command: "command -v namd3 >/dev/null 2>&1 || command -v namd2 >/dev/null 2>&1",
          workingDirectory: ".",
          expectedOutputs: []
        },
        {
          stageId: "namd-run",
          label: "运行用户安装的 NAMD",
          command: `NAMD_BIN="\${NAMD_BIN:-$(command -v namd3 || command -v namd2)}"; "$NAMD_BIN" +p${request.plan.resources.cpuThreads} generated/namd/automd.conf > ${runDirectory}/namd.log 2>&1`,
          workingDirectory: ".",
          expectedOutputs: [`${runDirectory}/namd.log`, `${runDirectory}/prod.dcd`, `${runDirectory}/prod.restart.coor`]
        }
      ],
      files: [
        {
          path: "generated/namd/automd.conf",
          language: "tcl",
          contents: "structure inputs/system.psf\ncoordinates inputs/system.pdb\nparameters inputs/par_all36m_prot.prm\noutputName runs/namd-preview/prod\nminimize 5000\nrun 500000\n",
          written: request.writeToDisk
        },
        {
          path: `${runDirectory}/run-namd.sh`,
          language: "bash",
          contents: "#!/usr/bin/env bash\nset -euo pipefail\nNAMD_BIN=\"${NAMD_BIN:-$(command -v namd3 || command -v namd2)}\"\n\"$NAMD_BIN\" +p4 generated/namd/automd.conf\n",
          written: request.writeToDisk
        }
      ]
    };
  }
  if (request.plan.engineId !== "gromacs") {
    const slug = mockGeneratedSlug(request.plan.engineId);
    const script = mockRunScriptName(request.plan.engineId);
    return {
      engineId: request.plan.engineId,
      planId: request.plan.id,
      runDirectory,
      writable: Boolean(request.projectPath),
      warnings: [`${request.plan.engineId} uses a native preview template in web preview mode.`],
      commands: [
        {
          stageId: `${request.plan.engineId}-run`,
          label: "运行原生模板",
          command: `bash ${runDirectory}/${script}`,
          workingDirectory: ".",
          expectedOutputs: [`${runDirectory}/${request.plan.engineId}.log`]
        }
      ],
      files: [
        {
          path: `generated/${slug}/automd-plan.json`,
          language: "json",
          contents: JSON.stringify(request.plan, null, 2),
          written: request.writeToDisk
        },
        {
          path: `generated/${slug}/native-template.txt`,
          language: "text",
          contents: `# AutoMD ${request.plan.engineId} native preview template\n# Edit with validated engine-specific inputs before real runs.\n`,
          written: request.writeToDisk
        },
        {
          path: `${runDirectory}/${script}`,
          language: "bash",
          contents: `#!/usr/bin/env bash\nset -euo pipefail\necho "AutoMD ${request.plan.engineId} preview run"\n`,
          written: request.writeToDisk
        }
      ]
    };
  }
  return {
    engineId: request.plan.engineId,
    planId: request.plan.id,
    runDirectory,
    writable: Boolean(request.projectPath),
    warnings: ["Web 预览模式生成 GROMACS mock run package。"],
    commands: [
      {
        stageId: "em",
        label: "能量最小化",
        command: `gmx grompp -f generated/gromacs/em.mdp -o ${runDirectory}/em.tpr && gmx mdrun -deffnm ${runDirectory}/em`,
        workingDirectory: ".",
        expectedOutputs: [`${runDirectory}/em.gro`, `${runDirectory}/em.log`]
      },
      {
        stageId: "production",
        label: "生产模拟",
        command: `gmx grompp -f generated/gromacs/md.mdp -o ${runDirectory}/md.tpr && gmx mdrun -deffnm ${runDirectory}/md`,
        workingDirectory: ".",
        expectedOutputs: [`${runDirectory}/md.xtc`, `${runDirectory}/md.log`]
      }
    ],
    files: [
      {
        path: "generated/gromacs/em.mdp",
        language: "ini",
        contents: "integrator = steep\nnsteps = 50000\n",
        written: request.writeToDisk
      },
      {
        path: `${runDirectory}/run-gromacs.sh`,
        language: "bash",
        contents: "#!/usr/bin/env bash\nset -euo pipefail\ngmx --version\n",
        written: request.writeToDisk
      }
    ]
  };
}

export function mockBatchExperimentPackage(request: BatchExperimentRequest): BatchExperimentPackage {
  const replicateCount = Math.max(1, Math.min(64, Math.floor(request.replicateCount || 1)));
  const seedStart = Math.max(0, Math.floor(request.seedStart || 1));
  const replicas = Array.from({ length: replicateCount }, (_, index) => {
    const seed = seedStart + index;
    const plan = clonePlanForReplica(request.plan, index + 1, seed);
    const runDirectory = `runs/${plan.engineId}-${plan.id}`;
    return {
      replicaIndex: index + 1,
      seed,
      plan,
      runDirectory
    };
  });
  const replicaPackages = replicas.map((replica) =>
    namespaceMockRunPackage(
      mockRunPackage({
        plan: replica.plan,
        projectPath: request.projectPath,
        writeToDisk: request.writeToDisk
      }),
      replica.replicaIndex
    )
  );
  const replicaCommands = replicaPackages.map((packagePreview, index) => {
    const script = packagePreview.files.find((file) =>
      file.path.startsWith(packagePreview.runDirectory) && file.path.endsWith(".sh")
    )?.path ?? packagePreview.commands[0]?.command ?? "true";
    return {
      stageId: `batch-replica-${String(index + 1).padStart(2, "0")}`,
      label: `运行 replica ${String(index + 1).padStart(2, "0")} (seed ${replicas[index].seed})`,
      command: script.endsWith(".sh") ? `bash "${script}"` : script,
      workingDirectory: ".",
      expectedOutputs: packagePreview.commands.flatMap((command) => command.expectedOutputs)
    };
  });
  const batchCommand = {
    stageId: "batch-run",
    label: `顺序运行 ${replicateCount} 个 replica`,
    command: "bash generated/batch/run-batch.sh",
    workingDirectory: ".",
    expectedOutputs: replicas.map(
      (replica) => `${replica.runDirectory}/batch-replica-${String(replica.replicaIndex).padStart(2, "0")}.log`
    )
  };
  const files = [
    ...replicaPackages.flatMap((packagePreview) => packagePreview.files),
    ...replicas.map((replica) => ({
      path: `generated/batch/replica-${String(replica.replicaIndex).padStart(2, "0")}/automd-plan.json`,
      language: "json",
      contents: JSON.stringify(replica.plan, null, 2),
      written: request.writeToDisk
    })),
    {
      path: "generated/batch/automd-batch.json",
      language: "json",
      contents: JSON.stringify(
        {
          engineId: request.plan.engineId,
          sourcePlanId: request.plan.id,
          replicateCount,
          replicas,
          generatedAt: now()
        },
        null,
        2
      ),
      written: request.writeToDisk
    },
    {
      path: "generated/batch/run-batch.sh",
      language: "bash",
      contents: [
        "#!/usr/bin/env bash",
        "set -euo pipefail",
        `echo "AutoMD batch experiment: ${request.plan.name}"`,
        ...replicaCommands.flatMap((command, index) => [
          `echo "[AutoMD] replica ${String(index + 1).padStart(2, "0")} seed ${replicas[index].seed}"`,
          `(${command.command}) 2>&1 | tee "${replicas[index].runDirectory}/batch-replica-${String(index + 1).padStart(2, "0")}.log"`
        ]),
        "echo \"[AutoMD] batch experiment completed\"",
        ""
      ].join("\n"),
      written: request.writeToDisk
    }
  ];

  return {
    engineId: request.plan.engineId,
    planId: request.plan.id,
    generatedDirectory: "generated/batch",
    replicas,
    files,
    commands: [batchCommand, ...replicaCommands],
    warnings: replicaPackages.flatMap((packagePreview, index) =>
      packagePreview.warnings.map((warning) => `replica ${String(index + 1).padStart(2, "0")}: ${warning}`)
    ),
    writable: Boolean(request.projectPath)
  };
}

function clonePlanForReplica(plan: SimulationPlan, replicaIndex: number, seed: number): SimulationPlan {
  const clone = JSON.parse(JSON.stringify(plan)) as SimulationPlan;
  clone.id = randomId();
  clone.name = `${plan.name} replica ${String(replicaIndex).padStart(2, "0")}`;
  clone.createdAt = now();
  clone.stages = clone.stages.map((stage) => {
    if (stage.kind === "nvtEquilibration") {
      return { ...stage, parameters: { ...stage.parameters, velocitySeed: String(seed) } };
    }
    if (stage.kind === "production") {
      return { ...stage, parameters: { ...stage.parameters, randomSeed: String(seed) } };
    }
    return stage;
  });
  return clone;
}

function namespaceMockRunPackage(packagePreview: EngineRunPackage, replicaIndex: number): EngineRunPackage {
  const prefix = `generated/batch/replica-${String(replicaIndex).padStart(2, "0")}`;
  const replacements: Array<[string, string]> = [];
  const directoryReplacements = new Map<string, string>();
  const files = packagePreview.files.map((file) => {
    if (!file.path.startsWith("generated/")) {
      return { ...file };
    }
    const nextPath = `${prefix}/${file.path.slice("generated/".length)}`;
    const [, oldSlug] = file.path.split("/");
    const [, , , newSlug] = nextPath.split("/");
    if (oldSlug && newSlug) {
      directoryReplacements.set(`generated/${oldSlug}`, `${prefix}/${newSlug}`);
    }
    replacements.push([file.path, nextPath]);
    return { ...file, path: nextPath };
  });
  replacements.push(...directoryReplacements.entries());
  replacements.sort((left, right) => right[0].length - left[0].length);
  const replaceAll = (value: string) => replacements.reduce((current, [oldValue, newValue]) =>
    current.split(oldValue).join(newValue), value);

  return {
    ...packagePreview,
    files: files.map((file) => ({ ...file, contents: replaceAll(file.contents) })),
    commands: packagePreview.commands.map((command) => ({
      ...command,
      command: replaceAll(command.command),
      expectedOutputs: command.expectedOutputs.map(replaceAll)
    }))
  };
}

export function mockStructurePreparationPackage(request: StructurePreparationRequest): StructurePreparationPackage {
  return {
    planId: request.plan.id,
    generatedDirectory: "generated/prep",
    writable: Boolean(request.projectPath),
    warnings: request.plan.system.hasLigand
      ? ["配体体系需要用户审查 mol2/frcmod 参数；mock 只生成准备包。"]
      : [],
    commands: [
      {
        stageId: "science-sidecar-diagnostics",
        label: "检测 Python 科学侧车依赖",
        command: "python3 generated/prep/prepare_structure.py --diagnostics",
        workingDirectory: ".",
        expectedOutputs: []
      },
      {
        stageId: "science-sidecar-prepare",
        label: "运行结构修复/加氢/溶剂盒准备",
        command: "python3 generated/prep/prepare_structure.py --plan generated/prep/automd-plan.json --project .",
        workingDirectory: ".",
        expectedOutputs: ["generated/prep/prepared_structure.pdb", "generated/prep/structure-prep-report.json"]
      }
    ],
    files: [
      {
        path: "generated/prep/prepare_structure.py",
        language: "python",
        contents: "# AutoMD science sidecar structure preparation preview\n",
        written: request.writeToDisk
      },
      {
        path: "generated/prep/environment.yml",
        language: "yaml",
        contents: mockScienceSidecarDiagnostics.environmentRecipe,
        written: request.writeToDisk
      },
      {
        path: "generated/prep/ligand_parameterization.md",
        language: "markdown",
        contents: "# Ligand Parameterization\n\nReview RDKit/Open Babel/AmberTools outputs before production MD.\n",
        written: request.writeToDisk
      }
    ]
  };
}

export function mockParseLog(request: EngineLogParseRequest): EngineLogReport {
  const performance = request.logContents.match(/Performance:\s+([0-9.]+)/);
  const step = request.logContents.match(/step\s+([0-9]+)(?:\s+of\s+([0-9]+))?/i);
  const currentStep = step?.[1] ? Number(step[1]) : null;
  const total = step?.[2] ? Number(step[2]) : null;
  return {
    engineId: request.engineId,
    nsPerDay: performance?.[1] ? Number(performance[1]) : null,
    currentStep,
    progressPercent: currentStep && total ? (currentStep / total) * 100 : null,
    fatalError: request.logContents.toLowerCase().includes("fatal error") ? "Fatal error detected" : null,
    events: [
      ...(performance?.[1]
        ? [{ kind: "performance" as const, lineNumber: 1, message: `${performance[1]} ns/day` }]
        : []),
      ...(currentStep
        ? [{ kind: "progress" as const, lineNumber: 1, message: `step ${currentStep}` }]
        : [])
    ]
  };
}

export function mockClassifyFailure(request: FailureAnalysisRequest): FailureAnalysis {
  const lower = request.logContents.toLowerCase();
  const category = lower.includes("atomtype") || lower.includes("force field")
    ? "missingForceField"
    : lower.includes("gpu") || lower.includes("cuda")
      ? "gpuUnavailable"
      : lower.includes("no such file") || lower.includes("cannot open")
        ? "missingInput"
        : lower.includes("lincs") || lower.includes("nan")
          ? "numericalInstability"
          : "unknown";
  return {
    engineId: request.engineId,
    category,
    severity: category === "unknown" ? "warning" : "error",
    message: request.logContents.includes("Fatal error") ? "Fatal error detected in mock log." : "Mock classifier did not find a fatal headline.",
    suggestions: [
      {
        title: "查看完整日志",
        detail: "Web 预览模式使用本地规则示例；Tauri 模式会调用 Rust 分类器。",
        actionLabel: "Open diagnostics",
        commandHint: "gmx --version"
      }
    ]
  };
}

export function mockDiscoverResumePlan(request: ResumePlanRequest): ResumePlan {
  const checkpointPath = request.engineId === "openmm"
    ? `${request.runDirectory}/openmm.chk`
    : request.engineId === "ambertools"
      ? `${request.runDirectory}/prod.rst7`
      : request.engineId === "namd"
        ? `${request.runDirectory}/prod.restart.coor`
        : `${request.runDirectory}/md.cpt`;
  const command = request.engineId === "openmm"
    ? `python generated/openmm/run_openmm.py --plan generated/openmm/automd-plan.json --out ${request.runDirectory} --resume ${checkpointPath}`
    : request.engineId === "ambertools"
      ? `sander -O -i generated/ambertools/prod.mdin -o ${request.runDirectory}/prod-resume.out -p generated/ambertools/system.prmtop -c ${checkpointPath} -r ${request.runDirectory}/prod-resume.rst7`
      : request.engineId === "namd"
        ? `bash ${request.runDirectory}/run-namd.sh`
        : `gmx mdrun -deffnm ${request.runDirectory}/md -cpi ${checkpointPath} -append`;
  return {
    engineId: request.engineId,
    runDirectory: request.runDirectory,
    checkpoints: [
      {
        path: checkpointPath,
        sizeBytes: 28,
        modifiedAt: now(),
        stageHint: "production",
        commandHint: command
      }
    ],
    recommended: {
      path: checkpointPath,
      sizeBytes: 28,
      modifiedAt: now(),
      stageHint: "production",
      commandHint: command
    },
    resumeCommand: command,
    warnings: []
  };
}

export function mockProjectTextFile(request: ProjectTextFileRequest): ProjectTextFilePayload {
  const packagePreview = mockRunPackage({
    plan: mockPlan({ name: "mock", engineId: "gromacs", domain: "biomolecular" }),
    projectPath: request.projectPath,
    writeToDisk: false
  });
  const file = packagePreview.files.find((item) => item.path === request.path);
  const contents = file?.contents ?? "# Web preview placeholder\n# Real Tauri mode reads the file from the selected project.\n";
  return {
    path: request.path,
    language: file?.language ?? "text",
    contents,
    sizeBytes: contents.length,
    modifiedAt: now()
  };
}

export function mockStartLocalRun(request: StartLocalRunRequest): LocalTaskSnapshot {
  const runDirectory = `runs/${request.plan.engineId}-${request.plan.id}`;
  const resumePlan = mockDiscoverResumePlan({
    projectPath: request.projectPath ?? "/mock/AutoMD/project",
    runDirectory,
    engineId: request.plan.engineId
  });
  return {
    id: randomId(),
    planId: request.plan.id,
    engineId: request.plan.engineId,
    mode: request.mode,
    status: request.mode === "dryRun" ? "completed" : "running",
    runDirectory,
    command:
      request.mode === "real"
        ? `bash ${runDirectory}/${mockRunScriptName(request.plan.engineId)}`
        : `python3 scripts/automd_mock_engine.py --plan generated/${mockGeneratedSlug(request.plan.engineId)}/automd-plan.json`,
    progressPercent: request.mode === "dryRun" ? 100 : 16.67,
    nsPerDay: request.mode === "dryRun" ? null : 47.5,
    currentStep: request.mode === "dryRun" ? null : 1,
    logTail: [
      `Prepared ${request.plan.engineId} run package.`,
      request.mode === "dryRun" ? "Dry run completed without launching a process." : "[stdout] step 1 of 6",
      request.mode === "dryRun" ? "No process launched." : "[stdout] Performance: 47.500 ns/day"
    ],
    errorMessage: null,
    exitCode: request.mode === "dryRun" ? 0 : null,
    artifacts: [
      {
        path: "analysis/rmsd.xvg",
        kind: "analysisTable",
        sizeBytes: 120,
        modifiedAt: now(),
        summary: "6 data rows; last=5 0.130"
      },
      {
        path: "reports/automd-report.md",
        kind: "report",
        sizeBytes: 900,
        modifiedAt: now(),
        summary: "Mock report"
      }
    ],
    reportPath: "reports/automd-report.md",
    failureAnalysis: null,
    resumePlan,
    startedAt: now(),
    finishedAt: request.mode === "dryRun" ? now() : null
  };
}

export function mockTaskRecords(projectId?: string | null): TaskRecord[] {
  const planId = randomId();
  return [
    {
      id: randomId(),
      projectId: projectId ?? null,
      planId,
      engineId: "gromacs",
      status: "completed",
      currentStage: "production",
      progressPercent: 100,
      createdAt: now(),
      updatedAt: now()
    },
    {
      id: randomId(),
      projectId: projectId ?? null,
      planId,
      engineId: "openmm",
      status: "running",
      currentStage: "production",
      progressPercent: 52,
      createdAt: now(),
      updatedAt: now()
    }
  ];
}

export function mockArtifactIndex(request: ArtifactIndexRequest): ArtifactIndex {
  return {
    projectPath: request.projectPath,
    runDirectory: request.runDirectory ?? null,
    generatedAt: now(),
    artifacts: [
      {
        path: "generated/gromacs/md.mdp",
        kind: "generatedInput",
        sizeBytes: 1200,
        modifiedAt: now(),
        summary: "Generated production MDP"
      },
      {
        path: "analysis/rmsd.xvg",
        kind: "analysisTable",
        sizeBytes: 180,
        modifiedAt: now(),
        summary: "6 data rows; last=5 0.130"
      },
      {
        path: "trajectories/mock-preview.pdb",
        kind: "trajectory",
        sizeBytes: 352,
        modifiedAt: now(),
        summary: "2 text frames; previewable with chunked reader"
      },
      {
        path: "checkpoints/mock.cpt",
        kind: "checkpoint",
        sizeBytes: 28,
        modifiedAt: now(),
        summary: "Mock checkpoint"
      }
    ]
  };
}

export function mockTrajectoryIndex(request: TrajectoryIndexRequest): TrajectoryIndex {
  const sampledFrames = [
    {
      frameIndex: 0,
      byteStart: 0,
      byteEnd: 176,
      atomCount: 2,
      timePs: 0,
      label: "MODEL 1"
    },
    {
      frameIndex: 1,
      byteStart: 176,
      byteEnd: 352,
      atomCount: 2,
      timePs: 1,
      label: "MODEL 2"
    }
  ];
  return {
    projectPath: request.projectPath,
    trajectoryPath: request.trajectoryPath,
    format: request.trajectoryPath.endsWith(".xtc") ? "xtc" : "pdb",
    strategy: request.trajectoryPath.endsWith(".xtc") ? "metadataOnly" : "textOffsets",
    sizeBytes: request.trajectoryPath.endsWith(".xtc") ? 12_800_000 : 352,
    frameCount: request.trajectoryPath.endsWith(".xtc") ? null : 2,
    sampledFrames: request.trajectoryPath.endsWith(".xtc") ? [] : sampledFrames,
    indexPath: request.writeIndex ? "trajectories/.automd-index/mock-preview_pdb.json" : null,
    warnings: request.trajectoryPath.endsWith(".xtc")
      ? ["Binary trajectory registered as metadata-only in web preview."]
      : [],
    generatedAt: now()
  };
}

export function mockTrajectoryChunk(request: TrajectoryChunkRequest): TrajectoryChunk {
  const frameIndex = request.frameIndices?.[0] ?? request.startFrame ?? 0;
  return {
    projectPath: request.projectPath,
    trajectoryPath: request.trajectoryPath,
    generatedAt: now(),
    truncated: false,
    warnings: [],
    frames: [
      {
        frameIndex,
        label: `MODEL ${frameIndex + 1}`,
        format: "pdb",
        atomCount: 2,
        timePs: frameIndex,
        contents: `MODEL     ${String(frameIndex + 1).padStart(4, " ")}
ATOM      1  N   ALA A   1       ${frameIndex}.000   0.000   0.000
ATOM      2  CA  ALA A   1       ${frameIndex}.000   1.000   0.000
ENDMDL
`
      }
    ]
  };
}

export function mockTrajectoryAnalysisPackage(request: TrajectoryAnalysisRequest): TrajectoryAnalysisPackage {
  const outputs = [
    "analysis/mdanalysis_rmsd.csv",
    "analysis/mdanalysis_rg.csv",
    "analysis/mdanalysis_rmsf.csv",
    "analysis/mdanalysis_contacts.csv",
    "analysis/mdanalysis_hbonds.csv",
    "analysis/mdanalysis_distances.csv",
    "analysis/mdanalysis_angles.csv",
    "analysis/mdanalysis_dihedrals.csv",
    "analysis/mdanalysis-summary.json"
  ];
  return {
    planId: request.plan.id,
    generatedDirectory: "generated/analysis",
    writable: Boolean(request.projectPath),
    warnings: request.trajectoryPath ? [] : ["Web 预览使用默认轨迹路径；真实项目会从 artifacts 中选择轨迹。"],
    expectedOutputs: outputs,
    commands: [
      {
        stageId: "science-sidecar-analysis-diagnostics",
        label: "检测 MDAnalysis 分析侧车依赖",
        command: "python3 generated/analysis/run_mdanalysis.py --diagnostics",
        workingDirectory: ".",
        expectedOutputs: []
      },
      {
        stageId: "science-sidecar-analysis",
        label: "运行 MDAnalysis RMSD/RMSF/Rg/氢键/距离/角度/二面角/接触分析",
        command: `python3 generated/analysis/run_mdanalysis.py --plan generated/analysis/automd-plan.json --project . --topology ${request.topologyPath ?? "inputs/system.pdb"} --trajectory ${request.trajectoryPath ?? "trajectories/mock-preview.pdb"} --selection "${request.selection}"`,
        workingDirectory: ".",
        expectedOutputs: outputs
      }
    ],
    files: [
      {
        path: "generated/analysis/run_mdanalysis.py",
        language: "python",
        contents: "# AutoMD MDAnalysis analysis sidecar preview\n",
        written: Boolean(request.projectPath)
      },
      {
        path: "generated/analysis/environment.yml",
        language: "yaml",
        contents: mockScienceSidecarDiagnostics.environmentRecipe,
        written: Boolean(request.projectPath)
      },
      {
        path: "generated/analysis/README.md",
        language: "markdown",
        contents: "# AutoMD MDAnalysis Trajectory Analysis\n",
        written: Boolean(request.projectPath)
      }
    ]
  };
}

export function mockArtifactRecords(projectPath: string): ArtifactRecord[] {
  const index = mockArtifactIndex({ projectPath, runDirectory: null });
  return index.artifacts.map((artifact) => ({
    projectPath,
    path: artifact.path,
    kind: artifact.kind,
    sizeBytes: artifact.sizeBytes,
    modifiedAt: artifact.modifiedAt ?? null,
    summary: artifact.summary ?? null,
    runDirectory: null,
    indexedAt: index.generatedAt
  }));
}

export function mockAnalysisResults(request: AnalysisParseRequest): AnalysisParseResult {
  return {
    projectPath: request.projectPath,
    generatedAt: now(),
    warnings: [],
    series: [
      {
        path: "analysis/rmsd.xvg",
        label: "Mock RMSD",
        xLabel: "Time (ns)",
        yLabel: "RMSD (nm)",
        points: Array.from({ length: 8 }, (_, index) => ({
          x: index,
          y: Number((0.08 + index * 0.012 + (index % 2) * 0.004).toFixed(3))
        })),
        minY: 0.08,
        maxY: 0.168,
        lastY: 0.168
      },
      {
        path: "analysis/rg.xvg",
        label: "Mock Radius of gyration",
        xLabel: "Time (ns)",
        yLabel: "Rg (nm)",
        points: Array.from({ length: 8 }, (_, index) => ({
          x: index,
          y: Number((1.9 + index * 0.015).toFixed(3))
        })),
        minY: 1.9,
        maxY: 2.005,
        lastY: 2.005
      }
    ]
  };
}

export function mockAnalysisCacheRecords(projectPath: string): AnalysisCacheRecord[] {
  const result = mockAnalysisResults({ projectPath, artifactPaths: null, maxPoints: 800 });
  return result.series.map((series) => ({
    projectPath,
    path: series.path,
    label: series.label,
    xLabel: series.xLabel,
    yLabel: series.yLabel,
    pointCount: series.points.length,
    minY: series.minY ?? null,
    maxY: series.maxY ?? null,
    lastY: series.lastY ?? null,
    generatedAt: result.generatedAt
  }));
}

export function mockExportReport(request: ReportExportRequest): ExportedReport {
  const contents = `# AutoMD Simulation Report\n\n- Plan: ${request.plan.name}\n- Engine: ${request.plan.engineId}\n- Artifacts: ${request.artifactIndex?.artifacts.length ?? 0}\n`;
  const extension = request.format === "html" ? "html" : request.format === "pdf" ? "pdf" : "md";
  return {
    path: `reports/automd-report.${extension}`,
    format: request.format,
    contents
  };
}
