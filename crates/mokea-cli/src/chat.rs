use crate::config::AppConfig;
use anyhow::{Context, Result, bail};
use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use mokea_agents::{AgentAdapter, ClaudeAdapter, CodexAdapter};
use mokea_core::AgentKind;
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use std::{
    io::{self, IsTerminal},
    path::PathBuf,
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
    time::sleep,
};

type UiTerminal = Terminal<CrosstermBackend<io::Stdout>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum View {
    Chat,
    Agents,
}

#[derive(Clone, Debug)]
struct ChatLine {
    speaker: String,
    text: String,
    color: Color,
}

#[derive(Debug)]
enum RunEvent {
    Output(AgentKind, String),
    Finished(AgentKind, Option<i32>),
}

pub async fn open(config: AppConfig) -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        println!("MOKEA's chat needs an interactive terminal. Try `mokea --help`.");
        return Ok(());
    }
    config.validate()?;
    let project_dir = std::env::current_dir()?;
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut ui = Ui::new(config, project_dir, tx);
    let (mut terminal, mut terminal_guard) = enter_terminal()?;

    loop {
        terminal.draw(|frame| ui.draw(frame.area(), frame))?;
        ui.check_child(&mut rx).await?;
        if event::poll(Duration::from_millis(35))? {
            if let Event::Key(key) = event::read()? {
                if ui
                    .handle_key(key, &mut terminal, &mut terminal_guard)
                    .await?
                {
                    break;
                }
            }
        } else if ui.active.is_some() {
            if let Some(event) = rx.try_recv().ok() {
                ui.handle_run_event(event);
            } else {
                sleep(Duration::from_millis(15)).await;
            }
        }
    }

    if let Some(mut active) = ui.active.take() {
        let _ = active.child.start_kill();
        let _ = active.child.wait().await;
    }
    drop(terminal);
    Ok(())
}

struct ActiveRun {
    kind: AgentKind,
    child: Child,
}

struct Ui {
    config: AppConfig,
    project_dir: PathBuf,
    view: View,
    selected: AgentKind,
    input: String,
    editing_name: bool,
    active: Option<ActiveRun>,
    tx: UnboundedSender<RunEvent>,
    lines: Vec<ChatLine>,
    status: String,
}

impl Ui {
    fn new(config: AppConfig, project_dir: PathBuf, tx: UnboundedSender<RunEvent>) -> Self {
        let selected = AgentKind::ALL
            .into_iter()
            .find(|kind| config.agent(*kind).enabled)
            .unwrap_or(AgentKind::Codex);
        let mut ui = Self {
            config,
            project_dir,
            view: View::Chat,
            selected,
            input: String::new(),
            editing_name: false,
            active: None,
            tx,
            lines: Vec::new(),
            status: "Type a message or start with @agent-name · /agents · /help".into(),
        };
        ui.lines.push(ChatLine {
            speaker: "MOKEA".into(),
            text: "Your agents are here. Mention one to route a task, for example: @codex explain this project".into(),
            color: Color::Cyan,
        });
        ui
    }

