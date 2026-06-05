use crate::models::*;
use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;

pub const HELPER_VERSION: &str = "0.1.0";

#[derive(Debug, Error)]
pub enum RemoteHelperError {
    #[error("remote helper failed: {0}")]
    Command(String),
    #[error("remote helper json parse failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("remote helper did not report an install path")]
    MissingInstallPath,
}

#[derive(Debug, Clone)]
pub struct RemoteEngineProbe {
    pub location: String,
    pub version: Option<String>,
    pub platform: Option<Platform>,
    pub arch: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HelperProbeOutput {
    helper_version: Option<String>,
    platform: Option<String>,
    arch: Option<String>,
    hostname: Option<String>,
    hardware: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HelperScanOutput {
    found: bool,
    path: Option<String>,
    version: Option<String>,
    platform: Option<String>,
    arch: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HelperInstallOutput {
    status: Option<String>,
    path: Option<String>,
    version: Option<String>,
    stderr: Option<String>,
}

pub fn default_install_path(profile: &RemoteProfile) -> String {
    format!(
        "{}/.automd/helper/{}",
        profile.workdir.trim_end_matches('/'),
        HELPER_VERSION
    )
}

pub fn bash_helper_script() -> &'static str {
    r#"#!/usr/bin/env bash
set -euo pipefail

helper_version="0.1.0"
cmd="${1:-probe}"
shift || true

json_escape() {
  python3 -c 'import json,sys; print(json.dumps(sys.stdin.read())[1:-1])' 2>/dev/null || sed 's/\\/\\\\/g; s/"/\\"/g'
}

detect_platform() {
  case "$(uname -s 2>/dev/null || echo unknown)" in
    Linux*) echo linux ;;
    Darwin*) echo macos ;;
    MINGW*|MSYS*|CYGWIN*) echo windows ;;
    *) echo linux ;;
  esac
}

detect_memory_bytes() {
  if command -v getconf >/dev/null 2>&1; then
    pages="$(getconf _PHYS_PAGES 2>/dev/null || echo 0)"
    page_size="$(getconf PAGE_SIZE 2>/dev/null || echo 0)"
    if [ "${pages:-0}" -gt 0 ] 2>/dev/null && [ "${page_size:-0}" -gt 0 ] 2>/dev/null; then
      echo $((pages * page_size))
      return
    fi
  fi
  echo 0
}

case "$cmd" in
  probe)
    platform="$(detect_platform)"
    arch="$(uname -m 2>/dev/null || echo unknown)"
    hostname_value="$(hostname 2>/dev/null || echo unknown)"
    cpu_count="$(getconf _NPROCESSORS_ONLN 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 0)"
    memory_bytes="$(detect_memory_bytes)"
    if command -v nvidia-smi >/dev/null 2>&1; then
      gpu_summary="$(nvidia-smi --query-gpu=name,memory.total --format=csv,noheader 2>/dev/null | head -n 8 | json_escape)"
    else
      gpu_summary=""
    fi
    printf '{"helperVersion":"%s","platform":"%s","arch":"%s","hostname":"%s","hardware":{"cpuCount":%s,"memoryBytes":%s,"gpuSummary":"%s"}}\n' \
      "$helper_version" "$platform" "$arch" "$hostname_value" "${cpu_count:-0}" "${memory_bytes:-0}" "$gpu_summary"
    ;;
  scan-engines|find-executable)
    platform="$(detect_platform)"
    arch="$(uname -m 2>/dev/null || echo unknown)"
    for candidate in "$@"; do
      if [ -z "$candidate" ]; then
        continue
      fi
      if [[ "$candidate" == python\ module:* ]]; then
        module="${candidate#python module:}"
        module="$(echo "$module" | xargs)"
        if python3 -c "import ${module}" >/dev/null 2>&1; then
          version="$(python3 - <<PY 2>/dev/null || true
