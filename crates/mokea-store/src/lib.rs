//! Local run history boundary. SQLite persistence is introduced with the history milestone.

use anyhow::{Result, bail};
use mokea_core::RunRecord;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct RunStore {
    pub database_path: PathBuf,
}

impl RunStore {
    pub fn local() -> Result<Self> {
        let data_dir = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            })
            .ok_or_else(|| anyhow::anyhow!("Could not determine a local data directory"))?;
        Ok(Self {
            database_path: data_dir.join("mokea/runs.sqlite3"),
        })
    }

    pub fn list(&self) -> Result<Vec<RunRecord>> {
        bail!(
            "Run history is not wired up yet. The local store is reserved for the SQLite milestone."
        )
    }
}