    fn draw(&self, area: Rect, frame: &mut ratatui::Frame<'_>) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(1),
                Constraint::Length(3),
            ])
            .split(area);
        let header = Paragraph::new(Line::from(vec![
            Span::styled(
                " MOKEA ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  local agent workspace  "),
            Span::styled(
                path_label(&self.project_dir),
                Style::default().fg(Color::DarkGray),
            ),
        ]))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(Color::DarkGray)),
        );
        frame.render_widget(header, chunks[0]);

        let content_chunks = if area.width >= 76 {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(25), Constraint::Min(0)])
                .split(chunks[1])
        } else {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(0), Constraint::Min(0)])
                .split(chunks[1])
        };
        if area.width >= 76 {
            self.draw_sidebar(content_chunks[0], frame);
        }
        match self.view {
            View::Chat => self.draw_chat(content_chunks[1], frame),
            View::Agents => self.draw_agents(content_chunks[1], frame),
        }

        let status =
            Paragraph::new(self.status.as_str()).style(Style::default().fg(Color::DarkGray));
        frame.render_widget(status, chunks[2]);
        let input_title;
        let shown = self.input.as_str();
        let title = if self.editing_name {
            input_title = format!(" Rename @{} ", self.config.agent(self.selected).name);
            input_title.as_str()
        } else {
            input_title = format!(
                " Message → @{} · Enter to send · Esc cancels ",
                self.config.agent(self.selected).name
            );
            input_title.as_str()
        };
        let input = Paragraph::new(shown)
            .style(Style::default().fg(Color::White))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(Style::default().fg(Color::Cyan)),
            );
        frame.render_widget(input, chunks[3]);
        if self.editing_name || self.view == View::Chat {
            let cursor_x = chunks[3].x.saturating_add(1).saturating_add(
                shown
                    .chars()
                    .count()
                    .min(chunks[3].width.saturating_sub(3) as usize) as u16,
            );
            frame.set_cursor_position((cursor_x, chunks[3].y.saturating_add(1)));
        }
    }

    fn draw_sidebar(&self, area: Rect, frame: &mut ratatui::Frame<'_>) {
        let mut rows = vec![
            Line::styled(
                " AGENTS",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Line::raw(""),
        ];
        for kind in AgentKind::ALL {
            let agent = self.config.agent(kind);
            let marker = if agent.enabled { "●" } else { "○" };
            let color = if agent.enabled {
                Color::Green
            } else {
                Color::DarkGray
            };
            let selected = if self.selected == kind { "> " } else { "  " };
            rows.push(Line::from(vec![
                Span::raw(selected),
                Span::styled(marker, Style::default().fg(color)),
                Span::raw(" @"),
                Span::styled(agent.name.as_str(), Style::default().fg(color)),
            ]));
        }
        rows.extend([
            Line::raw(""),
            Line::styled(" /agents manage", Style::default().fg(Color::DarkGray)),
            Line::styled(" /help commands", Style::default().fg(Color::DarkGray)),
        ]);
        frame.render_widget(
            Paragraph::new(rows).block(
                Block::default()
                    .borders(Borders::RIGHT)
                    .border_style(Style::default().fg(Color::DarkGray)),
            ),
            area,
        );
    }

    fn draw_chat(&self, area: Rect, frame: &mut ratatui::Frame<'_>) {
        let mut transcript = Vec::new();
        for entry in &self.lines {
            transcript.push(Line::from(Span::styled(
                format!("{} ", entry.speaker),
                Style::default()
                    .fg(entry.color)
                    .add_modifier(Modifier::BOLD),
            )));
            transcript.extend(
                entry
                    .text
                    .lines()
                    .map(|line| Line::raw(format!("  {line}"))),
            );
            transcript.push(Line::raw(""));
        }
        let available = area.height.saturating_sub(2) as usize;
        let text_width = area.width.saturating_sub(4).max(1) as usize;
        let rendered_lines = transcript
            .iter()
            .map(|line| {
                let line_width = line
                    .spans
                    .iter()
                    .map(|span| span.content.chars().count())
                    .sum::<usize>()
                    .max(1);
                line_width.div_ceil(text_width)
            })
            .sum::<usize>();
        let offset = rendered_lines
            .saturating_sub(available)
            .min(u16::MAX as usize) as u16;
        let panel = Paragraph::new(Text::from(transcript))
            .wrap(Wrap { trim: false })
            .scroll((offset, 0))
            .block(
                Block::default()
                    .borders(Borders::LEFT)
                    .title(" Conversation ")
                    .border_style(Style::default().fg(Color::DarkGray)),
            );
        frame.render_widget(panel, area);
    }

    fn draw_agents(&self, area: Rect, frame: &mut ratatui::Frame<'_>) {
        let mut rows = vec![
            Line::styled(
                " AGENT SETTINGS",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Line::raw(""),
        ];
        for (index, kind) in AgentKind::ALL.into_iter().enumerate() {
            let agent = self.config.agent(kind);
            let mark = if agent.enabled { "ON " } else { "OFF" };
            let color = if agent.enabled {
                Color::Green
            } else {
                Color::DarkGray
            };
            rows.push(Line::from(vec![
                Span::styled(
                    format!(" [{}] ", index + 1),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(
                    mark,
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::raw("  @"),
                Span::styled(agent.name.as_str(), Style::default().fg(Color::White)),
                Span::styled(
                    format!("  ({})", kind.display_name()),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }
        rows.extend([
            Line::raw(""),
            Line::styled(
                " 1 / 2   toggle Codex / Claude",
                Style::default().fg(Color::DarkGray),
            ),
            Line::styled(
                " Tab     select agent",
                Style::default().fg(Color::DarkGray),
            ),
            Line::styled(
                " N       rename selected agent",
                Style::default().fg(Color::DarkGray),
            ),
            Line::styled(
                " Esc     return to chat",
                Style::default().fg(Color::DarkGray),
            ),
            Line::raw(""),
            Line::styled(
                "Changes save to local config immediately.",
                Style::default().fg(Color::DarkGray),
            ),
        ]);
        frame.render_widget(
            Paragraph::new(rows).wrap(Wrap { trim: false }).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" /agents ")
                    .border_style(Style::default().fg(Color::Cyan)),
            ),
            area,
        );
    }

    async fn handle_key(
        &mut self,
        key: KeyEvent,
        terminal: &mut UiTerminal,
        guard: &mut TerminalGuard,
    ) -> Result<bool> {
        if key.code == KeyCode::Esc {
            if self.editing_name {
                self.editing_name = false;
                self.input.clear();
            } else if self.active.is_some() {
                self.cancel_run().await?;
            } else {
                self.view = View::Chat;
            }
            return Ok(false);
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Ok(true);
        }

        if self.view == View::Agents && !self.editing_name {
            match key.code {
                KeyCode::Char('1') => self.toggle_agent(AgentKind::Codex)?,
                KeyCode::Char('2') => self.toggle_agent(AgentKind::Claude)?,
                KeyCode::Tab => {
                    self.selected = if self.selected == AgentKind::Codex {
                        AgentKind::Claude
                    } else {
                        AgentKind::Codex
                    }
                }
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    self.editing_name = true;
                    self.input.clear();
                    self.status =
                        format!("Choose a new @name for {}", self.selected.display_name());
                }
                _ => {}
            }
            return Ok(false);
        }

        match key.code {
            KeyCode::Enter => {
                if self.editing_name {
                    self.save_agent_name()?;
                } else {
                    self.submit(terminal, guard).await?;
                }
            }
            KeyCode::Char(ch) => self.input.push(ch),
            KeyCode::Backspace => {
                self.input.pop();
            }
            _ => {}
        }
        Ok(false)
    }

    async fn submit(&mut self, terminal: &mut UiTerminal, guard: &mut TerminalGuard) -> Result<()> {
        let input = self.input.trim().to_owned();
        if input.is_empty() {
            return Ok(());
        }
        self.input.clear();

        if input == "/agents" {
            self.view = View::Agents;
            return Ok(());
        }
        if input == "/help" {
            self.lines.push(ChatLine { speaker: "MOKEA".into(), text: "Use @name followed by a task to route it. /agents manages names and on/off state. /login @name signs in through the provider. /clear clears this transcript. Ctrl+C exits.".into(), color: Color::Cyan });
            return Ok(());
        }
        if input == "/clear" {
            self.lines.clear();
            return Ok(());
        }
        if input.starts_with("/login") {
            self.provider_login(&input, terminal, guard).await?;
            return Ok(());
        }
        if self.active.is_some() {
            self.status =
                "An agent is still working. Press Esc to cancel it, then send another message."
                    .into();
            self.input = input;
            return Ok(());
        }

        let (kind, prompt) = match route(&input, &self.config, self.selected) {
            Ok(route) => route,
            Err(error) => {
                self.status = error.to_string();
                return Ok(());
            }
        };
        self.selected = kind;
        let handle = self.config.agent(kind).name.clone();
        self.lines.push(ChatLine {
            speaker: "you".into(),
            text: input,
            color: Color::White,
        });
        self.lines.push(ChatLine {
            speaker: format!("@{handle}"),
            text: String::new(),
            color: if kind == AgentKind::Codex {
                Color::Blue
            } else {
                Color::Magenta
            },
        });
        self.status = format!("@{handle} is working… · Esc to cancel");
        if let Err(error) = self.start_run(kind, &prompt).await {
            self.status = format!(
                "Could not start @{}: {error:#}",
                self.config.agent(kind).name
            );
            self.lines.push(ChatLine {
                speaker: "MOKEA".into(),
                text: self.status.clone(),
                color: Color::Red,
            });
        }
        Ok(())
    }

    async fn start_run(&mut self, kind: AgentKind, prompt: &str) -> Result<()> {
        let adapter: Box<dyn AgentAdapter> = match kind {
            AgentKind::Codex => Box::new(CodexAdapter::new("codex")),
            AgentKind::Claude => Box::new(ClaudeAdapter::new("claude")),
        };
        let mut command = adapter.command(prompt);
        command
            .current_dir(&self.project_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .with_context(|| format!("Could not start {}", kind.display_name()))?;
        let stdout = child.stdout.take().context("Agent stdout was not piped")?;
        let stderr = child.stderr.take().context("Agent stderr was not piped")?;
        spawn_reader(stdout, kind, self.tx.clone());
        spawn_reader(stderr, kind, self.tx.clone());
        self.active = Some(ActiveRun { kind, child });
        Ok(())
    }

    fn handle_run_event(&mut self, event: RunEvent) {
        match event {
            RunEvent::Output(kind, text) => {
                if let Some(last) = self.lines.last_mut() {
                    if last.speaker == format!("@{}", self.config.agent(kind).name) {
                        if !last.text.is_empty() {
                            last.text.push('\n');
                        }
                        last.text.push_str(&text);
                    }
                }
            }
            RunEvent::Finished(kind, code) => {
                let handle = &self.config.agent(kind).name;
                self.status = match code {
                    Some(0) => format!("@{handle} finished · choose another agent with @name"),
                    Some(code) => format!("@{handle} exited with code {code}"),
                    None => format!("@{handle} ended without an exit code"),
                };
                self.active = None;
            }
        }
    }

    async fn check_child(&mut self, rx: &mut UnboundedReceiver<RunEvent>) -> Result<()> {
        while let Ok(event) = rx.try_recv() {
            self.handle_run_event(event);
        }
        if let Some(active) = self.active.as_mut() {
            if let Some(status) = active.child.try_wait()? {
                let kind = active.kind;
                self.handle_run_event(RunEvent::Finished(kind, status.code()));
                while let Ok(event) = rx.try_recv() {
                    self.handle_run_event(event);
                }
            }
        }
        Ok(())
    }

    async fn cancel_run(&mut self) -> Result<()> {
        if let Some(active) = self.active.as_mut() {
            active
                .child
                .start_kill()
                .context("Could not stop the active agent")?;
            self.status = "Stopping the active agent…".into();
        }
        Ok(())
    }

    fn toggle_agent(&mut self, kind: AgentKind) -> Result<()> {
        let new_state = !self.config.agent(kind).enabled;
        if !new_state
            && AgentKind::ALL
                .into_iter()
                .filter(|agent| *agent != kind)
                .all(|agent| !self.config.agent(agent).enabled)
        {
            self.status = "Keep at least one agent enabled.".into();
            return Ok(());
        }
        self.config.agent_mut(kind).enabled = new_state;
        self.config.save()?;
        if !new_state && self.selected == kind {
            self.selected = AgentKind::ALL
                .into_iter()
                .find(|other| self.config.agent(*other).enabled)
                .unwrap_or(kind);
        }
        self.status = format!(
            "@{} is now {}",
            self.config.agent(kind).name,
            if new_state { "on" } else { "off" }
        );
        Ok(())
    }

    fn save_agent_name(&mut self) -> Result<()> {
        let name = self.input.trim().trim_start_matches('@').to_owned();
        let old = self.config.agent(self.selected).name.clone();
        self.config.agent_mut(self.selected).name = name;
        if let Err(error) = self.config.validate() {
            self.config.agent_mut(self.selected).name = old;
            self.status = error.to_string();
            return Ok(());
        }
        self.config.save()?;
        self.input.clear();
        self.editing_name = false;
        self.status = format!(
            "Renamed agent to @{}",
            self.config.agent(self.selected).name
        );
        Ok(())
    }

    async fn provider_login(
        &mut self,
        input: &str,
        terminal: &mut UiTerminal,
        guard: &mut TerminalGuard,
    ) -> Result<()> {
        let handle = input
            .split_whitespace()
            .nth(1)
            .unwrap_or("")
            .trim_start_matches('@');
        let Some(kind) = AgentKind::ALL
            .into_iter()
            .find(|kind| self.config.agent(*kind).name.eq_ignore_ascii_case(handle))
        else {
            self.status = "Usage: /login @agent-name".into();
            return Ok(());
        };
        let executable = match kind {
            AgentKind::Codex => "codex",
            AgentKind::Claude => "claude",
        };
        leave_terminal(terminal, guard)?;
        println!(
            "\nMOKEA will open the provider's own sign-in. Credentials stay with that provider.\n"
        );
        let mut command = Command::new(executable);
        match kind {
            AgentKind::Codex => {
                command.arg("login");
            }
            AgentKind::Claude => {
                command.args(["auth", "login"]);
            }
        }
        let result = command.status().await;
        enter_terminal_modes(terminal, guard)?;
        result.with_context(|| format!("Could not start {} sign-in", kind.display_name()))?;
        self.status = format!("Returned from {} sign-in", kind.display_name());
        Ok(())
    }
}

fn route(input: &str, config: &AppConfig, selected: AgentKind) -> Result<(AgentKind, String)> {
    let (kind, prompt) = if let Some(rest) = input.strip_prefix('@') {
        let (handle, prompt) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        let Some(kind) = AgentKind::ALL
            .into_iter()
            .find(|kind| config.agent(*kind).name.eq_ignore_ascii_case(handle))
        else {
            bail!("Unknown agent @{handle}. Check names with /agents.");
        };
        (kind, prompt.trim())
    } else {
        (selected, input)
    };
    for other in AgentKind::ALL {
        if other == kind {
            continue;
        }
        let other_name = format!("@{}", config.agent(other).name);
        if prompt
            .split_whitespace()
            .any(|word| word.eq_ignore_ascii_case(&other_name))
        {
            bail!(
                "Send one agent at a time until MOKEA can isolate each agent in its own worktree."
            );
        }
    }
    if !config.agent(kind).enabled {
        bail!(
            "@{} is turned off. Enable it with /agents.",
            config.agent(kind).name
        );
    }
    if prompt.is_empty() {
        bail!(
            "Add a task after the agent mention, like `@{} explain this project`.",
            config.agent(kind).name
        );
    }
    Ok((kind, prompt.to_owned()))
}

fn spawn_reader<R>(reader: R, kind: AgentKind, tx: UnboundedSender<RunEvent>)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if tx.send(RunEvent::Output(kind, line)).is_err() {
                break;
            }
        }
    });
}

struct TerminalGuard {
    active: bool,
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = terminal::disable_raw_mode();
            let _ = execute!(io::stdout(), Show, LeaveAlternateScreen);
        }
    }
}

fn enter_terminal() -> Result<(UiTerminal, TerminalGuard)> {
    let mut guard = TerminalGuard { active: false };
    terminal::enable_raw_mode()?;
    guard.active = true;
    execute!(
        io::stdout(),
        EnterAlternateScreen,
        Hide,
        Clear(ClearType::All)
    )?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    terminal.clear()?;
    Ok((terminal, guard))
}

fn leave_terminal(terminal: &mut UiTerminal, guard: &mut TerminalGuard) -> Result<()> {
    terminal.show_cursor()?;
    terminal::disable_raw_mode()?;
    execute!(io::stdout(), Show, LeaveAlternateScreen)?;
    guard.active = false;
    Ok(())
}

fn enter_terminal_modes(terminal: &mut UiTerminal, guard: &mut TerminalGuard) -> Result<()> {
    terminal::enable_raw_mode()?;
    guard.active = true;
    execute!(
        io::stdout(),
        EnterAlternateScreen,
        Hide,
        Clear(ClearType::All)
    )?;
    terminal.clear()?;
    Ok(())
}

fn path_label(path: &std::path::Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project")
        .to_owned()
}