import importlib
m=importlib.import_module("${module}")
print(getattr(m, "__version__", "python-module"))
PY
)"
          py_path="$(command -v python3 || echo python3)"
          printf '{"found":true,"path":"%s","version":"%s","platform":"%s","arch":"%s"}\n' "$py_path" "${version:-python-module}" "$platform" "$arch"
          exit 0
        fi
        continue
      fi
      if command -v "$candidate" >/dev/null 2>&1; then
        path="$(command -v "$candidate")"
        version="$($candidate --version 2>&1 | head -n 1 | json_escape || true)"
        printf '{"found":true,"path":"%s","version":"%s","platform":"%s","arch":"%s"}\n' "$path" "$version" "$platform" "$arch"
        exit 0
      fi
    done
    printf '{"found":false,"path":null,"version":null,"platform":"%s","arch":"%s"}\n' "$platform" "$arch"
    ;;
  install-engine)
    engine_id="${1:-}"
    package="${2:-}"
    shift 2 || true
    prefix="$HOME/.automd/engines/$engine_id"
    if [ -z "$engine_id" ] || [ -z "$package" ]; then
      echo '{"status":"failed","stderr":"missing engine id or package"}'
      exit 2
    fi
    manager=""
    for candidate in micromamba mamba conda; do
      if command -v "$candidate" >/dev/null 2>&1; then
        manager="$candidate"
        break
      fi
    done
    if [ -z "$manager" ]; then
      echo '{"status":"failed","stderr":"remote conda/mamba/micromamba not found"}'
      exit 3
    fi
    if [ "$manager" = "conda" ] || [ "$manager" = "mamba" ]; then
      "$manager" create -y -p "$prefix" -c conda-forge "$package"
    else
      "$manager" create -y -p "$prefix" -c conda-forge "$package"
    fi
    for candidate in "$@"; do
      if [ -x "$prefix/bin/$candidate" ]; then
        printf '{"status":"completed","path":"%s","version":"conda-forge"}\n' "$prefix/bin/$candidate"
        exit 0
      fi
    done
    printf '{"status":"completed","path":"%s","version":"conda-forge"}\n' "$prefix"
    ;;
  build-engine)
    engine_id="${1:-engine}"
    mkdir -p "$HOME/.automd/builds/$engine_id"
    script="$HOME/.automd/builds/$engine_id/build-$engine_id.sh"
    cat > "$script"
    chmod +x "$script"
    bash "$script"
    ;;
  task-status|tail-log)
    echo '{"status":"completed","message":"helper command is available"}'
    ;;
  *)
    echo '{"status":"failed","stderr":"unknown helper command"}'
    exit 1
    ;;
esac
"#
}

pub fn powershell_helper_script() -> &'static str {
    r#"
param(
  [string]$Command = "probe",
  [Parameter(ValueFromRemainingArguments = $true)]
  [string[]]$Rest
)
$helperVersion = "0.1.0"
if ($Command -eq "probe") {
  $cpu = [Environment]::ProcessorCount
  $memory = 0
  try { $memory = (Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory } catch {}
  [pscustomobject]@{
    helperVersion = $helperVersion
    platform = "windows"
    arch = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString().ToLowerInvariant()
    hostname = $env:COMPUTERNAME
    hardware = @{ cpuCount = $cpu; memoryBytes = $memory; gpuSummary = "" }
  } | ConvertTo-Json -Compress
  exit 0
}
if ($Command -eq "scan-engines" -or $Command -eq "find-executable") {
  foreach ($candidate in $Rest) {
    if (-not $candidate) { continue }
    $resolved = Get-Command $candidate -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($resolved) {
      [pscustomobject]@{
        found = $true
        path = $resolved.Source
        version = "detected"
        platform = "windows"
        arch = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString().ToLowerInvariant()
      } | ConvertTo-Json -Compress
      exit 0
    }
  }
  [pscustomobject]@{
    found = $false
    path = $null
    version = $null
    platform = "windows"
    arch = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString().ToLowerInvariant()
  } | ConvertTo-Json -Compress
  exit 0
}
"#
}

