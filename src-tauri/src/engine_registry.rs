use crate::models::*;
use std::process::Command;

#[derive(Debug, Clone)]
enum Detector {
    Executable(Vec<&'static str>),
    PythonModule(&'static str),
}

#[derive(Debug, Clone)]
struct EngineDefinition {
    id: &'static str,
    name: &'static str,
    category: EngineCategory,
    maturity: EngineMaturity,
    license: LicensePolicy,
    platform_support: PlatformSupport,
    detector: Detector,
    gpu_backends: Vec<GpuBackend>,
    execution_modes: Vec<ExecutionMode>,
    supported_inputs: Vec<&'static str>,
    supported_outputs: Vec<&'static str>,
    supported_stages: Vec<SimulationStageKind>,
    docs_url: &'static str,
    notes: Vec<&'static str>,
}

pub fn detect_all() -> Vec<EngineCapability> {
    definitions().into_iter().map(detect_engine).collect()
}

#[cfg(test)]
pub fn known_engine_ids() -> Vec<String> {
    definitions()
        .into_iter()
        .map(|definition| definition.id.to_string())
        .collect()
}

pub fn detect_engine_by_id(engine_id: &str) -> Option<EngineCapability> {
    definitions()
        .into_iter()
        .find(|definition| definition.id == engine_id)
        .map(detect_engine)
}

fn detect_engine(definition: EngineDefinition) -> EngineCapability {
    let executable_names = match &definition.detector {
        Detector::Executable(names) => names.iter().map(|name| name.to_string()).collect(),
        Detector::PythonModule(module) => vec![format!("python module: {module}")],
    };

    let detection = match &definition.detector {
        Detector::Executable(names) => detect_executable(names, &definition.license),
        Detector::PythonModule(module) => detect_python_module(module, &definition.license),
    };

    EngineCapability {
        id: definition.id.to_string(),
        name: definition.name.to_string(),
        category: definition.category,
        maturity: definition.maturity,
        license: definition.license,
        platform_support: definition.platform_support,
        executable_names,
        gpu_backends: definition.gpu_backends,
        execution_modes: definition.execution_modes,
        supported_inputs: definition
            .supported_inputs
            .into_iter()
            .map(String::from)
            .collect(),
        supported_outputs: definition
            .supported_outputs
            .into_iter()
            .map(String::from)
            .collect(),
        supported_stages: definition.supported_stages,
        detection,
        docs_url: definition.docs_url.to_string(),
        notes: definition.notes.into_iter().map(String::from).collect(),
    }
}

fn detect_executable(names: &[&str], license: &LicensePolicy) -> DetectionState {
    for name in names {
        if let Ok(path) = which::which(name) {
            return DetectionState {
                status: if license.requires_user_license {
                    DetectionStatus::MissingLicense
                } else {
                    DetectionStatus::Ready
                },
                path: Some(path.display().to_string()),
                version: detect_version(path.display().to_string(), &["--version"]),
                message: if license.requires_user_license {
                    "已检测到可执行文件；仍需用户确认本机许可/授权环境。".to_string()
                } else {
                    "已检测到本地可执行文件。".to_string()
                },
            };
        }
    }

    DetectionState {
        status: DetectionStatus::MissingInstall,
        path: None,
        version: None,
        message: format!("未在 PATH 中找到：{}", names.join(", ")),
    }
}

fn detect_python_module(module: &str, license: &LicensePolicy) -> DetectionState {
    let script = format!(
        "import importlib.util, importlib.metadata as m; name='{module}'; spec=importlib.util.find_spec(name); assert spec is not None; print(m.version(name))"
    );

    match Command::new("python3").args(["-c", &script]).output() {
        Ok(output) if output.status.success() => DetectionState {
            status: if license.requires_user_license {
                DetectionStatus::MissingLicense
            } else {
                DetectionStatus::Ready
            },
            path: Some(format!("python3::{module}")),
            version: String::from_utf8(output.stdout)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            message: "已检测到 Python 模块。".to_string(),
        },
        _ => DetectionState {
            status: DetectionStatus::MissingInstall,
            path: None,
            version: None,
            message: format!("未在当前 python3 环境中检测到模块：{module}"),
        },
    }
}

fn detect_version(command: String, args: &[&str]) -> Option<String> {
    let output = Command::new(command).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    stdout
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
}

fn open_source(guidance: &str) -> LicensePolicy {
    LicensePolicy {
        class: LicenseClass::OpenSource,
        distribution: DistributionPolicy::InstallerRecipe,
        bundled_by_automd: false,
        requires_user_license: false,
        guidance: guidance.to_string(),
    }
}

fn free_toolkit(guidance: &str) -> LicensePolicy {
    LicensePolicy {
        class: LicenseClass::FreeToolkit,
        distribution: DistributionPolicy::InstallerRecipe,
        bundled_by_automd: false,
        requires_user_license: false,
        guidance: guidance.to_string(),
    }
}

fn restricted(guidance: &str) -> LicensePolicy {
    LicensePolicy {
        class: LicenseClass::RestrictedAcademic,
        distribution: DistributionPolicy::UserLicenseRequired,
        bundled_by_automd: false,
        requires_user_license: true,
        guidance: guidance.to_string(),
    }
}

fn commercial(guidance: &str) -> LicensePolicy {
    LicensePolicy {
        class: LicenseClass::Commercial,
        distribution: DistributionPolicy::UserLicenseRequired,
        bundled_by_automd: false,
        requires_user_license: true,
        guidance: guidance.to_string(),
    }
}

fn all_desktop_platforms() -> PlatformSupport {
    PlatformSupport {
        native: vec![Platform::Windows, Platform::Macos, Platform::Linux],
        recommended_fallbacks: vec![Platform::Wsl2, Platform::RemoteLinux],
    }
}

fn linux_first_platforms() -> PlatformSupport {
    PlatformSupport {
        native: vec![Platform::Linux],
        recommended_fallbacks: vec![Platform::Wsl2, Platform::RemoteLinux],
    }
}

fn standard_biomolecular_stages() -> Vec<SimulationStageKind> {
    vec![
        SimulationStageKind::StructurePreparation,
        SimulationStageKind::EnergyMinimization,
        SimulationStageKind::NvtEquilibration,
        SimulationStageKind::NptEquilibration,
        SimulationStageKind::Production,
        SimulationStageKind::Analysis,
    ]
}

fn standard_materials_stages() -> Vec<SimulationStageKind> {
    vec![
        SimulationStageKind::EnergyMinimization,
        SimulationStageKind::NvtEquilibration,
        SimulationStageKind::NptEquilibration,
        SimulationStageKind::Production,
        SimulationStageKind::Analysis,
    ]
}

fn definitions() -> Vec<EngineDefinition> {
    vec![
        EngineDefinition {
            id: "gromacs",
            name: "GROMACS",
            category: EngineCategory::Biomolecular,
            maturity: EngineMaturity::FirstClass,
            license: open_source("LGPL 开源引擎；AutoMD 提供安装/容器/编译 recipe，不直接捆绑二进制。"),
            platform_support: all_desktop_platforms(),
            detector: Detector::Executable(vec!["gmx", "gmx_mpi"]),
            gpu_backends: vec![GpuBackend::Cuda, GpuBackend::OpenCl, GpuBackend::Sycl, GpuBackend::CpuOnly],
            execution_modes: vec![ExecutionMode::LocalProcess, ExecutionMode::CondaEnvironment, ExecutionMode::Container, ExecutionMode::Ssh, ExecutionMode::Slurm],
            supported_inputs: vec!["pdb", "gro", "top", "mdp", "xtc", "trr"],
            supported_outputs: vec!["gro", "tpr", "xtc", "trr", "edr", "log"],
            supported_stages: standard_biomolecular_stages(),
            docs_url: "https://manual.gromacs.org/documentation/current/",
            notes: vec!["首版完整闭环目标。", "支持 PLUMED 和 MPI/GPU 编译 recipe。"],
        },
        EngineDefinition {
            id: "openmm",
            name: "OpenMM",
            category: EngineCategory::Biomolecular,
            maturity: EngineMaturity::FirstClass,
            license: open_source("MIT/LGPL 生态；通过 Python 侧车环境安装并由 AutoMD 调用脚本模板。"),
            platform_support: all_desktop_platforms(),
            detector: Detector::PythonModule("openmm"),
            gpu_backends: vec![GpuBackend::Cuda, GpuBackend::OpenCl, GpuBackend::CpuOnly],
            execution_modes: vec![ExecutionMode::CondaEnvironment, ExecutionMode::LocalProcess, ExecutionMode::Container, ExecutionMode::Ssh],
            supported_inputs: vec!["pdb", "xml", "dcd", "pdbx", "sdf"],
            supported_outputs: vec!["dcd", "pdb", "state", "log"],
            supported_stages: standard_biomolecular_stages(),
            docs_url: "https://openmm.org/documentation",
            notes: vec!["适合作为跨平台首选执行后端。", "参数化和脚本生成走 Python adapter。"],
        },
        EngineDefinition {
            id: "ambertools",
            name: "AmberTools",
            category: EngineCategory::Biomolecular,
            maturity: EngineMaturity::Supported,
            license: free_toolkit("AmberTools 可自由获取；AutoMD 优先调用 tleap/sander/cpptraj 等工具。"),
            platform_support: all_desktop_platforms(),
            detector: Detector::Executable(vec!["tleap", "sander", "cpptraj"]),
            gpu_backends: vec![GpuBackend::CpuOnly],
            execution_modes: vec![ExecutionMode::CondaEnvironment, ExecutionMode::LocalProcess, ExecutionMode::Container, ExecutionMode::Ssh, ExecutionMode::Slurm],
            supported_inputs: vec!["pdb", "mol2", "frcmod", "prmtop", "inpcrd", "mdin"],
            supported_outputs: vec!["prmtop", "inpcrd", "nc", "mdout", "mdcrd"],
            supported_stages: standard_biomolecular_stages(),
            docs_url: "https://ambermd.org/AmberTools.php",
            notes: vec!["首版用于 Amber 输入生态和轻量本地运行。", "pmemd/pmemd.cuda 作为受限 AMBER 模块单独处理。"],
        },
        EngineDefinition {
            id: "lammps",
            name: "LAMMPS",
            category: EngineCategory::Materials,
            maturity: EngineMaturity::Supported,
            license: open_source("GPL 开源引擎；材料体系优先，生物体系按模板逐步扩展。"),
            platform_support: all_desktop_platforms(),
            detector: Detector::Executable(vec!["lmp", "lmp_serial", "lmp_mpi"]),
            gpu_backends: vec![GpuBackend::Cuda, GpuBackend::Rocm, GpuBackend::OpenCl, GpuBackend::CpuOnly],
            execution_modes: vec![ExecutionMode::LocalProcess, ExecutionMode::Container, ExecutionMode::Ssh, ExecutionMode::Slurm],
            supported_inputs: vec!["lmp", "data", "dump", "in"],
            supported_outputs: vec!["dump", "log", "restart"],
            supported_stages: standard_materials_stages(),
            docs_url: "https://docs.lammps.org/",
            notes: vec!["M5 完整模板目标。", "参数页保留原生 input 编辑器。"],
        },
        EngineDefinition {
            id: "cp2k",
            name: "CP2K",
            category: EngineCategory::Quantum,
            maturity: EngineMaturity::Supported,
            license: open_source("GPL 开源；QM/MM 和从头算 MD 扩展模块。"),
            platform_support: linux_first_platforms(),
            detector: Detector::Executable(vec!["cp2k", "cp2k.psmp", "cp2k.popt"]),
            gpu_backends: vec![GpuBackend::Cuda, GpuBackend::Rocm, GpuBackend::CpuOnly],
            execution_modes: vec![ExecutionMode::LocalProcess, ExecutionMode::Container, ExecutionMode::Ssh, ExecutionMode::Slurm],
            supported_inputs: vec!["inp", "xyz", "pdb"],
            supported_outputs: vec!["xyz", "ener", "restart", "log"],
            supported_stages: standard_materials_stages(),
            docs_url: "https://www.cp2k.org/",
            notes: vec!["Linux/远程优先。", "编译向导需检查 libint/libxc/MPI/GPU。"],
        },
        EngineDefinition {
            id: "genesis",
            name: "GENESIS",
            category: EngineCategory::Biomolecular,
            maturity: EngineMaturity::Preview,
            license: open_source("开源生物分子 MD 引擎；首版提供检测和模板入口。"),
            platform_support: linux_first_platforms(),
            detector: Detector::Executable(vec!["atdyn", "spdyn"]),
            gpu_backends: vec![GpuBackend::Cuda, GpuBackend::CpuOnly],
            execution_modes: vec![ExecutionMode::LocalProcess, ExecutionMode::Container, ExecutionMode::Ssh, ExecutionMode::Slurm],
            supported_inputs: vec!["pdb", "psf", "prmtop", "inp"],
            supported_outputs: vec!["dcd", "rst", "log"],
            supported_stages: standard_biomolecular_stages(),
            docs_url: "https://www.r-ccs.riken.jp/labs/cbrt/",
            notes: vec!["后续加入完整模板。"],
        },
        EngineDefinition {
            id: "hoomd",
            name: "HOOMD-blue",
            category: EngineCategory::Materials,
            maturity: EngineMaturity::Preview,
            license: open_source("开源 Python/C++ 粒子模拟引擎；材料和软物质体系扩展。"),
            platform_support: linux_first_platforms(),
            detector: Detector::PythonModule("hoomd"),
            gpu_backends: vec![GpuBackend::Cuda, GpuBackend::CpuOnly],
            execution_modes: vec![ExecutionMode::CondaEnvironment, ExecutionMode::Container, ExecutionMode::Ssh, ExecutionMode::Slurm],
            supported_inputs: vec!["gsd", "python"],
            supported_outputs: vec!["gsd", "log"],
            supported_stages: standard_materials_stages(),
            docs_url: "https://hoomd-blue.readthedocs.io/",
            notes: vec!["M5 模板目标。"],
        },
        EngineDefinition {
            id: "dl_poly",
            name: "DL_POLY",
            category: EngineCategory::Materials,
            maturity: EngineMaturity::Preview,
            license: open_source("开源/可获取材料 MD 引擎；按用户安装环境调用。"),
            platform_support: linux_first_platforms(),
            detector: Detector::Executable(vec!["DLPOLY.Z", "dl_poly"]),
            gpu_backends: vec![GpuBackend::CpuOnly],
            execution_modes: vec![ExecutionMode::LocalProcess, ExecutionMode::Container, ExecutionMode::Ssh, ExecutionMode::Slurm],
            supported_inputs: vec!["CONTROL", "CONFIG", "FIELD"],
            supported_outputs: vec!["HISTORY", "STATIS", "REVCON"],
            supported_stages: standard_materials_stages(),
            docs_url: "https://www.scd.stfc.ac.uk/Pages/DL_POLY.aspx",
            notes: vec!["M5 模板目标。"],
        },
        EngineDefinition {
            id: "tinker",
            name: "Tinker / Tinker-HP",
            category: EngineCategory::Biomolecular,
            maturity: EngineMaturity::Preview,
            license: open_source("Tinker 生态按用户安装环境检测；高性能变体可能有单独授权要求。"),
            platform_support: all_desktop_platforms(),
            detector: Detector::Executable(vec!["dynamic", "tinker9"]),
            gpu_backends: vec![GpuBackend::Cuda, GpuBackend::CpuOnly],
            execution_modes: vec![ExecutionMode::LocalProcess, ExecutionMode::Container, ExecutionMode::Ssh, ExecutionMode::Slurm],
            supported_inputs: vec!["xyz", "key", "arc"],
            supported_outputs: vec!["arc", "dyn", "log"],
            supported_stages: standard_biomolecular_stages(),
            docs_url: "https://dasher.wustl.edu/tinker/",
            notes: vec!["M5 模板目标。"],
        },
        EngineDefinition {
            id: "namd",
            name: "NAMD",
            category: EngineCategory::Biomolecular,
            maturity: EngineMaturity::ExternalOnly,
            license: restricted("AutoMD 不分发 NAMD；用户需自行下载并确认其许可条款后配置路径。"),
            platform_support: all_desktop_platforms(),
            detector: Detector::Executable(vec!["namd3", "namd2"]),
            gpu_backends: vec![GpuBackend::Cuda, GpuBackend::CpuOnly],
            execution_modes: vec![ExecutionMode::LocalProcess, ExecutionMode::Container, ExecutionMode::Ssh, ExecutionMode::Slurm],
            supported_inputs: vec!["pdb", "psf", "conf", "dcd"],
            supported_outputs: vec!["dcd", "xst", "log", "restart"],
            supported_stages: standard_biomolecular_stages(),
            docs_url: "https://www.ks.uiuc.edu/Research/namd/",
            notes: vec!["提供兼容入口和授权向导。", "检测到二进制后仍标记为需要用户许可确认。"],
        },
        EngineDefinition {
            id: "amber_pmemd",
            name: "AMBER pmemd",
            category: EngineCategory::Biomolecular,
            maturity: EngineMaturity::ExternalOnly,
            license: restricted("AutoMD 不分发 AMBER pmemd；用户需自行获取 AMBER 许可并配置 pmemd/pmemd.cuda。"),
            platform_support: all_desktop_platforms(),
            detector: Detector::Executable(vec!["pmemd.cuda", "pmemd"]),
            gpu_backends: vec![GpuBackend::Cuda, GpuBackend::CpuOnly],
            execution_modes: vec![ExecutionMode::LocalProcess, ExecutionMode::Ssh, ExecutionMode::Slurm],
            supported_inputs: vec!["prmtop", "inpcrd", "mdin"],
            supported_outputs: vec!["nc", "mdout", "restrt"],
            supported_stages: standard_biomolecular_stages(),
            docs_url: "https://ambermd.org/",
            notes: vec!["AmberTools 可独立使用；pmemd 属于用户自带许可模块。"],
        },
        EngineDefinition {
            id: "charmm",
            name: "CHARMM",
            category: EngineCategory::Biomolecular,
            maturity: EngineMaturity::ExternalOnly,
            license: restricted("AutoMD 只提供 CHARMM 适配器入口；用户需自行获取许可并配置可执行文件。"),
            platform_support: linux_first_platforms(),
            detector: Detector::Executable(vec!["charmm"]),
            gpu_backends: vec![GpuBackend::Cuda, GpuBackend::CpuOnly],
            execution_modes: vec![ExecutionMode::LocalProcess, ExecutionMode::Ssh, ExecutionMode::Slurm],
            supported_inputs: vec!["inp", "psf", "crd", "pdb"],
            supported_outputs: vec!["dcd", "log", "rst"],
            supported_stages: standard_biomolecular_stages(),
            docs_url: "https://academiccharmm.org/",
            notes: vec!["受限许可模块。"],
        },
        EngineDefinition {
            id: "desmond",
            name: "Desmond",
            category: EngineCategory::Biomolecular,
            maturity: EngineMaturity::ExternalOnly,
            license: commercial("AutoMD 不分发 Desmond；用户需在自己的 Schrodinger 授权环境中配置。"),
            platform_support: linux_first_platforms(),
            detector: Detector::Executable(vec!["desmond"]),
            gpu_backends: vec![GpuBackend::Cuda, GpuBackend::CpuOnly],
            execution_modes: vec![ExecutionMode::LocalProcess, ExecutionMode::Ssh, ExecutionMode::Slurm],
            supported_inputs: vec!["cms", "cfg"],
            supported_outputs: vec!["trj", "log", "cms"],
            supported_stages: standard_biomolecular_stages(),
            docs_url: "https://www.schrodinger.com/",
            notes: vec!["商业授权模块。"],
        },
        EngineDefinition {
            id: "acemd",
            name: "ACEMD",
            category: EngineCategory::Biomolecular,
            maturity: EngineMaturity::ExternalOnly,
            license: commercial("AutoMD 不分发 ACEMD；用户需在自己的授权环境中配置。"),
            platform_support: linux_first_platforms(),
            detector: Detector::Executable(vec!["acemd"]),
            gpu_backends: vec![GpuBackend::Cuda],
            execution_modes: vec![ExecutionMode::LocalProcess, ExecutionMode::Ssh, ExecutionMode::Slurm],
            supported_inputs: vec!["pdb", "psf", "prmtop", "conf"],
            supported_outputs: vec!["dcd", "log", "restart"],
            supported_stages: standard_biomolecular_stages(),
            docs_url: "https://www.acellera.com/acemd/",
            notes: vec!["商业授权模块。"],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_contains_first_class_biomolecular_engines() {
        let ids = known_engine_ids();
        assert!(ids.contains(&"gromacs".to_string()));
        assert!(ids.contains(&"openmm".to_string()));
        assert!(ids.contains(&"ambertools".to_string()));
    }

    #[test]
    fn restricted_engines_are_not_bundled() {
        let namd = detect_engine_by_id("namd").expect("NAMD definition exists");
        assert!(namd.license.requires_user_license);
        assert!(!namd.license.bundled_by_automd);
    }
}
