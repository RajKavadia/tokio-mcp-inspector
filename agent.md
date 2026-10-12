# tokio-inspector — Agent Usage Guide

You have six read-only tools to investigate live Tokio services. You cannot
break anything with them: there are no write, exec, or deploy tools. Read this
file before your first investigation.

## 0. Prerequisites

A target service must be **running and instrumented** (`console-subscriber`,
`--cfg tokio_unstable`). Without one, every tool returns `Unavailable` — that
means "start the target", never "guess the answer". Default addresses used in
this repo's demos:

| Service            | HTTP | Console (gRPC) | Shape            |
| ------------------ | ---- | -------------- | ---------------- |
| `normal_service`   | 3000 | 6669           | healthy baseline |
| `slow_service`     | 3001 | 6670           | blocking + burn  |
| `resource_content` | 3002 | 6671           | pool exhaustion  |

Each demo also serves `GET /debug/latency` → `{"samples","sum","p50","p95","p99"}`.

## 1. Tool catalog

| Tool                  | When to use it                                              |
| --------------------- | ----------------------------------------------------------- |
| `get_runtime_overview`| First call. Identity, task count, telemetry health.         |
| `list_tasks`          | Suspect tasks. Paginated; `limit` clamps to [1, 200].       |
| `list_resources`      | Suspect contention. Semaphores, locks, timers + attributes. |
| `capture_snapshot`    | Store a bounded observation. Pass `metrics_url` when the    |
|                       | target exposes `/debug/latency` to merge the real histogram.|
| `compare_snapshots`   | Delta between two stored IDs. Handles resets explicitly.    |
| `investigate_latency` | Deterministic rules over two stored IDs. Returns latency,   |
|                       | task-growth, and contention verdicts in one call.           |

All tools clamp `timeout_ms` to [1000, 30000] and `limit` to [1, 200].
Only loopback addresses are accepted. Timeouts are errors, never empty data.

## 2. The investigation loop (follow this order)

1. `get_runtime_overview` → confirm telemetry is flowing.
2. Check `completeness`: if `partial` is true, read `missing_fields` and
   `warnings` first. Absent fields constrain what you may conclude.
3. `capture_snapshot` (with `metrics_url` if available) → baseline ID.
4. Let load run, or switch to the suspect service → second `capture_snapshot`.
5. `compare_snapshots` → is the delta `unreliable`? If yes, re-baseline.
6. `investigate_latency` → report each verdict separately.

## 3. How to read a verdict

Every diagnosis has `status`, `finding`, `hypothesis`, `confidence [0,1]`,
`evidence` (observation IDs), and optional `verification`.

- `Observed` — a measurement. Quote it with its evidence IDs.
- `Hypothesis` — an association with confidence < 1. Present it as "may be
  associated with…", never as root cause.
- `Verified` — only after a controlled A/B (reproduce → remove bottleneck →
  re-capture shows improvement). Never self-award this status.
- `NoIssue` — the rule checked and found nothing. Say what was checked.

Rules of evidence (non-negotiable):

- Missing is not zero. `scheduled_ns: null` means "unknown", not "fast".
- No evidence IDs → no finding. Reject any conclusion you cannot cite.
- `latency_delta_ratio: null` means the histogram was absent, not that
  latency is fine.
- `tasks_truncated: true` means the true task count is unknown; growth
  verdicts across truncated snapshots are weak — say so.

## 4. Worked example (healthy vs slow)

```
get_runtime_overview {console_addr: 127.0.0.1:6669}          → flowing, N tasks
capture_snapshot {console_addr: …6669, metrics_url: http://127.0.0.1:3000/debug/latency}
  → id A   (hit /work ~40× first so the histogram has samples)
capture_snapshot {console_addr: …6670, metrics_url: http://127.0.0.1:3001/debug/latency}
  → id B   (hit :3001/work ~40× first)
compare_snapshots {base_id: A, current_id: B}                → ratio ×6.6, reliable
investigate_latency {base_id: A, current_id: B}
  → latency Hypothesis 0.72 "rose 46ms → 306ms"; contention NoIssue
```

Correct report: "Observed ×6.6 latency rise (A, B). Hypothesis: associated with
the slow service's blocking/burn defects. Contention ruled out by the
scheduling-delay rule. To verify: remove the defects, re-capture, expect
p50 < 80ms." Note what this does NOT say: it does not claim which defect
dominates — the tools cannot separate blocking from burn, and neither can you.

## 5. Limits — say these out loud instead of working around them

- No exact queue lengths, no live variables, no per-line profilers.
- `task.busy_ns` is currently undecodable — always check `missing_fields`.
- Minimum 30 histogram samples before the latency rule gives a verdict; below
  that it abstains by design. Generate load instead of lowering the bar.
- Same-service baseline → regression is the only methodologically strict
  comparison. Cross-service comparison is illustrative, not conclusive.
- The snapshot store holds 16 observations per server process and is
  in-memory: IDs die with the server. Long investigations should re-capture.

## 6. Starter prompts

- "Check whether the service on 127.0.0.1:6669 is healthy."
- "Capture a baseline, then compare against 127.0.0.1:6670 and report latency,
  task-growth, and contention verdicts with evidence."
- "The latency rule abstained — generate load against /work and re-capture."
