/**
 * Background analysis progress for the list toolbar (Phase 3 task 11):
 * "Analysing 12/340" with a bar, pause/resume and cancel. Hidden while the
 * queue is idle, except for a count of tracks that failed in the last batch.
 */

import { isIdle, useAnalysis } from "../store/analysis";
import { CloseIcon, PauseIcon, PlayIcon } from "./icons";
import { IconButton } from "./ui/IconButton";
import { Progress } from "./ui/Progress";
import { Tooltip } from "./ui/Tooltip";

export function AnalysisIndicator() {
  const status = useAnalysis((s) => s.status);
  if (status === null) return null;
  const finished = status.done + status.failed;
  const failed =
    status.failed > 0 ? (
      <span className="whitespace-nowrap text-xs text-fg-muted tabular-nums">
        {status.failed.toLocaleString()} failed
      </span>
    ) : null;
  if (isIdle(status)) return failed;

  const { pause, resume, cancel } = useAnalysis.getState();
  return (
    <div className="flex items-center gap-1.5" role="status" aria-label="Analysis">
      <span className="whitespace-nowrap text-xs text-fg-muted tabular-nums">
        {status.paused ? "Analysis paused" : "Analysing"} {finished.toLocaleString()}/
        {status.total.toLocaleString()}
      </span>
      <Progress size="sm" className="w-16" value={finished} max={Math.max(status.total, 1)} />
      {failed}
      <Tooltip content={status.paused ? "Resume analysis" : "Pause analysis"}>
        <IconButton
          size="sm"
          aria-label={status.paused ? "Resume analysis" : "Pause analysis"}
          onClick={() => void (status.paused ? resume() : pause())}
        >
          {status.paused ? <PlayIcon /> : <PauseIcon />}
        </IconButton>
      </Tooltip>
      <Tooltip content="Cancel analysis">
        <IconButton size="sm" aria-label="Cancel analysis" onClick={() => void cancel()}>
          <CloseIcon />
        </IconButton>
      </Tooltip>
    </div>
  );
}
