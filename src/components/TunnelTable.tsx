import {
  PlayIcon,
  PowerIcon,
  PlusIcon,
  ServerIcon,
  SpinnerIcon,
  TrashIcon,
} from "@/components/icons";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import type { Tunnel } from "@/lib/api";
import { cn } from "@/lib/utils";

interface TunnelTableProps {
  tunnels: Tunnel[];
  loading: boolean;
  /** 正在执行操作的隧道 ID（用于禁用按钮） */
  busyId: string | null;
  /** 关闭（终止进程，保留记录） */
  onStop: (tunnel: Tunnel) => void;
  /** 启动（复用已保存的记录重新拉起 ssh） */
  onStart: (tunnel: Tunnel) => void;
  /** 删除（仅已停止的记录） */
  onDelete: (tunnel: Tunnel) => void;
  onNew: () => void;
}

function StatusBadge({ alive }: { alive: boolean }) {
  return alive ? (
    <Badge variant="success">
      <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-emerald-500" />
      运行中
    </Badge>
  ) : (
    <Badge variant="secondary">
      <span className="h-1.5 w-1.5 rounded-full bg-slate-400" />
      已停止
    </Badge>
  );
}

/** 骨架屏行 */
function SkeletonRows() {
  return (
    <>
      {[0, 1, 2].map((i) => (
        <TableRow key={i}>
          {Array.from({ length: 8 }).map((_, j) => (
            <TableCell key={j}>
              <div className="h-4 animate-pulse rounded bg-muted" style={{ width: `${45 + ((i + j) % 4) * 15}%` }} />
            </TableCell>
          ))}
        </TableRow>
      ))}
    </>
  );
}

export function TunnelTable({
  tunnels,
  loading,
  busyId,
  onStop,
  onStart,
  onDelete,
  onNew,
}: TunnelTableProps) {
  if (!loading && tunnels.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-4 p-10 text-center">
        <div className="flex h-14 w-14 items-center justify-center rounded-2xl bg-muted text-muted-foreground">
          <ServerIcon className="h-7 w-7" />
        </div>
        <div className="space-y-1">
          <p className="text-sm font-medium">还没有任何隧道</p>
          <p className="max-w-sm text-xs text-muted-foreground">
            点击右上角「新建转发」，选择 ~/.ssh/config 中的 Host，把远程服务映射到本地端口。
          </p>
        </div>
        <Button variant="outline" onClick={onNew}>
          <PlusIcon className="h-4 w-4" />
          新建转发
        </Button>
      </div>
    );
  }

  return (
    <Table>
      <TableHeader className="sticky top-0 z-10 bg-card/95 backdrop-blur">
        <TableRow className="hover:bg-transparent">
          <TableHead className="w-[90px]">ID</TableHead>
          <TableHead className="w-[110px]">Host</TableHead>
          <TableHead>本地地址:端口</TableHead>
          <TableHead>远程地址:端口</TableHead>
          <TableHead className="w-[90px]">PID</TableHead>
          <TableHead className="w-[150px]">创建时间</TableHead>
          <TableHead className="w-[104px]">状态</TableHead>
          <TableHead className="w-[210px] text-right">操作</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {loading ? (
          <SkeletonRows />
        ) : (
          tunnels.map((t) => {
            const busy = busyId === t.id;
            return (
              <TableRow key={t.id}>
                <TableCell>
                  <code className="rounded bg-muted px-1.5 py-0.5 text-xs font-medium">
                    {t.id}
                  </code>
                </TableCell>
                <TableCell>
                  <span className="font-medium">{t.host}</span>
                </TableCell>
                <TableCell>
                  <code className="text-xs">
                    {t.bind}:{t.local_port}
                  </code>
                  {t.bind === "0.0.0.0" && (
                    <span className="ml-2 rounded bg-amber-500/10 px-1 py-0.5 text-[10px] text-amber-600 dark:text-amber-400">
                      局域网
                    </span>
                  )}
                </TableCell>
                <TableCell>
                  <code className="text-xs">
                    {t.remote_host}:{t.remote_port}
                  </code>
                </TableCell>
                <TableCell>
                  <span
                    className={cn(
                      "font-mono text-xs",
                      !t.alive && "text-muted-foreground/60",
                      t.pid > 0 && !t.alive && "line-through",
                    )}
                  >
                    {t.pid > 0 ? t.pid : "—"}
                  </span>
                </TableCell>
                <TableCell className="text-xs text-muted-foreground">{t.created_at}</TableCell>
                <TableCell>
                  <StatusBadge alive={t.alive} />
                </TableCell>
                <TableCell className="text-right">
                  {t.alive ? (
                    <Button
                      variant="outline"
                      size="sm"
                      className="border-destructive/30 text-destructive hover:bg-destructive/10 hover:text-destructive"
                      disabled={busy}
                      onClick={() => onStop(t)}
                      title="关闭隧道（保留记录，之后可随时重新启动）"
                    >
                      {busy ? (
                        <SpinnerIcon className="h-3.5 w-3.5 animate-spin" />
                      ) : (
                        <PowerIcon className="h-3.5 w-3.5" />
                      )}
                      关闭
                    </Button>
                  ) : (
                    <div className="flex justify-end gap-1.5">
                      <Button
                        variant="outline"
                        size="sm"
                        className="border-emerald-600/30 text-emerald-700 hover:bg-emerald-600/10 hover:text-emerald-700 dark:text-emerald-400 dark:hover:text-emerald-400"
                        disabled={busy}
                        onClick={() => onStart(t)}
                        title="用保存的配置重新启动这条转发"
                      >
                        {busy ? (
                          <SpinnerIcon className="h-3.5 w-3.5 animate-spin" />
                        ) : (
                          <PlayIcon className="h-3.5 w-3.5" />
                        )}
                        启动
                      </Button>
                      <Button
                        variant="ghost"
                        size="sm"
                        className="text-muted-foreground"
                        disabled={busy}
                        onClick={() => onDelete(t)}
                        title="从列表中删除这条记录"
                      >
                        <TrashIcon className="h-3.5 w-3.5" />
                        删除
                      </Button>
                    </div>
                  )}
                </TableCell>
              </TableRow>
            );
          })
        )}
      </TableBody>
    </Table>
  );
}