pub fn install_helper(
    profile: &RemoteProfile,
    password: Option<&str>,
) -> Result<RemoteHelperStatus, RemoteHelperError> {
    let install_path = default_install_path(profile);
    let bash_path = format!("{install_path}/automd-helper.sh");
    let ps_path = format!("{install_path}/automd-helper.ps1");
    let bash_cmd = format!(
        "mkdir -p {dir} && cat > {bash} && chmod +x {bash}",
        dir = shell_quote(&install_path),
        bash = shell_quote(&bash_path)
    );
    match ssh_with_stdin(profile, password, &bash_cmd, bash_helper_script()) {
        Ok(_) => check_helper(profile, Some(install_path), password),
        Err(bash_error) => {
            let ps_cmd = format!(
                "powershell -NoProfile -ExecutionPolicy Bypass -Command \"$d='{dir}'; $p='{path}'; New-Item -ItemType Directory -Force -Path $d | Out-Null; [Console]::In.ReadToEnd() | Set-Content -Encoding UTF8 -Path $p\"",
                dir = ps_escape(&install_path),
                path = ps_escape(&ps_path)
            );
            ssh_with_stdin(profile, password, &ps_cmd, powershell_helper_script())
                .map_err(|ps_error| RemoteHelperError::Command(format!("{bash_error}; {ps_error}")))?;
            check_helper(profile, Some(install_path), password)
        }
    }
}

pub fn check_helper(
    profile: &RemoteProfile,
    install_path: Option<String>,
    password: Option<&str>,
) -> Result<RemoteHelperStatus, RemoteHelperError> {
    let install_path = install_path.unwrap_or_else(|| default_install_path(profile));
    let bash_cmd = format!(
        "bash {}/automd-helper.sh probe",
        shell_quote(&install_path)
    );
    let output = match ssh_capture(profile, password, &bash_cmd) {
        Ok(output) => output,
        Err(bash_error) => {
            let ps_cmd = format!(
                "powershell -NoProfile -ExecutionPolicy Bypass -File {} probe",
                ps_remote_quote(&format!("{install_path}/automd-helper.ps1"))
            );
            ssh_capture(profile, password, &ps_cmd)
                .map_err(|ps_error| RemoteHelperError::Command(format!("{bash_error}; {ps_error}")))?
        }
    };
    parse_probe(profile, &install_path, &output)
}

pub fn scan_engine(
    profile: &RemoteProfile,
    install_path: &str,
    commands: &[String],
    password: Option<&str>,
) -> Result<Option<RemoteEngineProbe>, RemoteHelperError> {
    let args = commands
        .iter()
        .map(|value| shell_quote(value))
        .collect::<Vec<_>>()
        .join(" ");
    let bash_cmd = format!(
        "bash {}/automd-helper.sh scan-engines {}",
        shell_quote(install_path),
        args
    );
    let output = match ssh_capture(profile, password, &bash_cmd) {
        Ok(output) => output,
        Err(_) => {
            let ps_args = commands
                .iter()
                .map(|value| ps_remote_quote(value))
                .collect::<Vec<_>>()
                .join(" ");
            let ps_cmd = format!(
                "powershell -NoProfile -ExecutionPolicy Bypass -File {} scan-engines {}",
                ps_remote_quote(&format!("{install_path}/automd-helper.ps1")),
                ps_args
            );
            ssh_capture(profile, password, &ps_cmd)?
        }
    };
    let parsed: HelperScanOutput = serde_json::from_str(output.trim())?;
    if !parsed.found {
        return Ok(None);
    }
    Ok(parsed.path.map(|location| RemoteEngineProbe {
        location,
        version: parsed.version,
        platform: parsed.platform.as_deref().and_then(platform_from_helper),
        arch: parsed.arch,
    }))
}

pub fn install_engine_with_helper(
    profile: &RemoteProfile,
    install_path: &str,
    engine_id: &str,
    package: &str,
    executable_names: &[String],
    password: Option<&str>,
) -> Result<RemoteEngineProbe, RemoteHelperError> {
    let args = executable_names
        .iter()
        .map(|value| shell_quote(value))
        .collect::<Vec<_>>()
        .join(" ");
    let command = format!(
        "bash {}/automd-helper.sh install-engine {} {} {}",
        shell_quote(install_path),
        shell_quote(engine_id),
        shell_quote(package),
        args
    );
    let output = ssh_capture(profile, password, &command)?;
    let parsed: HelperInstallOutput = serde_json::from_str(output.trim())?;
    if parsed.status.as_deref() == Some("failed") {
        return Err(RemoteHelperError::Command(
            parsed.stderr.unwrap_or_else(|| "remote install failed".to_string()),
        ));
    }
    let location = parsed.path.ok_or(RemoteHelperError::MissingInstallPath)?;
    let probe = scan_engine(profile, install_path, executable_names, password)?.unwrap_or(RemoteEngineProbe {
        location,
        version: parsed.version,
        platform: None,
        arch: None,
    });
    Ok(probe)
}

