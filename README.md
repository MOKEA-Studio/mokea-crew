# MOKEA Crew

**A calm command center for your local coding agents.** MOKEA runs the coding CLIs you already use, right inside your project.

> Early scaffold. The command line and agent process adapters are in place; local run history, isolated multi-agent worktrees, and the interactive TUI are planned next.

## Quick start

Install Rust, Git, and at least one supported agent CLI, then run from a project:

```sh
cargo run -p mokea-cli -- doctor
cargo run -p mokea-cli -- agents list
cargo run -p mokea-cli -- run --agent codex "Explain this project and suggest one improvement"
cargo run -p mokea-cli -- --json doctor
```

Choose `--agent claude` to use Claude Code. The process adapter reuses each CLI's own installation, credentials, and permission behavior. MOKEA does not copy or store provider credentials.

## Commands

```text
mokea doctor
mokea agents list
mokea run --agent <codex|claude> <prompt> [--dir <path>]
mokea runs <list|show|cancel>    # scaffolded; local history is not wired up yet
mokea ui                         # planned TUI entry point
```

`--agent all` is intentionally gated until each agent gets its own Git worktree. A single-agent run uses the selected project directory directly; inspect its changes with `git diff` when the agent finishes.

## Workspace layout

| Crate | Responsibility |
| --- | --- |
| `mokea-cli` | clap commands, presentation, JSON output |
| `mokea-core` | shared agent, run, and status types |
| `mokea-agents` | provider adapters and process streaming |
| `mokea-workspace` | project discovery and Git state |
| `mokea-store` | local run history boundary (SQLite planned) |

## Product principles

- Local-first; no MOKEA account, backend, or telemetry service.
- Existing provider CLI authentication and permission settings stay under user control.
- No automatic commit, merge, or push.
- MIT licensed.

See [`docs/MVP-Roadmap.md`](docs/MVP-Roadmap.md) for the product direction and milestones.
