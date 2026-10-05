# MOKEA Crew

**A calm command center for your local coding agents.** MOKEA runs the coding CLIs you already use, right inside your project.

> Early scaffold. In-app chat, local agent settings, and provider process adapters are available. Local run history, isolated multi-agent worktrees, and plugin/skill support are planned next.

## Quick start

Install Rust, Git, and at least one supported agent CLI. Install MOKEA once:

```sh
cargo install --path crates/mokea-cli
```

Then launch MOKEA with `mokea`. On first start, a short tutorial lets you name and enable each agent. After that, MOKEA opens its own chat screen; provider CLI output streams inside this screen instead of opening a second interactive CLI. Use `mokea setup` to repeat the tutorial. For local development, commands can also be run with `cargo run -p mokea-cli -- <command>`.

The process adapter reuses each CLI's own installation, credentials, and permission behavior. MOKEA does not copy or store provider credentials.

## Commands

```text
mokea doctor
mokea agents list
mokea run --agent <codex|claude> <prompt> [--dir <path>]
mokea                             # MOKEA chat; first launch runs the setup tutorial
mokea setup                       # rename and enable/disable agents
mokea runs <list|show|cancel>    # scaffolded; local history is not wired up yet
mokea ui                         # open the MOKEA chat screen
```

In chat, route a prompt with an agent mention such as `@codex explain this project` or `@claude review the latest changes`. Enter `/agents` to rename or toggle agents. Enter `/login @agent-name` to hand sign-in to that provider's existing CLI. MOKEA stores only local display names and enabled states; provider credentials stay with the provider.

The first chat iteration runs one agent at a time. Parallel mentions will be enabled after per-agent Git worktree isolation is in place.

## Interaction direction

MOKEA uses OpenCode's in-place TUI interaction as a reference: keep the prompt, agent controls, and streaming response in one terminal workspace. MOKEA adds configurable `@handles` and per-agent on/off switches for the Codex and Claude CLIs already installed by the user. Plugin and skill support remains a later milestone.

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
