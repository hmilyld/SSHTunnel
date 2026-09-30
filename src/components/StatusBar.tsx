import { FileIcon } from "@/components/icons";

interface StatusBarProps {
  total: number;
  running: number;
  dataPath: string;
}

/** 底部状态栏：统计 + 提示信息 */
export function StatusBar({ total, running, dataPath }: StatusBarProps) {
  const stopped = total - running;
  return (
    <footer className="flex shrink-0 items-center justify-between gap-4 rounded-lg border border-border bg-card px-4 py-2 text-xs text-muted-foreground shadow-card">
      <div className="flex items-center gap-4">
        <span>
          共 <b className="font-semibold text-foreground">{total}</b> 条隧道
        </span>
        <span className="flex items-center gap-1.5">
          <span className="h-1.5 w-1.5 rounded-full bg-emerald-500" />
          运行中 <b className="font-semibold text-foreground">{running}</b>
        </span>
        <span className="flex items-center gap-1.5">
          <span className="h-1.5 w-1.5 rounded-full bg-slate-400" />
          已停止 <b className="font-semibold text-foreground">{stopped}</b>
        </span>
      </div>

      <div className="flex min-w-0 items-center gap-4">
        {dataPath && (
          <span className="hidden max-w-[340px] items-center gap-1 truncate md:flex" title={dataPath}>
            <FileIcon className="h-3.5 w-3.5 shrink-0" />
            <span className="truncate">{dataPath}</span>
          </span>
        )}
        <span className="whitespace-nowrap">每 3 秒自动刷新</span>
        <span className="whitespace-nowrap">关闭窗口将最小化到托盘</span>
      </div>
    </footer>
  );
}
