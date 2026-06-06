//! Profile-aware SSH/rsync execution.
//!
//! Drives the *system* `ssh`/`rsync`/`scp` binaries (so the user's existing
//! keys, `~/.ssh/config`, jump hosts and host keys all keep working) but, unlike
//! the old bare `ssh <host>` calls, honors the connection details a beginner
//! actually has: host/IP, port, username and an auth method.
//!
//! Auth handling:
//! - `Agent` / `Key`: plain `std::process::Command` with `BatchMode=yes` so a
//!   misconfigured key fails fast instead of hanging on a hidden prompt.
//! - `Password`: a one-time **ControlMaster** connection is established through a
//!   pseudo-terminal that types the password once; every later command and rsync
//!   reuses that multiplexed socket (`ControlPath`), so they never re-prompt and
//!   piped stdin / rsync work normally. This also keeps us off `sshpass`.
//!
//! macOS/Linux local hosts support ControlMaster; on a Windows *local* host
//! password auth is a known v1 gap (agent/key still work). Remote targets are
//! Linux-first by design.

use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::models::{RemoteAuthMethod, RemoteProfile};
use crate::sysenv;

/// Captured result of a remote command / transfer.
pub struct SshOutcome {
    pub stdout: String,
    pub stderr: String,
    /// Retained for diagnostics/logging even though most callers branch on
    /// `success`.
    #[allow(dead_code)]
    pub exit_code: Option<i32>,
    pub success: bool,
}

impl SshOutcome {
    /// stdout, falling back to stderr — handy for probes that print to either.
    pub fn combined(&self) -> String {
        if self.stdout.trim().is_empty() {
            self.stderr.clone()
        } else if self.stderr.trim().is_empty() {
            self.stdout.clone()
        } else {
            format!("{}\n{}", self.stdout, self.stderr)
        }
    }
}

fn ssh_program() -> PathBuf {
    sysenv::resolve_command("ssh").unwrap_or_else(|| PathBuf::from("ssh"))
}

fn rsync_program() -> Option<PathBuf> {
    sysenv::resolve_command("rsync")
}

/// `user@host` when a username is set, otherwise the bare host (relies on
/// `~/.ssh/config` to supply the user).
pub fn target(profile: &RemoteProfile) -> String {
    let host = profile.host.trim();
    let user = profile.username.trim();
    if user.is_empty() {
        host.to_string()
    } else {
        format!("{user}@{host}")
    }
}

/// Common dial options shared by every invocation (no auth-specific bits).
fn dial_opts(profile: &RemoteProfile) -> Vec<String> {
    let mut opts = vec![
        "-p".to_string(),
        profile.port.to_string(),
        "-o".to_string(),
        "ConnectTimeout=10".to_string(),
        "-o".to_string(),
        "StrictHostKeyChecking=accept-new".to_string(),
        "-o".to_string(),
        "ServerAliveInterval=15".to_string(),
        "-o".to_string(),
        "ServerAliveCountMax=3".to_string(),
    ];
    if profile.auth_method == RemoteAuthMethod::Key {
        if let Some(identity) = profile.identity_file.as_deref() {
            let identity = identity.trim();
            if !identity.is_empty() {
                opts.push("-i".to_string());
                opts.push(identity.to_string());
                opts.push("-o".to_string());
                opts.push("IdentitiesOnly=yes".to_string());
            }
        }
    }
    opts
}

/// Per-profile multiplexing socket used for password auth (ControlMaster).
fn control_socket(profile: &RemoteProfile) -> PathBuf {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    profile.host.hash(&mut hasher);
    profile.port.hash(&mut hasher);
    profile.username.hash(&mut hasher);
    std::env::temp_dir().join(format!("automd-cm-{:016x}", hasher.finish()))
}

fn control_opts(profile: &RemoteProfile) -> Vec<String> {
    vec![
        "-o".to_string(),
        "ControlMaster=auto".to_string(),
        "-o".to_string(),
        format!("ControlPath={}", control_socket(profile).display()),
    ]
}

