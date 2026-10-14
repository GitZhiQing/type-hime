import { useState } from "react";
import { api, SettingMeta, TreeNode } from "@/lib/tauri";
import { ResourceTree } from "@/components/ResourceTree";
import { EditorPane } from "@/components/EditorPane";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
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
import { BookOpen, Sparkles } from "lucide-react";

/**
 * 文稿制品（R5-① · REDESIGN §5.5）：资源树侧栏 + 编辑器。
 * 资源树从 Workbench 左栏移入此处；删除/恢复与「新设定」入口随树驻留（留人操作）。
 */

interface DocViewProps {
  workPath: string;
  tree: TreeNode | null;
  settings: SettingMeta[];
  selected: TreeNode | null;
  onSelect: (node: TreeNode) => void;
  onAction: (action: "delete" | "restore", node: TreeNode) => void | Promise<void>;
  onFileCreated: () => void;
}

export function DocView({
  workPath,
  tree,
  settings,
  selected,
  onSelect,
  onAction,
  onFileCreated,
}: DocViewProps) {
  const [loreOpen, setLoreOpen] = useState(false);
  const [loreKind, setLoreKind] = useState("character");
  const [loreName, setLoreName] = useState("");
  const [loreContent, setLoreContent] = useState("");
  const [loreHidden, setLoreHidden] = useState(false);
  const [loreOpenAt, setLoreOpenAt] = useState("");
  const [loreBusy, setLoreBusy] = useState(false);

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
      onFileCreated();
    } finally {
      setLoreBusy(false);
    }
  }

  return (
    <div className="flex h-full min-w-0">
      {/* 资源树侧栏 */}
      <aside className="flex w-56 shrink-0 flex-col border-r">
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
                onSelect={onSelect}
                onAction={onAction}
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
      </aside>

      {/* 编辑器 */}
      <div className="min-w-0 flex-1">
        <EditorPane
          workPath={workPath}
          relPath={selected && !selected.is_dir ? selected.path : null}
          onFileCreated={onFileCreated}
        />
      </div>

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
              <Select
                items={{ character: "人物", worldview: "世界观", mystery: "悬念/伏笔" }}
                value={loreKind}
                onValueChange={(v) => v && setLoreKind(v)}
              >
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
