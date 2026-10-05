/**
 * 运行平台判定（仅用于「是否自绘标题栏」这类与窗口装饰相关的取舍）。
 *
 * 窗口装饰是**按平台**配置的：
 * - macOS / Linux：`src-tauri/tauri.conf.json` 里 `decorations: true`，用系统原生标题栏（红绿灯）；
 * - Windows：`src-tauri/tauri.windows.conf.json` 覆盖为 `decorations: false`，配合 `TitleBar` 自绘。
 *
 * 这里同步读取 UA 判断，避免用异步 IPC（如 `isDecorated()`）导致首帧标题栏缺失/闪烁。
 * 判断方向刻意选「是否 macOS」而不是「是否 Windows」：识别不出平台时退回现在的自绘标题栏
 * （Windows 行为），不会出现无边框又无标题栏、窗口没法拖动的死局。
 */
export function usesNativeTitleBar(): boolean {
  if (typeof navigator === "undefined") return false;
  const hint = `${navigator.userAgent} ${navigator.platform ?? ""}`;
  return /mac/i.test(hint);
}
