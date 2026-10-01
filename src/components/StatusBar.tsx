import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { toast } from "sonner";

import { FolderIcon } from "@/components/icons";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";

/** 版权年份（版本发布时更新） */
const COPYRIGHT_YEAR = 2026;

interface StatusBarProps {
  total: number;
  running: number;
}

/** 「共 N 条隧道 · ●运行中 N · ●已停止 N」——左侧统计 */
function Counts({ total, running }: StatusBarProps) {
  return (
    <div className="flex shrink-0 items-center gap-4">
      <span>
        共 <b className="font-semibold text-foreground">{total}</b> 条隧道
      </span>
      <span className="flex items-center gap-1.5">
        <span className="h-1.5 w-1.5 rounded-full bg-emerald-500" />
        运行中 <b className="font-semibold text-foreground">{running}</b>
      </span>
      <span className="flex items-center gap-1.5">
        <span className="h-1.5 w-1.5 rounded-full bg-slate-400" />
        已停止 <b className="font-semibold text-foreground">{total - running}</b>
      </span>
    </div>
  );
}

/**
 * 底部信息栏。
 *
 * 只保留两类真正有信息量的内容：左侧隧道计数，右侧版本号 / 版权 / 数据目录入口。
 * 路径不再整行铺开（长路径会把这一行撑满且无法点击），改为按钮 + 悬停提示；
 * 「每 3 秒自动刷新」「关闭窗口将最小化到托盘」这类静态说明已移除——
 * 前者对使用没有帮助，后者在窗口关闭按钮的 tooltip 里已有说明。
 */
export function StatusBar({ total, running }: StatusBarProps) {
  const [version, setVersion] = useState("");
  const [dataDir, setDataDir] = useState("");
  const [revealing, setRevealing] = useState(false);

  // 版本号来自 tauri.conf.json（构建时写入，release 与 dev 都可用）
  useEffect(() => {
    getVersion()
      .then(setVersion)
      .catch(() => setVersion(""));
  }, []);

  // 数据目录路径只用于按钮的悬停提示，取不到就不显示提示
  useEffect(() => {
    api
      .dataDir()
      .then(setDataDir)
      .catch(() => setDataDir(""));
  }, []);

  const openDataDir = async () => {
    setRevealing(true);
    try {
      await api.revealDataDir();
    } catch (e) {
      toast.error("无法打开数据目录", { description: String(e) });
    } finally {
      setRevealing(false);
    }
  };

  return (
    <footer className="flex shrink-0 items-center justify-between gap-4 text-xs text-muted-foreground">
      <Counts total={total} running={running} />

      <div className="flex min-w-0 shrink-0 items-center gap-3">
        {version && (
          <span className="whitespace-nowrap font-mono" title={`SSH 隧道管理器 v${version}`}>
            v{version}
          </span>
        )}

        <span className="whitespace-nowrap border-l border-border pl-3">
          © {COPYRIGHT_YEAR} hmilyld.com
        </span>

        <Button
          variant="ghost"
          size="icon-sm"
          onClick={() => void openDataDir()}
          disabled={revealing}
          aria-label="打开数据目录"
          title={dataDir ? `打开数据目录：${dataDir}` : "打开数据目录"}
        >
          <FolderIcon className="h-3.5 w-3.5" />
        </Button>
      </div>
    </footer>
  );
}