pub fn run_build_engine_with_helper(
    profile: &RemoteProfile,
    install_path: &str,
    engine_id: &str,
    script: &str,
    password: Option<&str>,
) -> Result<String, RemoteHelperError> {
    let command = format!(
        "bash {}/automd-helper.sh build-engine {}",
        shell_quote(install_path),
        shell_quote(engine_id)
    );
    ssh_with_stdin(profile, password, &command, script)
}

fn parse_probe(profile: &RemoteProfile, install_path: &str, output: &str) -> Result<RemoteHelperStatus, RemoteHelperError> {
    let parsed: HelperProbeOutput = serde_json::from_str(output.trim())?;
    let status = if parsed.helper_version.as_deref() == Some(HELPER_VERSION) {
        RemoteHelperState::Ready
    } else {
        RemoteHelperState::Outdated
    };
    Ok(RemoteHelperStatus {
        profile_id: profile.id.clone(),
        helper_version: parsed.helper_version,
        status,
        install_path: Some(install_path.to_string()),
        platform: parsed.platform.as_deref().and_then(platform_from_helper),
        arch: parsed.arch,
        hostname: parsed.hostname,
        hardware_json: parsed.hardware.map(|value| value.to_string()),
        checked_at: Utc::now(),
        last_error: None,
    })
}

fn ssh_capture(
    profile: &RemoteProfile,
    password: Option<&str>,
    remote_command: &str,
) -> Result<String, RemoteHelperError> {
    let outcome = crate::ssh::run_remote(profile, password, remote_command)
        .map_err(RemoteHelperError::Command)?;
    if !outcome.success {
        return Err(RemoteHelperError::Command(
            crate::ssh::classify_connection_error(&outcome.combined()),
        ));
    }
    Ok(outcome.stdout)
}

fn ssh_with_stdin(
    profile: &RemoteProfile,
    password: Option<&str>,
    remote_command: &str,
    stdin_text: &str,
) -> Result<String, RemoteHelperError> {
    let outcome = crate::ssh::run_remote_stdin(profile, password, remote_command, stdin_text)
        .map_err(RemoteHelperError::Command)?;
    if !outcome.success {
        return Err(RemoteHelperError::Command(
            crate::ssh::classify_connection_error(&outcome.combined()),
        ));
    }
    Ok(outcome.stdout)
}

fn platform_from_helper(value: &str) -> Option<Platform> {
    match value {
        "windows" => Some(Platform::Windows),
        "macos" => Some(Platform::Macos),
        "linux" => Some(Platform::Linux),
        _ => None,
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn ps_escape(value: &str) -> String {
    value.replace('\'', "''")
}

fn ps_remote_quote(value: &str) -> String {
    format!("'{}'", ps_escape(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_scripts_cover_required_commands() {
        let bash = bash_helper_script();
        assert!(bash.contains("probe"));
        assert!(bash.contains("scan-engines"));
        assert!(bash.contains("install-engine"));
        assert!(bash.contains("build-engine"));
        let ps = powershell_helper_script();
        assert!(ps.contains("probe"));
        assert!(ps.contains("scan-engines"));
    }

    #[test]
    fn default_install_path_uses_profile_workdir_and_version() {
        let profile = RemoteProfile {
            id: "cluster".to_string(),
            name: "Cluster".to_string(),
            host: "login.example".to_string(),
            username: String::new(),
            port: 22,
            auth_method: RemoteAuthMethod::Agent,
            identity_file: None,
            scheduler: ExecutionMode::Ssh,
            workdir: "/scratch/noir/automd/".to_string(),
            module_load: vec![],
            default_queue: None,
        };
        assert_eq!(
            default_install_path(&profile),
            format!("/scratch/noir/automd/.automd/helper/{HELPER_VERSION}")
        );
    }
}
