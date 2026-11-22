export const BIND_OPTIONS = [
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
export function bindForRemoteHost(remoteHost: string, current?: string): string {
  const t = remoteHost.trim().toLowerCase();
  if (t === "localhost") return "localhost";
  if (t === "127.0.0.1") return "127.0.0.1";
  return current ?? "127.0.0.1";
}

/** 端口校验：必填、纯数字、1-65535 */
export function validatePort(value: string): string | null {
  const v = value.trim();
  if (!v) return "必填";
  if (!/^\d+$/.test(v)) return "必须是整数";
  const n = Number(v);
  if (n < 1 || n > 65535) return "范围 1-65535";
  return null;
}

export interface CommandPreviewInput {
  bind: string;
  host: string;
  localPort: string;
  remoteHost: string;
  remotePort: string;
}

/** 生成表单中的 `ssh -L` 命令预览（占位符只在字段为空时出现） */
export function buildCommandPreview({
  bind,
  host,
  localPort,
  remoteHost,
  remotePort,
}: CommandPreviewInput): string {
  const lp = localPort.trim() || remotePort.trim() || "<local_port>";
  const rp = remotePort.trim() || "<remote_port>";
  return `ssh -N -L ${bind}:${lp}:${remoteHost.trim() || "localhost"}:${rp} ${host || "<host>"}`;
}
