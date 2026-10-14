import { useCallback, useEffect, useState, type ReactNode } from "react";
import {
  SettingMeta,
  TreeNode,
} from "@/lib/tauri";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { cn } from "cn";
import {
  BookOpen,
  FileText,
  GitBranch,
  Layers,
  ListChecks,
  Plus,
  ScrollText,
  Settings2,
  Wallet,
  X,
} from "lucide-react";
import { DocView } from "@/components/artifacts/DocView";
import { DiffPane } from "@/components/DiffPane";
import { IssuesView } from "@/components/artifacts/IssuesView";
import { LedgerView } from "@/components/artifacts/LedgerView";
import { UsageView } from "@/components/artifacts/UsageView";
import { GitView } from "@/components/artifacts/GitView";
import { SkillsView } from "@/components/artifacts/SkillsView";
import { AssemblyPane } from "@/components/AssemblyPane";

/**
 * 制品面板（R5-① · REDESIGN §5.2/§5.5）：VS Code 式多标签组。
 * 标签组状态（多开/关闭/切换/激活）存 localStorage（按作品区分）；
 * 制品注册表：文稿（资源树+编辑器）/ 审阅 / 问题清单 / 账本 / 装配 / 成本 / git / 技能。
 * 文稿标签由资源树点击打开（同文件复用已有标签）；其余经「+」菜单打开（同类单例）。
 */

export type ArtifactKind =
  | "doc"
  | "review"
  | "issues"
  | "ledger"
  | "assembly"
  | "usage"
  | "git"
  | "skills";

export interface ArtifactTab {
  /** 唯一 id：doc 用 `doc:<path>`，其余用 kind */
  id: string;
  kind: ArtifactKind;
  title: string;
  /** kind=doc 的文件相对路径 */
  path?: string;
}

interface ArtifactsPaneProps {
  workPath: string;
  tree: TreeNode | null;
  settings: SettingMeta[];
  selected: TreeNode | null;
  /** 资源树选中文件（加载进文稿编辑器） */
  onSelectFile: (node: TreeNode) => void;
  /** 删除/恢复文件（留人操作；Workbench 实现，含确认副作用刷新） */
  onFileAction: (action: "delete" | "restore", node: TreeNode) => Promise<void>;
  /** 树/产物变化后的刷新（提议应用、新建文件等） */
  onTreeChanged: () => void;
  /** 会话/一轮结束 +1（成本等视图刷新用） */
  refreshKey?: number;
}

/** 单例制品（同类只开一个标签） */
const SINGLETONS: {
  kind: ArtifactKind;
  title: string;
  icon: ReactNode;
}[] = [
  { kind: "review", title: "审阅", icon: <FileText className="size-3" /> },
  { kind: "issues", title: "问题清单", icon: <ListChecks className="size-3" /> },
  { kind: "ledger", title: "账本", icon: <ScrollText className="size-3" /> },
  { kind: "assembly", title: "装配", icon: <Layers className="size-3" /> },
  { kind: "usage", title: "成本", icon: <Wallet className="size-3" /> },
  { kind: "git", title: "git 历史", icon: <GitBranch className="size-3" /> },
  { kind: "skills", title: "技能文档", icon: <Settings2 className="size-3" /> },
];

function storageKey(workPath: string) {
  return `type-hime-artifacts:${workPath}`;
}

function defaultTabs(): { tabs: ArtifactTab[]; activeId: string } {
  const tabs: ArtifactTab[] = [
    { id: "doc", kind: "doc", title: "文稿" },
    { id: "review", kind: "review", title: "审阅" },
  ];
  return { tabs, activeId: "doc" };
}

function loadTabs(workPath: string): { tabs: ArtifactTab[]; activeId: string } {
  try {
    const raw = localStorage.getItem(storageKey(workPath));
    if (raw) {
      const parsed = JSON.parse(raw) as { tabs: ArtifactTab[]; activeId: string };
      if (Array.isArray(parsed.tabs) && parsed.tabs.length > 0) return parsed;
    }
  } catch {
    /* 损坏即回落默认 */
  }
  return defaultTabs();
}

/** 视图占位（R5-② 逐个替换为真实制品视图） */
function Placeholder({ label }: { label: string }) {
  return (
    <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
      {label}
    </div>
  );
}

