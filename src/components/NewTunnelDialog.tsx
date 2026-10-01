import { useEffect, useMemo, useState } from "react";
import { toast } from "sonner";

import { AlertIcon, PencilIcon, SpinnerIcon, TerminalIcon } from "@/components/icons";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  api,
  isPasswordRequired,
  passwordRequiredMessage,
  type SshHost,
  type StartTunnelRequest,
  type Tunnel,
} from "@/lib/api";

interface NewTunnelDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** 启动/保存成功后回调（父组件负责关闭弹窗并刷新列表） */
  onStarted: () => void;
  /** 后端探测到需要密码认证：把请求交给父组件弹出密码框 */
  onPasswordNeeded: (request: StartTunnelRequest, message: string) => void;
  /** 传入记录 = 修改模式（仅用于已停止的记录，保存配置、不启动进程） */
  editing?: Tunnel | null;
}

const BIND_OPTIONS = [
  { value: "127.0.0.1", label: "127.0.0.1 — 仅本机访问（推荐）" },
  { value: "localhost", label: "localhost — 仅本机访问（等价于 127.0.0.1）" },
  { value: "0.0.0.0", label: "0.0.0.0 — 允许局域网 / 外部访问" },
  { value: "::1", label: "::1 — IPv6 仅本机访问" },
];

/**
 * 远程目标主机 → 本地绑定地址 的联动规则：
 * - `localhost` → 绑定 `localhost`
 * - `127.0.0.1` → 绑定 `127.0.0.1`
 * - 其他地址不干预（保持当前选择）
 */
function bindForRemoteHost(remoteHost: string, current?: string): string {
  const t = remoteHost.trim().toLowerCase();
  if (t === "localhost") return "localhost";
  if (t === "127.0.0.1") return "127.0.0.1";
  return current ?? "127.0.0.1";
}

/** 端口校验：必填、纯数字、1-65535 */
function validatePort(value: string): string | null {
  const v = value.trim();
  if (!v) return "必填";
  if (!/^\d+$/.test(v)) return "必须是整数";
  const n = Number(v);
  if (n < 1 || n > 65535) return "范围 1-65535";
  return null;
}

