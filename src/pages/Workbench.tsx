import { useCallback, useEffect, useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import {
  api,
  PipelineView,
  SettingMeta,
  TreeNode,
  WorkSummary,
} from "@/lib/tauri";
import { ResourceTree } from "@/components/ResourceTree";
import { EditorPane } from "@/components/EditorPane";
import { ChatPane } from "@/components/ChatPane";
import { SessionList } from "@/components/SessionList";
import { GeneratePane } from "@/components/GeneratePane";
import { StageBar } from "@/components/StageBar";
import { DiffPane } from "@/components/DiffPane";
import { AssemblyPane } from "@/components/AssemblyPane";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Switch } from "@/components/ui/switch";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { toast } from "sonner";
import {
  BookOpen,
  History,
  ArrowLeft,
  RefreshCw,
  Sparkles,
} from "lucide-react";

export function Workbench() {
  const navigate = useNavigate();
  const location = useLocation();
  const work = (location.state as { work: WorkSummary } | null)?.work;

  const [tree, setTree] = useState<TreeNode | null>(null);
  const [selected, setSelected] = useState<TreeNode | null>(null);
  const [log, setLog] = useState<string[]>([]);
  const [pipeline, setPipeline] = useState<PipelineView | null>(null);
  const [settings, setSettings] = useState<SettingMeta[]>([]);
  const [loreOpen, setLoreOpen] = useState(false);
  const [loreKind, setLoreKind] = useState("character");
  const [loreName, setLoreName] = useState("");
  const [loreContent, setLoreContent] = useState("");
  const [loreHidden, setLoreHidden] = useState(false);
  const [loreOpenAt, setLoreOpenAt] = useState("");
  const [loreBusy, setLoreBusy] = useState(false);
  const [usage, setUsage] = useState<{ input_tokens: number; output_tokens: number } | null>(
    null,
  );
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
      const [t, l, p, s, u] = await Promise.all([
        api.readTree(workPath),
        api.gitLog(workPath, 15),
        api.getPipeline(workPath),
        api.listSettings(workPath).catch(() => [] as SettingMeta[]),
        api.getUsageSummary(workPath).catch(() => null),
      ]);
      setTree(t);
      setLog(l);
      setPipeline(p);
      setSettings(s);
      setUsage(u);
    } catch (e) {
      toast.error(String(e));
    }
  }, [workPath]);

  async function handleCreateSetting() {
    if (!loreName.trim() || loreBusy) return;
    setLoreBusy(true);
    try {
      await api.createSetting(
        workPath,
        loreKind,
        loreName.trim(),
        loreContent,
        loreHidden,
        loreOpenAt ? Number(loreOpenAt) : null,
      );
      setLoreOpen(false);
      setLoreName("");
      setLoreContent("");
      setLoreHidden(false);
      setLoreOpenAt("");
      refresh();
      toast.success("设定条目已创建");
    } catch (e) {
      toast.error(String(e));
    } finally {
      setLoreBusy(false);
    }
  }

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
        {usage && (usage.input_tokens > 0 || usage.output_tokens > 0) && (
          <span
            className="text-[11px] text-muted-foreground"
            title="本作品全部 agent 调用的真实 token 用量（provider 计数）"
          >
            ≈{((usage.input_tokens + usage.output_tokens) / 10000).toFixed(1)}万 tokens
          </span>
        )}
        <Button
          variant="ghost"
          size="sm"
          className="h-8 px-2 text-xs"
          onClick={refresh}
        >
          <RefreshCw data-icon="inline-start" />
          刷新
        </Button>
      </header>

      {pipeline && (
        <StageBar workPath={workPath} pipeline={pipeline} onChange={setPipeline} />
      )}

      <ResizablePanelGroup orientation="horizontal" className="flex-1">
        {/* 左栏（R1 过渡形态）：会话列表（上）+ 资源树/git（下）；R5 资源树移入制品面板 */}
        <ResizablePanel defaultSize={22} minSize={14}>
          <aside className="flex h-full flex-col">
            <div className="flex h-[45%] min-h-0 flex-col border-b">
              <SessionList
                scope="work"
                workPath={workPath}
                sessionId={sessionId}
                onSelect={setSessionId}
                onNew={createSession}
                refreshKey={sessionRefresh}
              />
            </div>
            <div className="flex min-h-0 flex-1 flex-col">
              <div className="flex h-9 shrink-0 items-center gap-2 border-b px-3 text-xs font-medium text-muted-foreground">
                <BookOpen className="size-3.5" />
                资源树
                <Button
                  variant="ghost"
                  size="sm"
                  className="ml-auto h-7 px-2 text-xs"
                  onClick={() => setLoreOpen(true)}
                >
                  <Sparkles data-icon="inline-start" />
                  新设定
                </Button>
              </div>
              <ScrollArea className="min-h-0 flex-1">
                {tree ? (
                  <div className="p-1">
                    <ResourceTree
                      node={tree}
                      selected={selected?.path ?? null}
                      onSelect={setSelected}
                      onAction={handleAction}
                      hiddenPaths={
                        new Set(
                          settings
                            .filter((s) => s.visibility === "hidden")
                            .map((s) => s.path),
                        )
                      }
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
            </div>
          </aside>
        </ResizablePanel>

        <ResizableHandle withHandle />

        {/* 中栏：聊天主操作面（R1 起独占，生成按钮降级到右栏过渡 Tab，R5 退役） */}
        <ResizablePanel defaultSize={44} minSize={24}>
          <ChatPane
            scope="work"
            workPath={workPath}
            sessionId={sessionId}
            onNewSession={createSession}
            onTurnDone={() => setSessionRefresh((k) => k + 1)}
          />
        </ResizablePanel>

        <ResizableHandle withHandle />

        {/* 右栏：制品/审阅/装配 + 生成（过渡 Tab，R5 移除） */}
        <ResizablePanel defaultSize={34} minSize={18}>
          <Tabs defaultValue="editor" className="flex h-full flex-col">
            <TabsList className="m-2 grid w-auto grid-cols-4 self-start">
              <TabsTrigger value="editor">编辑器</TabsTrigger>
              <TabsTrigger value="review">审阅</TabsTrigger>
              <TabsTrigger value="assembly">装配</TabsTrigger>
              <TabsTrigger value="generate">生成</TabsTrigger>
            </TabsList>
            <TabsContent value="editor" className="mt-0 flex-1 data-[state=inactive]:hidden">
              <EditorPane
                workPath={work.path}
                relPath={selected && !selected.is_dir ? selected.path : null}
                onFileCreated={refresh}
              />
            </TabsContent>
            <TabsContent value="review" className="mt-0 flex-1 data-[state=inactive]:hidden">
              <DiffPane workPath={workPath} onApplied={refresh} />
            </TabsContent>
            <TabsContent value="assembly" className="mt-0 flex-1 data-[state=inactive]:hidden">
              <AssemblyPane workPath={workPath} />
            </TabsContent>
            <TabsContent value="generate" className="mt-0 flex-1 data-[state=inactive]:hidden">
              <GeneratePane workPath={workPath} onProposal={refresh} />
            </TabsContent>
          </Tabs>
        </ResizablePanel>
      </ResizablePanelGroup>

      {/* 新建设定条目 */}
      <Dialog open={loreOpen} onOpenChange={setLoreOpen}>
        <DialogContent className="max-w-lg">
          <DialogHeader>
            <DialogTitle>新建设定条目</DialogTitle>
            <DialogDescription>
              保存为 md + frontmatter 并自动 git 提交。隐藏条目是红线：不进任何 prompt。
            </DialogDescription>
          </DialogHeader>
          <FieldGroup>
            <Field>
              <FieldLabel>类型</FieldLabel>
              <Select value={loreKind} onValueChange={(v) => v && setLoreKind(v)}>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="character">人物</SelectItem>
                  <SelectItem value="worldview">世界观</SelectItem>
                  <SelectItem value="mystery">悬念/伏笔</SelectItem>
                </SelectContent>
              </Select>
            </Field>
            <Field>
              <FieldLabel htmlFor="lore-name">名称</FieldLabel>
              <Input
                id="lore-name"
                value={loreName}
                onChange={(e) => setLoreName(e.currentTarget.value)}
                placeholder="例如：林晚 / 修行体系 / 黑龙身份"
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="lore-content">内容</FieldLabel>
              <Textarea
                id="lore-content"
                value={loreContent}
                onChange={(e) => setLoreContent(e.currentTarget.value)}
                className="min-h-24 text-sm"
                placeholder="设定的正文内容"
              />
            </Field>
            <div className="flex items-center gap-4">
              <label className="flex items-center gap-2 text-sm">
                <Switch checked={loreHidden} onCheckedChange={setLoreHidden} />
                隐藏
              </label>
              <label className="flex items-center gap-2 text-sm text-muted-foreground">
                开放卷
                <Input
                  value={loreOpenAt}
                  onChange={(e) =>
                    setLoreOpenAt(e.currentTarget.value.replace(/[^0-9]/g, ""))
                  }
                  placeholder="不限"
                  className="h-8 w-16"
                />
              </label>
            </div>
          </FieldGroup>
          <DialogFooter>
            <Button onClick={handleCreateSetting} disabled={!loreName.trim() || loreBusy}>
              创建
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
