use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use mokea_agents::{AgentAdapter, AgentInfo, ClaudeAdapter, CodexAdapter};
use serde_json::json;
use std::{path::PathBuf, process::Stdio};
use tokio::process::Command;

#[derive(Debug, Parser)]
#[command(name = "mokea", version, about = "A calm command center for your local coding agents", long_about = None)]
struct Cli {
    /// Print machine-readable JSON where supported.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Check local tools and agent CLI availability.
    Doctor,
    /// Show available coding agents.
    Agents { #[command(subcommand)] command: AgentCommands },
    /// Give a task to a local coding agent.
    Run {
        /// Agent to run. Use `all` after isolated worktrees are available.
        #[arg(long, value_enum, default_value = "codex")]
        agent: AgentArg,
        /// Task prompt.
        prompt: String,
        /// Project directory (defaults to the current directory).
        #[arg(long, short)]
        dir: Option<PathBuf>,
    },
    /// Browse or cancel saved runs (local history is coming soon).
    Runs { #[command(subcommand)] command: RunCommands },
    /// Open the interactive workspace (TUI is coming soon).
    Ui,
}

#[derive(Debug, Subcommand)]
enum AgentCommands { List }

#[derive(Debug, Subcommand)]
enum RunCommands {
    List,
    Show { id: String },
    Cancel { id: String },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum AgentArg { Codex, Claude, All }

fn adapters() -> [Box<dyn AgentAdapter>; 2] {
    [Box::new(CodexAdapter::new("codex")), Box::new(ClaudeAdapter::new("claude"))]
}

#[tokio::main]
async fn main() {
    if let Err(error) = execute().await {
        eprintln!("\x1b[31m✗\x1b[0m {error:#}");
        std::process::exit(1);
    }
}

async fn execute() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Doctor => doctor(cli.json).await,
        Commands::Agents { command: AgentCommands::List } => agents_list(cli.json).await,
        Commands::Run { agent, prompt, dir } => run(agent, &prompt, dir, cli.json).await,
        Commands::Runs { command } => runs(command, cli.json),
        Commands::Ui => bail!("The interactive TUI is planned for a later milestone. Use `mokea --help` to explore the CLI."),
    }
}

async fn doctor(as_json: bool) -> Result<()> {
    let mut checks = vec![check_tool("Rust", "rustc", "--version").await, check_tool("Git", "git", "--version").await];
    for adapter in adapters() { checks.push(check_agent(adapter.as_ref()).await); }
    if as_json {
        println!("{}", serde_json::to_string_pretty(&checks_json(&checks))?);
    } else {
        println!("\n  \x1b[1mMOKEA\x1b[0m  Local setup check\n");
        for (name, ok, detail) in &checks {
            let icon = if *ok { "\x1b[32m✓\x1b[0m" } else { "\x1b[33m○\x1b[0m" };
            println!("  {icon}  {name:<16} {detail}");
        }
        println!("\n  Existing CLI authentication is managed by each provider.\n");
    }
    Ok(())
}

async fn agents_list(as_json: bool) -> Result<()> {
    let mut agents = Vec::new();
    for adapter in adapters() { agents.push(adapter.inspect().await); }
    if as_json {
        println!("{}", serde_json::to_string_pretty(&agents.iter().map(agent_json).collect::<Vec<_>>())?);
    } else {
        println!("\n  \x1b[1mAVAILABLE AGENTS\x1b[0m\n");
        for agent in agents {
            let (icon, state) = if agent.installed { ("\x1b[32m●\x1b[0m", agent.version.unwrap_or_else(|| "ready".into())) } else { ("\x1b[90m○\x1b[0m", format!("not found · {}", agent.executable)) };
            println!("  {icon}  {:<16} {state}", agent.kind.display_name());
        }
        println!();
    }
    Ok(())
}

async fn run(agent: AgentArg, prompt: &str, dir: Option<PathBuf>, as_json: bool) -> Result<()> {
    if matches!(agent, AgentArg::All) {
        bail!("`--agent all` is not available until separate Git worktrees are implemented. Run one agent at a time to keep edits isolated.");
    }
    let project = dir.unwrap_or(std::env::current_dir()?);
    let project = mokea_workspace::discover(&project).await?;
    if project.has_uncommitted_changes && !as_json {
        eprintln!("\x1b[33m!\x1b[0m This project has uncommitted changes. MOKEA will leave them untouched.");
    }
    let adapter: Box<dyn AgentAdapter> = match agent {
        AgentArg::Codex => Box::new(CodexAdapter::new("codex")),
        AgentArg::Claude => Box::new(ClaudeAdapter::new("claude")),
        AgentArg::All => unreachable!(),
    };
    if as_json {
        println!("{}", json!({"event":"run.started","agent":adapter.kind().name(),"project":project.root}));
    } else {
        println!("\n  \x1b[1mMOKEA\x1b[0m  starting {} in {}\n", adapter.kind().display_name(), project.root.display());
    }
    let code = adapter.run(prompt, &project.root, as_json).await?;
    if as_json {
        println!("{}", json!({"event":"run.finished","agent":adapter.kind().name(),"exit_code":code}));
    } else if code == 0 {
        println!("\n  \x1b[32m✓\x1b[0m Agent finished successfully. Review changes with `git diff`.\n");
    } else {
        println!("\n  \x1b[31m✗\x1b[0m Agent exited with code {code}.\n");
    }
    if code != 0 { std::process::exit(code); }
    Ok(())
}

fn runs(command: RunCommands, _as_json: bool) -> Result<()> {
    match command {
        RunCommands::List => {},
        RunCommands::Show { id } | RunCommands::Cancel { id } => {
            let _ = id;
        }
    }
    bail!("Run history is not wired up yet. The local SQLite store is a planned MVP milestone.")
}

async fn check_tool(label: &str, executable: &str, arg: &str) -> (String, bool, String) {
    match Command::new(executable).arg(arg).stdout(Stdio::piped()).stderr(Stdio::null()).output().await {
        Ok(output) if output.status.success() => (label.to_owned(), true, String::from_utf8_lossy(&output.stdout).trim().to_owned()),
        _ => (label.to_owned(), false, format!("not found · install {executable}")),
    }
}

async fn check_agent(adapter: &dyn AgentAdapter) -> (String, bool, String) {
    let AgentInfo { kind, executable, installed, version } = adapter.inspect().await;
    (kind.display_name().to_owned(), installed, version.unwrap_or_else(|| format!("not found · install {executable}")))
}

fn checks_json(checks: &[(String, bool, String)]) -> serde_json::Value {
    json!({"checks": checks.iter().map(|(name, ok, detail)| json!({"name":name,"ok":ok,"detail":detail})).collect::<Vec<_>>()})
}

fn agent_json(agent: &AgentInfo) -> serde_json::Value {
    json!({"id":agent.kind.name(),"name":agent.kind.display_name(),"installed":agent.installed,"executable":agent.executable,"version":agent.version})
}
