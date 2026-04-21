use serde::{Deserialize, Serialize};

/// 握手请求。没有密码字段——身份验证靠对方点「同意」
#[derive(Debug, Serialize, Deserialize)]
pub struct HandshakeReq {
    pub device_id: String,
    pub device_name: String,
    /// 本机 HTTP 服务端口；IP 由服务端从 TCP 连接信息提取
    pub listen_port: u16,
}

/// 公开的 peer 信息（用于 gossip）
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
    /// 当前节点已知的其它 peer 列表。新节点收到后会自动去握手这些
    /// 以形成完整 mesh：连上一个 == 连上整个组
    #[serde(default)]
    pub peers: Vec<PeerPublic>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClipboardReq {
    pub origin_device_id: String,
    pub origin_device_name: String,
    pub seq: u64,
    pub text: String,
}
