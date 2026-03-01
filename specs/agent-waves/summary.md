# Agent Waves: Summary

## Artifacts

| File | Purpose |
|------|---------|
| `specs/agent-waves/rough-idea.md` | Captured from issue #210 + discussion |
| `specs/agent-waves/requirements.md` | 9 requirements questions with answers |
| `specs/agent-waves/research/event-system.md` | Event struct, bus, reader, logger analysis |
| `specs/agent-waves/research/loop-runner.md` | Event loop, backend execution, sequential bottleneck |
| `specs/agent-waves/research/hat-system.md` | HatConfig, Hatless Ralph, prompt building, activation lifecycle |
| `specs/agent-waves/research/worktree-system.md` | Worktree isolation, parallel loop coordination |
| `specs/agent-waves/research/cli-tools.md` | CLI structure, `ralph emit`, tool documentation |
| `specs/agent-waves/design.md` | Full design document with architecture, data models, acceptance criteria |
| `specs/agent-waves/plan.md` | 12-step implementation plan |
| `specs/agent-waves/summary.md` | This file |

## Overview

Agent Waves introduce intra-loop parallelism to Ralph's orchestration — fan-out work to concurrent backend instances, collect results, aggregate. Built on three primitives: wave-aware event emission, concurrent hat execution, and an aggregator gate.

## Key Decisions

- Ralph dispatches (decides what to parallelize), loop runner executes concurrently
- Full hat execution per wave instance — agents are smart, let them do the work
- Ralph as aggregator with `wait_for_all` gate — just another hat activation
- CLI tools + context injection — same mechanism enables both explicit and NL dispatch
- Shared workspace only for v1 — zero overhead, sufficient for read-heavy workloads
- Best-effort failure handling — partial results are almost always useful
- Each instance = one activation — transparent cost accounting
- 300s default aggregation timeout — prevents hung waves

## Next Steps

1. **Implement with Ralph:** `ralph run --config presets/pdd-to-code-assist.yml`
2. **Simpler flow:** `ralph run --config presets/spec-driven.yml`
3. **Manual implementation:** Follow the 12-step plan in `plan.md`
