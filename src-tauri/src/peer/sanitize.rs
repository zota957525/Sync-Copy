//! sanitize — 外部输入字符串净化函数
//! see decisions/ADR-008-security-review-of-adr003.md (MUST-8 第 4.4 / 4.5 节)
//! see decisions/ADR-003-project-architecture-skeleton.md (第 3.6 节 错误日志总策略)
//!
//! 本模块提供三个净化函数，所有 handler 接收外部字符串后**首动作**调用：
//!   - sanitize_device_name  — handshake / approval / gossip handler
//!   - sanitize_filename     — file handler（file-transfer-drag）
//!   - sanitize_log_field    — tracing fields 记录前（diagnostic-logging）
//!
//! 设计原则（ADR-008 第 4.4 / 4.5 节）：
//! - 后端单点 sanitize；前端 / UI 层不再做（责任归集后端）
//! - 不符则替换为安全值，不返回 403（不让攻击者枚举"哪些名字被拒"）
//! - 单元测试 ≥ 12 条（3 函数 × 4 类输入：正常/path穿越/RTL/长串）

// ---------------------------------------------------------------------------
// 常量（ADR-008 第 4.4 节 / 第 4.5 节）
// ---------------------------------------------------------------------------

/// device_name 最大 codepoints（ADR-008 第 4.4 节）
const MAX_DEVICE_NAME_CODEPOINTS: usize = 64;

/// filename 最大字节长度（ADR-008 第 4.5 节，与 v0 保持一致）
const MAX_FILENAME_BYTES: usize = 200;

/// 日志字段最大字节（截短后记录）
const MAX_LOG_FIELD_BYTES: usize = 100;

/// Windows 保留文件名前缀（大写比对）
/// ADR-008 第 4.5 节必修：basename 去 ext 后大写比对
const WINDOWS_RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM0", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
    "COM8", "COM9", "LPT0", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

// ---------------------------------------------------------------------------
// 内部辅助：字符黑名单
// ---------------------------------------------------------------------------

/// 判断字符是否属于 Bidi 控制字符黑名单。
///
/// ADR-008 第 4.4 / 4.5 节黑名单：
///   U+202A-U+202E（LRE/RLE/PDF/LRO/RLO）
///   U+2066-U+2069（LRI/RLI/FSI/PDI）
///   U+200E（LRM）、U+200F（RLM）
fn is_bidi_control(c: char) -> bool {
    matches!(c,
        '\u{202A}'..='\u{202E}'
        | '\u{2066}'..='\u{2069}'
        | '\u{200E}'
        | '\u{200F}'
    )
}

/// 判断字符是否属于控制字符黑名单。
///
/// ADR-008 第 4.4 / 4.5 节黑名单：
///   U+0000-U+001F（C0 控制字符，含 NUL / TAB / LF / CR 等）
///   U+007F（DEL）
///   U+0080-U+009F（C1 控制字符）
fn is_control_char(c: char) -> bool {
    matches!(c, '\u{0000}'..='\u{001F}' | '\u{007F}' | '\u{0080}'..='\u{009F}')
}

/// 判断字符是否属于 filename 额外禁用字符（Windows 文件名禁用字符集）。
///
/// ADR-008 第 4.5 节：过滤 `< > : " | ? *` + Windows NTFS 保留语义。
fn is_filename_forbidden_char(c: char) -> bool {
    matches!(
        c,
        '<' | '>' | ':' | '"' | '|' | '?' | '*' | '/' | '\\' | '\0'
    )
}

// ---------------------------------------------------------------------------
// 公开 API
// ---------------------------------------------------------------------------

/// 净化 device_name（handshake / approval / gossip handler 首动作调用）。
///
/// 规则（ADR-008 第 4.4 节）：
/// 1. 过滤 Bidi 控制字符
/// 2. 过滤控制字符
/// 3. 截短到 ≤ MAX_DEVICE_NAME_CODEPOINTS（Unicode codepoints，不是字节）
/// 4. 结果为空则返回 "<unnamed>"
///
/// 注意：此处选择**过滤**而非**拒绝**，ADR-008 决议 implementer 可决定；
/// 选过滤是为了让攻击者无法通过"哪些名字被拒"枚举策略。
pub fn sanitize_device_name(s: &str) -> String {
    let filtered: String = s
        .chars()
        .filter(|c| !is_bidi_control(*c) && !is_control_char(*c))
        .take(MAX_DEVICE_NAME_CODEPOINTS)
        .collect();

    let trimmed = filtered.trim().to_string();
    if trimmed.is_empty() {
        "<unnamed>".to_string()
    } else {
        trimmed
    }
}

