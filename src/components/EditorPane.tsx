import { useEffect, useState } from "react";
import { api } from "@/lib/tauri";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { Separator } from "@/components/ui/separator";
import { Save, Loader2, FilePlus2 } from "lucide-react";
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

interface EditorProps {
  workPath: string;
  /** 当前打开的文件相对路径，null 为空 */
  relPath: string | null;
  onFileCreated: () => void;
}

export function EditorPane({ workPath, relPath, onFileCreated }: EditorProps) {
  const [content, setContent] = useState("");
  const [saved, setSaved] = useState("");
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [createOpen, setCreateOpen] = useState(false);
  const [newPath, setNewPath] = useState("");
  const [error, setError] = useState("");

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
  }, [workPath, relPath]);

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

      {loading ? (
        <div className="flex flex-1 items-center justify-center text-muted-foreground">
          <Loader2 className="mr-2 size-4 animate-spin" /> 读取中…
        </div>
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
