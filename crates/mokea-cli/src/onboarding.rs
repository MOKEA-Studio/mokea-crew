use anyhow::{Context, Result};
use crossterm::{
    cursor::MoveTo,
    execute,
    terminal::{Clear, ClearType},
};
use mokea_agents::{AgentAdapter, AgentInfo, ClaudeAdapter, CodexAdapter};
use mokea_core::AgentKind;
use std::{
    io::{self, IsTerminal, Write},
    process::Stdio,
};
use tokio::{
    process::Command,
    time::{Duration, sleep},
};

#[derive(Clone, Copy, Debug)]
enum AuthState {
    Ready,
    SignIn,
    Unknown,
    Missing,
}

#[derive(Clone, Debug)]
struct AgentSnapshot {
    info: AgentInfo,
    auth: AuthState,
}

pub async fn open(setup_first: bool) -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        println!("MOKEA needs an interactive terminal for its home screen. Try `mokea --help`.");
        return Ok(());
    }

    let snapshots = scan_with_animation().await?;
    if setup_first {
        setup_flow(snapshots).await.map(|_| ())
    } else {
        home_flow(snapshots).await
    }
}

async fn scan_with_animation() -> Result<Vec<AgentSnapshot>> {
    let scan = tokio::spawn(scan_agents());
    let frames = ["◜", "◠", "◝", "◞", "◡", "◟"];
    let messages = [
        "Finding your agents",
        "Checking local sign-in",
        "Preparing your workspace",
    ];

    for frame in 0..24 {
        draw_splash(Some((
            frames[frame % frames.len()],
            messages[(frame / 8).min(2)],
        )))?;
        if frame >= 11 && scan.is_finished() {
            break;
        }
        sleep(Duration::from_millis(75)).await;
    }
    draw_splash(None)?;
    scan.await.context("Could not inspect local agent CLIs")
}

async fn scan_agents() -> Vec<AgentSnapshot> {
    let (codex, claude) = tokio::join!(
        probe(Box::new(CodexAdapter::new("codex"))),
        probe(Box::new(ClaudeAdapter::new("claude"))),
    );
    vec![codex, claude]
}

async fn probe(adapter: Box<dyn AgentAdapter>) -> AgentSnapshot {
    let info = adapter.inspect().await;
    let auth = if !info.installed {
        AuthState::Missing
    } else {
        let mut command = Command::new(&info.executable);
        match info.kind {
            AgentKind::Codex => {
                command.args(["login", "status"]);
            }
            AgentKind::Claude => {
                command.args(["auth", "status"]);
            }
        }
        match command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
        {
            Ok(status) if status.success() => AuthState::Ready,
            Ok(_) => AuthState::SignIn,
            Err(_) => AuthState::Unknown,
        }
    };
    AgentSnapshot { info, auth }
}

async fn home_flow(mut snapshots: Vec<AgentSnapshot>) -> Result<()> {
    loop {
        draw_home(&snapshots)?;
        match read_choice("Choose an option")?.to_lowercase().as_str() {
            "1" => open_agent(&snapshots, AgentKind::Codex).await?,
            "2" => open_agent(&snapshots, AgentKind::Claude).await?,
            "s" => snapshots = setup_flow(snapshots).await?,
            "d" => draw_diagnostics(&snapshots)?,
            "q" | "" => break,
            _ => wait_message("Choose 1, 2, S, D, or Q")?,
        }
    }
    Ok(())
}

async fn setup_flow(mut snapshots: Vec<AgentSnapshot>) -> Result<Vec<AgentSnapshot>> {
    loop {
        draw_setup(&snapshots)?;
        match read_choice("Choose an option")?.to_lowercase().as_str() {
            "1" => snapshots = login(&snapshots, AgentKind::Codex).await?,
            "2" => snapshots = login(&snapshots, AgentKind::Claude).await?,
            "r" => return Ok(snapshots),
            "q" | "" => return Ok(snapshots),
            _ => wait_message("Choose 1, 2, R, or Q")?,
        }
    }
}

