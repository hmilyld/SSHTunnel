import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Toaster, toast } from "sonner";

import { AppHeader } from "@/components/AppHeader";
import { NewTunnelDialog } from "@/components/NewTunnelDialog";
import { StatusBar } from "@/components/StatusBar";
import { TitleBar } from "@/components/TitleBar";
import { TunnelTable } from "@/components/TunnelTable";
import { useTheme } from "@/hooks/useTheme";
import { api, type Tunnel, type TunnelExited } from "@/lib/api";

/** 自动刷新间隔（毫秒） */
const POLL_INTERVAL = 3000;

export default function App() {
  const { theme } = useTheme();

  const [tunnels, setTunnels] = useState<Tunnel[]>([]);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [dataPath, setDataPath] = useState("");

  /**
   * 拉取列表。
   * silent=true（轮询）时不触发顶部“刷新”按钮的旋转动画。
   */
  const refresh = useCallback(async (silent = false) => {
    if (!silent) setRefreshing(true);
    try {
      setTunnels(await api.listTunnels());
    } catch (e) {
      toast.error(`刷新失败：${e}`);
    } finally {
      setLoading(false);
      if (!silent) window.setTimeout(() => setRefreshing(false), 350);
    }
  }, []);

  // 首次加载 + 每 3 秒轮询状态
  useEffect(() => {
    void refresh(true);
    const timer = window.setInterval(() => void refresh(true), POLL_INTERVAL);
    return () => window.clearInterval(timer);
  }, [refresh]);

  // ssh 进程自行退出时即时通知（服务端断开 / 被外部 kill）
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    listen<TunnelExited>("tunnel-exited", (event) => {
      const detail = event.payload.stderr?.trim().split("\n").slice(-1)[0];
      toast.warning(`隧道 ${event.payload.id} 的 SSH 进程已退出`, {
        description: detail || undefined,
      });
      void refresh(true);
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [refresh]);

  // 状态栏展示数据文件路径
  useEffect(() => {
    api
      .dataPath()
      .then(setDataPath)
      .catch(() => {});
  }, []);

  /** 关闭转发：终止进程但保留记录（之后可随时启动复用） */
  const handleStop = async (t: Tunnel) => {
    setBusyId(t.id);
    try {
      setTunnels(await api.stopTunnel(t.id));
      toast.success(`已关闭转发 ${t.host}`, {
        description: `记录已保留：${t.bind}:${t.local_port} → ${t.remote_host}:${t.remote_port}，可随时启动`,
      });
    } catch (e) {
      toast.error(`关闭失败：${e}`);
      void refresh(true);
    } finally {
      setBusyId(null);
    }
  };

  /** 启动已保存的转发（复用配置重新拉起 ssh） */
  const handleStart = async (t: Tunnel) => {
    setBusyId(t.id);
    try {
      setTunnels(await api.restartTunnel(t.id));
      toast.success(`已启动 ${t.host}`, {
        description: `${t.bind}:${t.local_port} → ${t.remote_host}:${t.remote_port}`,
      });
    } catch (e) {
      toast.error(`启动失败：${e}`);
      void refresh(true);
    } finally {
      setBusyId(null);
    }
  };

  /** 显式删除记录（仅已停止的记录可删除） */
  const handleDelete = async (t: Tunnel) => {
    setBusyId(t.id);
    try {
      setTunnels(await api.removeTunnel(t.id));
      toast.success(`已删除记录 ${t.id}（${t.host}）`);
    } catch (e) {
      toast.error(`删除失败：${e}`);
      void refresh(true);
    } finally {
      setBusyId(null);
    }
  };

  /** 启动成功：关弹窗 + 立即刷新 */
  const handleStarted = () => {
    setDialogOpen(false);
    void refresh(true);
  };

  const running = tunnels.filter((t) => t.alive).length;

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* 无边框窗口的自绘标题栏：拖拽移动 / 双击最大化 / 右侧三个控制按钮 */}
      <TitleBar />

      <div className="flex min-h-0 flex-1 flex-col gap-4 p-5">
        <AppHeader
          onNew={() => setDialogOpen(true)}
          onRefresh={() => void refresh()}
          refreshing={refreshing}
        />

        <main className="min-h-0 flex-1 overflow-hidden rounded-xl border border-border bg-card shadow-card">
          <TunnelTable
            tunnels={tunnels}
            loading={loading}
            busyId={busyId}
            onStop={handleStop}
            onStart={handleStart}
            onDelete={handleDelete}
            onNew={() => setDialogOpen(true)}
          />
        </main>

        <StatusBar total={tunnels.length} running={running} dataPath={dataPath} />
      </div>

      <NewTunnelDialog
        open={dialogOpen}
        onOpenChange={setDialogOpen}
        onStarted={handleStarted}
      />

      {/* offset：避开顶部自绘标题栏 */}
      <Toaster
        theme={theme}
        position="top-right"
        richColors
        closeButton
        offset={{ top: 56, right: 12 }}
      />
    </div>
  );
}
