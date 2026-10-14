import { SettingMeta, TreeNode } from "@/lib/tauri";
import { ResourceTree } from "@/components/ResourceTree";
import { EditorPane } from "@/components/EditorPane";
import { ScrollArea } from "@/components/ui/scroll-area";
import { BookOpen } from "lucide-react";

/**
 * 文稿制品（R5-① · REDESIGN §5.5）：资源树侧栏 + 编辑器。
 * 资源树从 Workbench 左栏移入此处；删除/恢复随树驻留（留人操作，§11 保留清单）。
 * 新建设定/新建文件无按钮入口（§11「其余一切经聊天」：自然语言或 propose_write 建条目，
 * 可见性控件在编辑器保留）。
 */

interface DocViewProps {
  workPath: string;
  tree: TreeNode | null;
  settings: SettingMeta[];
  selected: TreeNode | null;
  onSelect: (node: TreeNode) => void;
  onAction: (action: "delete" | "restore", node: TreeNode) => void | Promise<void>;
}

export function DocView({
  workPath,
  tree,
  settings,
  selected,
  onSelect,
  onAction,
}: DocViewProps) {
  return (
    <div className="flex h-full min-w-0">
      {/* 资源树侧栏 */}
      <aside className="flex w-56 shrink-0 flex-col border-r">
        <div className="flex h-9 shrink-0 items-center gap-2 border-b px-3 text-xs font-medium text-muted-foreground">
          <BookOpen className="size-3.5" />
          资源树
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
        <EditorPane workPath={workPath} relPath={selected && !selected.is_dir ? selected.path : null} />
      </div>
    </div>
  );
}