async fn login(snapshots: &[AgentSnapshot], kind: AgentKind) -> Result<Vec<AgentSnapshot>> {
    let Some(snapshot) = snapshots.iter().find(|item| item.info.kind == kind) else {
        return Ok(snapshots.to_vec());
    };
    if !snapshot.info.installed {
        wait_message(&format!(
            "{} is not installed yet. Install its CLI, then choose this option again.",
            kind.display_name()
        ))?;
        return Ok(snapshots.to_vec());
    }

    clear_screen()?;
    println!("\n  \x1b[1;36mMOKEA / SIGN IN\x1b[0m\n");
    println!("  Opening {}'s own sign-in flow.", kind.display_name());
    println!("  MOKEA never sees or stores your credentials.\n");
    let mut command = Command::new(&snapshot.info.executable);
    match kind {
        AgentKind::Codex => {
            command.arg("login");
        }
        AgentKind::Claude => {
            command.args(["auth", "login"]);
        }
    }
    command
        .status()
        .await
        .with_context(|| format!("Could not start {} sign-in", kind.display_name()))?;
    Ok(scan_agents().await)
}

async fn open_agent(snapshots: &[AgentSnapshot], kind: AgentKind) -> Result<()> {
    let Some(snapshot) = snapshots.iter().find(|item| item.info.kind == kind) else {
        return Ok(());
    };
    if !snapshot.info.installed {
        return wait_message(&format!(
            "{} is not installed yet. Install its CLI, then try again.",
            kind.display_name()
        ));
    }
    clear_screen()?;
    println!(
        "\n  \x1b[1;36mMOKEA / {}[0m\n  Returning control to the provider CLI.\n",
        kind.name().to_uppercase()
    );
    Command::new(&snapshot.info.executable)
        .current_dir(std::env::current_dir()?)
        .status()
        .await
        .with_context(|| format!("Could not start {}", kind.display_name()))?;
    Ok(())
}

fn draw_splash(activity: Option<(&str, &str)>) -> Result<()> {
    clear_screen()?;
    println!("\n\x1b[38;5;81m  ███╗   ███╗ ██████╗ ██╗  ██╗███████╗ █████╗\x1b[0m");
    println!("\x1b[38;5;111m  ████╗ ████║██╔═══██╗██║ ██╔╝██╔════╝██╔══██╗\x1b[0m");
    println!("\x1b[38;5;141m  ██╔████╔██║██║   ██║█████╔╝ █████╗  ███████║\x1b[0m");
    println!("\x1b[38;5;171m  ██║╚██╔╝██║██║   ██║██╔═██╗ ██╔══╝  ██╔══██║\x1b[0m");
    println!("\x1b[38;5;201m  ██║ ╚═╝ ██║╚██████╔╝██║  ██╗███████╗██║  ██║\x1b[0m");
    println!("\x1b[38;5;213m  ╚═╝     ╚═╝ ╚═════╝ ╚═╝  ╚═╝╚══════╝╚═╝  ╚═╝\x1b[0m");
    println!("\n  \x1b[2mYOUR LOCAL AI WORKSPACE\x1b[0m");
    if let Some((spinner, message)) = activity {
        println!("\n  \x1b[38;5;111m{spinner}\x1b[0m  {message}...\n");
    } else {
        println!("\n  \x1b[32m✓\x1b[0m  Local workspace ready\n");
    }
    io::stdout().flush()?;
    Ok(())
}

fn draw_home(snapshots: &[AgentSnapshot]) -> Result<()> {
    clear_screen()?;
    print_header("YOUR AGENT WORKSPACE");
    for (index, kind) in [AgentKind::Codex, AgentKind::Claude].iter().enumerate() {
        let state = snapshots
            .iter()
            .find(|item| item.info.kind == *kind)
            .map(agent_state)
            .unwrap_or_else(|| "Unavailable".to_owned());
        println!(
            "  \x1b[1;37m[{}]\x1b[0m  {:<16} {}",
            index + 1,
            kind.display_name(),
            state
        );
    }
    println!("\n  \x1b[1;36m1 / 2\x1b[0m  Open an agent");
    println!("  \x1b[1;36mS\x1b[0m      Sign in / setup");
    println!("  \x1b[1;36mD\x1b[0m      Local diagnostics");
    println!("  \x1b[1;36mQ\x1b[0m      Quit\n");
    io::stdout().flush()?;
    Ok(())
}

