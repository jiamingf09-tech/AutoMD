import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import crypto from "node:crypto";
import { spawn } from "node:child_process";

const root = process.cwd();
const env = process.env;
const failures = [];
const startedAt = new Date();

function info(message) {
  console.log(`[remote-acceptance] ${message}`);
}

function fail(message) {
  failures.push(message);
  console.error(`[remote-acceptance] FAIL: ${message}`);
}

function check(condition, message) {
  if (!condition) fail(message);
}

function assertOrThrow(condition, message) {
  if (!condition) throw new Error(message);
}

function shellQuote(value) {
  return `'${String(value).replace(/'/g, `'\\''`)}'`;
}

function tclBrace(value) {
  return `{${String(value).replace(/[\\{}]/g, "\\$&")}}`;
}

function safeName(value) {
  return String(value).replace(/[^A-Za-z0-9_.-]+/g, "-").replace(/^-+|-+$/g, "") || "automd";
}

function read(file) {
  return fs.readFileSync(path.join(root, file), "utf8");
}

function parseScheduler(value) {
  const normalized = (value || "auto").trim().toLowerCase();
  if (["auto", "ssh", "slurm", "pbs", "lsf"].includes(normalized)) return normalized;
  throw new Error(`AUTOMD_REMOTE_SCHEDULER must be auto, ssh, slurm, pbs, or lsf; got ${value}`);
}

function run(command, args, options = {}) {
  const timeoutMs = options.timeoutMs ?? 120_000;
  return new Promise((resolve) => {
    const child = spawn(command, args, {
      cwd: options.cwd ?? root,
      env: options.env ?? env,
      stdio: ["pipe", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    const timer = setTimeout(() => {
      child.kill("SIGTERM");
      setTimeout(() => child.kill("SIGKILL"), 2_000).unref();
    }, timeoutMs);
    child.stdout.on("data", (chunk) => {
      stdout += chunk.toString();
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk.toString();
    });
    child.on("error", (error) => {
      clearTimeout(timer);
      resolve({ code: null, stdout, stderr: `${stderr}${error.message}`, ok: false });
    });
    child.on("close", (code) => {
      clearTimeout(timer);
      resolve({ code, stdout, stderr, ok: code === 0 });
    });
    if (options.stdin) child.stdin.end(options.stdin);
    else child.stdin.end();
  });
}

async function commandExists(command) {
  const result = await run("sh", ["-lc", `command -v ${shellQuote(command)} >/dev/null 2>&1`], { timeoutMs: 10_000 });
  return result.ok;
}

function remoteHelperBashScript() {
  const source = read("src-tauri/src/remote_helper.rs");
  const marker = "pub fn bash_helper_script() -> &'static str {";
  const start = source.indexOf(marker);
  if (start < 0) throw new Error("remote_helper.rs does not contain bash_helper_script()");
  const rawStart = source.indexOf('r#"#!/usr/bin/env bash', start);
  const rawEnd = source.indexOf('\n"#', rawStart);
  if (rawStart < 0 || rawEnd < 0) throw new Error("could not extract bash helper raw string");
  return source.slice(rawStart + 3, rawEnd);
}

class RemoteSession {
  constructor(config) {
    this.config = config;
    this.target = config.user ? `${config.user}@${config.host}` : config.host;
    const socketHash = crypto
      .createHash("sha1")
      .update(`${this.target}:${config.port}`)
      .digest("hex")
      .slice(0, 16);
    this.controlPath = path.join("/tmp", `automd-cm-${socketHash}`);
  }

  baseSshArgs({ batch = true } = {}) {
    const args = [
      "-p",
      String(this.config.port),
      "-o",
      "ConnectTimeout=10",
      "-o",
      "StrictHostKeyChecking=accept-new",
      "-o",
      "ServerAliveInterval=15",
      "-o",
      "ServerAliveCountMax=3",
    ];
    if (this.config.identityFile) {
      args.push("-i", this.config.identityFile, "-o", "IdentitiesOnly=yes");
    }
    if (this.config.auth === "password") {
      args.push("-o", "ControlMaster=auto", "-o", `ControlPath=${this.controlPath}`);
    }
    if (batch) args.push("-o", "BatchMode=yes");
    return args;
  }

  async ensurePasswordMaster() {
    if (this.config.auth !== "password") return;
    if (!this.config.password) throw new Error("AUTOMD_REMOTE_PASSWORD is required for password auth");
    if (!(await commandExists("expect"))) {
      throw new Error("password acceptance requires the system 'expect' command; use key/agent auth or install expect");
    }
    const checkArgs = [...this.baseSshArgs({ batch: false }), "-O", "check", this.target];
    const check = await run("ssh", checkArgs, { timeoutMs: 10_000 });
    if (check.ok) return;

    const args = [
      ...this.baseSshArgs({ batch: false }),
      "-o",
      "ControlMaster=yes",
      "-o",
      "ControlPersist=10m",
      "-o",
      "NumberOfPasswordPrompts=1",
      "-o",
      "PreferredAuthentications=password,keyboard-interactive",
      "-N",
      "-f",
      this.target,
    ];
    const expectScript = `
set timeout 30
spawn ssh ${args.map(tclBrace).join(" ")}
expect {
  "*yes/no*" { send -- "yes\\r"; exp_continue }
  "*assword:*" { send -- "$env(AUTOMD_REMOTE_PASSWORD)\\r"; exp_continue }
  eof
  timeout { exit 124 }
}
catch wait result
exit [lindex $result 3]
`;
    const result = await run("expect", ["-c", expectScript], {
      timeoutMs: 45_000,
      env: { ...env, AUTOMD_REMOTE_PASSWORD: this.config.password },
    });
    if (!result.ok) {
      throw new Error(`password ControlMaster failed: ${result.stderr || result.stdout}`);
    }
    const alive = await run("ssh", checkArgs, { timeoutMs: 10_000 });
    if (!alive.ok) {
      throw new Error(`password ControlMaster was created but is not reachable: ${alive.stderr || alive.stdout}`);
    }
  }

  async ssh(remoteCommand, options = {}) {
    await this.ensurePasswordMaster();
    const args = [...this.baseSshArgs({ batch: true }), this.target, remoteCommand];
    return run("ssh", args, { stdin: options.stdin, timeoutMs: options.timeoutMs ?? this.config.timeoutMs });
  }

  async rsync(src, dst, extraArgs = []) {
    await this.ensurePasswordMaster();
    const sshArgs = this.baseSshArgs({ batch: true }).join(" ");
    const args = [
      "-az",
      "--partial",
      "--stats",
      "-e",
      `ssh ${sshArgs}`,
      ...extraArgs,
      src,
      dst,
    ];
    return run("rsync", args, { timeoutMs: this.config.timeoutMs * 2 });
  }

  async closeMaster() {
    if (this.config.auth !== "password") return;
    await run("ssh", ["-o", `ControlPath=${this.controlPath}`, "-O", "exit", this.target], { timeoutMs: 10_000 });
  }
}

function transferredFiles(output) {
  const match = output.match(/Number of (?:regular )?files transferred:\s*([0-9,]+)/i);
  return match ? Number(match[1].replace(/,/g, "")) : 0;
}

async function waitFor(predicate, { timeoutMs, intervalMs = 2_000, label }) {
  const deadline = Date.now() + timeoutMs;
  let last = null;
  while (Date.now() < deadline) {
    last = await predicate();
    if (last.ok) return last;
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }
  throw new Error(`${label} timed out; last result: ${last ? JSON.stringify(last) : "none"}`);
}

function writeAcceptanceProject(localDir, scheduler) {
  fs.mkdirSync(path.join(localDir, "inputs"), { recursive: true });
  fs.mkdirSync(path.join(localDir, "remote"), { recursive: true });
  fs.mkdirSync(path.join(localDir, "runs", "old"), { recursive: true });
  fs.mkdirSync(path.join(localDir, "analysis"), { recursive: true });
  fs.mkdirSync(path.join(localDir, "trajectories"), { recursive: true });
  fs.writeFileSync(path.join(localDir, "inputs", "system.pdb"), "ATOM      1  N   GLY A   1       0.000   0.000   0.000  1.00  0.00           N\nEND\n");
  fs.writeFileSync(path.join(localDir, "runs", "old", "old.log"), "old run should not upload\n");
  fs.writeFileSync(path.join(localDir, "analysis", "old.csv"), "old analysis should not upload\n");
  fs.writeFileSync(path.join(localDir, "trajectories", "old.xtc"), "old trajectory should not upload\n");
  fs.writeFileSync(path.join(localDir, "remote", "run-success.sh"), `#!/usr/bin/env bash
set -euo pipefail
mkdir -p runs/mock analysis reports trajectories checkpoints logs
echo "AutoMD remote ${scheduler.toUpperCase()} job started at $(date -Is)"
echo "step 100 of 100"
echo "Performance: 12.500 ns/day"
echo "AutoMD acceptance result" > runs/mock/result.log
echo "time,rmsd" > analysis/result.csv
echo "0,0.0" >> analysis/result.csv
echo "# AutoMD acceptance report" > reports/report.md
echo "trajectory" > trajectories/mock.xtc
echo "checkpoint" > checkpoints/mock.cpt
echo "AutoMD remote ${scheduler.toUpperCase()} job finished at $(date -Is)"
`);
  fs.writeFileSync(path.join(localDir, "remote", "run-long.sh"), `#!/usr/bin/env bash
set -euo pipefail
mkdir -p logs
echo "AutoMD long job started"
sleep 120
echo "AutoMD long job should have been cancelled"
`);
  fs.writeFileSync(path.join(localDir, "remote", "run-fail.sh"), `#!/usr/bin/env bash
set -euo pipefail
mkdir -p logs
echo "Fatal error: mock failure for AutoMD remote acceptance" >&2
exit 42
`);
  for (const file of ["run-success.sh", "run-long.sh", "run-fail.sh"]) {
    fs.chmodSync(path.join(localDir, "remote", file), 0o755);
  }
}

function schedulerFiles(scheduler, remoteBase) {
  if (scheduler === "ssh") {
    return {
      success: "remote/run-success.sh",
      long: "remote/run-long.sh",
      submitSuccess: `cd ${shellQuote(remoteBase)} && mkdir -p logs && (nohup bash remote/run-success.sh > logs/automd-ssh.out 2> logs/automd-ssh.err < /dev/null & echo $!)`,
      submitLong: `cd ${shellQuote(remoteBase)} && mkdir -p logs && (nohup bash remote/run-long.sh > logs/automd-ssh.out 2> logs/automd-ssh.err < /dev/null & echo $!)`,
      status: (jobId) => `ps -p ${shellQuote(jobId)} -o pid=,stat=,etime=,cmd= 2>/dev/null || echo not-running`,
      cancel: (jobId) => `kill ${shellQuote(jobId)}`,
    };
  }
  if (scheduler === "slurm") {
    return {
      script: "remote/submit.slurm",
      scriptLong: "remote/submit-long.slurm",
      contents: `#!/usr/bin/env bash
#SBATCH --job-name=automd-acceptance
#SBATCH --output=logs/slurm-%j.out
#SBATCH --error=logs/slurm-%j.err
#SBATCH --time=00:05:00
#SBATCH --nodes=1
#SBATCH --ntasks=1
cd ${shellQuote(remoteBase)}
bash remote/run-success.sh
`,
      contentsLong: `#!/usr/bin/env bash
#SBATCH --job-name=automd-acceptance-long
#SBATCH --output=logs/slurm-long-%j.out
#SBATCH --error=logs/slurm-long-%j.err
#SBATCH --time=00:10:00
#SBATCH --nodes=1
#SBATCH --ntasks=1
cd ${shellQuote(remoteBase)}
bash remote/run-long.sh
`,
      submitSuccess: `cd ${shellQuote(remoteBase)} && mkdir -p logs && sbatch --parsable remote/submit.slurm`,
      submitLong: `cd ${shellQuote(remoteBase)} && mkdir -p logs && sbatch --parsable remote/submit-long.slurm`,
      status: (jobId) => `squeue -j ${shellQuote(jobId)} -h -o '%i %T %M %R' || sacct -j ${shellQuote(jobId)} --format=JobID,State,Elapsed -n 2>/dev/null || true`,
      cancel: (jobId) => `scancel ${shellQuote(jobId)}`,
    };
  }
  if (scheduler === "pbs") {
    return {
      script: "remote/submit.pbs",
      scriptLong: "remote/submit-long.pbs",
      contents: `#!/usr/bin/env bash
#PBS -N automd-acceptance
#PBS -l select=1:ncpus=1
#PBS -l walltime=00:05:00
#PBS -o logs/pbs-$PBS_JOBID.out
#PBS -e logs/pbs-$PBS_JOBID.err
cd ${shellQuote(remoteBase)}
bash remote/run-success.sh
`,
      contentsLong: `#!/usr/bin/env bash
#PBS -N automd-acceptance-long
#PBS -l select=1:ncpus=1
#PBS -l walltime=00:10:00
#PBS -o logs/pbs-long-$PBS_JOBID.out
#PBS -e logs/pbs-long-$PBS_JOBID.err
cd ${shellQuote(remoteBase)}
bash remote/run-long.sh
`,
      submitSuccess: `cd ${shellQuote(remoteBase)} && mkdir -p logs && qsub remote/submit.pbs`,
      submitLong: `cd ${shellQuote(remoteBase)} && mkdir -p logs && qsub remote/submit-long.pbs`,
      status: (jobId) => `qstat ${shellQuote(jobId)} 2>/dev/null || true`,
      cancel: (jobId) => `qdel ${shellQuote(jobId)}`,
    };
  }
  return {
    script: "remote/submit.lsf",
    scriptLong: "remote/submit-long.lsf",
    contents: `#!/usr/bin/env bash
#BSUB -J automd-acceptance
#BSUB -n 1
#BSUB -W 00:05
#BSUB -o logs/lsf-%J.out
#BSUB -e logs/lsf-%J.err
cd ${shellQuote(remoteBase)}
bash remote/run-success.sh
`,
    contentsLong: `#!/usr/bin/env bash
#BSUB -J automd-acceptance-long
#BSUB -n 1
#BSUB -W 00:10
#BSUB -o logs/lsf-long-%J.out
#BSUB -e logs/lsf-long-%J.err
cd ${shellQuote(remoteBase)}
bash remote/run-long.sh
`,
    submitSuccess: `cd ${shellQuote(remoteBase)} && mkdir -p logs && bsub < remote/submit.lsf`,
    submitLong: `cd ${shellQuote(remoteBase)} && mkdir -p logs && bsub < remote/submit-long.lsf`,
    status: (jobId) => `bjobs ${shellQuote(jobId)} 2>/dev/null || bhist ${shellQuote(jobId)} 2>/dev/null || true`,
    cancel: (jobId) => `bkill ${shellQuote(jobId)}`,
  };
}

function parseJobId(scheduler, output) {
  const text = output.trim();
  if (scheduler === "ssh") return text.split(/\s+/).find(Boolean) ?? "";
  if (scheduler === "slurm") return text.split(/[;\s]+/).find((part) => /^\d+/.test(part)) ?? "";
  if (scheduler === "pbs") return text.split(/\s+/).find(Boolean) ?? "";
  const match = text.match(/Job\s+<([^>]+)>/i);
  return match ? match[1] : text.split(/\s+/).find(Boolean) ?? "";
}

async function detectScheduler(session) {
  const result = await session.ssh("(command -v sbatch || command -v qsub || command -v bsub) 2>/dev/null || true");
  const text = result.stdout.toLowerCase();
  if (text.includes("sbatch")) return "slurm";
  if (text.includes("qsub")) return "pbs";
  if (text.includes("bsub")) return "lsf";
  return "ssh";
}

async function main() {
  if (!env.AUTOMD_REMOTE_HOST) {
    info("skipped: set AUTOMD_REMOTE_HOST to run live SSH/HPC acceptance");
    return;
  }
  const schedulerRequested = parseScheduler(env.AUTOMD_REMOTE_SCHEDULER);
  const config = {
    host: env.AUTOMD_REMOTE_HOST,
    port: Number(env.AUTOMD_REMOTE_PORT || 22),
    user: env.AUTOMD_REMOTE_USER || "",
    auth: env.AUTOMD_REMOTE_AUTH || (env.AUTOMD_REMOTE_PASSWORD ? "password" : "agent"),
    password: env.AUTOMD_REMOTE_PASSWORD || "",
    identityFile: env.AUTOMD_REMOTE_IDENTITY_FILE || "",
    timeoutMs: Number(env.AUTOMD_ACCEPTANCE_TIMEOUT_SECONDS || 180) * 1000,
  };
  const timestamp = `${startedAt.toISOString().replace(/[-:.TZ]/g, "").slice(0, 14)}-${process.pid}`;
  const workdirRoot = env.AUTOMD_REMOTE_WORKDIR || "/tmp/automd-acceptance";
  const remoteBase = `${workdirRoot.replace(/\/+$/, "")}/run-${timestamp}`;
  const localDir = path.join(os.tmpdir(), `automd-remote-acceptance-${timestamp}`);
  const fetchedDir = path.join(os.tmpdir(), `automd-remote-acceptance-fetched-${timestamp}`);
  fs.mkdirSync(localDir, { recursive: true });
  fs.mkdirSync(fetchedDir, { recursive: true });

  const session = new RemoteSession(config);
  info(`target ${config.user ? `${config.user}@` : ""}${config.host}:${config.port}`);

  const probe = await session.ssh("echo automd-ok; uname -srm; echo ---AUTOMD---; hostname; echo ---AUTOMD---; (command -v sbatch || command -v qsub || command -v bsub) 2>/dev/null || true");
  assertOrThrow(probe.ok && probe.stdout.includes("automd-ok"), `SSH connection probe failed: ${probe.stderr || probe.stdout}`);
  info(`probe: ${probe.stdout.trim().split("\n").slice(0, 5).join(" | ")}`);

  const scheduler = schedulerRequested === "auto" ? await detectScheduler(session) : schedulerRequested;
  info(`scheduler mode: ${scheduler}`);
  if (scheduler !== "ssh") {
    const command = scheduler === "slurm" ? "sbatch" : scheduler === "pbs" ? "qsub" : "bsub";
    const found = await session.ssh(`command -v ${command} >/dev/null 2>&1`);
    assertOrThrow(found.ok, `${scheduler} scheduler command not found on target`);
  }

  await session.ssh(`mkdir -p ${shellQuote(remoteBase)}`);
  const helperScript = remoteHelperBashScript();
  const helperDir = `${remoteBase}/.automd/helper/0.1.0`;
  const helperPath = `${helperDir}/automd-helper.sh`;
  const helperInstall = await session.ssh(`mkdir -p ${shellQuote(helperDir)} && cat > ${shellQuote(helperPath)} && chmod +x ${shellQuote(helperPath)}`, {
    stdin: helperScript,
  });
  assertOrThrow(helperInstall.ok, `helper upload failed: ${helperInstall.stderr || helperInstall.stdout}`);
  const helperProbe = await session.ssh(`${shellQuote(helperPath)} probe`);
  assertOrThrow(helperProbe.ok, `helper probe failed: ${helperProbe.stderr || helperProbe.stdout}`);
  const helperJson = JSON.parse(helperProbe.stdout.trim().split("\n").pop());
  check(helperJson.helperVersion === "0.1.0", "helper version mismatch");
  info(`helper: ${helperJson.platform}/${helperJson.arch} cpu=${helperJson.hardware?.cpuCount ?? "unknown"}`);

  const engineScan = await session.ssh(`${shellQuote(helperPath)} scan-engines gmx gmx_mpi`);
  check(engineScan.ok, `engine scan command failed: ${engineScan.stderr || engineScan.stdout}`);
  info(`GROMACS scan: ${engineScan.stdout.trim()}`);
  if ((env.AUTOMD_REMOTE_INSTALL_ENGINE || "").toLowerCase() === "gromacs") {
    const install = await session.ssh(`${shellQuote(helperPath)} install-engine gromacs gromacs gmx gmx_mpi`, {
      timeoutMs: config.timeoutMs * 4,
    });
    assertOrThrow(install.ok, `GROMACS install failed: ${install.stderr || install.stdout}`);
    info(`GROMACS install: ${install.stdout.trim().split("\n").slice(-1)[0]}`);
  }

  writeAcceptanceProject(localDir, scheduler);
  const schedulerSpec = schedulerFiles(scheduler, remoteBase);
  if (schedulerSpec.script) {
    fs.writeFileSync(path.join(localDir, schedulerSpec.script), schedulerSpec.contents);
  }
  if (schedulerSpec.scriptLong) {
    fs.writeFileSync(path.join(localDir, schedulerSpec.scriptLong), schedulerSpec.contentsLong);
  }
  const upload = await session.rsync(`${localDir}/`, `${session.target}:${remoteBase}/`, [
    "--exclude=runs/",
    "--exclude=trajectories/",
    "--exclude=analysis/",
    "--exclude=reports/",
    "--exclude=checkpoints/",
  ]);
  assertOrThrow(upload.ok, `rsync upload failed: ${upload.stderr || upload.stdout}`);
  assertOrThrow(transferredFiles(upload.stdout + upload.stderr) >= 3, "rsync upload did not report transferred files");
  const uploadedOld = await session.ssh(`test -e ${shellQuote(`${remoteBase}/runs/old/old.log`)} && echo old-uploaded || echo old-not-uploaded`);
  check(uploadedOld.stdout.includes("old-not-uploaded"), "upload filters transferred old result directories");

  if (schedulerSpec.submitLong) {
    const longSubmit = await session.ssh(schedulerSpec.submitLong);
    const longJobId = parseJobId(scheduler, longSubmit.stdout);
    check(longSubmit.ok && longJobId, `${scheduler} long submit did not return a job id/PID: ${longSubmit.stdout}${longSubmit.stderr}`);
    await session.closeMaster();
    info(`${scheduler} connection reset before cancellation check`);
    if (scheduler === "ssh") {
      const running = await session.ssh(schedulerSpec.status(longJobId));
      check(running.stdout.includes(longJobId), "SSH long job was not running after detached submit");
    }
    const cancel = await session.ssh(schedulerSpec.cancel(longJobId));
    check(cancel.ok, `${scheduler} cancel failed: ${cancel.stderr || cancel.stdout}`);
    if (scheduler === "ssh") {
      await waitFor(async () => {
        const status = await session.ssh(schedulerSpec.status(longJobId));
        return { ok: status.stdout.includes("not-running"), status: status.stdout.trim() };
      }, { timeoutMs: 30_000, label: "SSH long job cancellation" });
    }
    info(`${scheduler} cancellation ok for ${longJobId}`);
  }

  const submit = await session.ssh(schedulerSpec.submitSuccess);
  const jobId = parseJobId(scheduler, submit.stdout);
  check(submit.ok && jobId, `${scheduler} submit did not return a job id/PID: ${submit.stdout}${submit.stderr}`);
  info(`submitted ${scheduler} job ${jobId}`);

  await waitFor(async () => {
    const tail = await session.ssh(`cd ${shellQuote(remoteBase)} && tail -n 100 logs/*.out logs/*.err runs/*/*.log remote/*.log 2>/dev/null || true`);
    const completed = tail.stdout.includes("AutoMD remote") && tail.stdout.includes("finished");
    if (scheduler !== "ssh" && !completed) await session.ssh(schedulerSpec.status(jobId));
    return { ok: completed, tail: tail.stdout.trim().slice(-300) };
  }, { timeoutMs: config.timeoutMs, label: `${scheduler} success job completion` });

  const download = await session.rsync(`${session.target}:${remoteBase}/`, `${fetchedDir}/`, [
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
  ]);
  assertOrThrow(download.ok, `rsync download failed: ${download.stderr || download.stdout}`);
  check(fs.existsSync(path.join(fetchedDir, "runs/mock/result.log")), "fetched results missing runs/mock/result.log");
  check(fs.existsSync(path.join(fetchedDir, "analysis/result.csv")), "fetched results missing analysis/result.csv");
  check(!fs.existsSync(path.join(fetchedDir, "inputs/system.pdb")), "fetch filters downloaded inputs unexpectedly");
  info(`fetch ok: ${transferredFiles(download.stdout + download.stderr)} files reported`);

  await session.closeMaster();
  const reconnect = await session.ssh(`${shellQuote(helperPath)} probe`);
  check(reconnect.ok && reconnect.stdout.includes("helperVersion"), "drop/reconnect helper probe failed");
  info("drop/reconnect probe ok");

  if (!env.AUTOMD_REMOTE_KEEP) {
    await session.ssh(`rm -rf ${shellQuote(remoteBase)}`);
    fs.rmSync(localDir, { recursive: true, force: true });
    fs.rmSync(fetchedDir, { recursive: true, force: true });
  } else {
    info(`kept local evidence: ${localDir}`);
    info(`kept fetched evidence: ${fetchedDir}`);
    info(`kept remote evidence: ${remoteBase}`);
  }
}

try {
  await main();
} catch (error) {
  fail(error instanceof Error ? error.message : String(error));
}

if (failures.length > 0) {
  console.error("\nAutoMD remote acceptance failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

info("passed");