/// True when a ControlMaster for this profile is already alive.
fn master_alive(profile: &RemoteProfile) -> bool {
    Command::new(ssh_program())
        .arg("-o")
        .arg(format!("ControlPath={}", control_socket(profile).display()))
        .arg("-O")
        .arg("check")
        .arg(target(profile))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// For password auth, make sure an authenticated master connection exists.
/// No-op for agent/key. Errors with a human-readable reason on failure.
pub fn ensure_session(profile: &RemoteProfile, password: Option<&str>) -> Result<(), String> {
    if profile.auth_method != RemoteAuthMethod::Password {
        return Ok(());
    }
    if master_alive(profile) {
        return Ok(());
    }
    let password = password
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "需要密码：该 profile 使用密码认证，但本会话还没有输入密码。".to_string())?;

    let mut args = dial_opts(profile);
    args.extend(control_opts(profile));
    args.extend([
        "-o".to_string(),
        "ControlPersist=10m".to_string(),
        "-o".to_string(),
        "NumberOfPasswordPrompts=1".to_string(),
        "-o".to_string(),
        "PreferredAuthentications=password,keyboard-interactive".to_string(),
        target(profile),
        "true".to_string(),
    ]);

    let outcome = pty_password_run(&ssh_program(), &args, password)?;
    if master_alive(profile) || outcome.success {
        return Ok(());
    }
    Err(classify_connection_error(&outcome.combined()))
}

/// Run a remote command and capture its output.
pub fn run_remote(
    profile: &RemoteProfile,
    password: Option<&str>,
    remote_command: &str,
) -> Result<SshOutcome, String> {
    run_remote_inner(profile, password, remote_command, None)
}

/// Run a remote command, piping `stdin_text` to it.
pub fn run_remote_stdin(
    profile: &RemoteProfile,
    password: Option<&str>,
    remote_command: &str,
    stdin_text: &str,
) -> Result<SshOutcome, String> {
    run_remote_inner(profile, password, remote_command, Some(stdin_text))
}

fn run_remote_inner(
    profile: &RemoteProfile,
    password: Option<&str>,
    remote_command: &str,
    stdin_text: Option<&str>,
) -> Result<SshOutcome, String> {
    ensure_session(profile, password)?;

    let mut args = dial_opts(profile);
    if profile.auth_method == RemoteAuthMethod::Password {
        args.extend(control_opts(profile));
    }
    // BatchMode: with agent/key it fails fast on auth problems; with the
    // password master already up, it just guarantees we never hang on a prompt.
    args.push("-o".to_string());
    args.push("BatchMode=yes".to_string());
    args.push(target(profile));
    args.push(remote_command.to_string());

    let mut command = Command::new(ssh_program());
    command
        .args(&args)
        .stdin(if stdin_text.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|error| format!("无法启动 ssh：{error}"))?;
    if let Some(text) = stdin_text {
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(text.as_bytes())
                .map_err(|error| format!("写入 ssh stdin 失败：{error}"))?;
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|error| format!("ssh 执行失败：{error}"))?;
    Ok(SshOutcome {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code(),
        success: output.status.success(),
    })
}

/// rsync a local directory up to `remote_dir` on the target (archive + resume).
pub fn rsync_up(
    profile: &RemoteProfile,
    password: Option<&str>,
    local_dir: &str,
    remote_dir: &str,
) -> Result<SshOutcome, String> {
    let local = format!("{}/", local_dir.trim_end_matches('/'));
    let remote_dir = expand_remote_path_vars(profile, remote_dir);
    let remote = format!("{}:{}", target(profile), remote_dir);
    rsync_transfer(profile, password, &local, &remote, upload_filter_args())
}

/// rsync results down from `remote_dir` into `local_dir`.
pub fn rsync_down(
    profile: &RemoteProfile,
    password: Option<&str>,
    remote_dir: &str,
    local_dir: &str,
) -> Result<SshOutcome, String> {
    let remote_dir = expand_remote_path_vars(profile, remote_dir);
    let remote = format!("{}:{}/", target(profile), remote_dir.trim_end_matches('/'));
    let local = format!("{}/", local_dir.trim_end_matches('/'));
    rsync_transfer(profile, password, &remote, &local, result_filter_args())
}

fn expand_remote_path_vars(profile: &RemoteProfile, remote_dir: &str) -> String {
    let user = profile.username.trim();
    if user.is_empty() {
        return remote_dir.to_string();
    }
    remote_dir.replace("${USER}", user).replace("$USER", user)
}

