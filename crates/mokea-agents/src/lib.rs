use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use mokea_core::AgentKind;
use std::{ffi::OsStr, path::Path, process::Stdio};
use tokio::{io::{AsyncBufReadExt, BufReader}, process::Command};

#[derive(Clone, Debug)]
pub struct AgentInfo {
    pub kind: AgentKind,
    pub executable: String,
    pub installed: bool,
    pub version: Option<String>,
}

#[async_trait]
pub trait AgentAdapter: Send + Sync {
    fn kind(&self) -> AgentKind;
    fn executable(&self) -> &str;
    fn command(&self, prompt: &str) -> Command;

    async fn inspect(&self) -> AgentInfo {
        let result = Command::new(self.executable())
            .arg("--version")
            .output()
            .await;
        match result {
            Ok(output) if output.status.success() => AgentInfo {
                kind: self.kind(),
                executable: self.executable().to_owned(),
                installed: true,
                version: Some(String::from_utf8_lossy(&output.stdout).trim().to_owned()),
            },
            _ => AgentInfo {
                kind: self.kind(),
                executable: self.executable().to_owned(),
                installed: false,
                version: None,
            },
        }
    }

    async fn run(&self, prompt: &str, project_dir: &Path, json_output: bool) -> Result<i32> {
        let mut command = self.command(prompt);
        command
            .current_dir(project_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().with_context(|| {
            format!("Could not start {}. Is it installed and on PATH?", self.kind())
        })?;

        let stdout = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take().expect("stderr is piped");
        let out_task = tokio::spawn(stream_lines(stdout, "stdout", json_output));
        let err_task = tokio::spawn(stream_lines(stderr, "stderr", json_output));

        tokio::select! {
            status = child.wait() => {
                out_task.await??;
                err_task.await??;
                let status = status.context("Agent process exited without a status")?;
                Ok(status.code().unwrap_or(1))
            }
            _ = tokio::signal::ctrl_c() => {
                let _ = child.kill().await;
                out_task.abort();
                err_task.abort();
                bail!("Run cancelled (Ctrl-C)")
            }
        }
    }
}

async fn stream_lines<R>(reader: R, stream: &'static str, json_output: bool) -> Result<()>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines.next_line().await? {
        if json_output {
            println!("{}", serde_json::json!({"event":"run.output","stream":stream,"text":line}));
        } else if stream == "stderr" {
            println!("! {line}");
        } else {
            println!("{line}");
        }
    }
    Ok(())
}

pub struct CodexAdapter { executable: String }
pub struct ClaudeAdapter { executable: String }

impl CodexAdapter {
    pub fn new(executable: impl Into<String>) -> Self { Self { executable: executable.into() } }
}
impl ClaudeAdapter {
    pub fn new(executable: impl Into<String>) -> Self { Self { executable: executable.into() } }
}

#[async_trait]
impl AgentAdapter for CodexAdapter {
    fn kind(&self) -> AgentKind { AgentKind::Codex }
    fn executable(&self) -> &str { &self.executable }
    fn command(&self, prompt: &str) -> Command {
        let mut command = Command::new(&self.executable);
        command.args([OsStr::new("exec"), OsStr::new(prompt)]);
        command
    }
}

#[async_trait]
impl AgentAdapter for ClaudeAdapter {
    fn kind(&self) -> AgentKind { AgentKind::Claude }
    fn executable(&self) -> &str { &self.executable }
    fn command(&self, prompt: &str) -> Command {
        let mut command = Command::new(&self.executable);
        command.args([OsStr::new("-p"), OsStr::new(prompt)]);
        command
    }
}
