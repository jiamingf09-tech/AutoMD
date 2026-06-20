use crate::models::*;
use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;
use thiserror::Error;

pub const HELPER_VERSION: &str = "0.1.1";

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

helper_version="0.1.1"
cmd="${1:-probe}"
shift || true

json_escape() {
  python3 -c 'import json,sys; print(json.dumps(sys.stdin.read())[1:-1])' 2>/dev/null || sed 's/\\/\\\\/g; s/"/\\"/g'
}

probe_version() {
  executable="$1"
  candidate="$2"
  case "$candidate" in
    lmp|lmp_*|lmp-*) raw="$("$executable" -h 2>&1 || true)" ;;
    tleap|sander|cpptraj|antechamber|parmchk2) raw="$("$executable" -h 2>&1 || true)" ;;
    *) raw="$("$executable" --version 2>&1 || true)" ;;
  esac
  printf '%s\n' "$raw" | awk -v candidate="$candidate" '
    {
      line=$0
      low=tolower(line)
      if (line == "" || line ~ /^-I:/ || low ~ /^adding / || low ~ /^usage:/ || low ~ /^error:/ || low ~ /invalid command-line argument/) next
      if (candidate ~ /^lmp/ && (low ~ /lammps/ || low ~ /large-scale atomic/)) { print line; found=1; exit }
      if ((candidate == "tleap" || candidate == "sander" || candidate == "cpptraj" || candidate == "antechamber" || candidate == "parmchk2") &&
          (low ~ /amber/ || low ~ /leap/ || low ~ /tleap/ || low ~ /sander/ || low ~ /cpptraj/)) { print line; found=1; exit }
      if (first == "") first=line
    }
    END {
      if (!found && first != "") print first
      if (!found && first == "" && (candidate == "tleap" || candidate == "sander" || candidate == "cpptraj" || candidate == "antechamber" || candidate == "parmchk2")) print "AmberTools detected"
    }
  '
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

micromamba_subdir() {
  platform="$(detect_platform)"
  arch="$(uname -m 2>/dev/null || echo unknown)"
  case "$platform:$arch" in
    linux:x86_64|linux:amd64) echo linux-64 ;;
    linux:aarch64|linux:arm64) echo linux-aarch64 ;;
    macos:x86_64|macos:amd64) echo osx-64 ;;
    macos:aarch64|macos:arm64) echo osx-arm64 ;;
    *) return 1 ;;
  esac
}

download_file() {
  url="$1"
  output="$2"
  errors=""
  if command -v curl >/dev/null 2>&1; then
    if curl -fsSL "$url" -o "$output"; then
      return 0
    fi
    errors="${errors}curl failed; "
  fi
  if command -v wget >/dev/null 2>&1; then
    if wget -qO "$output" "$url"; then
      return 0
    fi
    errors="${errors}wget failed; "
  fi
  if command -v python3 >/dev/null 2>&1; then
    if python3 - "$url" "$output" <<'PY'
import pathlib
import sys
import urllib.request

url, output = sys.argv[1], sys.argv[2]
pathlib.Path(output).parent.mkdir(parents=True, exist_ok=True)
with urllib.request.urlopen(url, timeout=120) as response:
    pathlib.Path(output).write_bytes(response.read())
PY
    then
      return 0
    fi
    errors="${errors}python3 urllib failed; "
  fi
  if [ -z "$errors" ]; then
    echo "remote curl/wget/python3 not found; cannot download micromamba" >&2
  else
    echo "failed to download managed micromamba: $errors" >&2
  fi
  return 1
}

