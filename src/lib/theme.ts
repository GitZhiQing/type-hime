/**
 * 主题系统（R11）：浅色 / 深色 / 跟随系统 三态。
 * - 持久化 localStorage（key: type-hime-theme）
 * - 应用方式：documentElement 加/去 .dark 类（Tailwind v4 @custom-variant dark 已配置，
 *   shadcn 全套 design token 变量在 index.css 的 :root/.dark 块中）
 * - 系统模式监听 prefers-color-scheme 变化实时跟随
 * - main.tsx 在渲染前调用 initTheme()（避免首帧闪白）
 * - 切换广播 window CustomEvent "theme:changed"（Toaster 等订阅）
 */

export type ThemeMode = "light" | "dark" | "system";

const STORAGE_KEY = "type-hime-theme";

export function getStoredTheme(): ThemeMode {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (v === "light" || v === "dark" || v === "system") return v;
  } catch {
    /* 隐私模式等 */
  }
  return "system";
}

/** 解析生效主题（system → 系统偏好）。 */
export function resolveTheme(mode: ThemeMode): "light" | "dark" {
  if (mode === "light" || mode === "dark") return mode;
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

/** 应用主题（class 切换 + 广播）。 */
export function applyTheme(mode: ThemeMode) {
  const resolved = resolveTheme(mode);
  document.documentElement.classList.toggle("dark", resolved === "dark");
  document.documentElement.style.colorScheme = resolved;
  window.dispatchEvent(new CustomEvent("theme:changed", { detail: { mode, resolved } }));
}

/** 设置并持久化（UI 切换入口调用）。 */
export function setTheme(mode: ThemeMode) {
  try {
    localStorage.setItem(STORAGE_KEY, mode);
  } catch {
    /* 忽略持久化失败 */
  }
  applyTheme(mode);
}

/** 启动初始化：渲染前应用存储的主题 + system 模式跟随系统变化。 */
export function initTheme() {
  applyTheme(getStoredTheme());
  window
    .matchMedia?.("(prefers-color-scheme: dark)")
    .addEventListener?.("change", () => {
      if (getStoredTheme() === "system") applyTheme("system");
    });
}
