import type { DetectionStatus } from "../types";
import { statusText } from "../lib/labels";

export function formatBytes(value?: number | null) {
  if (value == null || !Number.isFinite(value)) {
    return "未知";
  }
  const units = ["B", "KB", "MB", "GB", "TB", "PB"];
  let normalized = value;
  let unitIndex = 0;
  while (normalized >= 1024 && unitIndex < units.length - 1) {
    normalized /= 1024;
    unitIndex += 1;
  }
  const precision = unitIndex === 0 ? 0 : normalized >= 100 ? 0 : 1;
  return `${normalized.toFixed(precision)} ${units[unitIndex]}`;
}


export function formatNumber(value: number) {
  if (!Number.isFinite(value)) {
    return "n/a";
  }
  if (value === 0) {
    return "0";
  }
  if (Math.abs(value) >= 1000 || Math.abs(value) < 0.01) {
    return value.toExponential(2);
  }
  return value.toFixed(3).replace(/\.?0+$/, "");
}


export function StatusPill({ status }: { status: DetectionStatus }) {
  return <span className={`status-pill ${status}`}>{statusText[status]}</span>;
}


export function EmptyState({ title, text }: { title: string; text: string }) {
  return (
    <div className="empty-state">
      <strong>{title}</strong>
      <p>{text}</p>
    </div>
  );
}


export function CodeBlock({ value }: { value: string }) {
  return <pre className="code-block">{value}</pre>;
}