pub fn transferred_regular_file_count(output: &str) -> Option<u32> {
    output.lines().find_map(|line| {
        let (label, value) = line.split_once(':')?;
        if !label.trim().eq_ignore_ascii_case("Number of regular files transferred") {
            return None;
        }
        value
            .trim()
            .replace(',', "")
            .parse::<u32>()
            .ok()
    })
}

fn upload_filter_args() -> &'static [&'static str] {
    &[
        "--exclude=.git/",
        "--exclude=.claude/",
        "--exclude=.omc/",
        "--exclude=node_modules/",
        "--exclude=src-tauri/target/",
        "--exclude=dist/",
        "--exclude=target/",
        "--exclude=runs/",
        "--exclude=trajectories/",
        "--exclude=analysis/",
        "--exclude=reports/",
        "--exclude=checkpoints/",
        "--exclude=build-recipes/",
        "--exclude=*.dmg",
        "--exclude=.DS_Store",
    ]
}

fn result_filter_args() -> &'static [&'static str] {
    &[
        "--prune-empty-dirs",
        "--include=*/",
        "--include=runs/***",
        "--include=trajectories/***",
        "--include=analysis/***",
        "--include=reports/***",
        "--include=checkpoints/***",
        "--include=logs/***",
        "--include=remote/***",
        "--exclude=*",
    ]
}

fn rsync_transfer(
    profile: &RemoteProfile,
    password: Option<&str>,
    src: &str,
    dst: &str,
    filter_args: &[&str],
) -> Result<SshOutcome, String> {
    ensure_session(profile, password)?;
    let rsync = rsync_program()
        .ok_or_else(|| "未找到 rsync。请安装 rsync 或在高级模式用导出命令手动同步。".to_string())?;

    // Build the inner ssh transport string rsync uses (-e).
    let mut ssh_parts = vec![format!("{}", ssh_program().display())];
    ssh_parts.extend(dial_opts(profile));
    if profile.auth_method == RemoteAuthMethod::Password {
        ssh_parts.extend(control_opts(profile));
    }
    ssh_parts.push("-o".to_string());
    ssh_parts.push("BatchMode=yes".to_string());
    let ssh_transport = ssh_parts.join(" ");

    let output = Command::new(rsync)
        .arg("-az")
        .arg("--partial")
        .arg("--stats")
        .args(filter_args)
        .arg("-e")
        .arg(&ssh_transport)
        .arg(src)
        .arg(dst)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("无法启动 rsync：{error}"))?;
    Ok(SshOutcome {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code(),
        success: output.status.success(),
    })
}

/// Turn raw ssh stderr into a beginner-friendly cause.
pub fn classify_connection_error(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    if lower.contains("permission denied") || lower.contains("authentication failed") {
        "认证失败：用户名、密码或密钥不正确（HPC 通常三次失败会临时封禁该 IP，请稍后再试）。".to_string()
    } else if lower.contains("connection timed out") || lower.contains("operation timed out") {
        "连接超时：主机/端口不可达，请检查 IP、端口和网络（校园网/VPN）。".to_string()
    } else if lower.contains("connection refused") {
        "连接被拒绝：端口不对或目标未开放 SSH。租用实例的端口通常不是 22。".to_string()
    } else if lower.contains("could not resolve") || lower.contains("name or service not known") {
        "无法解析主机名：请检查主机/IP 是否正确。".to_string()
    } else if lower.contains("host key verification failed") {
        "主机密钥校验失败：远程主机密钥变了，请清理 ~/.ssh/known_hosts 中对应条目后重试。".to_string()
    } else if raw.trim().is_empty() {
        "连接失败：未知原因（无输出）。".to_string()
    } else {
        format!("连接失败：{}", raw.trim())
    }
}

