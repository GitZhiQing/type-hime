import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { api, WorkSummary } from "@/lib/tauri";
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
import { WorkspacePicker } from "@/components/WorkspacePicker";
import { FolderOpen, Loader2, Plus } from "lucide-react";
import { toast } from "sonner";

/**
 * 作品库（D56 重设计）：书架式布局——每部作品是一本「书」（封面卡：书名竖排 + 书脊色条），
 * 网格排列、悬停上浮；整页可滚动（此前 fixed 高度 + 纵向居中，作品一多就溢出窗口）。
 * 新建作品可指定工作区（文件夹）；浏览器通道文本输入，Tauri 外壳加原生文件夹选择。
 */

/** 按作品名散列取书脊色相（同一作品颜色稳定） */
function bookHue(name: string): number {
  let h = 7;
  for (const ch of name) h = (h * 31 + (ch.codePointAt(0) ?? 0)) % 360;
  return h;
}

function BookCard({ work, onOpen }: { work: WorkSummary; onOpen: () => void }) {
  const hue = bookHue(work.name);
  return (
    <button
      type="button"
      className="group flex flex-col items-stretch text-left focus-visible:outline-none"
      title={work.path}
      onClick={onOpen}
    >
      <div
        className="relative aspect-[3/4] w-full overflow-hidden rounded-md border border-black/25 shadow-sm transition-all duration-150 group-hover:-translate-y-1 group-hover:shadow-lg group-hover:brightness-110 group-focus-visible:ring-2 group-focus-visible:ring-ring"
        style={{
          background: `linear-gradient(155deg, hsl(${hue} 42% 32%) 0%, hsl(${(hue + 24) % 360} 48% 18%) 100%)`,
        }}
      >
        {/* 书脊（左缘色条）与书口（右缘亮边） */}
        <span
          className="absolute inset-y-0 left-0 w-[6px]"
          style={{ background: `hsl(${hue} 52% 44%)` }}
        />
        <span className="absolute inset-y-0.5 right-0.5 w-[3px] rounded-full bg-white/15" />
        {/* 书名：竖排（中文书封惯例），自右上起排 */}
        <span className="absolute top-2.5 right-3 bottom-2.5 max-h-[calc(100%-1.25rem)] overflow-hidden text-sm leading-relaxed font-medium tracking-wider text-white/90 [writing-mode:vertical-rl]">
          {work.name}
        </span>
      </div>
      <span className="mt-1.5 truncate text-center text-[10px] tabular-nums text-muted-foreground">
        {work.created_at.slice(0, 10)}
      </span>
    </button>
  );
}