export function ArtifactsPane({
  workPath,
  tree,
  settings,
  selected,
  onSelectFile,
  onFileAction,
  onTreeChanged,
  refreshKey = 0,
}: ArtifactsPaneProps) {
  const [{ tabs, activeId }, setTabState] = useState(() => loadTabs(workPath));

  useEffect(() => {
    setTabState(loadTabs(workPath));
  }, [workPath]);

  const persist = useCallback(
    (next: { tabs: ArtifactTab[]; activeId: string }) => {
      setTabState(next);
      try {
        localStorage.setItem(storageKey(workPath), JSON.stringify(next));
      } catch {
        /* 配额满等持久化失败不阻塞 UI */
      }
    },
    [workPath],
  );

  function closeTab(id: string) {
    const idx = tabs.findIndex((t) => t.id === id);
    if (idx < 0) return;
    const next = tabs.filter((t) => t.id !== id);
    const activeIdNext =
      activeId === id ? (next[Math.max(0, idx - 1)]?.id ?? "") : activeId;
    persist({ tabs: next, activeId: activeIdNext });
  }

  function openSingleton(kind: ArtifactKind) {
    const def = SINGLETONS.find((s) => s.kind === kind);
    if (!def) return;
    const existing = tabs.find((t) => t.kind === kind);
    if (existing) {
      persist({ tabs, activeId: existing.id });
    } else {
      persist({ tabs: [...tabs, { id: kind, kind, title: def.title }], activeId: kind });
    }
  }

  const active = tabs.find((t) => t.id === activeId);

  // 聊天内联问题卡等入口的跨组件开签请求（R5-③：detail.kind 指定制品）
  useEffect(() => {
    const onOpen = (e: Event) => {
      const kind = (e as CustomEvent<{ kind: ArtifactKind }>).detail?.kind;
      if (!kind || kind === "doc") return;
      const def = SINGLETONS.find((s) => s.kind === kind);
      if (!def) return;
      setTabState((cur) => {
        const existing = cur.tabs.find((t) => t.kind === kind);
        const next = existing
          ? { tabs: cur.tabs, activeId: existing.id }
          : { tabs: [...cur.tabs, { id: kind, kind, title: def.title }], activeId: kind };
        try {
          localStorage.setItem(storageKey(workPath), JSON.stringify(next));
        } catch {
          /* 忽略持久化失败 */
        }
        return next;
      });
    };
    window.addEventListener("artifacts:open", onOpen);
    return () => window.removeEventListener("artifacts:open", onOpen);
  }, [workPath]);

  return (
    <section className="flex h-full min-w-0 flex-col">
      {/* 标签条 */}
      <div className="flex h-9 shrink-0 items-stretch border-b bg-muted/30">
        <div className="flex min-w-0 flex-1 items-stretch overflow-x-auto">
          {tabs.map((t) => (
            <div
              key={t.id}
              className={cn(
                "group flex max-w-44 shrink-0 items-center gap-1.5 border-r px-2.5 text-xs transition-colors",
                t.id === activeId
                  ? "border-t-2 border-t-primary bg-background text-foreground"
                  : "border-t-2 border-t-transparent text-muted-foreground hover:bg-background/60",
              )}
            >
              <button
                type="button"
                className="flex min-w-0 items-center gap-1.5 py-1.5"
                onClick={() => persist({ tabs, activeId: t.id })}
                title={t.path ?? t.title}
              >
                {t.kind === "doc" ? (
                  <FileText className="size-3 shrink-0" />
                ) : (
                  SINGLETONS.find((s) => s.kind === t.kind)?.icon ?? (
                    <FileText className="size-3 shrink-0" />
                  )
                )}
                <span className="truncate">{t.title}</span>
              </button>
              <button
                type="button"
                aria-label={`关闭 ${t.title}`}
                className="rounded p-0.5 opacity-0 transition-opacity hover:bg-accent group-hover:opacity-100"
                onClick={() => closeTab(t.id)}
              >
                <X className="size-3" />
              </button>
            </div>
          ))}
        </div>
        {/* + 菜单：可打开的制品 */}
        <DropdownMenu>
          <DropdownMenuTrigger
            render={
              <Button
                variant="ghost"
                size="sm"
                className="h-9 w-9 shrink-0 rounded-none border-l"
                aria-label="打开制品"
              />
            }
          >
            <Plus className="size-4" />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuGroup>
              <DropdownMenuLabel>打开制品</DropdownMenuLabel>
              {SINGLETONS.map((s) => (
                <DropdownMenuItem
                  key={s.kind}
                  disabled={tabs.some((t) => t.kind === s.kind)}
                  onClick={() => openSingleton(s.kind)}
                >
                  {s.icon}
                  {s.title}
                  {tabs.some((t) => t.kind === s.kind) && "（已打开）"}
                </DropdownMenuItem>
              ))}
            </DropdownMenuGroup>
            <DropdownMenuSeparator />
            <DropdownMenuGroup>
              <DropdownMenuLabel>文稿经左侧资源树打开</DropdownMenuLabel>
              <DropdownMenuItem onClick={() => persist({ tabs, activeId: "doc" })}>
                <BookOpen className="size-3" />
                转到文稿
              </DropdownMenuItem>
            </DropdownMenuGroup>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      {/* 内容区 */}
      {/* 内容区：包裹层 min-h-0 flex-1 修正 flex 子项 h-full 基准
          （否则 h-full 按含标签条的容器全高解析，视图整体高出一个标签条，页面右侧出滚动条） */}
      <div className="flex min-h-0 flex-1 flex-col">
        {!active ? (
          <Placeholder label="没有打开的制品 —— 点右上角 + 打开，或在文稿标签的资源树里选文件" />
        ) : active.kind === "doc" ? (
          <DocView
            workPath={workPath}
            tree={tree}
            settings={settings}
            selected={selected}
            onSelect={onSelectFile}
            onAction={onFileAction}
            onFileCreated={onTreeChanged}
          />
        ) : active.kind === "review" ? (
          <DiffPane workPath={workPath} onApplied={onTreeChanged} />
        ) : active.kind === "issues" ? (
          <IssuesView workPath={workPath} refreshKey={refreshKey} />
        ) : active.kind === "ledger" ? (
          <LedgerView workPath={workPath} />
        ) : active.kind === "assembly" ? (
          <AssemblyPane workPath={workPath} />
        ) : active.kind === "usage" ? (
          <UsageView workPath={workPath} refreshKey={refreshKey} />
        ) : active.kind === "git" ? (
          <GitView workPath={workPath} />
        ) : active.kind === "skills" ? (
          <SkillsView workPath={workPath} />
        ) : (
          <Placeholder label={`${active.title}视图（未知类型）`} />
        )}
      </div>
    </section>
  );
}
