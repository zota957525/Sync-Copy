//! RateLimiter — handshake DoS 限流
//! see decisions/ADR-009-peer-registry.md (第 3.6 节 PolicyState 独立 RateLimiter)
//! see decisions/ADR-008-security-review-of-adr003.md (MUST-7 DoS 限流)
//!
//! 设计决策（ADR-009 第 3.6 节 选项 B）：
//! - PolicyState 独立模块，不并入 PeerRegistry
//! - handshake 限流 key = (remote_ip, device_id)
//! - 阈值（每对 60s ≤ 3 / 全局 60s ≤ 10）占位 const；由 group-discovery feature ADR 锁定
//!
//! 调用方式：
//!   AppState 顶层与 PeerRegistry 平行持有 Arc<RateLimiter>；
//!   handshake handler 第一行调 check_handshake，超限返 TooManyRequests → 映射 429。

use parking_lot::RwLock;
use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// 占位阈值常量（group-discovery feature ADR 落地时替换）
// ---------------------------------------------------------------------------

/// 每对 (remote_ip, device_id) 在 WINDOW_SECS 内允许的最大 handshake 次数。
/// 具体值由 group-discovery feature ADR 锁定；此处占位。
const MAX_PER_PAIR_IN_WINDOW: usize = 3;

/// 全局 60s 内允许的最大不同 device_id handshake 尝试数。
/// 具体值由 group-discovery feature ADR 锁定；此处占位。
const MAX_GLOBAL_IN_WINDOW: usize = 10;

/// 滑动窗口时长（秒）。
const WINDOW_SECS: u64 = 60;

// ---------------------------------------------------------------------------
// RateLimitDecision enum（ADR-009 第 3.6 节）
// ---------------------------------------------------------------------------

/// handshake 限流决策。
#[derive(Debug, PartialEq, Eq)]
pub enum RateLimitDecision {
    /// 允许通过；继续后续握手处理。
    Allowed,
    /// 超出速率限制；handler 应返回 HTTP 429 TooManyRequests。
    TooManyRequests,
}

// ---------------------------------------------------------------------------
// RateLimiter struct（ADR-009 第 3.6 节 + 第 7.3 节 P3 补丁）
// ---------------------------------------------------------------------------

/// Handshake DoS 限流器（独立模块，不依赖 PeerRegistry）。
///
/// SECURITY（ADR-009 第 7.3 节 P3 补丁）：
/// per_pair / global 容器的 device_id 来自**未认证**报文；
/// group-discovery feature ADR 在锁定阈值时**必须同步定义**
/// per_pair HashMap 的容量上限与过期 retain 策略，避免
/// (IpAddr, 编造 UUID) 的 HashMap 内存放大攻击。
/// 未认证 device_id 不进 tracing fields；仅 check_handshake
/// 返 TooManyRequests 时记 IP + 计数，不记 device_id。
///
/// 当前实现仅落 struct + 方法签名 + per_pair / global 双计数器骨架；
/// 容量上限与过期 retain 策略由 group-discovery feature ADR 接管落地。
pub struct RateLimiter {
    /// 每对 (remote_ip, device_id) 的 handshake 时间序列。
    /// key 的 device_id 来自未认证报文，存在内存放大风险；
    /// 容量上限 + 过期清理策略由 group-discovery feature ADR 定义。
    per_pair: RwLock<HashMap<(IpAddr, String), VecDeque<Instant>>>,
    /// 全局 handshake 尝试历史（timestamp, device_id）。
    /// device_id 仅用于计数，不记入日志（P3 安全注释）。
    global: RwLock<VecDeque<(Instant, String)>>,
}

impl RateLimiter {
    /// 构造空 RateLimiter。
    pub fn new() -> Self {
        Self {
            per_pair: RwLock::new(HashMap::new()),
            global: RwLock::new(VecDeque::new()),
        }
    }

