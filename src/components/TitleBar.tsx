import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";

import { MaximizeIcon, MinusIcon, RestoreIcon, TerminalIcon, XIcon } from "@/components/icons";
import { cn } from "@/lib/utils";

/**
 * 自绘标题栏（**仅 Windows**：`tauri.windows.conf.json` 关闭了 `decorations`）。
 *
 * macOS 用系统原生标题栏（红绿灯）、不需要这个组件，`App` 会按平台决定是否渲染它
 * （见 `src/lib/platform.ts::usesNativeTitleBar`）。
 *
 * 布局参考常见的桌面应用（VS Code / Windows 11 风格）：
 * - 左侧：应用图标 + 窗口标题（整条区域可拖拽移动窗口，双击最大化/还原）
 * - 右侧：最小化 / 最大化(还原) / 关闭 三个 46px 宽的方形按钮，
 *   关闭按钮悬停为 Windows 红色（#E81123 风格）
 *
 * 拖拽依赖 Tauri 的 `data-tauri-drag-region`（需 `core:window:allow-start-dragging`）；
 * 关闭按钮调用 `hide()` 直接最小化到托盘（与原生关窗行为一致）。
 */
export function TitleBar() {
  const win = getCurrentWindow();
  const [maximized, setMaximized] = useState(false);

  // 跟踪最大化状态以切换 还原 图标
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    (async () => {
      try {
        setMaximized(await win.isMaximized());
        const fn = await win.onResized(async () => {
          try {
            setMaximized(await win.isMaximized());
          } catch {
            /* 权限缺失时静默 */
          }
        });
        if (disposed) fn();
        else unlisten = fn;
      } catch {
        /* isMaximized 不可用时退化为静态图标 */
      }
    })();
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [win]);

  const onDoubleClick = (e: React.MouseEvent) => {
    if ((e.target as HTMLElement).closest("button")) return;
    void win.toggleMaximize().catch(() => {});
  };

  const controlBase =
    "flex h-full w-[46px] items-center justify-center text-foreground/75 transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-ring";

  return (
    <header
      data-tauri-drag-region
      onDoubleClick={onDoubleClick}
      className="flex h-10 shrink-0 select-none items-center justify-between border-b border-border/60 bg-background/85 pl-3"
    >
      {/* 左侧：图标 + 标题（拖拽区） */}
      <div className="flex min-w-0 items-center gap-2" data-tauri-drag-region>
        <span
          data-tauri-drag-region
          className="flex h-5 w-5 shrink-0 items-center justify-center rounded-[5px] bg-gradient-to-br from-sky-500 to-indigo-600 text-white"
        >
          <TerminalIcon className="h-3 w-3" data-tauri-drag-region />
        </span>
        <span data-tauri-drag-region className="truncate text-xs font-semibold tracking-wide text-foreground/90">
          SSH 隧道管理器
        </span>
      </div>

      {/* 右侧：窗口控制按钮（不参与拖拽） */}
      <div className="flex h-full items-center">
        <button
          type="button"
          aria-label="最小化"
          title="最小化"
          onClick={() => void win.minimize().catch(() => {})}
          className={cn(controlBase, "hover:bg-muted hover:text-foreground")}
        >
          <MinusIcon className="h-4 w-4" />
        </button>
        <button
          type="button"
          aria-label={maximized ? "还原" : "最大化"}
          title={maximized ? "还原" : "最大化"}
          onClick={() => void win.toggleMaximize().catch(() => {})}
          className={cn(controlBase, "hover:bg-muted hover:text-foreground")}
        >
          {maximized ? <RestoreIcon className="h-3.5 w-3.5" /> : <MaximizeIcon className="h-3.5 w-3.5" />}
        </button>
        <button
          type="button"
          aria-label="关闭"
          title="关闭（最小化到托盘）"
          onClick={() => void win.hide().catch(() => {})}
          className={cn(
            controlBase,
            "hover:bg-[#e81123] hover:text-white focus-visible:ring-0 active:bg-[#e81123]/80",
          )}
        >
          <XIcon className="h-4 w-4" />
        </button>
      </div>
    </header>
  );
}
