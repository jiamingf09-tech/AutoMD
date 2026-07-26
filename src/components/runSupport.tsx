import type {
  AnalysisParseResult,
  RunArtifact,
  TrajectoryAnalysisPackage,
  TrajectoryChunk,
  TrajectoryIndex
} from "../types";
import { CodeBlock, EmptyState, formatBytes, formatNumber } from "./ui";

export function isNativeEditablePath(path: string) {
  return /^(generated|runs|remote|build-recipes|analysis|reports)\//.test(path)
    && /\.(mdp|mdin|conf|cfg|inp|in|key|txt|json|ya?ml|py|sh|slurm|pbs|lsf|md)$/i.test(path);
}


export function ArtifactTable({ artifacts }: { artifacts: RunArtifact[] }) {
  return (
    <div className="artifact-table">
      <div className="artifact-head">
        <span>类型</span>
        <span>路径</span>
        <span>大小</span>
        <span>摘要</span>
      </div>
      {artifacts.map((artifact) => (
        <div className="artifact-row" key={`${artifact.kind}-${artifact.path}`}>
          <span>{artifact.kind}</span>
              <span className="mono artifact-path" title={artifact.path}>{artifact.path}</span>
          <span>{formatBytes(artifact.sizeBytes)}</span>
          <span>{artifact.summary ?? " "}</span>
        </div>
      ))}
    </div>
  );
}


export function TrajectoryIndexPanel({
  artifacts,
  trajectoryIndex,
  trajectoryChunk,
  indexTrajectory,
  previewTrajectoryFrame
}: {
  artifacts: RunArtifact[];
  trajectoryIndex: TrajectoryIndex | null;
  trajectoryChunk: TrajectoryChunk | null;
  indexTrajectory: (trajectoryPath?: string) => void;
  previewTrajectoryFrame: (frameIndex: number) => void;
}) {
  const trajectories = artifacts.filter((artifact) => artifact.kind === "trajectory");
  const indexedPath = trajectoryIndex?.trajectoryPath ? normalizeArtifactPath(trajectoryIndex.trajectoryPath) : null;

  return (
    <div className="trajectory-panel">
      <div className="panel-title-row">
        <h3>轨迹索引与分块预览</h3>
        <button type="button" onClick={() => indexTrajectory()} disabled={!trajectories.length}>
          索引首个轨迹
        </button>
      </div>
      {trajectories.length ? (
        <div className="trajectory-layout">
          <div className="trajectory-list">
            {trajectories.map((artifact) => {
              const isIndexed = Boolean(indexedPath && artifactPathMatches(indexedPath, artifact.path));
              const status = isIndexed
                ? trajectoryIndexSummary(trajectoryIndex)
                : (artifact.summary ?? "等待索引");
              return (
                <button type="button" key={artifact.path} onClick={() => indexTrajectory(artifact.path)}>
                  <span className="mono">{artifact.path}</span>
                  <small>{formatBytes(artifact.sizeBytes)} · {status}</small>
                </button>
              );
            })}
          </div>
          <div className="trajectory-summary">
            {trajectoryIndex ? (
              <>
                <dl className="definition-list">
                  <div><dt>格式</dt><dd>{trajectoryIndex.format}</dd></div>
                  <div><dt>策略</dt><dd>{trajectoryIndex.strategy === "textOffsets" ? "文本 offset 索引" : "metadata-only"}</dd></div>
                  <div><dt>帧数</dt><dd>{trajectoryIndex.frameCount ?? "未解码"}</dd></div>
                  <div><dt>索引</dt><dd className="mono">{trajectoryIndex.indexPath ?? "未写入"}</dd></div>
                </dl>
                {trajectoryIndex.warnings.length ? (
                  <div className="warning-stack">
                    {trajectoryIndex.warnings.map((warning) => <p key={warning}>{warning}</p>)}
                  </div>
                ) : null}
                {trajectoryIndex.sampledFrames.length ? (
                  <div className="frame-chip-row">
                    {trajectoryIndex.sampledFrames.slice(0, 24).map((frame) => (
                      <button type="button" key={frame.frameIndex} onClick={() => previewTrajectoryFrame(frame.frameIndex)}>
                        #{frame.frameIndex}
                        <small>{frame.atomCount ? `${frame.atomCount} atoms` : frame.label}</small>
                      </button>
                    ))}
                  </div>
                ) : (
                  <EmptyState
                    title="暂无可预览帧"
                    text="二进制 XTC/TRR/DCD/GSD 当前只登记 metadata，帧解码会交给后续 MDAnalysis/Mol* 后台路径。"
                  />
                )}
              </>
            ) : (
              <EmptyState title="等待索引" text="选择轨迹后会生成 frame offset manifest，并按需读取小块帧内容。" />
            )}
          </div>
          <div className="trajectory-preview">
            {trajectoryChunk?.frames.length ? (
              <>
                <div className="analysis-card-head">
                  <div>
                    <strong>{trajectoryChunk.frames[0].label}</strong>
                    <span className="mono">{trajectoryChunk.trajectoryPath}</span>
                  </div>
                  <small>{trajectoryChunk.truncated ? "已截断" : "完整 chunk"}</small>
                </div>
                <CodeBlock value={trajectoryChunk.frames.map((frame) => frame.contents).join("\n")} />
                {trajectoryChunk.warnings.length ? (
                  <div className="warning-stack">
                    {trajectoryChunk.warnings.map((warning) => <p key={warning}>{warning}</p>)}
                  </div>
                ) : null}
              </>
            ) : (
              <EmptyState title="暂无 chunk" text="文本轨迹可以读取指定帧，避免一次性把大文件送进 UI。" />
            )}
          </div>
        </div>
      ) : (
        <EmptyState title="暂无轨迹 artifact" text="产生 trajectories/*.xtc、*.dcd、*.pdb、*.xyz 或 LAMMPS dump 后，这里会建立后台索引。" />
      )}
    </div>
  );
}