export function Launcher() {
  const navigate = useNavigate();
  const [works, setWorks] = useState<WorkSummary[] | null>(null);
  const [worksRoot, setWorksRoot] = useState("");
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [wsRoot, setWsRoot] = useState("");
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState("");
  const [pickerOpen, setPickerOpen] = useState(false);

  useEffect(() => {
    api.listWorks().then(setWorks).catch((e) => setError(String(e)));
    api.getWorksRoot().then(setWorksRoot).catch(() => {});
  }, []);

  function openCreate() {
    setName("");
    setWsRoot(worksRoot);
    setError("");
    setOpen(true);
  }

  async function handleCreate() {
    if (!name.trim() || creating) return;
    setCreating(true);
    setError("");
    const base = wsRoot.trim();
    const switchWorkspace = base !== "" && base !== worksRoot.trim();
    try {
      const work = await api.createWork(name.trim(), switchWorkspace ? base : undefined);
      // D56：在自定义文件夹创建 → 同步切换默认工作区（作品列表按默认工作区读取，
      // 否则新作品会「消失」；配置页仍可随时改回）
      if (switchWorkspace) {
        try {
          const normalized = await api.setWorksRoot(base);
          setWorksRoot(normalized);
          toast.info(`默认工作区已切换为 ${normalized}`);
        } catch (e) {
          toast.warning(`作品已创建，但默认工作区切换失败：${e}`);
        }
      }
      setOpen(false);
      navigate("/work", { state: { work } });
    } catch (e) {
      setError(String(e));
    } finally {
      setCreating(false);
    }
  }

  async function pickWorkspace() {
    setPickerOpen(true);
  }

  return (
    /* D56：h-full + overflow-y-auto——作品变多时整页滚动，不再溢出窗口 */
    <div className="h-full overflow-y-auto bg-background">
      <div className="mx-auto flex min-h-full w-full max-w-3xl flex-col gap-6 px-8 py-10">
        <div className="flex flex-col items-center gap-2">
          <h1 className="text-2xl font-bold tracking-wide">type-hime</h1>
          <p className="text-sm text-muted-foreground">
            AI 辅助中文网文长篇写作工作台
          </p>
        </div>

        {/* 工具条：默认工作区（点击去配置页） + 新建作品 */}
        <div className="flex items-center justify-between gap-3">
          <button
            type="button"
            className="flex min-w-0 items-center gap-2 rounded-md px-1.5 py-1 text-sm text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
            title="当前默认工作区（在配置页修改）"
            onClick={() => navigate("/settings")}
          >
            <FolderOpen className="size-4 shrink-0" />
            <span className="truncate">{worksRoot || "…"}</span>
          </button>
          <Button size="sm" onClick={openCreate}>
            <Plus data-icon="inline-start" />
            新建作品
          </Button>
        </div>

        {works === null ? (
          <div className="flex items-center justify-center p-10 text-muted-foreground">
            <Loader2 className="mr-2 size-4 animate-spin" /> 加载作品列表…
          </div>
        ) : (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(124px,1fr))] gap-x-5 gap-y-7">
            {/* 书架首格 = 新增 */}
            <button
              type="button"
              className="group flex flex-col items-stretch text-left focus-visible:outline-none"
              onClick={openCreate}
            >
              <div className="flex aspect-[3/4] w-full flex-col items-center justify-center gap-2 rounded-md border border-dashed border-muted-foreground/30 text-muted-foreground transition-colors group-hover:border-muted-foreground/60 group-hover:bg-accent/40 group-focus-visible:ring-2 group-focus-visible:ring-ring">
                <Plus className="size-6" />
                <span className="text-xs">新建作品</span>
              </div>
              <span className="mt-1.5 text-center text-[10px] text-transparent">·</span>
            </button>
            {(works ?? []).map((w) => (
              <BookCard
                key={w.path}
                work={w}
                onOpen={() => navigate("/work", { state: { work: w } })}
              />
            ))}
          </div>
        )}
        {works !== null && works.length === 0 && (
          <p className="text-center text-xs text-muted-foreground">
            书架还空着——从上面的「新建作品」开始第一本
          </p>
        )}
      </div>

      <Dialog open={open} onOpenChange={setOpen}>
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
            <Field>
              <FieldLabel htmlFor="work-ws">工作区（文件夹）</FieldLabel>
              <div className="flex items-center gap-2">
                <Input
                  id="work-ws"
                  value={wsRoot}
                  onChange={(e) => setWsRoot(e.currentTarget.value)}
                  placeholder="例如：D:\novels"
                  className="min-w-0 flex-1"
                />
                <Button
                  type="button"
                  variant="outline"
                  size="icon"
                  aria-label="浏览文件夹"
                  title="浏览文件夹"
                  onClick={pickWorkspace}
                >
                  <FolderOpen className="size-4" />
                </Button>
              </div>
              <p className="text-xs text-muted-foreground">
                作品将创建在该文件夹下；它同时会成为默认工作区（配置页可随时修改）。
              </p>
            </Field>
          </FieldGroup>
          {error && <p className="text-sm text-destructive">{error}</p>}
          <DialogFooter>
            <Button
              onClick={handleCreate}
              disabled={!name.trim() || !wsRoot.trim() || creating}
            >
              {creating && <Loader2 data-icon="inline-start" className="animate-spin" />}
              创建
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 应用内文件夹选择器（D56）：浏览器通道同样可用 */}
      <WorkspacePicker
        open={pickerOpen}
        onOpenChange={setPickerOpen}
        initialPath={wsRoot || worksRoot}
        onPick={setWsRoot}
      />
    </div>
  );
}
