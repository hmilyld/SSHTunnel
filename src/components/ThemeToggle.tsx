import { MoonIcon, SunIcon } from "@/components/icons";
import { Button } from "@/components/ui/button";
import { useTheme } from "@/hooks/useTheme";

/** 深色/浅色主题切换按钮 */
export function ThemeToggle() {
  const { theme, toggle } = useTheme();
  return (
    <Button
      variant="outline"
      size="icon"
      aria-label={theme === "dark" ? "切换到浅色主题" : "切换到深色主题"}
      title={theme === "dark" ? "浅色主题" : "深色主题"}
      onClick={toggle}
    >
      {theme === "dark" ? <SunIcon className="h-4 w-4" /> : <MoonIcon className="h-4 w-4" />}
    </Button>
  );
}
