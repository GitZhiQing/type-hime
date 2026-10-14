import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
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
  Check,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  FileText,
  GitBranch,
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
import { SubagentView } from "@/components/artifacts/SubagentView";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";

/**
 * 制品面板（R5-①）：VS Code 式多标签组。
 * 标签组状态（多开/关闭/切换/激活）存 localStorage（按作品区分）；
 * 制品注册表：文稿（资源树+编辑器）/ 审阅 / 问题清单 / 账本 / 成本 / git / 技能。
 * 文稿标签由资源树点击打开（同文件复用已有标签）；其余经「+」菜单打开（同类单例）。
 */

export type ArtifactKind =
  | "doc"
  | "review"
  | "issues"
  | "ledger"
  | "usage"
  | "git"
  | "skills"
  /** 委派上下文（D49：每次 spawn_agent 一个标签，会话级不持久化） */
  | "subagent";

export interface DelegationPayload {
  id: string;
  agent?: string | null;
  label: string;
  /** 子会话档案 id（D50：视图据此 loadSessionView 回看；持久化只存这些元数据，不含 prompt） */
  session?: string | null;
}

export interface ArtifactTab {
  /** 唯一 id：doc 用 `doc:<path>`，subagent 用 `sub:<taskId>`，其余用 kind */
  id: string;
  kind: ArtifactKind;
  title: string;
  /** kind=doc 的文件相对路径 */
  path?: string;
  /** kind=subagent 的委派数据（会话级，不入 localStorage） */
  payload?: DelegationPayload;
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

/** 单例制品（同类只开一个标签）；装配预览已退役（R8 起根 Agent 装配委派上下文，D49） */
const SINGLETONS: {
  kind: ArtifactKind;
  title: string;
  icon: ReactNode;
}[] = [
  { kind: "review", title: "审阅", icon: <FileText className="size-3" /> },
  { kind: "issues", title: "问题清单", icon: <ListChecks className="size-3" /> },
  { kind: "ledger", title: "账本", icon: <ScrollText className="size-3" /> },
  { kind: "usage", title: "成本", icon: <Wallet className="size-3" /> },
  { kind: "git", title: "git 历史", icon: <GitBranch className="size-3" /> },
  { kind: "skills", title: "技能文档", icon: <Settings2 className="size-3" /> },
];

/** 持久化里已知 kind 的白名单（装配退役后清残留 tab 用；subagent 存元数据可恢复） */
const KNOWN_KINDS = new Set<ArtifactKind>([
  "doc",
  ...SINGLETONS.map((s) => s.kind),
  "subagent",
]);

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
      // 过滤已退役/未知 kind 的残留标签（如「装配」），activeId 失效则回落首个
      const tabs = Array.isArray(parsed.tabs)
        ? parsed.tabs.filter((t) => KNOWN_KINDS.has(t.kind))
        : [];
      if (tabs.length > 0) {
        const activeId = tabs.some((t) => t.id === parsed.activeId)
          ? parsed.activeId
          : tabs[0].id;
        return { tabs, activeId };
      }
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
    },
    [],
  );

  // 标签状态统一落盘（D50：delegation:new 等直接 setState 的路径也持久化——
  // 子会话标签靠元数据（agent/label/session id）恢复，重载后可回看档案）
  useEffect(() => {
    try {
      localStorage.setItem(storageKey(workPath), JSON.stringify({ tabs, activeId }));
    } catch {
      /* 配额满等持久化失败不阻塞 UI */
    }
  }, [workPath, tabs, activeId]);

  // 委派上下文标签（D50，ZCode subagent-session tab 同构）：ChatPane 在 SubTaskStart 时
  // 按序配对 spawn_agent 并派发 delegation:new——此处登记/幂等更新制品标签（不抢当前激活）。
  useEffect(() => {
    const onDelegation = (e: Event) => {
      const d = (e as CustomEvent<DelegationPayload>).detail;
      if (!d?.id) return;
      const tabId = `sub:${d.id}`;
      const title = `${d.agent ?? "子Agent"}·${d.label.slice(0, 10)}`;
      setTabState((cur) => {
        const existing = cur.tabs.find((t) => t.id === tabId);
        if (existing) {
          // 幂等：已存在则仅更新标题/元数据（ZCode openSubagentSessionSidePane 同语义）
          if (existing.title === title) return cur;
          return {
            tabs: cur.tabs.map((t) => (t.id === tabId ? { ...t, title, payload: d } : t)),
            activeId: cur.activeId,
          };
        }
        return {
          tabs: [...cur.tabs, { id: tabId, kind: "subagent", title, payload: d }],
          activeId: cur.activeId,
        };
      });
    };
    window.addEventListener("delegation:new", onDelegation);
    return () => window.removeEventListener("delegation:new", onDelegation);
  }, []);

  // 委派会话直达（D50/D51：任务卡或历史任务行的入口按钮）——标签不存在则由事件元数据补建
  // （localStorage 被清/换机器后历史任务行仍可打开；缺 session 元数据则忽略）
  useEffect(() => {
    const onOpen = (e: Event) => {
      const d = (e as CustomEvent<DelegationPayload>).detail;
      if (!d?.id) return;
      const tabId = `sub:${d.id}`;
      setTabState((cur) => {
        const existing = cur.tabs.find((t) => t.id === tabId);
        if (existing) {
          if (cur.activeId === tabId && existing.payload?.session === d.session) return cur;
          return {
            tabs: cur.tabs.map((t) => (t.id === tabId ? { ...t, payload: { ...t.payload, ...d } } : t)),
            activeId: tabId,
          };
        }
        if (!d.session) return cur; // 无档案可看：不动
        const title = `${d.agent ?? "子Agent"}·${d.label.slice(0, 10)}`;
        return {
          tabs: [...cur.tabs, { id: tabId, kind: "subagent", title, payload: d }],
          activeId: tabId,
        };
      });
    };
    window.addEventListener("delegation:open", onOpen);
    return () => window.removeEventListener("delegation:open", onOpen);
  }, []);

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

  // ---- 右键菜单（浏览器式）：关闭/关闭其他/关闭右侧/关闭全部 ----
  const menuTabRef = useRef<string | null>(null);
  const [ctxOpen, setCtxOpen] = useState(false);

  /** 激活保留区最后一个标签；空则置空（渲染层已有空态指引） */
  function fallbackActive(kept: ArtifactTab[]): string {
    return kept.some((t) => t.id === activeId) ? activeId : (kept[kept.length - 1]?.id ?? "");
  }

  function closeOthers(id: string) {
    const kept = tabs.filter((t) => t.id === id);
    persist({ tabs: kept, activeId: fallbackActive(kept) });
  }

  function closeRight(id: string) {
    const idx = tabs.findIndex((t) => t.id === id);
    const kept = tabs.slice(0, idx + 1);
    persist({ tabs: kept, activeId: fallbackActive(kept) });
  }

  function closeAll() {
    persist({ tabs: [], activeId: "" });
  }

  // ---- 拖拽排序（原生 HTML5 DnD，drop 位置按指针在标签左/右半侧判定） ----
  const [dragId, setDragId] = useState<string | null>(null);
  const [dropAt, setDropAt] = useState<{ id: string; pos: "before" | "after" } | null>(null);

  function moveTab(fromId: string, toId: string, pos: "before" | "after") {
    const from = tabs.find((t) => t.id === fromId);
    if (!from || fromId === toId) return;
    const next = tabs.filter((t) => t.id !== fromId);
    const ti = next.findIndex((t) => t.id === toId);
    if (ti < 0) return;
    next.splice(pos === "before" ? ti : ti + 1, 0, from);
    persist({ tabs: next, activeId });
  }

  const menuIdx = menuTabRef.current ? tabs.findIndex((t) => t.id === menuTabRef.current) : -1;
  const menuTab = menuIdx >= 0 ? tabs[menuIdx] : null;

  const active = tabs.find((t) => t.id === activeId);

  // 浏览器式标签栏（用户反馈）：标签保最小可读宽度，溢出时横向滚动——
  // 滚轮转横向 + 边缘箭头（按可滚动状态显隐）+ 激活标签自动滚入视野
  const stripRef = useRef<HTMLDivElement>(null);
  const [canLeft, setCanLeft] = useState(false);
  const [canRight, setCanRight] = useState(false);

  const updateScrollState = useCallback(() => {
    const el = stripRef.current;
    if (!el) return;
    setCanLeft(el.scrollLeft > 1);
    setCanRight(el.scrollLeft + el.clientWidth < el.scrollWidth - 1);
  }, []);

  useEffect(() => {
    updateScrollState();
    const el = stripRef.current;
    if (!el) return;
    el.addEventListener("scroll", updateScrollState, { passive: true });
    const ro = new ResizeObserver(updateScrollState);
    ro.observe(el);
    return () => {
      el.removeEventListener("scroll", updateScrollState);
      ro.disconnect();
    };
  }, [updateScrollState, tabs]);

  // 激活标签滚入视野（切标签/新开签时）
  useEffect(() => {
    const el = stripRef.current?.querySelector('[data-tab-active="true"]');
    el?.scrollIntoView({ inline: "nearest", block: "nearest", behavior: "smooth" });
  }, [activeId, tabs.length]);

  function scrollStrip(dir: -1 | 1) {
    stripRef.current?.scrollBy({ left: dir * 240, behavior: "smooth" });
  }

  /** 竖向滚轮转横向滚动（浏览器标签栏惯例；触控板的横向分量原样生效） */
  function onStripWheel(e: React.WheelEvent) {
    const el = stripRef.current;
    if (!el) return;
    if (Math.abs(e.deltaY) > Math.abs(e.deltaX)) {
      el.scrollLeft += e.deltaY;
    }
  }

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
      {/* 标签条（浏览器式）：标签宽度内容自适应（min-w-20 保底、max-w-48 封顶）溢出滚动；
          隐藏滚动条（.tabbar-scroll），导航靠滚轮 + 边缘箭头 + 标签列表下拉；
          右键菜单（关闭/其他/右侧/全部）与拖拽排序由 Trigger 层承接 */}
      <div className="flex h-9 shrink-0 items-stretch border-b bg-muted/30">
        <ContextMenu
          open={ctxOpen}
          onOpenChange={(o) => {
            // 非标签区域右键不弹菜单（menuTabRef 在 contextmenu 捕获时写入）
            setCtxOpen(o && menuTabRef.current !== null);
          }}
        >
          <ContextMenuTrigger
            render={
              <div
                className="relative flex min-w-0 flex-1 items-stretch"
                onContextMenu={(e) => {
                  const el = (e.target as HTMLElement).closest("[data-tab-id]");
                  menuTabRef.current = el ? (el as HTMLElement).dataset.tabId ?? null : null;
                }}
              />
            }
          >
            {canLeft && (
              <button
                type="button"
                aria-label="向左滚动标签"
                className="absolute left-0 top-0 z-10 flex h-full items-center bg-muted/95 px-0.5 text-muted-foreground hover:text-foreground"
                onClick={() => scrollStrip(-1)}
              >
                <ChevronLeft className="size-3.5" />
              </button>
            )}
            <div
              ref={stripRef}
              onWheel={onStripWheel}
              onDragOver={(e) => e.preventDefault()}
              className="tabbar-scroll flex min-w-0 flex-1 items-stretch overflow-x-auto"
            >
              {tabs.map((t) => (
                <div
                  key={t.id}
                  data-tab-id={t.id}
                  data-tab-active={t.id === activeId}
                  draggable
                  onDragStart={(e) => {
                    setDragId(t.id);
                    e.dataTransfer.effectAllowed = "move";
                    e.dataTransfer.setData("text/plain", t.id);
                  }}
                  onDragEnd={() => {
                    setDragId(null);
                    setDropAt(null);
                  }}
                  onDragOver={(e) => {
                    if (!dragId || dragId === t.id) return;
                    e.preventDefault();
                    const r = e.currentTarget.getBoundingClientRect();
                    const pos = e.clientX < r.left + r.width / 2 ? "before" : "after";
                    setDropAt((cur) =>
                      cur?.id === t.id && cur.pos === pos ? cur : { id: t.id, pos },
                    );
                  }}
                  onDrop={(e) => {
                    e.preventDefault();
                    if (dragId && dropAt) moveTab(dragId, dropAt.id, dropAt.pos);
                    setDragId(null);
                    setDropAt(null);
                  }}
                  className={cn(
                    // R14：宽度内容自适应（此前 flex-1 均分容器致少标签时被拉满）；
                    // min-w 保底可读、max-w 封顶防长标题独占，超出走横向滚动（shrink-0）
                    "group relative flex min-w-20 max-w-48 shrink-0 items-center gap-1.5 border-r px-2.5 text-xs transition-colors",
                    t.id === activeId
                      ? "border-t-2 border-t-primary bg-background text-foreground"
                      : "border-t-2 border-t-transparent text-muted-foreground hover:bg-background/60",
                    dragId === t.id && "opacity-40",
                  )}
                >
                  {/* 拖拽落点指示线（左/右半侧） */}
                  {dropAt?.id === t.id && (
                    <span
                      className={cn(
                        "absolute top-0 bottom-0 z-10 w-0.5 bg-primary",
                        dropAt.pos === "before" ? "left-0" : "right-0",
                      )}
                    />
                  )}
                  <button
                    type="button"
                    className="flex min-w-0 flex-1 items-center gap-1.5 py-1.5"
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
                    className={cn(
                      "shrink-0 rounded p-0.5 transition-opacity hover:bg-accent",
                      t.id === activeId
                        ? "opacity-70 hover:opacity-100"
                        : "opacity-0 group-hover:opacity-100",
                    )}
                    onClick={() => closeTab(t.id)}
                  >
                    <X className="size-3" />
                  </button>
                </div>
              ))}
            </div>
            {canRight && (
              <button
                type="button"
                aria-label="向右滚动标签"
                className="absolute right-0 top-0 z-10 flex h-full items-center bg-muted/95 px-0.5 text-muted-foreground hover:text-foreground"
                onClick={() => scrollStrip(1)}
              >
                <ChevronRight className="size-3.5" />
              </button>
            )}
          </ContextMenuTrigger>
          <ContextMenuContent>
            {menuTab && (
              <>
                <ContextMenuItem onClick={() => closeTab(menuTab.id)}>
                  关闭
                </ContextMenuItem>
                <ContextMenuItem
                  disabled={tabs.length <= 1}
                  onClick={() => closeOthers(menuTab.id)}
                >
                  关闭其他
                </ContextMenuItem>
                <ContextMenuItem
                  disabled={menuIdx >= tabs.length - 1}
                  onClick={() => closeRight(menuTab.id)}
                >
                  关闭右侧
                </ContextMenuItem>
                <ContextMenuSeparator />
                <ContextMenuItem variant="destructive" onClick={closeAll}>
                  关闭全部
                </ContextMenuItem>
              </>
            )}
          </ContextMenuContent>
        </ContextMenu>
        {/* 标签列表下拉（浏览器 ⌄ 惯例）：全部开签一览，点选激活 */}
        <DropdownMenu>
          <DropdownMenuTrigger
            render={
              <Button
                variant="ghost"
                size="sm"
                className="h-9 w-8 shrink-0 rounded-none border-l"
                aria-label="标签列表"
              />
            }
          >
            <ChevronDown className="size-4" />
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuGroup>
              <DropdownMenuLabel>打开的标签（{tabs.length}）</DropdownMenuLabel>
              {tabs.map((t) => (
                <DropdownMenuItem key={t.id} onClick={() => persist({ tabs, activeId: t.id })}>
                  {t.kind === "doc" ? (
                    <FileText className="size-3 shrink-0" />
                  ) : (
                    SINGLETONS.find((s) => s.kind === t.kind)?.icon ?? (
                      <FileText className="size-3 shrink-0" />
                    )
                  )}
                  <span className="truncate">{t.title}</span>
                  {t.id === activeId && (
                    <span className="ml-auto text-[10px] text-primary">当前</span>
                  )}
                </DropdownMenuItem>
              ))}
            </DropdownMenuGroup>
          </DropdownMenuContent>
        </DropdownMenu>
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
              {SINGLETONS.map((s) => {
                const opened = tabs.some((t) => t.kind === s.kind);
                return (
                  <DropdownMenuItem key={s.kind} onClick={() => openSingleton(s.kind)}>
                    {s.icon}
                    {s.title}
                    {/* 已打开用勾选标识（用户反馈：不用「（已打开）」文字标注）；点击直达激活 */}
                    {opened && <Check className="ml-auto size-3 text-muted-foreground" />}
                  </DropdownMenuItem>
                );
              })}
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
          />
        ) : active.kind === "review" ? (
          <DiffPane workPath={workPath} onApplied={onTreeChanged} />
        ) : active.kind === "issues" ? (
          <IssuesView workPath={workPath} refreshKey={refreshKey} />
        ) : active.kind === "ledger" ? (
          <LedgerView workPath={workPath} />
        ) : active.kind === "usage" ? (
          <UsageView workPath={workPath} refreshKey={refreshKey} />
        ) : active.kind === "git" ? (
          <GitView workPath={workPath} />
        ) : active.kind === "skills" ? (
          <SkillsView workPath={workPath} />
        ) : active.kind === "subagent" ? (
          <SubagentView workPath={workPath} payload={active.payload} />
        ) : (
          <Placeholder label={`${active.title}视图（未知类型）`} />
        )}
      </div>
    </section>
  );
}