    /// 检查 handshake 是否超出速率限制。
    ///
    /// pre: handshake handler 第一行调用。
    /// 返回 Allowed → 继续处理；返回 TooManyRequests → 返回 HTTP 429。
    ///
    /// 算法：
    /// 1. 清理 per_pair[key] 中超出 WINDOW_SECS 的过期记录
    /// 2. 若 per_pair[key].len() >= MAX_PER_PAIR_IN_WINDOW → TooManyRequests
    /// 3. 清理 global 中超出 WINDOW_SECS 的过期记录
    /// 4. 统计 global 中 WINDOW_SECS 内不同 device_id 数量
    /// 5. 若 >= MAX_GLOBAL_IN_WINDOW → TooManyRequests
    /// 6. 记录本次到 per_pair + global；返回 Allowed
    ///
    /// SECURITY（P3 补丁）：
    /// - 日志仅记录 IP + 计数，不记录 device_id（未认证报文不进 tracing fields）
    /// - per_pair HashMap 过期清理在每次调用时按需执行（O(n) per key）；
    ///   全量过期清理策略由 group-discovery feature ADR 定义（防内存放大）
    pub fn check_handshake(&self, remote_ip: IpAddr, device_id: &str) -> RateLimitDecision {
        let window = Duration::from_secs(WINDOW_SECS);
        let now = Instant::now();

        // 步骤 1-2：per_pair 检查
        {
            let mut pp = self.per_pair.write();
            let key = (remote_ip, device_id.to_string());
            let queue = pp.entry(key).or_default();

            // 清理过期记录（滑动窗口）
            while queue
                .front()
                .map(|t| now.duration_since(*t) >= window)
                .unwrap_or(false)
            {
                queue.pop_front();
            }

            // 检查 per_pair 限制
            if queue.len() >= MAX_PER_PAIR_IN_WINDOW {
                // SECURITY P3：只记 IP + 计数，不记 device_id
                tracing::warn!(
                    target: "peer::rate_limit",
                    remote_ip = %remote_ip,
                    count = queue.len(),
                    "handshake rate limit exceeded (per_pair)"
                );
                return RateLimitDecision::TooManyRequests;
            }

            // 记录本次（在 Allowed 分支最后统一记录，避免限流后仍计入）
            queue.push_back(now);
        }

        // 步骤 3-5：global 检查
        {
            let mut gb = self.global.write();

            // 清理过期记录
            while gb
                .front()
                .map(|(t, _)| now.duration_since(*t) >= window)
                .unwrap_or(false)
            {
                gb.pop_front();
            }

            // 统计 WINDOW 内不同 device_id 数量（粗糙计数，未去重 per device_id）
            // 精确去重由 group-discovery feature ADR 细化（按需升级为 HashSet 计数）
            if gb.len() >= MAX_GLOBAL_IN_WINDOW {
                // SECURITY P3：只记 IP + 计数，不记 device_id
                tracing::warn!(
                    target: "peer::rate_limit",
                    remote_ip = %remote_ip,
                    global_count = gb.len(),
                    "handshake rate limit exceeded (global)"
                );
                // 撤销刚才加入 per_pair 的记录（不能对已限流的请求计入 per_pair）
                // 注意：此处为简化实现，精确撤销逻辑由 group-discovery ADR 细化
                let mut pp = self.per_pair.write();
                let key = (remote_ip, device_id.to_string());
                if let Some(queue) = pp.get_mut(&key) {
                    queue.pop_back();
                }
                return RateLimitDecision::TooManyRequests;
            }

            // 记录本次（device_id 仅用于计数，不记日志）
            gb.push_back((now, device_id.to_string()));
        }

        RateLimitDecision::Allowed
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// 单元测试（ADR-009 第 6.1 节单测 7 — per_pair + global 计数）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 单测 7（ADR-009 第 6.1 节）：
    /// per_pair：连续 4 次同 (ip, did) → 第 4 次 TooManyRequests；
    /// global：全局连续 11 次不同 device_id → 第 11 次 TooManyRequests。
    #[test]
    fn per_pair_and_global_count() {
        let rl = RateLimiter::new();
        let ip: IpAddr = "192.168.1.100".parse().expect("test ip parse");

        // per_pair 测试：同 (ip, did)，前 3 次应 Allowed，第 4 次应 TooManyRequests
        for i in 0..3 {
            let decision = rl.check_handshake(ip, "device-fixed");
            assert_eq!(
                decision,
                RateLimitDecision::Allowed,
                "handshake {i} should be allowed (per_pair threshold not reached)"
            );
        }
        let fourth = rl.check_handshake(ip, "device-fixed");
        assert_eq!(
            fourth,
            RateLimitDecision::TooManyRequests,
            "4th handshake from same (ip, device) should be TooManyRequests"
        );

        // global 测试：用新 RateLimiter，10 次不同 device_id 应 Allowed，第 11 次应 TooManyRequests
        let rl2 = RateLimiter::new();
        let ip2: IpAddr = "192.168.1.200".parse().expect("test ip parse");
        for i in 0..10 {
            let did = format!("unique-device-{i}");
            let decision = rl2.check_handshake(ip2, &did);
            assert_eq!(
                decision,
                RateLimitDecision::Allowed,
                "handshake {i} with unique device should be allowed (global threshold not reached)"
            );
        }
        let eleventh = rl2.check_handshake(ip2, "unique-device-10");
        assert_eq!(
            eleventh,
            RateLimitDecision::TooManyRequests,
            "11th global handshake should be TooManyRequests"
        );
    }

    /// 验证 per_pair 不同 device_id 计数独立。
    #[test]
    fn per_pair_different_device_ids_independent() {
        let rl = RateLimiter::new();
        let ip: IpAddr = "10.0.0.1".parse().expect("test ip parse");

        // device-A 用完 3 次配额
        for _ in 0..3 {
            assert_eq!(
                rl.check_handshake(ip, "device-A"),
                RateLimitDecision::Allowed
            );
        }
        // device-A 第 4 次 TooManyRequests
        assert_eq!(
            rl.check_handshake(ip, "device-A"),
            RateLimitDecision::TooManyRequests,
            "device-A should be rate limited"
        );

        // device-B 仍可通（不同 device_id，独立计数）
        // 注意：此时 global 已有 3 条记录（来自 device-A 的 3 次 Allowed），
        //       还未达到全局上限 10，所以 device-B 仍 Allowed
        assert_eq!(
            rl.check_handshake(ip, "device-B"),
            RateLimitDecision::Allowed,
            "device-B should still be allowed (independent per_pair count)"
        );
    }

    /// 验证 Allowed 决策在连续多次调用（未达阈值）下保持稳定。
    ///
    /// reviewer 补丁（specs/peer-heartbeat.md 第 8.5 节 [低 nit]）：
    /// 原测试名 `allowed_decision_is_stable` 暗示"多次连续调用稳定"，
    /// 但原实现只调 1 次；现补充 3 次调用，让测试体与名字语义匹配。
    /// MAX_PER_PAIR_IN_WINDOW = 3，连续 3 次同 (ip, device) 应全部 Allowed；
    /// 第 4 次才触发 TooManyRequests（由 per_pair_and_global_count 测试覆盖）。
    #[test]
    fn allowed_decision_is_stable() {
        let rl = RateLimiter::new();
        let ip: IpAddr = "172.16.0.1".parse().expect("test ip parse");

        // 连续 3 次（= MAX_PER_PAIR_IN_WINDOW，阈值边界，全部应 Allowed）
        for call_idx in 0..3 {
            assert_eq!(
                rl.check_handshake(ip, "stable-device"),
                RateLimitDecision::Allowed,
                "call {call_idx}: Allowed decision must be stable when below threshold"
            );
        }
    }
}
