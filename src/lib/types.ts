/**
 * 前端 DTO 类型定义 — 严格镜像 src-tauri/src/commands.rs 中 #[derive(Serialize)] DTO
 *
 * 规则（v5-6 外部接口 try-coerce）：
 * - 所有来自 invoke 的响应类型在此定义，前端组件 import 此处，不自定义内联类型。
 * - backend 若返回字段缺失，各处用 serde(default) → 前端对应字段做可选标注。
 */

// ---------------------------------------------------------------------------
// StatusInfo（get_status → floating-window 顶部状态栏）
// ---------------------------------------------------------------------------

export interface StatusInfo {
  /** 本机 device_id（UUID） */
  my_device_id: string;
  /** 监听地址 "ip:port"（底部 footer 展示用） */
  listen_addr: string;
  /** 当前已注册的 peer 总数 */
  peer_count: number;
  /** Approved peer 数量（状态点 + "小组 · N 台"）*/
  approved_count: number;
  /** Banned peer 数量 */
  banned_count: number;
}

// ---------------------------------------------------------------------------
// PeerInfo（get_peers → peer 列表 / group-approval 弹框）
// ---------------------------------------------------------------------------

export interface PeerInfo {
  device_id: string;
  addr: string;
  device_name: string;
  /** "approved" | "banned" | "pending" */
  trust_state: "approved" | "banned" | "pending";
  /** 相对时间字符串（"3 分钟前"）；None = 从未同步 */
  last_successful_sync_at: string | null;
}

// ---------------------------------------------------------------------------
// ConfigInfo（get_config → settings-panel 首屏）
// ---------------------------------------------------------------------------

export interface ConfigInfo {
  device_name: string;
  listen_port: number;
  /** 上次成功 join 的地址（join 对话框 placeholder） */
  peer_hint: string | null;
}

// ---------------------------------------------------------------------------
// ConfigPayload（set_config 参数）
// ---------------------------------------------------------------------------

export interface ConfigPayload {
  device_name?: string;
  listen_port?: number;
}

// ---------------------------------------------------------------------------
// HistoryItem（get_history → history-list spec 第 3 节）
// ---------------------------------------------------------------------------

/** 来源 — local 或 remote */
export type HistorySource =
  | { kind: "local" }
  | { kind: "remote"; device_name: string };

/** 内容载荷 — text / image / file */
export type HistoryPayload =
  | { type: "text"; text: string }
  | { type: "image"; width: number; height: number; data_url: string }
  | {
      type: "file";
      filename: string;
      size: number;
      saved_path: string | null;
      file_status: string;
      error: string | null;
    };

export interface HistoryItem {
  id: string;
  timestamp_ms: number;
  /** source 是 serde_json::Value，用 unknown 再手动 cast */
  source: HistorySource;
  content_hash: string | null;
  /** payload 是 serde_json::Value */
  payload: HistoryPayload;
}

// ---------------------------------------------------------------------------
// PeerPendingPayload（peer-pending 事件 payload）
// ---------------------------------------------------------------------------

export interface PeerPendingPayload {
  request_id: string;
  subject_device_id: string;
  subject_device_name: string;
  subject_ip: string;
  timestamp_ms: number;
}

// ---------------------------------------------------------------------------
// IpcError — 统一错误类
// ---------------------------------------------------------------------------

export type IpcErrorCode =
  | "forbidden"
  | "not_found"
  | "invalid_input"
  | "internal_error"
  | "rate_limited"
  | "unknown";

export class IpcError extends Error {
  readonly code: IpcErrorCode;

  constructor(code: IpcErrorCode, message?: string) {
    super(message ?? code);
    this.name = "IpcError";
    this.code = code;
  }
}

/** backend 返回的 string 映射到 IpcErrorCode */
export function toIpcErrorCode(raw: unknown): IpcErrorCode {
  switch (raw) {
    case "forbidden":      return "forbidden";
    case "not_found":      return "not_found";
    case "invalid_input":  return "invalid_input";
    case "internal_error": return "internal_error";
    case "rate_limited":   return "rate_limited";
    default:               return "unknown";
  }
}
