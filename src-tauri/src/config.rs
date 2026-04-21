use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// 本地 HTTP 服务监听端口
    pub port: u16,
    /// 本机显示名
    pub device_name: String,
    /// 首次加入使用的 peer 提示，格式如 "192.168.1.10:5858"
    pub peer_hint: Option<String>,
    /// 稳定的设备标识（首次生成并持久化，用于识别同一台机器）
    pub device_id: String,
}

impl Default for Config {
    fn default() -> Self {
        let hostname = hostname().unwrap_or_else(|| "device".to_string());
        Self {
            port: 5858,
            device_name: hostname,
            peer_hint: None,
            device_id: uuid::Uuid::new_v4().to_string(),
        }
    }
}

impl Config {
    /// 配置文件路径：~/Library/Application Support/com.synccopy.app/config.json (macOS)
    /// Windows: %APPDATA%\com.synccopy.app\config\config.json
    pub fn path() -> anyhow::Result<PathBuf> {
        let dirs = ProjectDirs::from("com", "synccopy", "app")
            .ok_or_else(|| anyhow::anyhow!("could not resolve config directory"))?;
        let dir = dirs.config_dir().to_path_buf();
        if !dir.exists() {
            fs::create_dir_all(&dir)?;
        }
        Ok(dir.join("config.json"))
    }

    pub fn load_or_default() -> Self {
        match Self::path().and_then(|p| {
            let bytes = fs::read(p)?;
            let cfg: Config = serde_json::from_slice(&bytes)?;
            Ok(cfg)
        }) {
            Ok(cfg) => cfg,
            Err(e) => {
                tracing::warn!(error = %e, "config load failed, using default");
                Self::default()
            }
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::path()?;
        let bytes = serde_json::to_vec_pretty(self)?;
        fs::write(path, bytes)?;
        Ok(())
    }
}

fn hostname() -> Option<String> {
    std::env::var("HOSTNAME")
        .ok()
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .or_else(|| {
            // 最后用 whoami 退化方案
            std::process::Command::new("hostname")
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
}