fn draw_setup(snapshots: &[AgentSnapshot]) -> Result<()> {
    clear_screen()?;
    print_header("FIRST-TIME SETUP");
    println!("  Connect an existing agent. Sign-in stays with its provider.\n");
    for (index, kind) in [AgentKind::Codex, AgentKind::Claude].iter().enumerate() {
        let state = snapshots
            .iter()
            .find(|item| item.info.kind == *kind)
            .map(agent_state)
            .unwrap_or_else(|| "Unavailable".to_owned());
        println!(
            "  \x1b[1;37m[{}]\x1b[0m  {:<16} {}",
            index + 1,
            kind.display_name(),
            state
        );
    }
    println!("\n  \x1b[1;36m1 / 2\x1b[0m  Continue with provider sign-in");
    println!("  \x1b[1;36mR\x1b[0m      Return to home");
    println!("  \x1b[1;36mQ\x1b[0m      Quit\n");
    io::stdout().flush()?;
    Ok(())
}

fn draw_diagnostics(snapshots: &[AgentSnapshot]) -> Result<()> {
    clear_screen()?;
    print_header("LOCAL DIAGNOSTICS");
    for snapshot in snapshots {
        let detail = snapshot.info.version.as_deref().unwrap_or("not found");
        println!(
            "  {:<16} {:<32} {}",
            snapshot.info.kind.display_name(),
            detail,
            auth_label(snapshot.auth)
        );
    }
    println!("\n  Sign-in is handled by each provider CLI. No MOKEA account is required.\n");
    wait_message("Press Enter to return")
}

fn print_header(title: &str) {
    println!("\n  \x1b[38;5;111m███╗   ███╗ ██████╗ ██╗  ██╗███████╗ █████╗\x1b[0m");
    println!("  \x1b[38;5;171m╚═╝     ╚═╝ ╚═════╝ ╚═╝  ╚═╝╚══════╝╚═╝  ╚═╝\x1b[0m");
    println!("\n  \x1b[1;37m{title}\x1b[0m\n");
}

fn agent_state(snapshot: &AgentSnapshot) -> String {
    if !snapshot.info.installed {
        return "\x1b[90m○  Not installed\x1b[0m".to_owned();
    }
    let version = snapshot.info.version.as_deref().unwrap_or("installed");
    match snapshot.auth {
        AuthState::Ready => format!("\x1b[32m●  Signed in\x1b[0m  \x1b[2m{version}\x1b[0m"),
        AuthState::SignIn => format!("\x1b[33m◐  Sign-in needed\x1b[0m  \x1b[2m{version}\x1b[0m"),
        AuthState::Unknown => {
            format!("\x1b[36m◌  Sign-in status unknown\x1b[0m  \x1b[2m{version}\x1b[0m")
        }
        AuthState::Missing => "\x1b[90m○  Not installed\x1b[0m".to_owned(),
    }
}

fn auth_label(auth: AuthState) -> &'static str {
    match auth {
        AuthState::Ready => "signed in",
        AuthState::SignIn => "sign-in needed",
        AuthState::Unknown => "status unavailable",
        AuthState::Missing => "not installed",
    }
}

fn read_choice(prompt: &str) -> Result<String> {
    print!("  \x1b[2m{prompt} ›\x1b[0m ");
    io::stdout().flush()?;
    let mut choice = String::new();
    io::stdin().read_line(&mut choice)?;
    Ok(choice.trim().to_owned())
}

fn wait_message(message: &str) -> Result<()> {
    println!("\n  {message}");
    print!("  Press Enter to continue…");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(())
}

fn clear_screen() -> Result<()> {
    execute!(io::stdout(), MoveTo(0, 0), Clear(ClearType::All))?;
    Ok(())
}
