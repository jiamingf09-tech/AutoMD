import type {
  AnalysisParseResult,
  ArtifactIndex,
  BatchExperimentPackage,
  EngineCapability,
  EngineLogReport,
  EngineRunPackage,
  FailureAnalysis,
  LocalRunMode,
  LocalTaskSnapshot,
  ProjectTextFilePayload,
  ResumePlan,
  SimulationPlan,
  SimulationTask,
  TaskRecord,
  TrajectoryAnalysisPackage,
  TrajectoryChunk,
  TrajectoryIndex,
  ValidationReport
} from "../types";
import {
  engineLabel,
  executionModeText,
  failureCategoryText,
  localRunModeText,
  severityText
} from "../lib/labels";
import {
  AnalysisChartGrid,
  ArtifactTable,
  isNativeEditablePath,
  runScriptNameForEngine,
  TrajectoryAnalysisPackagePanel,
  TrajectoryIndexPanel
} from "./runSupport";
import { CodeBlock, EmptyState, formatBytes, StatusPill } from "./ui";
import { ValidationList } from "./ValidationList";

export function RunPanel({
  plan,
  task,
  validation,
  slurmScript,
  runPackage,
  runPackageBusy,
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
  runPackageBusy: boolean;
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
  const engineName = plan ? engineLabel[plan.engineId] ?? plan.engineId : "当前引擎";
  const runScriptName = runScriptNameForEngine(plan?.engineId);
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
        <button type="button" className="primary fill" onClick={queueMockTask} disabled={runPackageBusy || !plan}>
          {runPackageBusy ? "生成中..." : "生成 run package"}
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
            <option value="real">真实本地执行：启动 {runScriptName}</option>
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
        {localSnapshot ? (
          <>
            <div className="progress-shell">
              <div className="progress-bar" style={{ width: `${localSnapshot.progressPercent}%` }} />
            </div>
            <dl className="definition-list">
              <div><dt>任务</dt><dd className="mono">{localSnapshot.id}</dd></div>
              <div><dt>状态</dt><dd>{localSnapshot.status}</dd></div>
              <div><dt>模式</dt><dd>{localRunModeText[localSnapshot.mode]}</dd></div>
              <div><dt>运行目录</dt><dd className="mono">{localSnapshot.runDirectory}</dd></div>
            </dl>
            <pre className="log-tail">{localSnapshot.logTail.join("\n")}</pre>
          </>
        ) : task ? (
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

        <h4>{engineName} Run Package</h4>
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
          <EmptyState title="尚未生成 run package" text={`点击生成后会创建 ${engineName} 的命令序列和 ${runScriptName}。`} />
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


export function FailureAnalysisCard({ analysis }: { analysis: FailureAnalysis | null }) {
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


