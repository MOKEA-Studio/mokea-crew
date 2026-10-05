# MOKEA Crew

**A calm command center for your local coding agents.** MOKEA runs the coding CLIs you already use, right inside your project.

> Early scaffold. The command line and agent process adapters are in place; local run history, isolated multi-agent worktrees, and the interactive TUI are planned next.

## Quick start

Install Rust, Git, and at least one supported agent CLI. Install MOKEA once:

```sh
cargo install --path crates/mokea-cli
```

Then launch the animated local home screen with `mokea`. It scans installed agent CLIs, shows their sign-in status, and lets you open Codex or Claude Code. Use `mokea setup` to go straight to provider sign-in. For local development, the commands can also be run with `cargo run -p mokea-cli -- <command>`.

The process adapter reuses each CLI's own installation, credentials, and permission behavior. MOKEA does not copy or store provider credentials.

## Commands

```text
mokea doctor
mokea agents list
mokea run --agent <codex|claude> <prompt> [--dir <path>]
mokea                             # animated home and agent chooser
mokea setup                       # provider sign-in and first-run setup
mokea runs <list|show|cancel>    # scaffolded; local history is not wired up yet
mokea ui                         # open the interactive home/setup screen
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
