//! System environment resolution.
//!
//! GUI apps launched from Finder/DMG inherit only a minimal PATH (roughly
//! `/usr/bin:/bin:/usr/sbin:/sbin`) and therefore cannot see conda, homebrew,
//! or other user-installed tools that live on the *shell* PATH. This module
//! recovers the real environment so detection / auto-find / auto-install work
//! the same way they do from a terminal.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

/// The user's real PATH as seen by a login+interactive shell. Best-effort and
/// cached for the process lifetime (the shell is spawned at most once).
pub fn login_shell_path() -> Option<&'static str> {
    static CACHE: OnceLock<Option<String>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            if cfg!(target_os = "windows") {
                return None;
            }
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
            // -i so the conda/homebrew init in ~/.zshrc/.bashrc is sourced; stdin
            // is /dev/null so an interactive rc never blocks waiting for input.
            let output = Command::new(shell)
                .args(["-ilc", "printf '%s' \"$PATH\""])
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output()
                .ok()?;
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            (!path.is_empty()).then_some(path)
        })
        .as_deref()
}

/// Directories likely to hold scientific / MD executables, merging (in priority
/// order) the login-shell PATH, the current PATH, well-known absolute dirs, and
/// home-based conda installs + their environments. De-duplicated.
pub fn search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let add = |dirs: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>, path: PathBuf| {
        if !path.as_os_str().is_empty() && seen.insert(path.clone()) {
            dirs.push(path);
        }
    };

    if let Some(path) = login_shell_path() {
        for entry in std::env::split_paths(path) {
            add(&mut dirs, &mut seen, entry);
        }
    }
    if let Ok(path) = std::env::var("PATH") {
        for entry in std::env::split_paths(&path) {
            add(&mut dirs, &mut seen, entry);
        }
    }

    for absolute in [
        "/opt/homebrew/bin",
        "/usr/local/bin",
        "/usr/bin",
        "/bin",
        "/usr/sbin",
        "/sbin",
        "/opt/local/bin",
        "/opt/conda/bin",
        "/opt/gromacs/bin",
        "/usr/local/gromacs/bin",
        "/Applications/Docker.app/Contents/Resources/bin",
    ] {
        add(&mut dirs, &mut seen, PathBuf::from(absolute));
    }

    if let Ok(home) = std::env::var("HOME") {
        for suffix in [
            "miniconda3/bin",
            "miniforge3/bin",
            "mambaforge/bin",
            "miniforge/bin",
            "micromamba/bin",
            "anaconda3/bin",
            "mamba/bin",
            ".local/bin",
            ".pixi/bin",
            "gromacs/bin",
        ] {
            add(&mut dirs, &mut seen, PathBuf::from(&home).join(suffix));
        }
        // conda environments: ~/<dist>/envs/<name>/bin
        for dist in [
            "miniconda3",
            "miniforge3",
            "mambaforge",
            "anaconda3",
            "miniforge",
        ] {
            let envs = PathBuf::from(&home).join(dist).join("envs");
            if let Ok(read) = std::fs::read_dir(&envs) {
                for entry in read.flatten() {
                    add(&mut dirs, &mut seen, entry.path().join("bin"));
                }
            }
        }

        for app_dir in automd_app_data_dirs(&home) {
            let engines = app_dir.join("engines");
            let miniforge = engines.join("_tools").join("miniforge3");
            for suffix in ["bin", "condabin", "Scripts"] {
                add(&mut dirs, &mut seen, miniforge.join(suffix));
            }
            if let Ok(read) = std::fs::read_dir(&engines) {
                for entry in read.flatten() {
                    add(&mut dirs, &mut seen, entry.path().join("bin"));
                    add(&mut dirs, &mut seen, entry.path().join("Scripts"));
                }
            }
            if let Ok(read) = std::fs::read_dir(engines.join("_tools")) {
                for entry in read.flatten() {
                    add(&mut dirs, &mut seen, entry.path().join("bin"));
                    add(&mut dirs, &mut seen, entry.path().join("Scripts"));
                }
            }
        }
    }

    dirs
}

fn automd_app_data_dirs(home: &str) -> Vec<PathBuf> {
    let home = PathBuf::from(home);
    let mut dirs = Vec::new();
    dirs.push(home.join(".automd"));
    if cfg!(target_os = "macos") {
        dirs.push(
            home.join("Library")
                .join("Application Support")
                .join("com.noir.automd"),
        );
    } else if cfg!(target_os = "windows") {
        if let Ok(appdata) = std::env::var("APPDATA") {
            dirs.push(PathBuf::from(appdata).join("com.noir.automd"));
        }
        if let Ok(local_appdata) = std::env::var("LOCALAPPDATA") {
            dirs.push(PathBuf::from(local_appdata).join("com.noir.automd"));
        }
    } else {
        dirs.push(home.join(".local").join("share").join("com.noir.automd"));
    }
    dirs
}

/// Resolve a command to an absolute path: an absolute path as-is, then `which`
/// on the current PATH, then a scan of [`search_dirs`]. Returns None if missing.
pub fn resolve_command(command: &str) -> Option<PathBuf> {
    let direct = Path::new(command);
    if direct.components().count() > 1 && direct.is_file() {
        return Some(direct.to_path_buf());
    }
    if let Ok(found) = which::which(command) {
        return Some(found);
    }
    let names = executable_candidates(command);
    for dir in search_dirs() {
        for name in &names {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Candidate filenames for a command (adds the `.exe` form on Windows).
pub fn executable_candidates(command: &str) -> Vec<String> {
    if cfg!(target_os = "windows") && !command.to_ascii_lowercase().ends_with(".exe") {
        vec![command.to_string(), format!("{command}.exe")]
    } else {
        vec![command.to_string()]
    }
}

/// Resolve a conda-family package manager (micromamba/mamba/conda), honoring the
/// `MAMBA_EXE` / `CONDA_EXE` env vars and the recovered shell PATH.
pub fn resolve_conda_manager() -> Option<PathBuf> {
    for var in ["MAMBA_EXE", "CONDA_EXE"] {
        if let Ok(value) = std::env::var(var) {
            let path = PathBuf::from(value);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    ["micromamba", "mamba", "conda"]
        .into_iter()
        .find_map(resolve_command)
}
