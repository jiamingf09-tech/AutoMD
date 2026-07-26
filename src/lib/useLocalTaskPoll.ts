import { useEffect } from "react";
import type { Dispatch, SetStateAction } from "react";
import { api } from "./api";
import type { ArtifactIndex, LocalTaskSnapshot } from "../types";

const TERMINAL = new Set(["completed", "failed", "cancelled"]);
const DEFAULT_POLL_MS = 1500;

function snapshotsEquivalent(a: LocalTaskSnapshot, b: LocalTaskSnapshot): boolean {
  return (
    a.id === b.id &&
    a.status === b.status &&
    a.progressPercent === b.progressPercent &&
    a.currentStep === b.currentStep &&
    a.logTail.length === b.logTail.length
  );
}

/**
 * Poll a running local task without thrashing React/Mol* on identical snapshots.
 * On terminal states, refreshes task records and seeds artifact index from the snapshot.
 */
export function useLocalTaskPoll(options: {
  localSnapshot: LocalTaskSnapshot | null;
  projectPath: string | null | undefined;
  setLocalSnapshot: Dispatch<SetStateAction<LocalTaskSnapshot | null>>;
  setArtifactIndex: Dispatch<SetStateAction<ArtifactIndex | null>>;
  refreshTaskRecords: () => void | Promise<void>;
  refreshAnalysis: (index: ArtifactIndex) => void | Promise<void>;
  reportError: (error: unknown) => void;
  pollMs?: number;
}) {
  const {
    localSnapshot,
    projectPath,
    setLocalSnapshot,
    setArtifactIndex,
    refreshTaskRecords,
    refreshAnalysis,
    reportError,
    pollMs = DEFAULT_POLL_MS
  } = options;

  useEffect(() => {
    if (!localSnapshot || TERMINAL.has(localSnapshot.status)) {
      if (localSnapshot) {
        void refreshTaskRecords();
      }
      if (localSnapshot?.artifacts.length) {
        const index: ArtifactIndex = {
          projectPath: projectPath ?? "",
          runDirectory: localSnapshot.runDirectory,
          artifacts: localSnapshot.artifacts,
          generatedAt: new Date().toISOString()
        };
        setArtifactIndex(index);
        void refreshAnalysis(index);
      }
      return;
    }

    const interval = window.setInterval(() => {
      void api
        .getLocalTask(localSnapshot.id)
        .then((snapshot) => {
          setLocalSnapshot((prev) => {
            if (prev && snapshotsEquivalent(prev, snapshot)) {
              return prev;
            }
            return snapshot;
          });
          if (TERMINAL.has(snapshot.status)) {
            void refreshTaskRecords();
          }
        })
        .catch(reportError);
    }, pollMs);

    return () => window.clearInterval(interval);
  }, [
    localSnapshot?.id,
    localSnapshot?.status,
    localSnapshot?.artifacts,
    localSnapshot?.runDirectory,
    projectPath,
    pollMs,
    setLocalSnapshot,
    setArtifactIndex,
    refreshTaskRecords,
    refreshAnalysis,
    reportError
  ]);
}