install_managed_micromamba() {
  tool_dir="$HOME/.automd/tools/micromamba"
  micromamba_bin="$tool_dir/bin/micromamba"
  if [ -x "$micromamba_bin" ]; then
    echo "$micromamba_bin"
    return 0
  fi
  subdir="$(micromamba_subdir)" || {
    echo "micromamba is not available for this remote platform/architecture" >&2
    return 1
  }
  mkdir -p "$tool_dir"
  archive="$tool_dir/micromamba.tar.bz2"
  url="https://micro.mamba.pm/api/micromamba/$subdir/latest"
  if ! download_file "$url" "$archive"; then
    return 1
  fi
  if ! tar -xjf "$archive" -C "$tool_dir" bin/micromamba; then
    echo "failed to extract managed micromamba archive" >&2
    return 1
  fi
  if [ ! -f "$micromamba_bin" ]; then
    echo "managed micromamba archive did not contain bin/micromamba" >&2
    return 1
  fi
  chmod +x "$micromamba_bin"
  echo "$micromamba_bin"
}

find_package_manager() {
  for candidate in micromamba mamba conda; do
    if command -v "$candidate" >/dev/null 2>&1; then
      command -v "$candidate"
      return 0
    fi
  done
  install_managed_micromamba
}

case "$cmd" in
  probe)
    platform="$(detect_platform)"
    arch="$(uname -m 2>/dev/null || echo unknown)"
    hostname_value="$(hostname 2>/dev/null || echo unknown)"
    cpu_count="$(getconf _NPROCESSORS_ONLN 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 0)"
    memory_bytes="$(detect_memory_bytes)"
    if command -v nvidia-smi >/dev/null 2>&1; then
      gpu_summary="$(nvidia-smi --query-gpu=name,memory.total --format=csv,noheader 2>/dev/null | head -n 8 | json_escape || true)"
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
        python_candidates=""
        if command -v python3 >/dev/null 2>&1; then
          python_candidates="$(command -v python3)"
        fi
        for managed_python in "$HOME"/.automd/engines/*/bin/python; do
          if [ -x "$managed_python" ]; then
            python_candidates="${python_candidates}
${managed_python}"
          fi
        done
        while IFS= read -r python_path; do
          if [ -z "$python_path" ] || [ ! -x "$python_path" ]; then
            continue
          fi
          if "$python_path" -c "import ${module}" >/dev/null 2>&1; then
            version="$("$python_path" - <<PY 2>/dev/null || true
import importlib
m=importlib.import_module("${module}")
print(getattr(m, "__version__", "python-module"))
PY
)"
            printf '{"found":true,"path":"%s","version":"%s","platform":"%s","arch":"%s"}\n' "$python_path" "${version:-python-module}" "$platform" "$arch"
            exit 0
          fi
        done <<PYTHONS
${python_candidates}
PYTHONS
        continue
      fi
      if command -v "$candidate" >/dev/null 2>&1; then
        path="$(command -v "$candidate")"
        version="$(probe_version "$path" "$candidate" | awk 'NR==1 { printf "%s", $0; exit }' | json_escape || true)"
        printf '{"found":true,"path":"%s","version":"%s","platform":"%s","arch":"%s"}\n' "$path" "$version" "$platform" "$arch"
        exit 0
      fi
      for managed_path in "$HOME"/.automd/engines/*/bin/"$candidate"; do
        if [ -x "$managed_path" ]; then
          version="$(probe_version "$managed_path" "$candidate" | awk 'NR==1 { printf "%s", $0; exit }' | json_escape || true)"
          printf '{"found":true,"path":"%s","version":"%s","platform":"%s","arch":"%s"}\n' "$managed_path" "$version" "$platform" "$arch"
          exit 0
        fi
      done
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
    manager_error="$(mktemp)"
    manager="$(find_package_manager 2>"$manager_error" || true)"
    if [ -z "$manager" ]; then
      manager_message="$(cat "$manager_error" | json_escape)"
      rm -f "$manager_error"
      if [ -z "$manager_message" ]; then
        manager_message="remote conda/mamba/micromamba not found and managed micromamba bootstrap failed"
      fi
      printf '{"status":"failed","stderr":"%s"}\n' "$manager_message"
      exit 3
    fi
    rm -f "$manager_error"
    export MAMBA_ROOT_PREFIX="$HOME/.automd/micromamba-root"
    mkdir -p "$MAMBA_ROOT_PREFIX"
    "$manager" create -y -p "$prefix" -c conda-forge "$package"
    for candidate in "$@"; do
      if [[ "$candidate" == python\ module:* ]]; then
        module="${candidate#python module:}"
        module="$(echo "$module" | xargs)"
        python_path="$prefix/bin/python"
        if [ -x "$python_path" ] && "$python_path" -c "import ${module}" >/dev/null 2>&1; then
          version="$("$python_path" - <<PY 2>/dev/null || true
