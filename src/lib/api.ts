/**
 * 后端通信层：所有 `#[tauri::command]` 的类型化封装。
 * 字段命名与 Rust 端保持一致（snake_case），错误以字符串形式抛出，可直接 toast。
 */
import { invoke } from "@tauri-apps/api/core";

/** `~/.ssh/config` 中的一个 Host 条目 */
export interface SshHost {
  alias: string;
  hostname: string;
  user: string | null;
  port: number | null;
  identity_file: string | null;
}

/** 隧道记录 + 实时存活状态 */
export interface Tunnel {
  id: string;
  host: string;
  bind: string;
  local_port: number;
  remote_host: string;
  remote_port: number;
  pid: number;
  created_at: string;
  /** 进程是否存活 */
  alive: boolean;
}

/** 新建转发请求 */
export interface StartTunnelRequest {
  host: string;
  bind: string;
  local_port: number;
  remote_host: string;
  remote_port: number;
}

/** 后端广播：ssh 进程自行退出（服务端断开、被外部 kill 等） */
export interface TunnelExited {
  id: string;
  stderr: string;
}

/**
 * 后端「需要密码认证」错误前缀（对应 Rust `AppError::PasswordRequired`）。
 * 首次启动（无密码、BatchMode 探测）或密码错误重试时后端返回该前缀错误，
 * 前端据此弹出密码输入框。
 */
export const PASSWORD_REQUIRED_PREFIX = "PASSWORD_REQUIRED::";

export function isPasswordRequired(err: unknown): boolean {
  return String(err).startsWith(PASSWORD_REQUIRED_PREFIX);
}

/** 取出前缀后面的服务器原始提示（如 `Permission denied (publickey,password).`） */
export function passwordRequiredMessage(err: unknown): string {
  return String(err).slice(PASSWORD_REQUIRED_PREFIX.length).trim();
}

export const api = {
  /** 读取 ~/.ssh/config 的 Host 别名列表 */
  listHosts: () => invoke<SshHost[]>("list_hosts"),

  /** 全部隧道（附带 alive 状态），前端每 3s 轮询 */
  listTunnels: () => invoke<Tunnel[]>("list_tunnels"),

  /** 新建并启动一条 ssh -L 隧道（记录持久化，除非显式删除否则长期保留）
   *  服务器需要密码认证时传入 password（由 SSH_ASKPASS 注入 ssh，不落盘） */
  startTunnel: (request: StartTunnelRequest, password?: string | null) =>
    invoke<Tunnel>("start_tunnel", { request, password: password ?? null }),

  /** 关闭隧道：终止进程但保留记录（状态变为已停止，可随时 restart） */
  stopTunnel: (id: string) => invoke<Tunnel[]>("stop_tunnel", { id }),

  /** 用已保存的记录重新启动隧道（复用配置，仅更新 PID；密码语义同 startTunnel） */
  restartTunnel: (id: string, password?: string | null) =>
    invoke<Tunnel[]>("restart_tunnel", { id, password: password ?? null }),

  /** 显式删除记录（仅允许删除已停止的记录） */
  removeTunnel: (id: string) => invoke<Tunnel[]>("remove_tunnel", { id }),

  /** 修改已停止记录的配置（ID/创建时间不变，不涉及进程） */
  updateTunnel: (id: string, request: StartTunnelRequest) =>
    invoke<Tunnel[]>("update_tunnel", { id, request }),

  /** ~/.ssh/config 路径（界面提示用） */
  sshConfigPath: () => invoke<string>("ssh_config_path"),

  /** 数据目录中 tunnels.json 的路径（界面提示用） */
  dataPath: () => invoke<string>("data_path"),
};
