import { useEffect, useRef, useState } from "react";

import { AlertIcon, SpinnerIcon } from "@/components/icons";
import { Button } from "@/components/ui/button";
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
import type { StartTunnelRequest } from "@/lib/api";

/** 待执行的带密码操作（App 统一持有，提交后由 App 调用后端） */
export type PasswordRequest =
  | { kind: "start"; request: StartTunnelRequest; message: string }
  | { kind: "restart"; id: string; label: string; message: string };

interface PasswordDialogProps {
  req: PasswordRequest | null;
  /** 后端执行中 */
  busy: boolean;
  /** 密码错误等留在弹窗内展示的错误 */
  error: string | null;
  onCancel: () => void;
  onSubmit: (password: string) => void;
}

/**
 * 密码认证弹窗。
 *
 * 触发时机：启动/重启转发时后端以 BatchMode 探测到
 * `Permission denied (publickey,password)`，说明该主机需要密码登录。
 * 密码仅在本次操作中通过 SSH_ASKPASS 交给 ssh，不写入任何持久化文件。
 */
export function PasswordDialog({ req, busy, error, onCancel, onSubmit }: PasswordDialogProps) {
  const [pwd, setPwd] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  // 每次打开/切换目标时清空
  useEffect(() => {
    setPwd("");
    if (req) {
      // 等弹窗动画后聚焦
      const t = window.setTimeout(() => inputRef.current?.focus(), 80);
      return () => window.clearTimeout(t);
    }
    return undefined;
  }, [req]);

  const submit = () => {
    if (!req || busy || !pwd) return;
    onSubmit(pwd);
  };

  const detail =
    req?.kind === "restart"
      ? req.label
      : req?.kind === "start"
        ? `${req.request.host}  →  ${req.request.bind}:${req.request.local_port} → ${req.request.remote_host}:${req.request.remote_port}`
        : "";

  return (
    <Dialog open={req !== null} onOpenChange={(open) => !open && !busy && onCancel()}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <span className="flex h-8 w-8 items-center justify-center rounded-lg bg-amber-500/10 text-amber-600 dark:text-amber-400">
              <AlertIcon className="h-4 w-4" />
            </span>
            需要密码认证
          </DialogTitle>
          <DialogDescription asChild>
            <div className="space-y-2">
              <p>
                该 SSH 服务器未接受密钥认证，需要输入登录密码：
                <code className="ml-1 rounded bg-muted px-1.5 py-0.5 text-xs">
                  {req?.message || "Permission denied (publickey,password)."}
                </code>
              </p>
              {detail && (
                <p className="truncate rounded bg-muted/60 px-2 py-1.5 font-mono text-xs text-foreground/80">
                  {detail}
                </p>
              )}
              <p className="text-xs text-muted-foreground">
                确认后会<strong>先单独验证密码</strong>（密码错误将直接提示、不会建立转发），
                验证通过才启动隧道；密码仅通过 SSH_ASKPASS 交给 ssh 进程，不保存、不写日志。
              </p>
            </div>
          </DialogDescription>
        </DialogHeader>

        <form
          className="grid gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            submit();
          }}
        >
          <Label htmlFor="tunnel-password">密码</Label>
          <Input
            id="tunnel-password"
            ref={inputRef}
            type="password"
            autoComplete="off"
            value={pwd}
            placeholder="输入 SSH 登录密码"
            onChange={(e) => setPwd(e.target.value)}
            disabled={busy}
          />
          {error && (
            <p className="flex items-center gap-1.5 text-xs text-destructive">
              <AlertIcon className="h-3.5 w-3.5 shrink-0" />
              {error}
            </p>
          )}
          {/* 回车提交 */}
          <button type="submit" className="hidden" aria-hidden="true" />
        </form>

        <DialogFooter>
          <Button variant="outline" onClick={onCancel} disabled={busy}>
            取消
          </Button>
          <Button onClick={submit} disabled={busy || !pwd}>
            {busy ? (
              <>
                <SpinnerIcon className="h-4 w-4 animate-spin" />
                验证并启动中…
              </>
            ) : (
              "验证并启动"
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
