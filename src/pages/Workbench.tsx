import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import {
  api,
  listenCompat,
  PipelineView,
  Proposal,
  SettingMeta,
  TreeNode,
  WorkSummary,
} from "@/lib/tauri";
import { computeNextStep } from "@/lib/nextStep";
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
  const work = (location.state as { work: WorkSummary } | null)?.work;

  const [tree, setTree] = useState<TreeNode | null>(null);
  const [selected, setSelected] = useState<TreeNode | null>(null);
  const [pipeline, setPipeline] = useState<PipelineView | null>(null);
  const [settings, setSettings] = useState<SettingMeta[]>([]);
  const [usage, setUsage] = useState<{ input_tokens: number; output_tokens: number } | null>(
    null,
  );
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [sessionRefresh, setSessionRefresh] = useState(0);
  /** 本作品待审提议数（R17 下一步行动条：有待审提议时「审 N 条提议」最优先） */
  const [pendingProposals, setPendingProposals] = useState(0);

  const workPath = work?.path ?? "";

  /** 新建会话：生成 id、设为当前并返回（R1：会话状态上移到页面层） */
  function createSession() {
    const id = `s${Date.now().toString(16)}-${Math.random().toString(16).slice(2, 6)}`;
    setSessionId(id);
    return id;
  }

  const refresh = useCallback(async () => {
    if (!workPath) return;
    try {
      const [t, p, s, u, props] = await Promise.all([
        api.readTree(workPath),
        api.getPipeline(workPath),
        api.listSettings(workPath).catch(() => [] as SettingMeta[]),
        api.getUsageSummary(workPath).catch(() => null),
        api.listProposals().catch(() => [] as Proposal[]),
      ]);
      setTree(t);
      setPipeline(p);
      setSettings(s);
      setUsage(u);
      // 提议队列是全局的，按作品过滤（R14 归属语义）
      setPendingProposals(props.filter((x) => x.work_path === workPath).length);
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

  // R17：样章是否已产出（开书完成信号）；下一步行动（主行动 + chips）纯函数计算
  const hasSample = treeHasPath(tree, "style/样章.md");
  const next = useMemo(
    () => computeNextStep(pipeline, tree, pendingProposals, hasSample),
    [pipeline, tree, pendingProposals, hasSample],
  );

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

  // 新提议即时反映到下一步行动（R17：待审提议最优先——「审 N 条提议」需要实时计数）
  useEffect(() => {
    if (!workPath) return;
    const un = listenCompat<Proposal>("review:new", (e) => {
      if (e.payload?.work_path === workPath) scheduleRefresh();
    });
    return () => {
      un.then((f) => f()).catch(() => {});
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
            workName={work?.name}
            sessionId={sessionId}
            onNewSession={createSession}
            onTurnDone={() => setSessionRefresh((k) => k + 1)}
            chips={next.chips}
            refreshKey={sessionRefresh}
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
                    next={next.primary}
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
