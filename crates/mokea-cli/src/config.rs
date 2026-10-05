use anyhow::{Context, Result, bail};
use mokea_core::AgentKind;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct AppConfig {
    pub setup_complete: bool,
    pub agents: AgentSettings,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            setup_complete: false,
            agents: AgentSettings::default(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct AgentSettings {
    pub codex: AgentConfig,
    pub claude: AgentConfig,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            codex: AgentConfig {
                name: "codex".into(),
                enabled: true,
            },
            claude: AgentConfig {
                name: "claude".into(),
                enabled: true,
            },
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct AgentConfig {
    pub name: String,
    pub enabled: bool,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            enabled: true,
        }
    }
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        let path = config_path()?;
        match fs::read_to_string(path) {
            Ok(source) => toml::from_str(&source).context("Could not parse MOKEA config"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error).context("Could not read MOKEA config"),
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path()?;
        let parent = path
            .parent()
            .context("Config path has no parent directory")?;
        fs::create_dir_all(parent).context("Could not create MOKEA config directory")?;
        let contents = toml::to_string_pretty(self).context("Could not serialize MOKEA config")?;
        let temporary = path.with_extension("toml.tmp");
        fs::write(&temporary, contents).context("Could not write MOKEA config")?;
        fs::rename(&temporary, &path).context("Could not update MOKEA config")
    }

    pub fn agent(&self, kind: AgentKind) -> &AgentConfig {
        match kind {
            AgentKind::Codex => &self.agents.codex,
            AgentKind::Claude => &self.agents.claude,
        }
    }

    pub fn agent_mut(&mut self, kind: AgentKind) -> &mut AgentConfig {
        match kind {
            AgentKind::Codex => &mut self.agents.codex,
            AgentKind::Claude => &mut self.agents.claude,
        }
    }

    pub fn validate(&self) -> Result<()> {
        for kind in AgentKind::ALL {
            let name = self.agent(kind).name.trim().trim_start_matches('@');
            if name.is_empty()
                || !name
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
            {
                bail!("Agent names can contain only letters, numbers, `_`, and `-`.");
            }
            for other in AgentKind::ALL {
                if kind != other
                    && name
                        .eq_ignore_ascii_case(self.agent(other).name.trim().trim_start_matches('@'))
                {
                    bail!("Each enabled agent needs a unique @name.");
                }
            }
        }
        Ok(())
    }
}

fn config_path() -> Result<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .context("Could not determine a local config directory")?;
    Ok(base.join("mokea/config.toml"))
}
