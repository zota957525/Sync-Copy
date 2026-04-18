use serde::{Deserialize, Serialize};

/// 握手请求：用以宣告自己并校验密码
#[derive(Debug, Serialize, Deserialize)]
pub struct HandshakeReq {
    /// 明文密码（M4 会改成 HMAC 签名）
    pub password: String,
    pub device_id: String,
    pub device_name: String,
    /// 对方应当如何回连我（我的 "ip:port"，ip 由调用方填 LAN IP）
    pub listen_addr: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HandshakeResp {
    pub device_id: String,
    pub device_name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClipboardReq {
    pub password: String,
    pub origin_device_id: String,
    pub origin_device_name: String,
    pub seq: u64,
    pub text: String,
}