/// Spawn `program` under a PTY, type `password` at the first prompt, and
/// collect all output until the child exits. Used only to bring a password
/// ControlMaster online.
fn pty_password_run(
    program: &std::path::Path,
    args: &[String],
    password: &str,
) -> Result<SshOutcome, String> {
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 120,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|error| format!("无法创建伪终端：{error}"))?;

    let mut cmd = CommandBuilder::new(program);
    for arg in args {
        cmd.arg(arg);
    }
    // Propagate the real environment (HOME, PATH, SSH_AUTH_SOCK…) so ssh behaves
    // as it does in a terminal.
    for (key, value) in std::env::vars() {
        cmd.env(key, value);
    }
    cmd.env("TERM", "xterm-256color");

    let mut child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|error| format!("无法启动 ssh：{error}"))?;
    // Drop the slave so the master sees EOF once the child exits.
    drop(pair.slave);

    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| format!("读取 ssh 输出失败：{error}"))?;
    let mut writer = pair
        .master
        .take_writer()
        .map_err(|error| format!("写入 ssh 失败：{error}"))?;

    let password = password.to_string();
    let reader_handle = std::thread::spawn(move || {
        let mut collected: Vec<u8> = Vec::new();
        let mut buffer = [0u8; 4096];
        let mut sent = false;
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    collected.extend_from_slice(&buffer[..n]);
                    if !sent {
                        let start = collected.len().saturating_sub(256);
                        let tail = String::from_utf8_lossy(&collected[start..]).to_ascii_lowercase();
                        if tail.contains("password") || tail.contains("passcode") {
                            let _ = writer.write_all(password.as_bytes());
                            let _ = writer.write_all(b"\n");
                            let _ = writer.flush();
                            sent = true;
                        }
                    }
                }
                Err(_) => break,
            }
        }
        collected
    });

    let status = child
        .wait()
        .map_err(|error| format!("等待 ssh 结束失败：{error}"))?;
    let collected = reader_handle.join().unwrap_or_default();
    let text = String::from_utf8_lossy(&collected).to_string();
    Ok(SshOutcome {
        stdout: text,
        stderr: String::new(),
        exit_code: Some(status.exit_code() as i32),
        success: status.success(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ExecutionMode;

    fn profile(auth: RemoteAuthMethod) -> RemoteProfile {
        RemoteProfile {
            id: "p".to_string(),
            name: "P".to_string(),
            host: "login.example".to_string(),
            username: "noir".to_string(),
            port: 2222,
            auth_method: auth,
            identity_file: Some("~/.ssh/id_ed25519".to_string()),
            scheduler: ExecutionMode::Slurm,
            workdir: "/scratch/noir/automd".to_string(),
            module_load: vec![],
            default_queue: None,
        }
    }

    #[test]
    fn target_uses_username_when_present() {
        assert_eq!(target(&profile(RemoteAuthMethod::Agent)), "noir@login.example");
        let mut anon = profile(RemoteAuthMethod::Agent);
        anon.username = String::new();
        assert_eq!(target(&anon), "login.example");
    }

    #[test]
    fn dial_opts_set_port_and_identity_for_key_auth() {
        let opts = dial_opts(&profile(RemoteAuthMethod::Key));
        assert!(opts.windows(2).any(|w| w[0] == "-p" && w[1] == "2222"));
        assert!(opts.windows(2).any(|w| w[0] == "-i" && w[1] == "~/.ssh/id_ed25519"));
        // Password auth should not attach the identity file.
        let pwd = dial_opts(&profile(RemoteAuthMethod::Password));
        assert!(!pwd.iter().any(|o| o == "-i"));
    }

    #[test]
    fn expands_user_placeholder_in_remote_rsync_paths() {
        let profile = profile(RemoteAuthMethod::Agent);

        assert_eq!(
            expand_remote_path_vars(&profile, "/scratch/$USER/automd/${USER}"),
            "/scratch/noir/automd/noir"
        );
    }

    #[test]
    fn classify_maps_common_failures() {
        assert!(classify_connection_error("Permission denied (publickey,password).").contains("认证失败"));
        assert!(classify_connection_error("ssh: connect to host x port 22: Connection refused").contains("端口"));
        assert!(classify_connection_error("Connection timed out").contains("超时"));
    }

    #[test]
    fn parses_rsync_regular_file_transfer_count() {
        let output = "\
Number of files: 42 (reg: 31, dir: 11)
Number of regular files transferred: 1,234
Total transferred file size: 10,240 bytes
";

        assert_eq!(transferred_regular_file_count(output), Some(1234));
    }
}
