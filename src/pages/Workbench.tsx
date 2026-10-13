import { useCallback, useEffect, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { api, TreeNode, WorkSummary } from "@/lib/tauri";
import { ResourceTree } from "@/components/ResourceTree";
import { EditorPane } from "@/components/EditorPane";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Button } from "@/components/ui/button";
import { toast } from "sonner";
import { BookOpen, History, MessageSquare, ArrowLeft, RefreshCw } from "lucide-react";

export function Workbench() {
  const navigate = useNavigate();
  const location = useLocation();
  const work = (location.state as { work: WorkSummary } | null)?.work;

  const [tree, setTree] = useState<TreeNode | null>(null);
  const [selected, setSelected] = useState<TreeNode | null>(null);
  const [log, setLog] = useState<string[]>([]);

  const workPath = work?.path ?? "";

  const refresh = useCallback(async () => {
    if (!workPath) return;
    try {
      const [t, l] = await Promise.all([
        api.readTree(workPath),
        api.gitLog(workPath, 15),
      ]);
      setTree(t);
      setLog(l);
    } catch (e) {
      toast.error(String(e));
    }
  }, [workPath]);

  useEffect(() => {
    if (!work) {
      navigate("/");
      return;
    }
    refresh();
  }, [work, refresh, navigate]);

  if (!work) return null;

  async function handleAction(action: "delete" | "restore", node: TreeNode) {
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
        <ResizablePanel defaultSize={22} minSize={14}>
          <aside className="flex h-full flex-col">
            <div className="flex h-9 shrink-0 items-center gap-2 border-b px-3 text-xs font-medium text-muted-foreground">
              <BookOpen className="size-3.5" />
              资源树
            </div>
            <ScrollArea className="flex-1">
              {tree ? (
                <div className="p-1">
                  <ResourceTree
                    node={tree}
                    selected={selected?.path ?? null}
                    onSelect={setSelected}
                    onAction={handleAction}
                  />
                </div>
              ) : (
                <div className="p-3 text-xs text-muted-foreground">加载中…</div>
              )}
            </ScrollArea>
            <div className="shrink-0 border-t">
              <div className="flex h-9 items-center gap-2 border-b px-3 text-xs font-medium text-muted-foreground">
                <History className="size-3.5" />
                git 历史
              </div>
              <ScrollArea className="h-32">
                <div className="flex flex-col gap-0.5 p-2 font-mono text-[11px] text-muted-foreground">
                  {log.map((line, i) => (
                    <span key={i} className="truncate" title={line}>
                      {line}
                    </span>
                  ))}
                </div>
              </ScrollArea>
            </div>
          </aside>
        </ResizablePanel>

        <ResizableHandle withHandle />

        <ResizablePanel defaultSize={44} minSize={24}>
          <main className="flex h-full flex-col">
            <div className="flex h-9 shrink-0 items-center gap-2 border-b px-3 text-xs font-medium text-muted-foreground">
              <MessageSquare className="size-3.5" />
              会话
            </div>
            <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
              Agent 会话区（切片 2 起启用）
            </div>
          </main>
        </ResizablePanel>

        <ResizableHandle withHandle />

        <ResizablePanel defaultSize={34} minSize={18}>
          <EditorPane
            workPath={work.path}
            relPath={selected && !selected.is_dir ? selected.path : null}
            onFileCreated={refresh}
          />
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  );
}
