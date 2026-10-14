import { useEffect, useState } from "react";
import { api } from "@/lib/tauri";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import { Save, Loader2, FilePlus2, EyeOff, Eye, Pencil } from "lucide-react";
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
import { Switch } from "@/components/ui/switch";
import { MessageResponse } from "@/components/ai/response";
import { toast } from "sonner";

interface EditorProps {
  workPath: string;
  /** 当前打开的文件相对路径，null 为空 */
  relPath: string | null;
  onFileCreated: () => void;
}

/** 编辑/预览分段按钮样式（激活态反色） */
function cnEditBtn(active: boolean): string {
  return [
    "flex items-center px-2 py-1 transition-colors",
    active ? "bg-accent text-foreground" : "text-muted-foreground hover:bg-accent/60",
  ].join(" ");
}

export function EditorPane({ workPath, relPath, onFileCreated }: EditorProps) {
  const [content, setContent] = useState("");
  const [saved, setSaved] = useState("");
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [createOpen, setCreateOpen] = useState(false);
  const [newPath, setNewPath] = useState("");
  const [error, setError] = useState("");
  const [settingHidden, setSettingHidden] = useState(false);
  const [settingOpenAt, setSettingOpenAt] = useState<string>("");
  /** md 文件的编辑/预览模式（R5-②：制品栏 markdown 渲染） */
  const [mode, setMode] = useState<"edit" | "preview">("edit");

  const isSetting = relPath?.startsWith("settings/") ?? false;
  const isMd = relPath?.endsWith(".md") ?? false;

  // 切换文件回编辑模式
  useEffect(() => {
    setMode("edit");
  }, [relPath]);

  useEffect(() => {
    if (!relPath) {
      setContent("");
      setSaved("");
      return;
    }
    setLoading(true);
    setError("");
    api
      .readFile(workPath, relPath)
      .then((text) => {
        setContent(text);
        setSaved(text);
      })
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
    // 同步设定元数据（可见性控制条）
    if (relPath.startsWith("settings/")) {
      api
        .listSettings(workPath)
        .then((all) => {
          const meta = all.find((m) => m.path === relPath);
          setSettingHidden(meta?.visibility === "hidden");
          setSettingOpenAt(meta?.open_at_vol ? String(meta.open_at_vol) : "");
        })
        .catch(() => {});
    }
  }, [workPath, relPath]);

  async function applyVisibility(hidden: boolean, openAt: string) {
    if (!relPath) return;
    try {
      await api.setSettingVisibility(
        workPath,
        relPath,
        hidden,
        openAt ? Number(openAt) : null,
      );
      setSettingHidden(hidden);
      setSettingOpenAt(openAt);
      // frontmatter 被改写，重读
      const text = await api.readFile(workPath, relPath);
      setContent(text);
      setSaved(text);
      toast.success(hidden ? "已隐藏（红线：不进任何 prompt）" : "已设为可见");
    } catch (e) {
      toast.error(String(e));
    }
  }

  const dirty = content !== saved;

  async function handleSave() {
    if (!relPath || saving) return;
    setSaving(true);
    setError("");
    try {
      await api.writeFile(workPath, relPath, content);
      setSaved(content);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  async function handleCreate() {
    const p = newPath.trim();
    if (!p) return;
    setError("");
    try {
      await api.createFile(workPath, p, "");
      setCreateOpen(false);
      setNewPath("");
      onFileCreated();
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <section className="flex h-full flex-col">
      <div className="flex h-9 shrink-0 items-center gap-2 border-b px-3 text-xs font-medium text-muted-foreground">
        <span className="truncate">{relPath ?? "未打开文件"}</span>
        {dirty && (
          <span className="rounded bg-amber-500/15 px-1.5 py-0.5 text-[10px] text-amber-600">
            未保存
          </span>
        )}
        <div className="ml-auto flex items-center gap-1">
          {isMd && relPath && (
            <span className="flex overflow-hidden rounded-md border text-xs">
              <button
                type="button"
                aria-label="编辑模式"
                className={cnEditBtn(mode === "edit")}
                onClick={() => setMode("edit")}
              >
                <Pencil data-icon="inline-start" />
                编辑
              </button>
              <button
                type="button"
                aria-label="预览模式"
                className={cnEditBtn(mode === "preview")}
                onClick={() => setMode("preview")}
              >
                <Eye data-icon="inline-start" />
                预览
              </button>
            </span>
          )}
          <Button
            variant="ghost"
            size="sm"
            className="h-7 px-2 text-xs"
            onClick={() => setCreateOpen(true)}
          >
            <FilePlus2 data-icon="inline-start" />
            新建文件
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="h-7 px-2 text-xs"
            disabled={!dirty || saving}
            onClick={handleSave}
          >
            {saving ? (
              <Loader2 data-icon="inline-start" className="animate-spin" />
            ) : (
              <Save data-icon="inline-start" />
            )}
            保存
          </Button>
        </div>
      </div>

      {error && (
        <div className="border-b bg-destructive/10 px-3 py-1.5 text-xs text-destructive">
          {error}
        </div>
      )}

      {isSetting && relPath && (
        <div className="flex shrink-0 items-center gap-3 border-b bg-muted/30 px-3 py-1.5 text-xs">
          <span className="flex items-center gap-1.5">
            {settingHidden ? (
              <EyeOff className="size-3.5 text-amber-600" />
            ) : (
              <Eye className="size-3.5 text-muted-foreground" />
            )}
            可见性
          </span>
          <label className="flex items-center gap-1">
            <Switch
              checked={settingHidden}
              onCheckedChange={(v) => applyVisibility(v, settingOpenAt)}
            />
            隐藏
          </label>
          <label className="flex items-center gap-1 text-muted-foreground">
            开放卷
            <Input
              value={settingOpenAt}
              onChange={(e) => {
                const v = e.currentTarget.value.replace(/[^0-9]/g, "");
                setSettingOpenAt(v);
              }}
              onBlur={() => applyVisibility(settingHidden, settingOpenAt)}
              placeholder="不限"
              className="h-6 w-14 text-xs"
            />
          </label>
          <span className="text-muted-foreground/70">
            隐藏/未到开放卷的条目不进任何 prompt（红线）
          </span>
          <Button
            variant="ghost"
            size="sm"
            className="ml-auto h-6 px-2 text-[11px]"
            onClick={async () => {
              try {
                const warns = await api.checkVisibilityLeak(workPath);
                if (warns.length === 0) {
                  toast.success("关联泄漏检查通过：可见条目未复述隐藏设定");
                } else {
                  warns.forEach((w) => toast.warning(w, { duration: 8000 }));
                }
              } catch (e) {
                toast.error(String(e));
              }
            }}
          >
            泄漏检查
          </Button>
        </div>
      )}

      {loading ? (
        <div className="flex flex-1 items-center justify-center text-muted-foreground">
          <Loader2 className="mr-2 size-4 animate-spin" /> 读取中…
        </div>
      ) : relPath && isMd && mode === "preview" ? (
        <ScrollArea className="min-h-0 flex-1">
          <div className="mx-auto max-w-3xl px-6 py-4">
            <MessageResponse className="text-[13px]">{content}</MessageResponse>
          </div>
        </ScrollArea>
      ) : relPath ? (
        <Textarea
          value={content}
          onChange={(e) => setContent(e.currentTarget.value)}
          spellCheck={false}
          className="flex-1 resize-none rounded-none border-0 font-mono text-[13px] leading-relaxed focus-visible:ring-0"
        />
      ) : (
        <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
          在左侧资源树选择文件
        </div>
      )}
      <Separator />

      <Dialog open={createOpen} onOpenChange={setCreateOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>新建文件</DialogTitle>
            <DialogDescription>
              相对作品根的路径，自动创建父目录。例如 volumes/vol-01/chapters/ch-001.md
            </DialogDescription>
          </DialogHeader>
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="new-file-path">路径</FieldLabel>
              <Input
                id="new-file-path"
                value={newPath}
                onChange={(e) => setNewPath(e.currentTarget.value)}
                onKeyDown={(e) => e.key === "Enter" && handleCreate()}
                placeholder="volumes/vol-01/chapters/ch-001.md"
              />
            </Field>
          </FieldGroup>
          <DialogFooter>
            <Button onClick={handleCreate} disabled={!newPath.trim()}>
              创建
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </section>
  );
}
