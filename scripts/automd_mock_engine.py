#!/usr/bin/env python3
"""Deterministic mock MD runner for AutoMD GUI and integration tests."""

from __future__ import annotations

import argparse
import json
import time
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser(description="Run a mock AutoMD simulation.")
    parser.add_argument("--plan", required=True, help="Path to automd-plan.json")
    parser.add_argument("--out", default="mock-run", help="Output directory")
    parser.add_argument("--sleep", type=float, default=0.05, help="Delay between stage updates")
    args = parser.parse_args()

    plan_path = Path(args.plan)
    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)
    project_root = plan_path.parents[2] if len(plan_path.parents) >= 3 else out_dir.parent
    analysis_dir = project_root / "analysis"
    reports_dir = project_root / "reports"
    trajectories_dir = project_root / "trajectories"
    checkpoints_dir = project_root / "checkpoints"
    for directory in (analysis_dir, reports_dir, trajectories_dir, checkpoints_dir):
        directory.mkdir(parents=True, exist_ok=True)

    plan = json.loads(plan_path.read_text())
    stages = [stage for stage in plan.get("stages", []) if stage.get("enabled", True)]
    log_path = out_dir / "automd-mock.log"
    metrics_path = out_dir / "metrics.jsonl"

    with log_path.open("w", encoding="utf-8") as log, metrics_path.open("w", encoding="utf-8") as metrics:
        log.write(f"AutoMD mock engine started for {plan.get('name', 'unnamed plan')}\n")
        for index, stage in enumerate(stages, start=1):
            label = stage.get("label", stage.get("id", "stage"))
            progress = round(index / max(len(stages), 1) * 100, 2)
            ns_per_day = 40.0 + index * 7.5
            stage_line = f"Stage {index}/{len(stages)}: {label}"
            step_line = f"step {index} of {len(stages)}"
            checkpoint_line = f"Writing checkpoint, step {index}"
            performance_line = f"Performance: {ns_per_day:.3f} ns/day"
            for line in (stage_line, step_line, checkpoint_line, performance_line):
                print(line, flush=True)
                log.write(f"{line}\n")
            log.flush()
            metrics.write(json.dumps({
                "stage": stage.get("kind"),
                "label": label,
                "progressPercent": progress,
                "nsPerDay": ns_per_day,
            }) + "\n")
            metrics.flush()
            time.sleep(args.sleep)
        log.write("AutoMD mock engine completed successfully.\n")

    (analysis_dir / "rmsd.xvg").write_text(
        '@ title "Mock RMSD"\n@ xaxis label "Time (ns)"\n@ yaxis label "RMSD (nm)"\n'
        + "\n".join(f"{i} {0.08 + i * 0.01:.3f}" for i in range(max(len(stages), 1)))
        + "\n",
        encoding="utf-8",
    )
    (analysis_dir / "rg.xvg").write_text(
        '@ title "Mock Radius of gyration"\n@ xaxis label "Time (ns)"\n@ yaxis label "Rg (nm)"\n'
        + "\n".join(f"{i} {1.9 + i * 0.015:.3f}" for i in range(max(len(stages), 1)))
        + "\n",
        encoding="utf-8",
    )
    (trajectories_dir / "mock.xtc").write_text("mock trajectory placeholder\n", encoding="utf-8")
    (checkpoints_dir / "mock.cpt").write_text("mock checkpoint placeholder\n", encoding="utf-8")
    (out_dir / "summary.json").write_text(json.dumps({
        "status": "completed",
        "engine": plan.get("engineId"),
        "stageCount": len(stages),
    }, indent=2), encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
