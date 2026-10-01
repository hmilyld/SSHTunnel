import { useCallback, useState } from "react";

export type Theme = "light" | "dark";

const STORAGE_KEY = "theme";

/** 读取当前主题：优先本地存储，其次系统偏好 */
export function getTheme(): Theme {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored === "light" || stored === "dark") return stored;
  } catch {
    /* localStorage 不可用 */
  }
  const prefersDark =
    typeof window !== "undefined" &&
    window.matchMedia?.("(prefers-color-scheme: dark)").matches;
  return prefersDark ? "dark" : "light";
}

/**
 * 深色/浅色主题切换：写 `<html class="dark">` + localStorage。
 *
 * 只对外暴露 `theme` 与 `toggle`：显式设置主题（`setTheme`）目前没有调用方，
 * 保留在内部即可，避免出现“看似可用却没有界面入口”的 API。
 */
export function useTheme() {
  const [theme, setThemeState] = useState<Theme>(getTheme);

  const applyTheme = useCallback((next: Theme) => {
    document.documentElement.classList.toggle("dark", next === "dark");
    try {
      localStorage.setItem(STORAGE_KEY, next);
    } catch {
      /* 忽略 */
    }
    setThemeState(next);
  }, []);

  const toggle = useCallback(() => {
    applyTheme(getTheme() === "dark" ? "light" : "dark");
  }, [applyTheme]);

  return { theme, toggle };
}
