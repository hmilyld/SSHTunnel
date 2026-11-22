import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Toaster, toast } from "sonner";

import { AppHeader } from "@/components/AppHeader";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { NewTunnelDialog } from "@/components/NewTunnelDialog";
import { PasswordDialog, type PasswordRequest } from "@/components/PasswordDialog";
import { StatusBar } from "@/components/StatusBar";
import { TitleBar } from "@/components/TitleBar";
import { TunnelTable } from "@/components/TunnelTable";
import { useTheme } from "@/hooks/useTheme";
import { usesNativeTitleBar } from "@/lib/platform";
import {
  api,
  isPasswordRequired,
  passwordRequiredMessage,
  type Tunnel,
  type TunnelExited,
} from "@/lib/api";

/** 自动刷新间隔（毫秒） */
const POLL_INTERVAL = 3000;

/**
 * macOS 用系统原生标题栏（红绿灯），只有 Windows 是无边框窗口，
 * 需要自绘标题栏来拖动窗口与提供 最小化/最大化/关闭 按钮。
 */
const NATIVE_TITLE_BAR = usesNativeTitleBar();

export default function App() {
  const { theme } = useTheme();

  const [tunnels, setTunnels] = useState<Tunnel[]>([]);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [dialogOpen, setDialogOpen] = useState(false);
  /** 正在修改的已停止记录（null = 新建模式） */
  const [editing, setEditing] = useState<Tunnel | null>(null);
  /** 待确认删除的记录 */
  const [deleteTarget, setDeleteTarget] = useState<Tunnel | null>(null);
  /** 轮询失败只提示一次，避免每 3 秒刷屏 */
  const refreshErrorRef = useRef(false);

  // ===== 密码认证流程（服务器要求密码时由后端 PASSWORD_REQUIRED 触发） =====
  const [pwdReq, setPwdReq] = useState<PasswordRequest | null>(null);
  const [pwdBusy, setPwdBusy] = useState(false);
  const [pwdError, setPwdError] = useState<string | null>(null);

  /**
   * 拉取列表。
   * silent=true（轮询）时不触发顶部“刷新”按钮的旋转动画。
   */
  const refresh = useCallback(async (silent = false) => {
    if (!silent) setRefreshing(true);
    try {
      setTunnels(await api.listTunnels());
      refreshErrorRef.current = false;
    } catch (e) {
      if (!refreshErrorRef.current) {
        refreshErrorRef.current = true;
        toast.error(`刷新失败：${e}`);
      }
    } finally {
      setLoading(false);
      if (!silent) window.setTimeout(() => setRefreshing(false), 350);
    }
  }, []);

  // 首次加载 + 每 3 秒轮询状态；窗口隐藏时暂停，重新可见时立即刷新
  useEffect(() => {
    void refresh(true);
    const timer = window.setInterval(() => {
      if (!document.hidden) void refresh(true);
    }, POLL_INTERVAL);
    const onVisibilityChange = () => {
      if (!document.hidden) void refresh(true);
    };
    document.addEventListener("visibilitychange", onVisibilityChange);
    return () => {
      window.clearInterval(timer);
      document.removeEventListener("visibilitychange", onVisibilityChange);
    };
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
      if (isPasswordRequired(e)) {
        setPwdError(null);
        setPwdReq({
          kind: "restart",
          id: t.id,
          label: `${t.host}   ${t.bind}:${t.local_port} → ${t.remote_host}:${t.remote_port}`,
          message: passwordRequiredMessage(e),
        });
      } else {
        toast.error(`启动失败：${e}`);
        void refresh(true);
      }
    } finally {
      setBusyId(null);
    }
  };

  /** 新建流程被后端判定为“需要密码”：关掉新建弹窗，改弹密码框 */
  const handlePasswordNeeded = (
    request: Parameters<typeof api.startTunnel>[0],
    message: string,
  ) => {
    handleDialogOpenChange(false);
    setPwdError(null);
    setPwdReq({ kind: "start", request, message });
  };

  /** 密码框确认：带密码重新执行挂起的启动/重启操作 */
  const submitPassword = async (password: string) => {
    const req = pwdReq;
    if (!req) return;
    setPwdBusy(true);
    setPwdError(null);
    try {
      if (req.kind === "start") {
        const tunnel = await api.startTunnel(req.request, password);
        toast.success("隧道已启动", {
          description: `${tunnel.host} → ${tunnel.bind}:${tunnel.local_port}（PID ${tunnel.pid}）`,
        });
      } else {
        await api.restartTunnel(req.id, password);
        toast.success("隧道已启动", { description: req.label });
        void refresh(true);
      }
      setPwdReq(null);
      void refresh(true);
    } catch (e) {
      if (isPasswordRequired(e)) {
        // 密码错误（预检阶段被服务器拒绝）：留在弹窗内红字提示，可直接重输
        const detail = passwordRequiredMessage(e);
        setPwdError(detail ? `密码错误或认证被拒绝：${detail}` : "密码错误，请重试");
      } else {
        toast.error(`启动失败：${e}`);
        setPwdReq(null);
      }
    } finally {
      setPwdBusy(false);
    }
  };

  const cancelPassword = () => {
    if (pwdBusy) return;
    setPwdReq(null);
    setPwdError(null);
  };

  /** 打开删除确认，不直接删除 */
  const requestDelete = (t: Tunnel) => {
    setDeleteTarget(t);
  };

  /** 确认后显式删除记录（仅已停止的记录可删除） */
  const confirmDelete = async () => {
    const t = deleteTarget;
    if (!t) return;
    setDeleteTarget(null);
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

  /** 启动成功/保存成功：关弹窗 + 立即刷新 */
  const handleStarted = () => {
    handleDialogOpenChange(false);
    void refresh(true);
  };

  /** 弹窗开合（关闭时同步清空“修改”上下文） */
  const handleDialogOpenChange = (open: boolean) => {
    setDialogOpen(open);
    if (!open) setEditing(null);
  };

  /** 打开新建弹窗 */
  const openNewDialog = () => {
    setEditing(null);
    setDialogOpen(true);
  };

  /** 打开修改弹窗（已停止的记录） */
  const openEditDialog = (t: Tunnel) => {
    setEditing(t);
    setDialogOpen(true);
  };

  const running = tunnels.filter((t) => t.alive).length;

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* Windows 无边框窗口的自绘标题栏：拖拽移动 / 双击最大化 / 右侧三个控制按钮；
          macOS 保留原生标题栏，这里不渲染，避免出现两条标题栏 */}
      {!NATIVE_TITLE_BAR && <TitleBar />}

      <div className="flex min-h-0 flex-1 flex-col gap-4 p-5">
        <AppHeader onNew={openNewDialog} onRefresh={() => void refresh()} refreshing={refreshing} />

        <main className="min-h-0 flex-1 overflow-hidden rounded-xl border border-border bg-card shadow-card">
          <TunnelTable
            tunnels={tunnels}
            loading={loading}
            busyId={busyId}
            onStop={handleStop}
            onStart={handleStart}
            onEdit={openEditDialog}
            onDelete={requestDelete}
            onNew={openNewDialog}
          />
        </main>

        <StatusBar total={tunnels.length} running={running} />
      </div>

      <NewTunnelDialog
        open={dialogOpen}
        onOpenChange={handleDialogOpenChange}
        onStarted={handleStarted}
        onPasswordNeeded={handlePasswordNeeded}
        editing={editing}
      />

      {/* 密码认证弹窗（新建 / 重启共用） */}
      <PasswordDialog
        req={pwdReq}
        busy={pwdBusy}
        error={pwdError}
        onCancel={cancelPassword}
        onSubmit={submitPassword}
      />

      {/* 删除记录二次确认 */}
      <ConfirmDialog
        open={deleteTarget !== null}
        title="删除隧道记录？"
        description={
          deleteTarget
            ? `将删除已停止的记录 ${deleteTarget.host}（${deleteTarget.bind}:${deleteTarget.local_port} → ${deleteTarget.remote_host}:${deleteTarget.remote_port}）。删除后将从 tunnels.json 移除，可重新创建。`
            : ""
        }
        confirmLabel="删除"
        destructive
        onCancel={() => setDeleteTarget(null)}
        onConfirm={() => void confirmDelete()}
      />

      {/* offset：避开顶部标题栏（Windows 自绘 / macOS 原生） */}
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