/// 净化 filename（file handler 首动作调用）。
///
/// 规则（ADR-008 第 4.5 节，加固 v0 `sanitize_filename`）：
/// 1. 取 basename（过滤路径穿越 / 绝对路径）— Path::file_name() 语义
/// 2. 过滤控制字符 + Bidi 控制字符 + Windows 文件名禁用字符
/// 3. 截短到 ≤ MAX_FILENAME_BYTES（字节数）
/// 4. 末尾 '.' 或 ' ' 去除（Windows 不允许）
/// 5. Windows 保留名前缀检测（basename 去 ext 后大写比对）→ 前缀 '_'
/// 6. 结果为空则返回 "file"
pub fn sanitize_filename(s: &str) -> String {
    // 步骤 1：取 basename（阻断路径穿越：`../../etc/passwd`、`C:\Windows\xxx`）
    // 先把 Windows 路径分隔符 `\` 替换为 `/`，确保 Path::file_name() 在 macOS/Linux 上
    // 也能正确取 basename（不依赖平台路径语义）。
    let normalized = s.replace('\\', "/");
    let basename = std::path::Path::new(normalized.as_str())
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(s);

    // 步骤 2：过滤控制字符 + Bidi + Windows 禁用字符
    let filtered: String = basename
        .chars()
        .filter(|c| !is_control_char(*c) && !is_bidi_control(*c) && !is_filename_forbidden_char(*c))
        .collect();

    // 步骤 3：截短到 MAX_FILENAME_BYTES（字节截断，保证 UTF-8 边界）
    let truncated = truncate_to_bytes(&filtered, MAX_FILENAME_BYTES);

    // 步骤 4：去除末尾 '.' 或 ' '（Windows 不允许）
    let trimmed = truncated.trim_end_matches(['.', ' ']).to_string();

    if trimmed.is_empty() {
        return "file".to_string();
    }

    // 步骤 5：Windows 保留名检测
    // 取去 ext 后的 stem，大写比对
    let stem = std::path::Path::new(&trimmed)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(&trimmed);

    let stem_upper = stem.to_uppercase();
    if WINDOWS_RESERVED_NAMES.contains(&stem_upper.as_str()) {
        // 前缀 '_' 使其合法
        format!("_{trimmed}")
    } else {
        trimmed
    }
}

/// 净化日志字段（tracing fields 记录前调用）。
///
/// 规则（ADR-008 第 6.2 节 + diagnostic-logging 规约）：
/// 1. 过滤 Bidi 控制字符 + 控制字符
/// 2. 截短到 ≤ MAX_LOG_FIELD_BYTES（字节数），超出加 "..."
pub fn sanitize_log_field(s: &str) -> String {
    let filtered: String = s
        .chars()
        .filter(|c| !is_bidi_control(*c) && !is_control_char(*c))
        .collect();

    if filtered.len() <= MAX_LOG_FIELD_BYTES {
        filtered
    } else {
        let truncated = truncate_to_bytes(&filtered, MAX_LOG_FIELD_BYTES);
        format!("{truncated}...")
    }
}

// ---------------------------------------------------------------------------
// 内部辅助：UTF-8 安全字节截断
// ---------------------------------------------------------------------------

