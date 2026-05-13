//! Config — 应用配置持久化（设备名 / 监听端口 / peer_hint）
//! see specs/settings-panel.md (第 3 节 get_config / set_config / Config::save)
//! see decisions/ADR-003-project-architecture-skeleton.md (第 3.2 节 step 2 Config::load)
//!
//! 存储路径（spec settings-panel 第 3 节）：
//!   macOS : ~/Library/Application Support/com.synccopy.app/config.json
//!   Windows: %APPDATA%\com.synccopy.app\config\config.json
//!
//! 不持久化的字段（spec 00-product-overview 第 3 节已锁定）：
//!   peers / peer_keys / approved / banned（进程退出即清）
//!
//! PR-FE-0 范围：
//! - Config struct（device_name / listen_port / peer_hint）
//! - Config::load()  — 从 ProjectDirs 读 config.json；文件不存在用 Default + 写盘
//! - Config::save()  — async tokio::fs::write 写盘（spec 第 5.4 节 v2 应挑战 async 化）
//! - AppState::config — Arc<parking_lot::Mutex<Config>>（短持锁，不跨 await）

use anyhow::{Context, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Config struct
// ---------------------------------------------------------------------------

/// 持久化的应用配置。
///
/// 字段（spec settings-panel 第 3 节 / v0 第 5.1 节）：
/// - device_name：可由用户设置（≤ 64 字符，过滤控制字符，spec 第 4 节 AC）
/// - listen_port：监听端口（v2 P1 不开放 UI 修改，但 Config 保留字段供未来 P2 使用）
/// - peer_hint：最后一次成功加入的地址（join 对话框 placeholder 来源，spec group-discovery 第 3 节）
///
/// serde default：所有字段均有 serde(default) 保证向前兼容（v5-6 规则）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// 本机展示名称（默认 hostname，可由用户修改）。
    #[serde(default = "Config::default_device_name")]
    pub device_name: String,

    /// 监听端口（默认 5858，v2 P1 不开放 UI 修改）。
    #[serde(default = "Config::default_listen_port")]
    pub listen_port: u16,

    /// 上次成功 join 的对端地址（供 join 对话框 placeholder 用）。
    #[serde(default)]
    pub peer_hint: Option<String>,
}

impl Config {
    fn default_device_name() -> String {
        // 尝试从环境变量读 hostname（v0 沿用方案）
        std::env::var("HOSTNAME")
            .or_else(|_| std::env::var("COMPUTERNAME"))
            .unwrap_or_else(|_| "My Device".to_string())
    }

    fn default_listen_port() -> u16 {
        crate::network::DEFAULT_PORT
    }

    /// 从 ProjectDirs config.json 同步加载。
    ///
    /// 文件不存在 → 使用 Default + 异步写盘（不阻塞调用方，最终一致）。
    /// JSON 解析失败 → 同上（视为首次启动）。
    ///
    /// lifecycle step 2 调用（≤ 50ms 时间预算，sync 读；写盘 async 化在 save()）。
    pub fn load() -> Self {
        match Self::config_path() {
            None => {
                tracing::warn!(
                    target: "app::config",
                    "ProjectDirs not found, using default config"
                );
                Self::default()
            }
            Some(path) => {
                match std::fs::read_to_string(&path) {
                    Ok(content) => match serde_json::from_str::<Config>(&content) {
                        Ok(cfg) => {
                            tracing::info!(
                                target: "app::config",
                                path = %path.display(),
                                device_name = %cfg.device_name,
                                "config loaded"
                            );
                            cfg
                        }
                        Err(e) => {
                            tracing::warn!(
                                target: "app::config",
                                path = %path.display(),
                                error = %e,
                                "config parse failed, using default"
                            );
                            Self::default()
                        }
                    },
                    Err(_) => {
                        // 文件不存在（首次启动）→ 用 default + 不阻塞写盘
                        tracing::info!(
                            target: "app::config",
                            path = %path.display(),
                            "config file not found, using default"
                        );
                        Self::default()
                    }
                }
            }
        }
    }

    /// 异步写盘到 ProjectDirs config.json。
    ///
    /// 写盘失败 tracing::warn（不 fatal；用户数据不丢失，下次重启用内存值即可）。
    /// spec settings-panel 第 5.4 节 v2 要求 async 化（避免主线程阻塞）。
    pub async fn save(&self) -> Result<()> {
        let path = Self::config_path().context("ProjectDirs not found; cannot save config")?;

        // 确保目录存在
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("create config dir: {}", parent.display()))?;
        }

        let content = serde_json::to_string_pretty(self).context("serialize config to JSON")?;

        tokio::fs::write(&path, content)
            .await
            .with_context(|| format!("write config: {}", path.display()))?;

        tracing::info!(
            target: "app::config",
            path = %path.display(),
            "config saved"
        );
        Ok(())
    }

    /// 返回平台 config.json 路径（或 None 若 ProjectDirs 不可用）。
    pub fn config_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("com", "synccopy", "app")
            .map(|dirs| dirs.config_dir().join("config.json"))
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            device_name: Self::default_device_name(),
            listen_port: Self::default_listen_port(),
            peer_hint: None,
        }
    }
}

// ---------------------------------------------------------------------------
// SharedConfig — Arc<Mutex<Config>>（供 AppState 持有）
// ---------------------------------------------------------------------------

/// SharedConfig：AppState 持有的线程安全 Config。
///
/// 使用 parking_lot::Mutex（短持锁，不跨 await）。
/// 读写规则：先 lock + clone/修改，立即 drop lock，再 async save()。
pub type SharedConfig = Arc<Mutex<Config>>;

/// 构造 SharedConfig（lifecycle step 2 load + 包装）。
pub fn load_shared_config() -> SharedConfig {
    Arc::new(Mutex::new(Config::load()))
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Default 包含合法的 device_name 和 listen_port
    #[test]
    fn default_config_has_valid_fields() {
        let cfg = Config::default();
        assert!(!cfg.device_name.is_empty(), "device_name must not be empty");
        assert!(cfg.listen_port > 0, "listen_port must be > 0");
        assert!(cfg.peer_hint.is_none(), "peer_hint must be None by default");
    }

    /// serde round-trip：序列化 + 反序列化一致
    #[test]
    fn serde_round_trip() {
        let cfg = Config {
            device_name: "My Test Device".to_string(),
            listen_port: 5858,
            peer_hint: Some("192.168.1.100:5858".to_string()),
        };
        let json = serde_json::to_string(&cfg).expect("serialize");
        let decoded: Config = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded.device_name, cfg.device_name);
        assert_eq!(decoded.listen_port, cfg.listen_port);
        assert_eq!(decoded.peer_hint, cfg.peer_hint);
    }

    /// serde default：缺失字段用 default 填充（v5-6 向前兼容）
    #[test]
    fn serde_missing_fields_use_defaults() {
        // 只有 device_name，缺 listen_port 和 peer_hint
        let partial = r#"{"device_name":"MyMac"}"#;
        let cfg: Config = serde_json::from_str(partial).expect("deserialize partial");
        assert_eq!(cfg.device_name, "MyMac");
        assert_eq!(cfg.listen_port, crate::network::DEFAULT_PORT);
        assert!(cfg.peer_hint.is_none());
    }
}
