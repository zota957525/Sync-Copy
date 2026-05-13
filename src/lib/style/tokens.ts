/**
 * 视觉语言字典 — floating-window.md 第 6.5 节定义源
 *
 * 其余组件 import 此处常量，不在各自文件里重复硬编码颜色/尺寸。
 */

// ---------------------------------------------------------------------------
// 画布规格
// ---------------------------------------------------------------------------

export const WINDOW_WIDTH = 320;
export const WINDOW_HEIGHT = 420;
export const WINDOW_RADIUS = 10; // border-radius px

// ---------------------------------------------------------------------------
// 状态点颜色（第 6.5 节 状态点颜色字典）
// ---------------------------------------------------------------------------

export const COLOR_DOT_CONNECTED    = "#22c55e"; // 已连接 · 绿
export const COLOR_DOT_DISCONNECTED = "#9ca3af"; // 未连接 · 灰
export const COLOR_DOT_PENDING      = "#3b82f6"; // 等待审批 · 蓝
export const COLOR_DOT_ERROR        = "#ef4444"; // 错误 · 红

// ---------------------------------------------------------------------------
// 按钮样式
// ---------------------------------------------------------------------------

export const COLOR_BTN_PRIMARY_BG   = "#3b82f6"; // primary blue 背景
export const COLOR_BTN_PRIMARY_TEXT = "#ffffff";
export const COLOR_BTN_GHOST_BG     = "rgba(255,255,255,0.12)"; // ghost 背景
export const COLOR_BTN_GHOST_TEXT   = "#f3f4f6";
export const COLOR_BTN_DANGER_BG    = "#ef4444"; // danger red 背景
export const COLOR_BTN_DANGER_TEXT  = "#ffffff";
export const COLOR_BTN_DISABLED_BG  = "rgba(255,255,255,0.06)";
export const COLOR_BTN_DISABLED_TEXT = "#6b7280";

// ---------------------------------------------------------------------------
// 文字颜色阶梯
// ---------------------------------------------------------------------------

export const COLOR_TEXT_PRIMARY   = "#f3f4f6"; // 主文字
export const COLOR_TEXT_SECONDARY = "#9ca3af"; // 次要文字 meta/hint
export const COLOR_TEXT_BRAND     = "rgba(255,255,255,0.22)"; // brand line
export const COLOR_TEXT_DANGER    = "#ef4444"; // 危险提示
export const COLOR_TEXT_SUCCESS   = "#22c55e"; // 成功状态（"已复制"）

// ---------------------------------------------------------------------------
// 背景 / 边框
// ---------------------------------------------------------------------------

export const COLOR_WINDOW_BG      = "rgba(28,28,32,0.88)";
export const COLOR_WINDOW_BG_WIN  = "rgba(28,28,32,0.94)"; // Win fallback（无 backdrop-filter）
export const COLOR_WINDOW_BORDER  = "rgba(255,255,255,0.08)";
export const COLOR_DIVIDER        = "rgba(255,255,255,0.07)";
export const COLOR_BTN_HOVER_BG   = "rgba(255,255,255,0.12)"; // 按钮 hover 圆形背景

// ---------------------------------------------------------------------------
// 字号阶梯
// ---------------------------------------------------------------------------

export const FONT_SIZE_DEFAULT  = "13px"; // 历史条目主文字 / 按钮文字
export const FONT_SIZE_SECONDARY = "12px"; // 状态栏文字 / meta 行
export const FONT_SIZE_HINT     = "11px"; // placeholder / badge
export const FONT_SIZE_BRAND    = "9px";  // "Made with Claude · by Tao"

// ---------------------------------------------------------------------------
// 字体栈
// ---------------------------------------------------------------------------

export const FONT_FAMILY =
  '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif';

// ---------------------------------------------------------------------------
// 圆点尺寸（StatusDot 默认值）
// ---------------------------------------------------------------------------

export const DOT_SIZE_DEFAULT = 8; // px

// ---------------------------------------------------------------------------
// 间距（4px 基准网格）
// ---------------------------------------------------------------------------

export const SPACE_1 = "4px";
export const SPACE_2 = "6px";
export const SPACE_3 = "8px";
export const SPACE_4 = "10px";
export const SPACE_5 = "14px";

// 顶部状态栏
export const STATUSBAR_HEIGHT = "36px";
export const STATUSBAR_PADDING_H = "10px";

// 底部 footer
export const FOOTER_HEIGHT = "24px";
export const FOOTER_PADDING_H = "10px";

// brand line
export const BRAND_HEIGHT = "16px";
export const BRAND_PADDING_V = "4px";