export function normalizeArtifactPath(path: string) {
  return path.replace(/\\/g, "/").replace(/^\.\//, "").toLowerCase();
}


export function artifactPathMatches(left: string, right: string) {
  const normalizedLeft = normalizeArtifactPath(left);
  const normalizedRight = normalizeArtifactPath(right);
  return (
    normalizedLeft === normalizedRight ||
    normalizedLeft.endsWith(`/${normalizedRight}`) ||
    normalizedRight.endsWith(`/${normalizedLeft}`)
  );
}


export function trajectoryIndexSummary(index: TrajectoryIndex | null) {
  if (!index) return "等待索引";
  const strategy = index.strategy === "textOffsets" ? "文本索引" : index.strategy === "metadataOnly" ? "metadata-only" : "不支持预览";
  const frameText = typeof index.frameCount === "number" ? `${index.frameCount} frames` : "未解码帧数";
  return `已索引 · ${index.format} · ${strategy} · ${frameText}`;
}


export function TrajectoryAnalysisPackagePanel({
  analysisPackage,
  generateTrajectoryAnalysisPackage
}: {
  analysisPackage: TrajectoryAnalysisPackage | null;
  generateTrajectoryAnalysisPackage: () => void;
}) {
  return (
    <div className="analysis-package-panel">
      <div className="panel-title-row">
        <h3>MDAnalysis 分析侧车</h3>
        <button type="button" onClick={generateTrajectoryAnalysisPackage}>
          生成分析包
        </button>
      </div>
      {analysisPackage ? (
        <div className="analysis-package-grid">
          <div>
            <dl className="definition-list">
              <div><dt>目录</dt><dd className="mono">{analysisPackage.generatedDirectory}</dd></div>
              <div><dt>文件</dt><dd>{analysisPackage.files.length}</dd></div>
              <div><dt>命令</dt><dd>{analysisPackage.commands.length}</dd></div>
              <div><dt>写入磁盘</dt><dd>{analysisPackage.files.some((file) => file.written) ? "是" : "否"}</dd></div>
            </dl>
            {analysisPackage.warnings.length ? (
              <div className="warning-stack">
                {analysisPackage.warnings.map((warning) => <p key={warning}>{warning}</p>)}
              </div>
            ) : null}
          </div>
          <div>
            <h4>输出</h4>
            <div className="chip-row">
              {analysisPackage.expectedOutputs.map((output) => (
                <span key={output}>{output}</span>
              ))}
            </div>
          </div>
          <div className="command-list">
            {analysisPackage.commands.map((command) => (
              <details key={command.stageId}>
                <summary>{command.label}</summary>
                <CodeBlock value={command.command} />
              </details>
            ))}
          </div>
        </div>
      ) : (
        <EmptyState
          title="等待生成"
          text="生成后会写入 generated/analysis/run_mdanalysis.py，并约定输出 RMSD、RMSF、Rg、氢键和接触计数 CSV。"
        />
      )}
    </div>
  );
}


export function AnalysisChartGrid({ analysisResult }: { analysisResult: AnalysisParseResult | null }) {
  if (!analysisResult?.series.length) {
    return (
      <EmptyState
        title="暂无分析曲线"
        text="任务产生 analysis/*.xvg 或 CSV 后，AutoMD 会解析为 RMSD、Rg、能量、温度等曲线。"
      />
    );
  }

  return (
    <div className="analysis-grid">
      {analysisResult.series.map((series) => (
        <AnalysisChart series={series} key={`${series.path}-${series.label}`} />
      ))}
      {analysisResult.warnings.length ? (
        <div className="warning-stack span-all">
          {analysisResult.warnings.map((warning) => <p key={warning}>{warning}</p>)}
        </div>
      ) : null}
    </div>
  );
}


export function AnalysisChart({ series }: { series: AnalysisParseResult["series"][number] }) {
  const points = series.points;
  const xValues = points.map((point) => point.x);
  const yValues = points.map((point) => point.y);
  const minX = Math.min(...xValues);
  const maxX = Math.max(...xValues);
  const minY = Math.min(...yValues);
  const maxY = Math.max(...yValues);
  const width = 360;
  const height = 160;
  const pad = 24;
  const xSpan = maxX - minX || 1;
  const ySpan = maxY - minY || 1;
  const polyline = points
    .map((point) => {
      const x = pad + ((point.x - minX) / xSpan) * (width - pad * 2);
      const y = height - pad - ((point.y - minY) / ySpan) * (height - pad * 2);
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");

  return (
    <div className="analysis-card">
      <div className="analysis-card-head">
        <div>
          <strong>{series.label}</strong>
          <span className="mono">{series.path}</span>
        </div>
        <small>{series.points.length} points</small>
      </div>
      <svg viewBox={`0 0 ${width} ${height}`} className="analysis-chart" role="img" aria-label={series.label}>
        <line x1={pad} y1={height - pad} x2={width - pad} y2={height - pad} />
        <line x1={pad} y1={pad} x2={pad} y2={height - pad} />
        <polyline points={polyline} />
      </svg>
      <dl className="analysis-stats">
        <div><dt>{series.xLabel}</dt><dd>{formatNumber(minX)} - {formatNumber(maxX)}</dd></div>
        <div><dt>min</dt><dd>{formatNumber(series.minY ?? minY)}</dd></div>
        <div><dt>max</dt><dd>{formatNumber(series.maxY ?? maxY)}</dd></div>
        <div><dt>last</dt><dd>{formatNumber(series.lastY ?? yValues[yValues.length - 1])} {series.yLabel}</dd></div>
      </dl>
    </div>
  );
}


export function runScriptNameForEngine(engineId?: string | null) {
  switch (engineId) {
    case "openmm":
      return "run-openmm.sh";
    case "ambertools":
      return "run-ambertools.sh";
    case "namd":
      return "run-namd.sh";
    case "lammps":
      return "run-lammps.sh";
    case "cp2k":
      return "run-cp2k.sh";
    case "genesis":
      return "run-genesis.sh";
    case "hoomd":
      return "run-hoomd.sh";
    case "dl_poly":
      return "run-dl-poly.sh";
    case "tinker":
      return "run-tinker.sh";
    case "amber_pmemd":
      return "run-amber-pmemd.sh";
    case "charmm":
      return "run-charmm.sh";
    case "desmond":
      return "run-desmond.sh";
    case "acemd":
      return "run-acemd.sh";
    default:
      return "run-gromacs.sh";
  }
}


