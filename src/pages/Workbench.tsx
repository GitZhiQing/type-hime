import { useCallback, useEffect, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import {
  api,
  PipelineView,
  SettingMeta,
  TreeNode,
  WorkSummary,
} from "@/lib/tauri";
import { ChatPane } from "@/components/ChatPane";
import { SessionList } from "@/components/SessionList";
import { PlanBar } from "@/components/PlanBar";
import { ContextBar } from "@/components/ContextBar";
import { GoalBar } from "@/components/GoalBar";
import { ArtifactsPane } from "@/components/artifacts/ArtifactsPane";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "@/components/ui/resizable";
import { Button } from "@/components/ui/button";
import { toast } from "sonner";
import { ArrowLeft, RefreshCw } from "lucide-react";

/**
 * 项目页（R5-① 起三栏 = 会话列表 | 聊天 | 制品面板；REDESIGN §5.2）。
 * 左栏资源树/git 历史已移入制品面板（文稿标签侧栏 / git 标签）。
 */
export function Workbench() {
  const navigate = useNavigate();
  const location = useLocation();
  const work = (location.state as { work: WorkSummary } | null)?.work;

  const [tree, setTree] = useState<TreeNode | null>(null);
  const [selected, setSelected] = useState<TreeNode | null>(null);
  const [pipeline, setPipeline] = useState<PipelineView | null>(null);
  const [settings, setSettings] = useState<SettingMeta[]>([]);
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [sessionRefresh, setSessionRefresh] = useState(0);

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
      const [t, p, s] = await Promise.all([
        api.readTree(workPath),
        api.getPipeline(workPath),
        api.listSettings(workPath).catch(() => [] as SettingMeta[]),
      ]);
      setTree(t);
      setPipeline(p);
      setSettings(s);
    } catch (e) {
      toast.error(String(e));
    }
  }, [workPath]);

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

  useEffect(() => {
    if (!work) {
      navigate("/");
      return;
    }
    refresh();
    // 聊天内联提议卡片接受落盘后同步资源树（R2-③）
    const onReviewChanged = () => refresh();
    window.addEventListener("review:changed", onReviewChanged);
    return () => window.removeEventListener("review:changed", onReviewChanged);
  }, [work, refresh, navigate]);

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
        <Button
          variant="ghost"
          size="sm"
          className="ml-auto h-8 px-2 text-xs"
          onClick={refresh}
        >
          <RefreshCw data-icon="inline-start" />
          刷新
        </Button>
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
                    onChange={setPipeline}
                  />
                )}
                <ContextBar
                  workPath={workPath}
                  sessionId={sessionId}
                  refreshKey={sessionRefresh}
                />
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
