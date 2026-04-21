use serde::{Deserialize, Serialize};

/// 握手请求：用以宣告自己并校验密码
#[derive(Debug, Serialize, Deserialize)]
pub struct HandshakeReq {
    /// 明文密码（M4 会改成 HMAC 签名）
    pub password: String,
    pub device_id: String,
    pub device_name: String,
    /// 本机 HTTP 服务端口。IP 由服务端从 TCP 连接信息中提取
    /// （避免发起方不知道自己真实 LAN IP 的问题）
    pub listen_port: u16,
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
