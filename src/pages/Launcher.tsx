import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { api, WorkSummary } from "@/lib/tauri";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { BookOpen, FolderOpen, Loader2, Plus } from "lucide-react";

export function Launcher() {
  const navigate = useNavigate();
  const [works, setWorks] = useState<WorkSummary[] | null>(null);
  const [worksRoot, setWorksRoot] = useState("");
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    api.listWorks().then(setWorks).catch((e) => setError(String(e)));
    api.getWorksRoot().then(setWorksRoot).catch(() => {});
  }, []);

  async function handleCreate() {
    if (!name.trim() || creating) return;
    setCreating(true);
    setError("");
    try {
      const work = await api.createWork(name.trim());
      setOpen(false);
      setName("");
      navigate("/work", { state: { work } });
    } catch (e) {
      setError(String(e));
    } finally {
      setCreating(false);
    }
  }

  return (
    <div className="flex min-h-screen flex-col items-center justify-center gap-8 bg-background p-8">
      <div className="flex flex-col items-center gap-2">
        <h1 className="text-2xl font-bold tracking-wide">type-hime</h1>
        <p className="text-sm text-muted-foreground">
          AI 辅助中文网文长篇写作工作台
        </p>
      </div>

      <div className="w-full max-w-2xl">
        <div className="mb-3 flex items-center justify-between">
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <FolderOpen className="size-4" />
            <span className="truncate" title={worksRoot}>
              {worksRoot || "…"}
            </span>
          </div>
          <Dialog open={open} onOpenChange={setOpen}>
            <DialogTrigger
              render={
                <Button size="sm">
                  <Plus data-icon="inline-start" />
                  新建作品
                </Button>
              }
            />
            <DialogContent>
              <DialogHeader>
                <DialogTitle>新建作品</DialogTitle>
                <DialogDescription>
                  创建作品目录骨架（设定 / 大纲 / 卷 / 账本）并初始化 git 仓库。
                </DialogDescription>
              </DialogHeader>
              <FieldGroup>
                <Field>
                  <FieldLabel htmlFor="work-name">作品名</FieldLabel>
                  <Input
                    id="work-name"
                    value={name}
                    onChange={(e) => setName(e.currentTarget.value)}
                    onKeyDown={(e) => e.key === "Enter" && handleCreate()}
                    placeholder="例如：问剑青云"
                  />
                </Field>
              </FieldGroup>
              {error && (
                <p className="text-sm text-destructive">{error}</p>
              )}
              <DialogFooter>
                <Button onClick={handleCreate} disabled={!name.trim() || creating}>
                  {creating && <Loader2 data-icon="inline-start" className="animate-spin" />}
                  创建
                </Button>
              </DialogFooter>
            </DialogContent>
          </Dialog>
        </div>

        {works === null ? (
          <div className="flex items-center justify-center p-10 text-muted-foreground">
            <Loader2 className="mr-2 size-4 animate-spin" /> 加载作品列表…
          </div>
        ) : works.length === 0 ? (
          <Card>
            <CardContent className="flex flex-col items-center gap-2 p-10 text-muted-foreground">
              <BookOpen className="size-8 opacity-50" />
              <p className="text-sm">还没有作品，从新建一部开始</p>
            </CardContent>
          </Card>
        ) : (
          <div className="flex flex-col gap-3">
            {works.map((w) => (
              <Card
                key={w.path}
                className="cursor-pointer transition-colors hover:bg-accent"
                onClick={() => navigate("/work", { state: { work: w } })}
              >
                <CardHeader>
                  <CardTitle className="text-base">{w.name}</CardTitle>
                  <CardDescription>{w.path}</CardDescription>
                  <CardAction>
                    <Badge variant="secondary">{w.created_at.slice(0, 10)}</Badge>
                  </CardAction>
                </CardHeader>
                <CardContent className="text-xs text-muted-foreground">
                  <span>设定 · 大纲 · 卷 · 账本 · git 历史可回溯</span>
                </CardContent>
              </Card>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
