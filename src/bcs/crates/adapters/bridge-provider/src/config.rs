use std::{net::SocketAddr, path::{Path, PathBuf}};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EngineKind { CfuseCc, CfuseCodex }

#[derive(Debug, Clone, Deserialize)]
pub struct BotConfig {
    pub provider_bot_ref: String,
    pub engine: EngineKind,
    pub model: Option<String>,
    pub cwd: PathBuf,
    pub permission_mode: Option<String>,
    pub cfuse_bin: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    pub provider_id: String,
    pub listen: SocketAddr,
    pub bcs_to_provider_token: String,
    pub bot_runtime_token: Option<String>,
    #[serde(default)]
    pub trace_dir: Option<PathBuf>,
    #[serde(rename = "bot")]
    pub bots: Vec<BotConfig>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("read config: {0}")]
    Read(#[from] std::io::Error),
    #[error("parse config: {0}")]
    Parse(#[from] toml::de::Error),
}

impl ProviderConfig {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&text)?)
    }
    pub fn bot(&self, provider_bot_ref: &str) -> Option<&BotConfig> {
        self.bots.iter().find(|b| b.provider_bot_ref == provider_bot_ref)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_provider_config_and_finds_bot() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bridge.toml");
        std::fs::write(
            &path,
            r#"
provider_id = "bridge-1"
listen = "127.0.0.1:21100"
bcs_to_provider_token = "tok-b2p"

[[bot]]
provider_bot_ref = "cc-worker"
engine = "cfuse-cc"
model = "sonnet"
cwd = "/tmp"
"#,
        )
        .unwrap();
        let cfg = ProviderConfig::load(&path).unwrap();
        assert_eq!(cfg.provider_id, "bridge-1");
        let bot = cfg.bot("cc-worker").unwrap();
        assert_eq!(bot.engine, EngineKind::CfuseCc);
        assert_eq!(bot.model.as_deref(), Some("sonnet"));
        assert!(cfg.bot("nope").is_none());
    }

    #[test]
    fn rejects_unknown_engine_kind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bridge.toml");
        std::fs::write(
            &path,
            r#"
provider_id = "bridge-1"
listen = "127.0.0.1:21100"
bcs_to_provider_token = "t"
[[bot]]
provider_bot_ref = "x"
engine = "bogus"
cwd = "/tmp"
"#,
        )
        .unwrap();
        assert!(ProviderConfig::load(&path).is_err());
    }
}