import importlib.metadata as metadata
try:
    print(metadata.version("${module}"))
except Exception:
    print("python-module")
PY
)"
          printf '{"status":"completed","path":"%s","version":"%s"}\n' "$python_path" "${version:-python-module}"
          exit 0
        fi
        continue
      fi
      if [ -x "$prefix/bin/$candidate" ]; then
        printf '{"status":"completed","path":"%s","version":"conda-forge"}\n' "$prefix/bin/$candidate"
        exit 0
      fi
    done
    printf '{"status":"failed","stderr":"package installed but no declared engine entrypoint was found under %s"}\n' "$prefix"
    exit 4
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
$helperVersion = "0.1.1"
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
    let remote_is_posix = remote_looks_posix(profile, password);
    match ssh_with_stdin(profile, password, &bash_cmd, bash_helper_script()) {
        Ok(_) => check_helper(profile, Some(install_path), password),
        Err(bash_error) => {
            let bash_error_text = bash_error.to_string();
            if remote_is_posix || !should_try_powershell_after_bash_error(&bash_error_text) {
                return Err(bash_error);
            }
            let ps_cmd = format!(
                "powershell -NoProfile -ExecutionPolicy Bypass -Command \"$d='{dir}'; $p='{path}'; New-Item -ItemType Directory -Force -Path $d | Out-Null; [Console]::In.ReadToEnd() | Set-Content -Encoding UTF8 -Path $p\"",
                dir = ps_escape(&install_path),
                path = ps_escape(&ps_path)
            );
            ssh_with_stdin(profile, password, &ps_cmd, powershell_helper_script()).map_err(
                |ps_error| RemoteHelperError::Command(format!("{bash_error}; {ps_error}")),
            )?;
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
    let remote_is_posix = remote_looks_posix(profile, password);
    let bash_cmd = format!("bash {}/automd-helper.sh probe", shell_quote(&install_path));
    let output = match ssh_capture(profile, password, &bash_cmd) {
        Ok(output) => output,
        Err(bash_error) => {
            let bash_error_text = bash_error.to_string();
            if remote_is_posix || !should_try_powershell_after_bash_error(&bash_error_text) {
                return Err(bash_error);
            }
            let ps_cmd = format!(
                "powershell -NoProfile -ExecutionPolicy Bypass -File {} probe",
                ps_remote_quote(&format!("{install_path}/automd-helper.ps1"))
            );
            ssh_capture(profile, password, &ps_cmd).map_err(|ps_error| {
                RemoteHelperError::Command(format!("{bash_error}; {ps_error}"))
            })?
        }
    };
    parse_probe(profile, &install_path, &output)
}

fn remote_looks_posix(profile: &RemoteProfile, password: Option<&str>) -> bool {
    ssh_capture(profile, password, "uname -s 2>/dev/null")
        .map(|output| is_posix_uname_output(&output))
        .unwrap_or(false)
}

fn platform_uses_bash(platform: Option<&Platform>) -> bool {
    matches!(
        platform,
        Some(Platform::Linux | Platform::Macos | Platform::Wsl2 | Platform::RemoteLinux)
    )
}

fn is_posix_uname_output(output: &str) -> bool {
    let lower = output.to_ascii_lowercase();
    lower.contains("linux") || lower.contains("darwin") || lower.contains("freebsd")
}

fn should_try_powershell_after_bash_error(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    !(lower.contains("permission denied")
        || lower.contains("权限")
        || lower.contains("无法创建目录")
        || lower.contains("mkdir:")
        || lower.contains("chmod:")
        || lower.contains("cat:"))
}

pub fn scan_engine(
    profile: &RemoteProfile,
    install_path: &str,
    commands: &[String],
    known_platform: Option<&Platform>,
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
        Err(bash_error) => {
            let bash_error_text = bash_error.to_string();
            let remote_is_posix =
                platform_uses_bash(known_platform) || remote_looks_posix(profile, password);
            if remote_is_posix || !should_try_powershell_after_bash_error(&bash_error_text) {
                return Err(bash_error);
            }
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
            ssh_capture(profile, password, &ps_cmd).map_err(|ps_error| {
                RemoteHelperError::Command(format!("{bash_error}; {ps_error}"))
            })?
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
    timeout_seconds: Option<u64>,
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
    let output = ssh_capture_timeout(profile, password, &command, timeout_seconds)?;
    let parsed: HelperInstallOutput = serde_json::from_str(last_json_line(&output)?)?;
    if parsed.status.as_deref() == Some("failed") {
        return Err(RemoteHelperError::Command(
            parsed
                .stderr
                .unwrap_or_else(|| "remote install failed".to_string()),
        ));
    }
    let location = parsed.path.ok_or(RemoteHelperError::MissingInstallPath)?;
    let probe = scan_engine(profile, install_path, executable_names, None, password)?.unwrap_or(
        RemoteEngineProbe {
            location,
            version: parsed.version,
            platform: None,
            arch: None,
        },
    );
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

fn last_json_line(output: &str) -> Result<&str, RemoteHelperError> {
    output
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| line.starts_with('{') && line.ends_with('}'))
        .ok_or_else(|| {
            RemoteHelperError::Command("remote helper did not emit a JSON result".to_string())
        })
}

fn parse_probe(
    profile: &RemoteProfile,
    install_path: &str,
    output: &str,
) -> Result<RemoteHelperStatus, RemoteHelperError> {
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

fn ssh_capture_timeout(
    profile: &RemoteProfile,
    password: Option<&str>,
    remote_command: &str,
    timeout_seconds: Option<u64>,
) -> Result<String, RemoteHelperError> {
    let timeout = Duration::from_secs(timeout_seconds.unwrap_or(600).max(1));
    let outcome = crate::ssh::run_remote_timeout(profile, password, remote_command, timeout)
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
        assert!(bash.contains("install_managed_micromamba"));
        assert!(bash.contains("micro.mamba.pm/api/micromamba"));
        assert!(bash.contains("python module:"));
        assert!(bash.contains("\"$HOME\"/.automd/engines/*/bin/python"));
        assert!(bash.contains("no declared engine entrypoint"));
        let ps = powershell_helper_script();
        assert!(ps.contains("probe"));
        assert!(ps.contains("scan-engines"));
    }

    #[test]
    fn install_output_parser_uses_last_json_line() {
        let output = "Collecting package metadata...\nTransaction finished\n{\"status\":\"completed\",\"path\":\"/tmp/gmx\",\"version\":\"conda-forge\"}\n";
        assert_eq!(
            last_json_line(output).expect("json line"),
            "{\"status\":\"completed\",\"path\":\"/tmp/gmx\",\"version\":\"conda-forge\"}"
        );
        assert!(last_json_line("no json here").is_err());
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

    #[test]
    fn posix_uname_output_is_detected() {
        assert!(is_posix_uname_output("Linux\n"));
        assert!(is_posix_uname_output("Darwin\n"));
        assert!(!is_posix_uname_output("Microsoft Windows [Version 10.0]\n"));
    }

    #[test]
    fn known_posix_platforms_use_bash_helper() {
        assert!(platform_uses_bash(Some(&Platform::Linux)));
        assert!(platform_uses_bash(Some(&Platform::Macos)));
        assert!(platform_uses_bash(Some(&Platform::Wsl2)));
        assert!(platform_uses_bash(Some(&Platform::RemoteLinux)));
        assert!(!platform_uses_bash(Some(&Platform::Windows)));
        assert!(!platform_uses_bash(None));
    }

    #[test]
    fn posix_permission_errors_do_not_try_powershell_fallback() {
        assert!(!should_try_powershell_after_bash_error(
            "mkdir: 无法创建目录 \"/root\": 权限不够"
        ));
        assert!(!should_try_powershell_after_bash_error(
            "mkdir: cannot create directory '/root': Permission denied"
        ));
        assert!(should_try_powershell_after_bash_error(
            "bash command unavailable on remote shell"
        ));
    }

    #[cfg(unix)]
    #[test]
    fn bash_scan_engine_cleans_ambertools_and_lammps_versions() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;
        use std::process::Command;

        let root =
            std::env::temp_dir().join(format!("automd-helper-scan-test-{}", uuid::Uuid::new_v4()));
        let bin = root.join("bin");
        fs::create_dir_all(&bin).expect("create bin");
        let tleap = bin.join("tleap");
        fs::write(
            &tleap,
            "#!/usr/bin/env sh\necho '-I: Adding /opt/amber/dat/leap/prep to search path.'\necho 'Welcome to LEaP!'\n",
        )
        .expect("write fake tleap");
        fs::set_permissions(&tleap, fs::Permissions::from_mode(0o755)).expect("chmod tleap");
        let lmp = bin.join("lmp");
        fs::write(
            &lmp,
            "#!/usr/bin/env sh\necho 'ERROR: Invalid command-line argument: --version'\necho 'Large-scale Atomic/Molecular Massively Parallel Simulator - 10 Sep 2025'\n",
        )
        .expect("write fake lmp");
        fs::set_permissions(&lmp, fs::Permissions::from_mode(0o755)).expect("chmod lmp");

        let helper = root.join("automd-helper.sh");
        fs::write(&helper, bash_helper_script()).expect("write helper");
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).expect("chmod helper");
        let path = format!(
            "{}:{}",
            bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );

        let amber_output = Command::new("bash")
            .arg(&helper)
            .arg("scan-engines")
            .arg("tleap")
            .env("PATH", &path)
            .output()
            .expect("run amber scan");
        let amber: HelperScanOutput =
            serde_json::from_slice(&amber_output.stdout).expect("amber json");
        assert_eq!(amber.version.as_deref(), Some("Welcome to LEaP!"));

        let lammps_output = Command::new("bash")
            .arg(&helper)
            .arg("scan-engines")
            .arg("lmp")
            .env("PATH", path)
            .output()
            .expect("run lammps scan");
        let lammps: HelperScanOutput =
            serde_json::from_slice(&lammps_output.stdout).expect("lammps json");
        assert_eq!(
            lammps.version.as_deref(),
            Some("Large-scale Atomic/Molecular Massively Parallel Simulator - 10 Sep 2025")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn bash_probe_survives_broken_nvidia_smi() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;
        use std::process::Command;

        let root =
            std::env::temp_dir().join(format!("automd-helper-test-{}", uuid::Uuid::new_v4()));
        let bin = root.join("bin");
        fs::create_dir_all(&bin).expect("create bin");
        let fake_nvidia_smi = bin.join("nvidia-smi");
        fs::write(
            &fake_nvidia_smi,
            "#!/usr/bin/env sh\necho 'driver unavailable' >&2\nexit 9\n",
        )
        .expect("write fake nvidia-smi");
        fs::set_permissions(&fake_nvidia_smi, fs::Permissions::from_mode(0o755))
            .expect("chmod fake");

        let helper = root.join("automd-helper.sh");
        fs::write(&helper, bash_helper_script()).expect("write helper");
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).expect("chmod helper");

        let path = format!(
            "{}:{}",
            bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let output = Command::new("bash")
            .arg(&helper)
            .arg("probe")
            .env("PATH", path)
            .output()
            .expect("run helper");

        let _ = fs::remove_dir_all(root);
        assert!(
            output.status.success(),
            "helper failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let parsed: HelperProbeOutput =
            serde_json::from_slice(&output.stdout).expect("probe output should be json");
        assert_eq!(parsed.helper_version.as_deref(), Some(HELPER_VERSION));
    }
}
