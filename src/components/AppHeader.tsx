import { PlusIcon, RefreshIcon } from "@/components/icons";
import { ThemeToggle } from "@/components/ThemeToggle";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

interface AppHeaderProps {
  onNew: () => void;
  onRefresh: () => void;
  refreshing: boolean;
}

/**
 * 内容区顶栏。
 *
 * 窗口标题「SSH 隧道管理器」已由自绘标题栏展示，这里不再重复大标题与 logo，
 * 仅保留一行说明文字（左）与操作按钮（右），避免左上角信息重复。
 */
export function AppHeader({ onNew, onRefresh, refreshing }: AppHeaderProps) {
  return (
    <header className="flex shrink-0 items-center justify-between gap-4">
      <p className="min-w-0 truncate text-sm text-muted-foreground">
        管理到 Linux 服务器的{" "}
        <span className="font-mono font-medium text-foreground/80">ssh -L</span>{" "}
        本地端口转发 — 记录持久保存，关闭后可随时重新启动
      </p>

      <div className="flex shrink-0 items-center gap-2">
        <ThemeToggle />
        <Button
          variant="outline"
          onClick={onRefresh}
          disabled={refreshing}
          title="重新拉取所有隧道状态"
        >
          <RefreshIcon className={cn("h-4 w-4", refreshing && "animate-spin")} />
          刷新
        </Button>
        <Button onClick={onNew}>
          <PlusIcon className="h-4 w-4" />
          新建转发
        </Button>
      </div>
    </header>
  );
}
