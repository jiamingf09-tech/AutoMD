import { useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import {
  executionModeText,
  remoteHelperStateText
} from "../lib/labels";
import type {
  EngineTarget,
  ExecutionMode,
  RemoteAuthMethod,
  RemoteConnectionTest,
  RemoteExecutionPackage,
  RemoteHardwareReport,
  RemoteJobSnapshot,
  RemoteJobSubmission,
  RemoteProfile,
  RemoteSubmitPreflight,
  RemoteWorkflowMode,
  RemoteWorkflowStepResult,
  RuntimeDiagnostics,
  SimulationPlan,
  ToolDiagnostic
} from "../types";
import { CodeBlock, EmptyState, StatusPill } from "./ui";
import { DeleteModal } from "./DeleteModal";

function HardwareReportModal({ loading, report, error, hostLabel, onRefresh, onClose }: {
  loading: boolean;
  report: RemoteHardwareReport | null;
  error: string | null;
  hostLabel: string;
  onRefresh: () => void;
  onClose: () => void;
}) {
  useEffect(() => {
    function onKey(e: KeyboardEvent) { if (e.key === "Escape") { e.preventDefault(); onClose(); } }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  const sections = report && report.ok ? [
    { label: "CPU", section: report.cpu },
    { label: "内存", section: report.memory },
    { label: "显卡 GPU", section: report.gpu },
    { label: "硬盘", section: report.disk }
  ] : [];
  const failMessage = error ?? (report && !report.ok ? report.message : null);
  return (
    <div className="modal-overlay" role="presentation" onMouseDown={onClose}>
      <div className="modal-dialog hardware-modal" role="dialog" aria-modal="true" aria-labelledby="hw-title" onMouseDown={(e) => e.stopPropagation()}>
        <div className="hardware-modal-head">
          <div>
            <h3 id="hw-title">远程硬件性能</h3>
            <p className="muted hw-host mono">{hostLabel}</p>
          </div>
          <button type="button" className="hw-close" onClick={onClose} aria-label="关闭">✕</button>
        </div>

        <div className="hw-body">
          {loading ? (
            <div className="hw-state">
              <span className="hw-spinner" aria-hidden="true" />
              <span>正在向远程主机发送查询指令…</span>
            </div>
          ) : failMessage ? (
            <div className="hw-state hw-state-error">
              <strong>查询失败</strong>
              <span>{failMessage}</span>
            </div>
          ) : report ? (
            <>
              <div className="hw-meta">
                {report.hostname ? <span><strong>主机名</strong> {report.hostname}</span> : null}
                {report.os ? <span><strong>系统</strong> {report.os}</span> : null}
                <span><strong>查询时间</strong> {new Date(report.checkedAt).toLocaleString()}</span>
              </div>
              <div className="hw-sections">
                {sections.map(({ label, section }) => (
                  <div key={label} className="hw-section">
                    <div className="hw-section-label">{label}</div>
                    <div className="hw-summary">{section.summary}</div>
                    {section.detail.trim() ? <pre className="hw-detail mono">{section.detail.trim()}</pre> : null}
                  </div>
                ))}
              </div>
            </>
          ) : null}
        </div>

        <div className="modal-actions hw-actions">
          <button type="button" onClick={onRefresh} disabled={loading}>{loading ? "查询中…" : "重新查询"}</button>
          <button type="button" className="modal-cancel" onClick={onClose}>关闭</button>
        </div>
      </div>
    </div>
  );
}


export function defaultRemoteWorkdir(username: string): string {
  const user = username.trim();
  if (!user || user === "root") return "/root/automd";
  if (/^[A-Za-z0-9._-]+$/.test(user)) return `/home/${user}/automd`;
  return "~/automd";
}


export function isAutoManagedRemoteWorkdir(workdir: string, username: string): boolean {
  const value = workdir.trim();
  return value === "/root/automd" || value === "~/automd" || value === defaultRemoteWorkdir(username);
}


export function RemotePanel({
  plan,
  diagnostics,
  remoteProfiles,
  selectedRemoteProfileId,
  setSelectedRemoteProfileId,
  remoteProfileDraft,
  setRemoteProfileDraft,
  remotePassword,
  setRemotePassword,
  remotePasswordReady,
  remoteConnectionTest,
  remoteConnecting,
  testRemoteConnection,
  queryRemoteHardware,
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
  remotePasswordReady: boolean;
  remoteConnectionTest: RemoteConnectionTest | null;
  remoteConnecting: boolean;
  testRemoteConnection: () => void;
  queryRemoteHardware: (profile: RemoteProfile) => Promise<RemoteHardwareReport>;
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
  const [remotePortText, setRemotePortText] = useState(String(remoteProfileDraft.port));
  const passwordInputRef = useRef<HTMLInputElement | null>(null);
  const connected = remoteConnectionTest?.ok ?? false;
  const draftSaved = remoteProfiles.some((profile) => profile.id === draft.id);
  const isTemplate = draft.id.endsWith("-template");
  // One-shot hardware query modal: opened by the button, never persisted. Each
  // press re-runs the query and discards the previous result.
  const [hwOpen, setHwOpen] = useState(false);
  const [hwLoading, setHwLoading] = useState(false);
  const [hwReport, setHwReport] = useState<RemoteHardwareReport | null>(null);
  const [hwError, setHwError] = useState<string | null>(null);
  async function runHardwareQuery() {
    setHwOpen(true);
    setHwLoading(true);
    setHwReport(null);
    setHwError(null);
    try {
      const report = await queryRemoteHardware(draft);
      setHwReport(report);
    } catch (caught) {
      setHwError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setHwLoading(false);
    }
  }
  function closeHardwareModal() {
    setHwOpen(false);
    setHwReport(null);
    setHwError(null);
  }
  const helperTarget = engineTargets.find((target) => target.id === `remote:${draft.id}`) ?? null;
  const helperState = helperTarget?.status ?? "missing";
  const helperReady = helperState === "ready" || helperState === "outdated";
  const submitReady = Boolean(remotePreflight?.allOk || (remotePreflight?.canOverride && remoteAllowNoHelper));
  const jobActive = Boolean(
    remoteJobSnapshot && !["completed", "failed", "cancelled"].includes(remoteJobSnapshot.status)
  );
  useEffect(() => {
    setRemotePortText(String(remoteProfileDraft.port));
  }, [remoteProfileDraft.id, remoteProfileDraft.port]);
  // Password is bound via onChange on the input; avoid 250ms polling that forced re-renders.
  function updateRemotePortText(value: string) {
    const digits = value.replace(/[^\d]/g, "").slice(0, 5);
    setRemotePortText(digits);
    const port = Number(digits);
    if (Number.isInteger(port) && port >= 1 && port <= 65535) {
      update({ port });
    }
  }
  function normalizeRemotePortText() {
    const port = Number(remotePortText);
    if (Number.isInteger(port) && port >= 1 && port <= 65535) {
      setRemotePortText(String(port));
      update({ port });
      return;
    }
    const fallback = draft.port >= 1 && draft.port <= 65535 ? draft.port : 22;
    setRemotePortText(String(fallback));
    update({ port: fallback });
  }
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
                  setRemotePassword("");
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
                    setRemotePassword("");
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
                autoCapitalize="none"
                autoCorrect="off"
                spellCheck={false}
              />
            </label>
            <label>
              端口
              <input
                type="text"
                inputMode="numeric"
                pattern="[0-9]*"
                value={remotePortText}
                onChange={(event) => updateRemotePortText(event.target.value)}
                onFocus={(event) => event.currentTarget.select()}
                onBlur={normalizeRemotePortText}
                placeholder="22"
                autoCapitalize="none"
                autoCorrect="off"
                spellCheck={false}
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
                autoCapitalize="none"
                autoCorrect="off"
                spellCheck={false}
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
                aria-label="SSH 密码（仅本次会话保存）"
                ref={passwordInputRef}
                value={remotePassword}
                onChange={(event) => setRemotePassword(event.target.value)}
                onInput={(event) => setRemotePassword(event.currentTarget.value)}
                placeholder="实例/账号密码"
                autoComplete="off"
                autoCapitalize="none"
                autoCorrect="off"
                spellCheck={false}
              />
              <span className="hint-text">
                {remotePassword
                  ? "密码已填入；连接成功后只在当前 App 会话内复用。"
                  : remotePasswordReady
                    ? "该连接的密码已在本次会话内缓存，可继续测试、提交和拉取结果。"
                    : "未输入密码；AutoMD 不会把密码写入磁盘。"}
              </span>
            </label>
          ) : draft.authMethod === "key" ? (
            <label>
              私钥文件路径
              <div className="input-with-browse">
                <input
                  value={draft.identityFile ?? ""}
                  onChange={(event) => update({ identityFile: event.target.value || null })}
                  placeholder="~/.ssh/id_ed25519"
                  autoCapitalize="none"
                  autoCorrect="off"
                  spellCheck={false}
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
            {connected ? (
              <button
                type="button"
                onClick={runHardwareQuery}
                disabled={hwLoading}
                title="发送一次性查询指令，读取该远程主机的 CPU / 内存 / 显卡 / 硬盘，不保存结果"
              >
                {hwLoading ? "查询中…" : "查询硬件性能"}
              </button>
            ) : null}
          </div>

          {hwOpen ? (
            <HardwareReportModal
              loading={hwLoading}
              report={hwReport}
              error={hwError}
              hostLabel={`${draft.username ? `${draft.username}@` : ""}${draft.host || "远程主机"}`}
              onRefresh={runHardwareQuery}
              onClose={closeHardwareModal}
            />
          ) : null}

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
              <input
                value={draft.workdir}
                onChange={(event) => update({ workdir: event.target.value })}
                placeholder="/home/用户名/automd 或 /scratch/$USER/automd"
                autoCapitalize="none"
                autoCorrect="off"
                spellCheck={false}
              />
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
        {remoteJobSnapshot ? (
          <div className="remote-snapshot manual-parse-result">
            <h4>手动解析结果</h4>
            {remoteJobSnapshot.progressPercent != null ? (
              <div className="progress-shell">
                <div className="progress-bar" style={{ width: `${remoteJobSnapshot.progressPercent}%` }} />
              </div>
            ) : null}
            <dl className="definition-list">
              <div><dt>状态</dt><dd>{remoteJobSnapshot.status}</dd></div>
              <div><dt>队列态</dt><dd>{remoteJobSnapshot.queueState ?? "未检测"}</dd></div>
              <div><dt>步数</dt><dd>{remoteJobSnapshot.currentStep ?? "未检测"}</dd></div>
              <div><dt>性能</dt><dd>{remoteJobSnapshot.nsPerDay != null ? `${remoteJobSnapshot.nsPerDay.toFixed(3)} ns/day` : "未检测"}</dd></div>
            </dl>
            {remoteJobSnapshot.reason ? <p className="hint-text">{remoteJobSnapshot.reason}</p> : null}
          </div>
        ) : null}

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