export function NewTunnelDialog({
  open,
  onOpenChange,
  onStarted,
  onPasswordNeeded,
  editing = null,
}: NewTunnelDialogProps) {
  const [hosts, setHosts] = useState<SshHost[]>([]);
  const [hostsLoading, setHostsLoading] = useState(false);
  const [configPath, setConfigPath] = useState("");
  const [hostsError, setHostsError] = useState("");

  const [host, setHost] = useState("");
  const [remoteHost, setRemoteHost] = useState("localhost");
  const [remotePort, setRemotePort] = useState("");
  const [localPort, setLocalPort] = useState("");
  const [localPortTouched, setLocalPortTouched] = useState(false);
  const [bind, setBind] = useState("127.0.0.1");

  const [errors, setErrors] = useState<Record<string, string>>({});
  const [submitting, setSubmitting] = useState(false);

  // 打开弹窗时：按模式（新建 / 修改）初始化表单 + 加载 Host 列表
  useEffect(() => {
    if (!open) return;
    setErrors({});
    setSubmitting(false);
    if (editing) {
      // 修改模式：回填记录（绑定地址按记录值，不做联动覆盖）
      setHost(editing.host);
      setRemoteHost(editing.remote_host);
      setRemotePort(String(editing.remote_port));
      setLocalPort(String(editing.local_port));
      setLocalPortTouched(true);
      setBind(editing.bind);
    } else {
      // 新建模式：默认远程 localhost → 绑定按联动规则取 localhost
      const rh = "localhost";
      setHost("");
      setRemoteHost(rh);
      setRemotePort("");
      setLocalPort("");
      setLocalPortTouched(false);
      setBind(bindForRemoteHost(rh));
    }

    setHostsLoading(true);
    setHostsError("");
    api
      .listHosts()
      .then(setHosts)
      .catch((e) => setHostsError(String(e)))
      .finally(() => setHostsLoading(false));
    api
      .sshConfigPath()
      .then(setConfigPath)
      .catch(() => {});
  }, [open, editing]);

  // 远程端口变化时，本地端口跟随（用户手动改过后不再跟随）
  const onRemotePortChange = (value: string) => {
    setRemotePort(value);
    if (!localPortTouched) setLocalPort(value);
    setErrors((prev) => ({ ...prev, remote_port: "", local_port: "" }));
  };

  // 将要执行的命令实时预览
  const preview = useMemo(() => {
    const lp = localPort.trim() || remotePort.trim() || "<local_port>";
    const rp = remotePort.trim() || "<remote_port>";
    return `ssh -N -L ${bind}:${lp}:${remoteHost.trim() || "localhost"}:${rp} ${host || "<host>"}`;
  }, [bind, host, localPort, remoteHost, remotePort]);

  const submit = async () => {
    const nextErrors: Record<string, string> = {};
    if (!host) nextErrors.host = "请选择 SSH Host";
    const rpError = validatePort(remotePort);
    if (rpError) nextErrors.remote_port = rpError;
    const lpError = validatePort(localPort);
    if (lpError) nextErrors.local_port = lpError;
    if (!remoteHost.trim()) nextErrors.remote_host = "不能为空，默认 localhost";
    setErrors(nextErrors);
    if (Object.keys(nextErrors).length > 0) return;

    const request: StartTunnelRequest = {
      host,
      bind,
      local_port: Number(localPort),
      remote_host: remoteHost.trim(),
      remote_port: Number(remotePort),
    };

    setSubmitting(true);
    try {
      if (editing) {
        // ===== 修改模式：仅保存配置，不启动进程 =====
        await api.updateTunnel(editing.id, request);
        toast.success("已保存修改", {
          description: `${request.host} → ${request.bind}:${request.local_port} → ${request.remote_host}:${request.remote_port}`,
        });
        onStarted();
        return;
      }
      // 首次不带密码启动：若服务器需要密码，后端返回 PASSWORD_REQUIRED 错误
      const tunnel = await api.startTunnel(request);
      toast.success("隧道已启动", {
        description: `${tunnel.host} → ${tunnel.bind}:${tunnel.local_port}（PID ${tunnel.pid}）`,
      });
      onStarted();
    } catch (e) {
      if (!editing && isPasswordRequired(e)) {
        // 交给父组件：关闭本弹窗、打开密码框，确认后带密码重新启动
        onPasswordNeeded(request, passwordRequiredMessage(e));
      } else {
        toast.error(editing ? "保存失败" : "启动失败", { description: String(e) });
      }
    } finally {
      setSubmitting(false);
    }
  };

  const noHosts = !hostsLoading && !hostsError && hosts.length === 0;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[calc(100vh-4rem)] overflow-y-auto">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <span
              className={cn(
                "flex h-8 w-8 items-center justify-center rounded-lg",
                editing
                  ? "bg-amber-500/10 text-amber-600 dark:text-amber-400"
                  : "bg-primary/10 text-primary",
              )}
            >
              {editing ? <PencilIcon className="h-4 w-4" /> : <TerminalIcon className="h-4 w-4" />}
            </span>
            {editing ? "修改转发" : "新建转发"}
          </DialogTitle>
          <DialogDescription>
            {editing
              ? "更新已停止的转发配置；保存后可随时「启动」复用。"
              : "选择 SSH Host，把远程服务映射到本地端口（ssh -L）；支持密钥与密码认证。"}
          </DialogDescription>
        </DialogHeader>

        {/* 命令预览 */}
        <div className="rounded-lg border border-border bg-muted/60 px-3 py-2.5">
          <p className="mb-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
            {editing ? "保存后启动时将执行" : "将执行"}
          </p>
          <code className="block break-all text-xs leading-relaxed text-foreground/90">
            {preview}
          </code>
        </div>

        <div className="grid gap-4">
          {/* SSH Host */}
          <div className="grid gap-2">
            <Label htmlFor="host">SSH Host</Label>
            {hostsLoading ? (
              <div className="flex h-9 items-center gap-2 rounded-md border border-input bg-card px-3 text-sm text-muted-foreground">
                <SpinnerIcon className="h-4 w-4 animate-spin" /> 正在读取 ~/.ssh/config …
              </div>
            ) : hostsError ? (
              <p className="text-xs text-destructive">{hostsError}</p>
            ) : noHosts ? (
              <div className="flex gap-2 rounded-lg border border-amber-500/30 bg-amber-500/10 p-3 text-xs text-amber-700 dark:text-amber-400">
                <AlertIcon className="mt-0.5 h-4 w-4 shrink-0" />
                <span>
                  未找到可用的 Host 别名。请在
                  <code className="mx-1 rounded bg-amber-500/10 px-1">{configPath || "~/.ssh/config"}</code>
                  中添加 Host 条目（通配符如 <code>*</code> 会被忽略）。
                </span>
              </div>
            ) : (
              <>
                <Select value={host} onValueChange={(v) => { setHost(v); setErrors((p) => ({ ...p, host: "" })); }}>
                  <SelectTrigger id="host" aria-invalid={!!errors.host}>
                    <SelectValue placeholder="选择 ~/.ssh/config 中的 Host" />
                  </SelectTrigger>
                  <SelectContent>
                    {hosts.map((h) => (
                      <SelectItem key={h.alias} value={h.alias}>
                        <span className="flex flex-col py-0.5">
                          <span className="font-medium">{h.alias}</span>
                          <span className="text-[11px] text-muted-foreground">
                            {h.user ? `${h.user}@` : ""}
                            {h.hostname}
                            {h.port ? `:${h.port}` : ""}
                          </span>
                        </span>
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                {errors.host && <p className="text-xs text-destructive">{errors.host}</p>}
              </>
            )}
          </div>

          {/* 远程主机 */}
          <div className="grid gap-2">
            <Label htmlFor="remote-host">远程目标主机</Label>
            <Input
              id="remote-host"
              value={remoteHost}
              placeholder="localhost"
              onChange={(e) => {
                const v = e.target.value;
                setRemoteHost(v);
                // 联动规则：localhost ↔ 127.0.0.1 同步绑定地址；其他地址不干预
                setBind(bindForRemoteHost(v, bind));
                setErrors((p) => ({ ...p, remote_host: "" }));
              }}
              aria-invalid={!!errors.remote_host}
            />
            <p className="text-[11px] text-muted-foreground">
              从 SSH 服务器视角访问的地址；填 localhost / 127.0.0.1 时会自动同步本地绑定地址
            </p>
            {errors.remote_host && <p className="text-xs text-destructive">{errors.remote_host}</p>}
          </div>

          {/* 端口 */}
          <div className="grid grid-cols-2 gap-4">
            <div className="grid gap-2">
              <Label htmlFor="remote-port">远程端口</Label>
              <Input
                id="remote-port"
                inputMode="numeric"
                value={remotePort}
                placeholder="例如 5432"
                onChange={(e) => onRemotePortChange(e.target.value)}
                aria-invalid={!!errors.remote_port}
              />
              {errors.remote_port && (
                <p className="text-xs text-destructive">{errors.remote_port}</p>
              )}
            </div>
            <div className="grid gap-2">
              <Label htmlFor="local-port">本地端口</Label>
              <Input
                id="local-port"
                inputMode="numeric"
                value={localPort}
                placeholder="默认与远程端口相同"
                onChange={(e) => {
                  setLocalPortTouched(true);
                  setLocalPort(e.target.value);
                  setErrors((p) => ({ ...p, local_port: "" }));
                }}
                aria-invalid={!!errors.local_port}
              />
              {errors.local_port && <p className="text-xs text-destructive">{errors.local_port}</p>}
            </div>
          </div>

          {/* 绑定地址 */}
          <div className="grid gap-2">
            <Label htmlFor="bind">本地绑定地址</Label>
            <Select value={bind} onValueChange={setBind}>
              <SelectTrigger id="bind">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {BIND_OPTIONS.map((opt) => (
                  <SelectItem key={opt.value} value={opt.value}>
                    {opt.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <p className="text-[11px] text-muted-foreground">
              绑定 0.0.0.0 会把转发端口暴露给局域网，请确认网络安全。
            </p>
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)} disabled={submitting}>
            取消
          </Button>
          <Button onClick={submit} disabled={submitting || !host || !!hostsError}>
            {submitting ? (
              <>
                <SpinnerIcon className="h-4 w-4 animate-spin" />
                {editing ? "保存中…" : "启动中…"}
              </>
            ) : editing ? (
              "保存修改"
            ) : (
              "启动转发"
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
