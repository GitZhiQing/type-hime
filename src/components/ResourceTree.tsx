import { useState } from "react";
import { TreeNode } from "@/lib/tauri";
import { cn } from "cn";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  ChevronDown,
  ChevronRight,
  FileText,
  Folder,
  MoreVertical,
} from "lucide-react";

interface TreeProps {
  node: TreeNode;
  selected: string | null;
  onSelect: (node: TreeNode) => void;
  onAction: (action: "delete" | "restore", node: TreeNode) => void;
  /** 深度（根为 -1，不渲染根行） */
  depth?: number;
}

export function ResourceTree({ node, selected, onSelect, onAction, depth = -1 }: TreeProps) {
  const [open, setOpen] = useState(true);

  if (depth === -1) {
    return (
      <div className="flex flex-col">
        {node.children.map((child) => (
          <ResourceTree
            key={child.path}
            node={child}
            selected={selected}
            onSelect={onSelect}
            onAction={onAction}
            depth={0}
          />
        ))}
      </div>
    );
  }

  const pad = { paddingLeft: `${8 + depth * 14}px` };

  if (node.is_dir) {
    return (
      <div>
        <button
          type="button"
          className="flex w-full items-center gap-1 rounded-sm py-1 pr-2 text-left text-xs hover:bg-accent"
          style={pad}
          onClick={() => setOpen((v) => !v)}
        >
          {open ? (
            <ChevronDown className="size-3 shrink-0 text-muted-foreground" />
          ) : (
            <ChevronRight className="size-3 shrink-0 text-muted-foreground" />
          )}
          <Folder className="size-3.5 shrink-0 text-muted-foreground" />
          <span className="truncate">{node.name}</span>
        </button>
        {open && (
          <div>
            {node.children.map((child) => (
              <ResourceTree
                key={child.path}
                node={child}
                selected={selected}
                onSelect={onSelect}
                onAction={onAction}
                depth={depth + 1}
              />
            ))}
          </div>
        )}
      </div>
    );
  }

  return (
    <div
      className={cn(
        "group flex items-center rounded-sm text-xs hover:bg-accent",
        selected === node.path && "bg-accent"
      )}
      style={pad}
    >
      <button
        type="button"
        className="flex flex-1 items-center gap-1.5 py-1 text-left"
        onClick={() => onSelect(node)}
      >
        <FileText className="size-3.5 shrink-0 text-muted-foreground" />
        <span className="truncate">{node.name}</span>
      </button>
      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <button
              type="button"
              className="mr-1 hidden rounded-sm p-0.5 group-hover:block"
              aria-label="文件操作"
            />
          }
        >
          <MoreVertical className="size-3.5 text-muted-foreground" />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start">
          <DropdownMenuItem onClick={() => onAction("delete", node)}>
            删除（入 git 历史，可恢复）
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem onClick={() => onAction("restore", node)}>
            从 git 历史恢复
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