/// 在 `max_bytes` 处截断字符串，保证 UTF-8 边界安全（不在多字节序列中间截断）。
fn truncate_to_bytes(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    // 从 max_bytes 位置往前找 UTF-8 边界
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

// ---------------------------------------------------------------------------
// 单元测试（ADR-008 MUST-8 / 第 10 节 — ≥ 12 条）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- sanitize_device_name ----

    /// 正常 ASCII 名称不变
    #[test]
    fn device_name_normal_ascii_unchanged() {
        assert_eq!(sanitize_device_name("Alice's Mac"), "Alice's Mac");
    }

    /// 正常 Unicode 名称（中文）不变（在字符数限制内）
    #[test]
    fn device_name_normal_unicode_unchanged() {
        let name = "张三的 MacBook";
        let result = sanitize_device_name(name);
        // 无控制字符 / Bidi，应原样保留
        assert_eq!(result, name);
    }

    /// RTL 控制字符被过滤（ADR-008 MUST-8 Bidi 黑名单）
    #[test]
    fn device_name_bidi_chars_filtered() {
        // U+202E (RIGHT-TO-LEFT OVERRIDE) must be stripped from device_name
        let rtl_override = '\u{202E}';
        let name = format!("exploit{rtl_override}gpj.exe");
        let result = sanitize_device_name(&name);
        assert!(
            !result.contains('\u{202E}'),
            "RTL override must be filtered"
        );
    }

    /// 超长字符串被截断到 64 codepoints
    #[test]
    fn device_name_truncated_to_64_codepoints() {
        let long = "a".repeat(200);
        let result = sanitize_device_name(&long);
        assert_eq!(result.chars().count(), 64);
    }

    // ---- sanitize_filename ----

    /// 正常文件名不变
    #[test]
    fn filename_normal_unchanged() {
        assert_eq!(sanitize_filename("document.pdf"), "document.pdf");
    }

    /// 路径穿越被阻断（取 basename）
    #[test]
    fn filename_path_traversal_blocked() {
        let result = sanitize_filename("../../etc/passwd");
        // 应取到 basename "passwd"
        assert_eq!(result, "passwd");
    }

    /// Windows 绝对路径被阻断
    #[test]
    fn filename_windows_absolute_path_blocked() {
        let result = sanitize_filename("C:\\Windows\\system32\\evil.exe");
        // Path::file_name 取出 "evil.exe"
        assert_eq!(result, "evil.exe");
    }

    /// Windows 保留名被前缀 '_'
    #[test]
    fn filename_windows_reserved_name_prefixed() {
        assert_eq!(sanitize_filename("CON"), "_CON");
        assert_eq!(sanitize_filename("NUL.txt"), "_NUL.txt");
        assert_eq!(sanitize_filename("COM1.exe"), "_COM1.exe");
    }

    /// RTL 控制字符被过滤
    #[test]
    fn filename_bidi_chars_filtered() {
        // U+202E (RIGHT-TO-LEFT OVERRIDE) can make "exploit<RLO>gpj.exe" display as "exploit.exe.jpg"
        let rtl_override = '\u{202E}';
        let name = format!("exploit{rtl_override}gpj.exe");
        let result = sanitize_filename(&name);
        assert!(
            !result.contains('\u{202E}'),
            "RTL override must be filtered from filename"
        );
    }

    /// 超长（≥ 8KB）被截断到 200 字节
    #[test]
    fn filename_long_truncated_to_200_bytes() {
        let long = "a".repeat(8192);
        let result = sanitize_filename(&long);
        assert!(
            result.len() <= 200,
            "filename must be truncated to 200 bytes"
        );
    }

    /// 末尾 '.' 被去除（Windows 不允许）
    #[test]
    fn filename_trailing_dot_removed() {
        assert_eq!(sanitize_filename("file."), "file");
        assert_eq!(sanitize_filename("file "), "file");
    }

    /// 空 / 只含非法字符 → 兜底 "file"
    #[test]
    fn filename_empty_fallback_to_file() {
        assert_eq!(sanitize_filename(""), "file");
        assert_eq!(sanitize_filename("\0\0\0"), "file");
    }

    // ---- sanitize_log_field ----

    /// 正常短字段不变
    #[test]
    fn log_field_normal_short_unchanged() {
        let s = "peer-123";
        assert_eq!(sanitize_log_field(s), s);
    }

    /// Bidi 控制字符被过滤
    #[test]
    fn log_field_bidi_filtered() {
        // U+202E must not appear in log fields
        let rtl_override = '\u{202E}';
        let s = format!("device{rtl_override}evil");
        let result = sanitize_log_field(&s);
        assert!(!result.contains('\u{202E}'));
    }

    /// RTL 超长（≥ 8KB）被截短并加 "..."
    #[test]
    fn log_field_long_truncated_with_ellipsis() {
        let long = "x".repeat(8192);
        let result = sanitize_log_field(&long);
        assert!(
            result.len() <= MAX_LOG_FIELD_BYTES + 3,
            "log field too long: len={}",
            result.len()
        );
        assert!(
            result.ends_with("..."),
            "truncated log field must end with '...'"
        );
    }
}
