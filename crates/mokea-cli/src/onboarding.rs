use crate::config::AppConfig;
use anyhow::Result;
use mokea_core::AgentKind;
use std::io::{self, IsTerminal, Write};
use tokio::time::{Duration, sleep};

pub async fn open(setup_first: bool) -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        println!("MOKEA needs an interactive terminal. Try `mokea --help`.");
        return Ok(());
    }

    let mut config = AppConfig::load()?;
    if setup_first || !config.setup_complete {
        config = setup(config).await?;
    }
    crate::chat::open(config).await
}

async fn setup(mut config: AppConfig) -> Result<AppConfig> {
    splash().await?;
    loop {
        clear_screen()?;
        print_wordmark();
        println!("\n  \x1b[1;37mFIRST RUN · MAKE THIS WORKSPACE YOURS\x1b[0m\n");
        println!("  Give each installed agent a short @name and choose which ones are active.");
        println!("  Your names and toggles stay in ~/.config/mokea/config.toml.\n");

        for kind in AgentKind::ALL {
            let old = config.agent(kind).clone();
            println!("  \x1b[1;36m{}\x1b[0m", kind.display_name());
            print!("  Name [@{}]: ", old.name);
            io::stdout().flush()?;
            let mut name = String::new();
            io::stdin().read_line(&mut name)?;
            let name = name.trim().trim_start_matches('@');
            if !name.is_empty() {
                config.agent_mut(kind).name = name.to_owned();
            }

            let default = if old.enabled { "Y/n" } else { "y/N" };
            print!("  Enable {}? [{default}]: ", config.agent(kind).name);
            io::stdout().flush()?;
            let mut enabled = String::new();
            io::stdin().read_line(&mut enabled)?;
            match enabled.trim().to_lowercase().as_str() {
                "y" | "yes" => config.agent_mut(kind).enabled = true,
                "n" | "no" => config.agent_mut(kind).enabled = false,
                _ => {}
            }
            println!();
        }

        match config.validate() {
            Ok(()) => break,
            Err(error) => {
                println!("  \x1b[31m{error}\x1b[0m");
                pause("Press Enter to set the names again")?;
            }
        }
    }

    config.setup_complete = true;
    config.save()?;
    clear_screen()?;
    print_wordmark();
    println!("\n  \x1b[32m✓ Setup saved\x1b[0m");
    println!("\n  Talk to an agent with a mention, for example:");
    println!(
        "  \x1b[1;36m@{}\x1b[0m Explain this project",
        config.agent(AgentKind::Codex).name
    );
    println!("  Switch agents or toggle them any time with \x1b[1;36m/agents\x1b[0m.");
    println!(
        "  Provider sign-in: \x1b[1;36m/login @{}\x1b[0m\n",
        config.agent(AgentKind::Claude).name
    );
    sleep(Duration::from_millis(800)).await;
    Ok(config)
}

async fn splash() -> Result<()> {
    let frames = ["◜", "◠", "◝", "◞", "◡", "◟"];
    for frame in 0..12 {
        clear_screen()?;
        print_wordmark();
        println!(
            "\n  \x1b[38;5;111m{}\x1b[0m  Setting up your local workspace…",
            frames[frame % frames.len()]
        );
        io::stdout().flush()?;
        sleep(Duration::from_millis(65)).await;
    }
    Ok(())
}

fn print_wordmark() {
    const FONT: [[&str; 5]; 5] = [
        ["#   #", "## ##", "# # #", "#   #", "#   #"],
        [" ### ", "#   #", "#   #", "#   #", " ### "],
        ["#   #", "#  # ", "###  ", "#  # ", "#   #"],
        ["#####", "#    ", "#### ", "#    ", "#####"],
        [" ### ", "#   #", "#####", "#   #", "#   #"],
    ];
    let colors = [81, 111, 141, 171, 201];
    println!();
    for row in 0..5 {
        print!("  ");
        for index in 0..FONT.len() {
            print!("\x1b[38;5;{}m{}\x1b[0m  ", colors[index], FONT[index][row]);
        }
        println!();
    }
    println!("\n  \x1b[2mYOUR LOCAL AGENT WORKSPACE\x1b[0m");
}

fn clear_screen() -> Result<()> {
    if io::stdout().is_terminal() {
        crossterm::execute!(
            io::stdout(),
            crossterm::cursor::MoveTo(0, 0),
            crossterm::terminal::Clear(crossterm::terminal::ClearType::All)
        )?;
    }
    Ok(())
}

fn pause(message: &str) -> Result<()> {
    println!("\n  {message}");
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(())
}
