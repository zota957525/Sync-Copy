use serde::{Deserialize, Serialize};

/// 握手请求：没有密码，身份由对方点「同意」决定
#[derive(Debug, Serialize, Deserialize)]
pub struct HandshakeReq {
    pub device_id: String,
    pub device_name: String,
    /// 本机 HTTP 服务端口；IP 由服务端从 TCP 连接信息提取
    pub listen_port: u16,
    /// 我的 X25519 临时公钥（32 字节，base64）—— 用于端到端加密密钥协商
    pub pubkey: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerPublic {
    pub device_id: String,
    pub device_name: String,
    pub addr: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HandshakeResp {
    pub device_id: String,
    pub device_name: String,
    /// 当前节点已知的其它 peer 列表（用于 gossip 形成完整 mesh）
    #[serde(default)]
    pub peers: Vec<PeerPublic>,
    /// 我的 X25519 临时公钥（32 字节，base64）
    pub pubkey: String,
}

/// 剪切板推送：内容经 AES-256-GCM 加密
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClipboardReq {
    pub origin_device_id: String,
    pub origin_device_name: String,
    pub seq: u64,
    /// 12 字节随机 nonce，base64
    pub nonce: String,
    /// AES-GCM 密文（含 tag），base64
    /// 对于文本：UTF-8 编码的字符串
    /// 对于图片：PNG 字节流
    pub ciphertext: String,
    /// "text" 或 "image_png"；老版本默认 "text"
    #[serde(default = "default_kind")]
    pub kind: String,
    /// 图片宽（仅 image_png 有值）
    #[serde(default)]
    pub image_width: Option<u32>,
    /// 图片高（仅 image_png 有值）
    #[serde(default)]
    pub image_height: Option<u32>,
}

fn default_kind() -> String {
    "text".to_string()
}

/// 文件传输请求（整份加密，5MB 上限）
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileReq {
    pub origin_device_id: String,
    pub origin_device_name: String,
    pub seq: u64,
    /// 文件名（不含路径）
    pub filename: String,
    /// 明文字节数（用于前端弹框显示大小 & 服务端校验）
    pub size: u64,
    pub nonce: String,
    pub ciphertext: String,
}

/// 同步删除某条历史（按内容 hash 跨机器识别）
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeleteHistoryReq {
    pub origin_device_id: String,
    pub seq: u64,
    /// 要删除的历史条目的 content_hash
    pub content_hash: String,
}
