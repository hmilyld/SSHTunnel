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

export const api = {
  /** 读取 ~/.ssh/config 的 Host 别名列表 */
  listHosts: () => invoke<SshHost[]>("list_hosts"),

  /** 全部隧道（附带 alive 状态），前端每 3s 轮询 */
  listTunnels: () => invoke<Tunnel[]>("list_tunnels"),

  /** 新建并启动一条 ssh -L 隧道（记录持久化，除非显式删除否则长期保留） */
  startTunnel: (request: StartTunnelRequest) =>
    invoke<Tunnel>("start_tunnel", { request }),

  /** 关闭隧道：终止进程但保留记录（状态变为已停止，可随时 restart） */
  stopTunnel: (id: string) => invoke<Tunnel[]>("stop_tunnel", { id }),

  /** 用已保存的记录重新启动隧道（复用配置，仅更新 PID） */
  restartTunnel: (id: string) => invoke<Tunnel[]>("restart_tunnel", { id }),

  /** 显式删除记录（仅允许删除已停止的记录） */
  removeTunnel: (id: string) => invoke<Tunnel[]>("remove_tunnel", { id }),

  /** ~/.ssh/config 路径（界面提示用） */
  sshConfigPath: () => invoke<string>("ssh_config_path"),

  /** 数据目录中 tunnels.json 的路径（界面提示用） */
  dataPath: () => invoke<string>("data_path"),
};
