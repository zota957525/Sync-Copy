/**
 * useBallCollapse — 浮窗 ↔ 球 折叠/展开逻辑
 *
 * floating-ball.md 第 3 节：
 *   collapseToBall: 记录当前逻辑尺寸 → setSize(BALL_SIZE_PX, BALL_SIZE_PX)
 *   expandFromBall: 还原逻辑尺寸 → setSize(w, h) → 视口校正
 *
 * 注：视口校正与 backend ensure_on_screen 双实现是已知架构债（PR-FE-3 review 第 9.2 节 [中等] 2），
 * 待 floating-ball ADR 决议后统一到 backend；本层保留为权宜实现。
 */

import { getCurrentWindow, LogicalSize, currentMonitor, PhysicalPosition } from "@tauri-apps/api/window";
import { BALL_SIZE_PX } from "$lib/style/tokens";

const DEFAULT_EXPANDED = { w: 320, h: 420 };

export function createBallCollapse() {
  let lastExpandedSize = { ...DEFAULT_EXPANDED };

  async function collapseToBall(): Promise<void> {
    const win = getCurrentWindow();
    const physical = await win.outerSize();
    const scale    = await win.scaleFactor();
    lastExpandedSize = {
      w: Math.round(physical.width  / scale),
      h: Math.round(physical.height / scale),
    };
    await win.setSize(new LogicalSize(BALL_SIZE_PX, BALL_SIZE_PX));
  }

  async function expandFromBall(): Promise<void> {
    const { w, h } = lastExpandedSize.w > 40 ? lastExpandedSize : DEFAULT_EXPANDED;
    const win = getCurrentWindow();
    await win.setSize(new LogicalSize(w, h));
    // 视口校正：确保展开后窗口完全在监视器内
    const monitor = await currentMonitor();
    if (monitor) {
      const pos   = await win.outerPosition();
      const scale = await win.scaleFactor();
      const monX  = monitor.position.x;
      const monY  = monitor.position.y;
      const winW  = w * scale;
      const winH  = h * scale;
      const maxX  = monX + monitor.size.width  - winW;
      const maxY  = monY + monitor.size.height - winH;
      const newX  = Math.max(monX, Math.min(pos.x, maxX));
      const newY  = Math.max(monY, Math.min(pos.y, maxY));
      if (newX !== pos.x || newY !== pos.y) {
        await win.setPosition(new PhysicalPosition(newX, newY));
      }
    }
  }

  return { collapseToBall, expandFromBall };
}
