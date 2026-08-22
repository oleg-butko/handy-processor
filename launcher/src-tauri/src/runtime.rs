use std::{fs, path::PathBuf};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

const DEFAULT_PORT: u16 = 11_434;
const CONFIG_FILE_NAME: &str = "config.json";
const SIDECAR_NAME: &str = "handy-processor-server";

#[derive(Debug, Deserialize)]
struct ConfigFile {
    port: u16,
}

#[derive(Clone, Debug)]
pub struct RuntimePaths {
    pub directory: PathBuf,
    pub sidecar: PathBuf,
    pub port: u16,
}

impl RuntimePaths {
    pub fn discover() -> Result<Self> {
        let executable = std::env::current_exe().context("cannot locate launcher executable")?;
        let directory = executable
            .parent()
            .context("launcher executable has no parent directory")?
            .to_path_buf();
        let sidecar = directory.join(format!("{SIDECAR_NAME}{}", std::env::consts::EXE_SUFFIX));
        let port = load_port(&directory)?;

        Ok(Self {
            directory,
            sidecar,
            port,
        })
    }

    pub fn web_url(&self) -> String {
        format!("http://127.0.0.1:{}/", self.port)
    }

    pub fn api_url(&self) -> String {
        format!("http://127.0.0.1:{}/v1", self.port)
    }
}

fn load_port(directory: &std::path::Path) -> Result<u16> {
    let config_path = directory.join(CONFIG_FILE_NAME);
    if !config_path.exists() {
        return Ok(DEFAULT_PORT);
    }

    let contents = fs::read_to_string(&config_path)
        .with_context(|| format!("cannot read {}", config_path.display()))?;
    let config: ConfigFile = serde_json::from_str(&contents)
        .with_context(|| format!("invalid JSON in {}", config_path.display()))?;
    if config.port == 0 {
        bail!("{}: port must be from 1 to 65535", config_path.display());
    }

    Ok(config.port)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_urls_share_the_configured_port() {
        let paths = RuntimePaths {
            directory: PathBuf::from("data"),
            sidecar: PathBuf::from("server.exe"),
            port: 12_345,
        };

        assert_eq!(paths.web_url(), "http://127.0.0.1:12345/");
        assert_eq!(paths.api_url(), "http://127.0.0.1:12345/v1");
    }
}
