use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use tokio::process::Command;

#[derive(Clone, Debug)]
pub struct ProjectContext {
    pub root: PathBuf,
    pub is_git: bool,
    pub has_uncommitted_changes: bool,
}

pub async fn discover(start: &Path) -> Result<ProjectContext> {
    let root = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(start)
        .output()
        .await
        .context("Could not run git; install Git to detect the current project")?;
    if !root.status.success() {
        return Ok(ProjectContext {
            root: start.canonicalize().unwrap_or_else(|_| start.to_path_buf()),
            is_git: false,
            has_uncommitted_changes: false,
        });
    }
    let root = PathBuf::from(String::from_utf8_lossy(&root.stdout).trim());
    let changes = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&root)
        .output()
        .await?;
    Ok(ProjectContext {
        root,
        is_git: true,
        has_uncommitted_changes: !changes.stdout.is_empty(),
    })
}

pub fn require_git(project: &ProjectContext) -> Result<()> {
    if !project.is_git {
        bail!("This command needs a Git repository. Run it from inside a repository.");
    }
    Ok(())
}
