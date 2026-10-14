import { useCallback, useEffect, useRef, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import {
  api,
  listenCompat,
  PipelineView,
  SettingMeta,
  TreeNode,
  WorkSummary,
} from "@/lib/tauri";
import { ChatPane } from "@/components/ChatPane";
import { SessionList } from "@/components/SessionList";
import { PlanBar } from "@/components/PlanBar";
import { GoalBar } from "@/components/GoalBar";
import { ArtifactsPane } from "@/components/artifacts/ArtifactsPane";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "@/components/ui/resizable";
import { Button } from "@/components/ui/button";
import { toast } from "sonner";
import { ArrowLeft } from "lucide-react";

/**
 * 项目页（R5-① 起三栏 = 会话列表 | 聊天 | 制品面板；REDESIGN §5.2）。
 * 左栏资源树/git 历史已移入制品面板（文稿标签侧栏 / git 标签）。
 */

/** 树里是否已有某文件（R15：样章存在 = 开书完成信号） */
function treeHasPath(node: TreeNode | null, path: string): boolean {
  if (!node) return false;
  if (!node.is_dir && node.path === path) return true;
  return node.children.some((c) => treeHasPath(c, path));
}

export function Workbench() {
  const navigate = useNavigate();
  const location = useLocation();
  const navState = location.state as { work: WorkSummary; kickoff?: boolean } | null;
  const work = navState?.work;

  const [tree, setTree] = useState<TreeNode | null>(null);
  const [selected, setSelected] = useState<TreeNode | null>(null);
  const [pipeline, setPipeline] = useState<PipelineView | null>(null);
  const [settings, setSettings] = useState<SettingMeta[]>([]);
  const [usage, setUsage] = useState<{ input_tokens: number; output_tokens: number } | null>(
    null,
  );
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [sessionRefresh, setSessionRefresh] = useState(0);

  const workPath = work?.path ?? "";

  // R15 开书工作室：从「新建作品」进入 → 剥离导航标志（防刷新/重挂载重放）后延迟派发开场事件
  // （仿 goal:resume 先例，由 ChatPane 以聊天流驱动发送 /开书）。定时器不清理是刻意的：
  // cleanup 会吞掉 StrictMode 双挂载下唯一一次派发，重放由 ref 守卫 + 标志剥离共同拦住。
  const kickoffFiredRef = useRef(false);
  useEffect(() => {
    if (!work || !navState?.kickoff || kickoffFiredRef.current) return;
    kickoffFiredRef.current = true;
    navigate(location.pathname, { replace: true, state: { work } });
    window.setTimeout(() => window.dispatchEvent(new CustomEvent("kickoff:start")), 300);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [work, navState, navigate, location.pathname]);

  /** 新建会话：生成 id、设为当前并返回（R1：会话状态上移到页面层） */
  function createSession() {
    const id = `s${Date.now().toString(16)}-${Math.random().toString(16).slice(2, 6)}`;
    setSessionId(id);
    return id;
  }

  const refresh = useCallback(async () => {
    if (!workPath) return;
    try {
      const [t, p, s, u] = await Promise.all([
        api.readTree(workPath),
        api.getPipeline(workPath),
        api.listSettings(workPath).catch(() => [] as SettingMeta[]),
        api.getUsageSummary(workPath).catch(() => null),
      ]);
      setTree(t);
      setPipeline(p);
      setSettings(s);
      setUsage(u);
    } catch (e) {
      toast.error(String(e));
    }
  }, [workPath]);

  // 事件源刷新调度（R13）：watcher 已防抖 400ms，此处再节流 800ms 防事件风暴；
  // 用户主动操作（删除/恢复等）仍直调 refresh（即时反馈优先）
  const refreshThrottleRef = useRef(0);
  const scheduleRefresh = useCallback(() => {
    const now = Date.now();
    if (now - refreshThrottleRef.current < 800) return;
    refreshThrottleRef.current = now;
    refresh();
  }, [refresh]);

  async function handleFileAction(action: "delete" | "restore", node: TreeNode) {
    try {
      if (action === "delete") {
        await api.deleteFile(workPath, node.path);
        if (selected?.path === node.path) setSelected(null);
        toast.success(`已删除 ${node.name}（可从 git 历史恢复）`);
      } else {
        await api.restoreFile(workPath, node.path);
        toast.success(`已恢复 ${node.name}`);
      }
      refresh();
    } catch (e) {
      toast.error(String(e));
    }
  }

  // R15：样章是否已产出（开书完成信号）与访谈是否可开始（自动开场失败时的空态兜底入口）
  const hasSample = treeHasPath(tree, "style/样章.md");
  const kickoffAvailable =
    !!pipeline &&
    pipeline.stage === "settings" &&
    pipeline.confirmed.length === 0 &&
    !hasSample;

  useEffect(() => {
    if (!work) {
      navigate("/");
      return;
    }
    refresh();
    // 聊天内联提议卡片接受落盘后同步资源树（R2-③）；watcher/focus 亦经 scheduleRefresh 合流
    const onReviewChanged = () => scheduleRefresh();
    window.addEventListener("review:changed", onReviewChanged);
    return () => window.removeEventListener("review:changed", onReviewChanged);
  }, [work, refresh, navigate, scheduleRefresh]);

  // 作品目录文件监听（R13：应用外变更——外部编辑器/终端 git/文件管理器 → 树与 git 历史同步）
  useEffect(() => {
    if (!workPath) return;
    api.watchWork(workPath).catch(() => {});
    const unP = listenCompat<{ work_path?: string }>("work:changed", (e) => {
      if (e.payload?.work_path && e.payload.work_path !== workPath) return;
      scheduleRefresh();
    });
    return () => {
      api.unwatchWork().catch(() => {});
      unP.then((f) => f()).catch(() => {});
    };
  }, [workPath, scheduleRefresh]);

  // 聚焦兜底（R13 方案 B）：切出改完切回来时刷新一次（3s 节流；覆盖 watcher 失效场景）
  useEffect(() => {
    if (!workPath) return;
    let last = 0;
    const onFocus = () => {
      const now = Date.now();
      if (now - last < 3000) return;
      last = now;
      refresh();
    };
    const onVisible = () => {
      if (!document.hidden) onFocus();
    };
    window.addEventListener("focus", onFocus);
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      window.removeEventListener("focus", onFocus);
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, [workPath, refresh]);

  if (!work) return null;

  return (
    <div className="flex h-screen flex-col bg-background text-foreground">
      <header className="flex h-12 shrink-0 items-center gap-3 border-b px-3">
        <Button
          variant="ghost"
          size="sm"
          className="h-8 px-2"
          onClick={() => navigate("/")}
        >
          <ArrowLeft data-icon="inline-start" />
          作品库
        </Button>
        <span className="text-sm font-semibold">{work.name}</span>
        <span className="text-xs text-muted-foreground">{work.path}</span>
        {usage && (usage.input_tokens > 0 || usage.output_tokens > 0) && (
          <span
            className="ml-auto text-[11px] text-muted-foreground"
            title="本作品全部 agent 调用的真实 token 用量（provider 计数；明细见制品面板「成本」）"
          >
            ≈{((usage.input_tokens + usage.output_tokens) / 10000).toFixed(1)}万 tokens
          </span>
        )}
      </header>

      <ResizablePanelGroup orientation="horizontal" className="flex-1">
        {/* 左栏：本项目会话列表（R5 起独占左栏；资源树/git 移入制品面板） */}
        <ResizablePanel defaultSize={20} minSize={13}>
          <SessionList
            scope="work"
            workPath={workPath}
            sessionId={sessionId}
            onSelect={setSessionId}
            onNew={createSession}
            refreshKey={sessionRefresh}
          />
        </ResizablePanel>

        <ResizableHandle withHandle />

        {/* 中栏：聊天主操作面；PLAN 条与目标横幅常驻输入框上方 */}
        <ResizablePanel defaultSize={45} minSize={24}>
          <ChatPane
            scope="work"
            workPath={workPath}
            sessionId={sessionId}
            onNewSession={createSession}
            onTurnDone={() => setSessionRefresh((k) => k + 1)}
            kickoffAvailable={kickoffAvailable}
            planSlot={
              <>
                <GoalBar
                  workPath={workPath}
                  sessionId={sessionId}
                  refreshKey={sessionRefresh}
                />
                {pipeline && (
                  <PlanBar
                    workPath={workPath}
                    pipeline={pipeline}
                    hasSample={hasSample}
                    onChange={setPipeline}
                  />
                )}
              </>
            }
          />
        </ResizablePanel>

        <ResizableHandle withHandle />

        {/* 右栏：制品面板（R5-①：VS Code 式多标签） */}
        <ResizablePanel defaultSize={35} minSize={18}>
          <ArtifactsPane
            workPath={workPath}
            tree={tree}
            settings={settings}
            selected={selected}
            onSelectFile={setSelected}
            onFileAction={handleFileAction}
            onTreeChanged={refresh}
            refreshKey={sessionRefresh}
          />
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  );
}
