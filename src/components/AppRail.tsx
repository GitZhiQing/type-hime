import { NavLink } from "react-router-dom";
import { BookOpenText, MessagesSquare, Settings2 } from "lucide-react";
import { cn } from "cn";

/** 最左应用导航栏：对话（无作品上下文）/ 创作（项目库）/ 配置（REDESIGN §5.1） */
export function AppRail() {
  const items = [
    { to: "/chat", label: "对话", icon: MessagesSquare, end: false },
    { to: "/", label: "创作", icon: BookOpenText, end: true },
    { to: "/settings", label: "配置", icon: Settings2, end: false },
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
    </nav>
  );
}
