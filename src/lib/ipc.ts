/**
 * IPC 封装层 — 所有 invoke 命令的类型化 wrapper
 *
 * 规则（v5-6 外部接口 try-coerce）：
 * - 每个 wrapper 显式 try/catch + 类型守卫
 * - 错误统一 throw IpcError（含 code 字段）
 * - 命令名严格从 src-tauri/src/lib.rs invoke_handler 来，不在前端发明
 *
 * 已注册命令（grep src-tauri/src/lib.rs 验证）：
 *   get_status / get_peers / join_group / get_config / set_config /
 *   approve_peer / reject_peer / get_history / delete_history_item /
 *   clear_history / recopy_history_item / quit_app
 */

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  IpcError,
  toIpcErrorCode,
  type ConfigInfo,
  type ConfigPayload,
  type HistoryItem,
  type PeerInfo,
  type PeerPendingPayload,
  type StatusInfo,
} from "./types";

// ---------------------------------------------------------------------------
// 内部工具：统一 catch → IpcError
// ---------------------------------------------------------------------------

function wrapError(e: unknown): IpcError {
  if (e instanceof IpcError) return e;
  if (typeof e === "string") return new IpcError(toIpcErrorCode(e), e);
  if (e instanceof Error) return new IpcError("unknown", e.message);
  return new IpcError("unknown", String(e));
}

// ---------------------------------------------------------------------------
// 状态 / 连接
// ---------------------------------------------------------------------------

/** 返回当前状态概览（floating-window 顶部状态栏数据来源）。*/
export async function getStatus(): Promise<StatusInfo> {
  try {
    return await invoke<StatusInfo>("get_status");
  } catch (e) {
    throw wrapError(e);
  }
}

/** 返回所有已注册 peer 列表。*/
export async function getPeers(): Promise<PeerInfo[]> {
  try {
    return await invoke<PeerInfo[]>("get_peers");
  } catch (e) {
    throw wrapError(e);
  }
}

/** 向目标地址发起握手加入小组（group-discovery spec 第 3 节）。*/
export async function joinGroup(targetAddr: string): Promise<void> {
  try {
    await invoke<void>("join_group", { targetAddr });
  } catch (e) {
    throw wrapError(e);
  }
}

// ---------------------------------------------------------------------------
// 配置
// ---------------------------------------------------------------------------

/** 返回当前持久化配置（settings-panel 首屏用）。*/
export async function getConfig(): Promise<ConfigInfo> {
  try {
    return await invoke<ConfigInfo>("get_config");
  } catch (e) {
    throw wrapError(e);
  }
}

/** 保存设备名等配置（settings-panel 保存按钮触发）。*/
export async function setConfig(cfg: ConfigPayload): Promise<void> {
  try {
    await invoke<void>("set_config", { cfg });
  } catch (e) {
    throw wrapError(e);
  }
}

// ---------------------------------------------------------------------------
// 审批
// ---------------------------------------------------------------------------

/** 同意某 peer 加入请求（group-approval 弹框 approve 按钮）。*/
export async function approvePeer(deviceId: string): Promise<void> {
  try {
    await invoke<void>("approve_peer", { deviceId });
  } catch (e) {
    throw wrapError(e);
  }
}

/** 拒绝某 peer 加入请求（group-approval 弹框 reject 按钮）。*/
export async function rejectPeer(deviceId: string): Promise<void> {
  try {
    await invoke<void>("reject_peer", { deviceId });
  } catch (e) {
    throw wrapError(e);
  }
}

// ---------------------------------------------------------------------------
// 历史
// ---------------------------------------------------------------------------

/** 返回当前历史列表（history-list spec 第 3 节）。*/
export async function getHistory(): Promise<HistoryItem[]> {
  try {
    return await invoke<HistoryItem[]>("get_history");
  } catch (e) {
    throw wrapError(e);
  }
}

/** 删除单条历史（history-list spec 第 3 节 单条删除）。*/
export async function deleteHistoryItem(id: string): Promise<void> {
  try {
    await invoke<void>("delete_history_item", { id });
  } catch (e) {
    throw wrapError(e);
  }
}

/** 清空所有历史（settings-panel 清除历史按钮触发）。*/
export async function clearHistory(): Promise<void> {
  try {
    await invoke<void>("clear_history");
  } catch (e) {
    throw wrapError(e);
  }
}

/** 把历史条目重新写入剪切板（history-list spec 第 3 节 单击复制）。*/
export async function recopyHistoryItem(id: string): Promise<void> {
  try {
    await invoke<void>("recopy_history_item", { id });
  } catch (e) {
    throw wrapError(e);
  }
}

// ---------------------------------------------------------------------------
// 应用生命周期
// ---------------------------------------------------------------------------

/** 退出应用（唯一退出路径；ADR-003 第 3.5 节）。*/
export async function quitApp(): Promise<void> {
  try {
    await invoke<void>("quit_app");
  } catch (e) {
    throw wrapError(e);
  }
}

// ---------------------------------------------------------------------------
// 事件订阅 helper（含 unlisten 返回，调用方负责在组件卸载时调用）
// ---------------------------------------------------------------------------

/**
 * 订阅 status-updated 事件。
 * backend 在 join_group / approve_peer / reject_peer 成功后 emit。
 * 返回 UnlistenFn，须在组件 onDestroy / $effect cleanup 中调用。
 */
export async function onStatusUpdated(cb: () => void): Promise<UnlistenFn> {
  return listen<void>("status-updated", () => cb());
}

/**
 * 订阅 history-updated 事件。
 * backend 在 delete_history_item / clear_history / 新增条目时 emit。
 */
export async function onHistoryUpdated(cb: () => void): Promise<UnlistenFn> {
  return listen<void>("history-updated", () => cb());
}

/**
 * 订阅 peer-pending 事件（group-approval 弹框触发）。
 * payload 含 request_id / subject_device_id / subject_device_name / subject_ip / timestamp_ms。
 * PR-FE-2 将基于此实现完整审批弹框；本批仅订阅。
 */
export async function onPeerPending(
  cb: (payload: PeerPendingPayload) => void
): Promise<UnlistenFn> {
  return listen<PeerPendingPayload>("peer-pending", (event) => cb(event.payload));
}
