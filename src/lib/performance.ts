import type { RuntimeDiagnostics, SimulationPlan } from "../types";

export type PerformancePreferences = {
  cpuThreads: number;
  gpuDeviceId: string;
  gpuCount: number;
  memoryLimitGb: number;
  diskId: string;
};

export const PERFORMANCE_PREF_KEY = "automd-performance-preferences";

export function loadPerformancePreferences(): PerformancePreferences {
  if (typeof window === "undefined") {
    return { cpuThreads: 0, gpuDeviceId: "auto", gpuCount: 1, memoryLimitGb: 0, diskId: "auto" };
  }
  try {
    const parsed = JSON.parse(
      window.localStorage.getItem(PERFORMANCE_PREF_KEY) ?? "{}"
    ) as Partial<PerformancePreferences>;
    return {
      cpuThreads: Number(parsed.cpuThreads) || 0,
      gpuDeviceId: parsed.gpuDeviceId || "auto",
      gpuCount: Number.isFinite(Number(parsed.gpuCount)) ? Math.max(0, Number(parsed.gpuCount)) : 1,
      memoryLimitGb: Number(parsed.memoryLimitGb) || 0,
      diskId: parsed.diskId || "auto"
    };
  } catch {
    return { cpuThreads: 0, gpuDeviceId: "auto", gpuCount: 1, memoryLimitGb: 0, diskId: "auto" };
  }
}

export function savePerformancePreferences(preferences: PerformancePreferences) {
  if (typeof window !== "undefined") {
    window.localStorage.setItem(PERFORMANCE_PREF_KEY, JSON.stringify(preferences));
  }
}

export function clampNumber(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

export function suggestedCpuThreads(diagnostics: RuntimeDiagnostics | null) {
  const logical = diagnostics?.hardware.cpu.logicalCores || 1;
  return clampNumber(Math.min(8, Math.max(1, logical - 1)), 1, logical);
}

export function effectiveCpuThreads(
  preferences: PerformancePreferences,
  diagnostics: RuntimeDiagnostics | null
) {
  const logical = diagnostics?.hardware.cpu.logicalCores || Math.max(1, preferences.cpuThreads || 1);
  return clampNumber(preferences.cpuThreads || suggestedCpuThreads(diagnostics), 1, logical);
}

export function effectiveGpuCount(
  preferences: PerformancePreferences,
  diagnostics: RuntimeDiagnostics | null
) {
  if (preferences.gpuDeviceId === "cpu") return 0;
  const availableGpus = diagnostics?.hardware.gpus.filter((gpu) => gpu.backend).length ?? 0;
  if (availableGpus <= 0) return 0;
  return clampNumber(preferences.gpuCount || 1, 0, availableGpus);
}

export function applyPerformanceToPlan(
  plan: SimulationPlan,
  preferences: PerformancePreferences,
  diagnostics: RuntimeDiagnostics | null
): SimulationPlan {
  return {
    ...plan,
    resources: {
      ...plan.resources,
      cpuThreads: effectiveCpuThreads(preferences, diagnostics),
      gpuCount: effectiveGpuCount(preferences, diagnostics)
    }
  };
}
