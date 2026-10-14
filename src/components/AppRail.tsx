import { useEffect, useState } from "react";
import { NavLink } from "react-router-dom";
import { BookOpenText, Bot, Check, MessagesSquare, Moon, Settings2, Sun, SunMoon } from "lucide-react";
import { cn } from "cn";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { getStoredTheme, setTheme, type ThemeMode } from "@/lib/theme";

/** 最左应用导航栏：对话（无作品上下文）/ 创作（项目库）/ Agents（子 Agent 团队）/ 配置 + 底部主题切换 */
export function AppRail() {
  const items = [
    { to: "/chat", label: "对话", icon: MessagesSquare, end: false },
    { to: "/", label: "创作", icon: BookOpenText, end: true },
    { to: "/agents", label: "Agents", icon: Bot, end: false },
    { to: "/settings", label: "配置", icon: Settings2, end: false },
  ];
  const [mode, setMode] = useState<ThemeMode>(() => getStoredTheme());
  useEffect(() => {
    const onChanged = (e: Event) => {
      const detail = (e as CustomEvent<{ mode: ThemeMode }>).detail;
      if (detail?.mode) setMode(detail.mode);
    };
    window.addEventListener("theme:changed", onChanged);
    return () => window.removeEventListener("theme:changed", onChanged);
  }, []);

  const THEME_ITEMS: { value: ThemeMode; label: string; icon: typeof Sun }[] = [
    { value: "light", label: "浅色", icon: Sun },
    { value: "dark", label: "深色", icon: Moon },
    { value: "system", label: "跟随系统", icon: SunMoon },
  ];

  return (
    <nav className="flex w-12 shrink-0 flex-col items-center gap-1 border-r bg-muted/30 py-2">
      {items.map(({ to, label, icon: Icon, end }) => (
        <NavLink
          key={to}
          to={to}
          end={end}
          title={label}
          aria-label={label}
          className={({ isActive }) =>
            cn(
              "flex size-9 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground",
              isActive && "bg-accent text-accent-foreground",
            )
          }
        >
          <Icon className="size-4" />
        </NavLink>
      ))}

      {/* 主题切换（R11）：底部三态下拉 */}
      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <button
              type="button"
              aria-label="主题"
              title="主题"
              className="mt-auto flex size-9 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
            />
          }
        >
          {mode === "dark" ? <Moon className="size-4" /> : mode === "light" ? <Sun className="size-4" /> : <SunMoon className="size-4" />}
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" side="right">
          <DropdownMenuGroup>
            <DropdownMenuLabel>主题</DropdownMenuLabel>
            {THEME_ITEMS.map(({ value, label, icon: Icon }) => (
              <DropdownMenuItem key={value} onClick={() => setTheme(value)}>
                <Icon className="size-3.5" />
                {label}
                {mode === value && <Check className="ml-auto size-3.5" />}
              </DropdownMenuItem>
            ))}
          </DropdownMenuGroup>
        </DropdownMenuContent>
      </DropdownMenu>
    </nav>
  );
}
