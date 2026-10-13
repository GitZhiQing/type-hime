import { useEffect, useRef, useState } from "react";
import { api, SessionMeta } from "@/lib/tauri";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "cn";
import { toast } from "sonner";
import { MessageSquarePlus, Pencil, Search, Trash2 } from "lucide-react";

interface SessionListProps {
  scope: "work" | "global";
  /** scope=work 时必填 */
  workPath?: string;
  sessionId: string | null;
  onSelect: (id: string) => void;
  /** 新建会话：父层生成 id、设为当前并返回 id */
  onNew: () => string;
  /** 父层在一轮对话完成后 +1 触发刷新（自动标题落盘） */
  refreshKey?: number;
}

/** 会话列表（R1）：新建 / 切换 / 搜索 / 重命名 / 删除，作品会话与全局会话共用。 */
export function SessionList({
  scope,
  workPath,
  sessionId,
  onSelect,
  onNew,
  refreshKey = 0,
}: SessionListProps) {
  const [list, setList] = useState<SessionMeta[]>([]);
  const [query, setQuery] = useState("");
  const [renaming, setRenaming] = useState<string | null>(null);
  const [renameText, setRenameText] = useState("");
  const autoSelected = useRef(false);

  useEffect(() => {
    const p =
      scope === "work" && workPath
        ? api.listSessions(workPath)
        : api.listSessionsGlobal();
    p.then((l) => {
      setList(l);
      if (!autoSelected.current) {
        autoSelected.current = true;
        if (sessionId === null && l.length > 0) onSelect(l[0].id);
      }
    }).catch((e) => toast.error(String(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scope, workPath, refreshKey]);

  const q = query.trim();
  const filtered = list.filter(
    (s) => !q || s.title.includes(q) || s.preview.includes(q),
  );

  function handleNew() {
    onNew();
  }

  async function commitRename(id: string) {
    const title = renameText.trim();
    setRenaming(null);
    if (!title) return;
    try {
      if (scope === "work" && workPath) {
        await api.renameSession(workPath, id, title);
      } else {
        await api.renameSessionGlobal(id, title);
      }
      setList((l) => l.map((s) => (s.id === id ? { ...s, title } : s)));
    } catch (e) {
      toast.error(String(e));
    }
  }

  async function handleDelete(id: string) {
    if (!window.confirm("删除该会话？历史不可恢复。")) return;
    try {
      if (scope === "work" && workPath) {
        await api.deleteSession(workPath, id);
      } else {
        await api.deleteSessionGlobal(id);
      }
      setList((l) => l.filter((s) => s.id !== id));
      if (sessionId === id) onNew();
    } catch (e) {
      toast.error(String(e));
    }
  }

  return (
    <div className="flex h-full flex-col">
      <div className="flex h-9 shrink-0 items-center gap-2 border-b px-3">
        <span className="text-xs font-medium text-muted-foreground">会话</span>
        <Button
          variant="ghost"
          size="sm"
          className="ml-auto h-7 px-2 text-xs"
          onClick={handleNew}
        >
          <MessageSquarePlus data-icon="inline-start" />
          新会话
        </Button>
      </div>

      <div className="shrink-0 border-b p-2">
        <div className="relative">
          <Search className="absolute top-1/2 size-3.5 -translate-y-1/2 left-2 text-muted-foreground" />
          <Input
            value={query}
            onChange={(e) => setQuery(e.currentTarget.value)}
            placeholder="搜索会话"
            className="h-7 pl-7 text-xs"
          />
        </div>
      </div>

      <ScrollArea className="min-h-0 flex-1">
        <div className="flex flex-col p-1">
          {filtered.map((s) => (
            <div
              key={s.id}
              className={cn(
                "group flex items-center gap-1 rounded-md px-1.5 py-1 hover:bg-accent",
                sessionId === s.id && "bg-accent",
              )}
            >
              {renaming === s.id ? (
                <Input
                  autoFocus
                  value={renameText}
                  onChange={(e) => setRenameText(e.currentTarget.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") commitRename(s.id);
                    if (e.key === "Escape") setRenaming(null);
                  }}
                  onBlur={() => commitRename(s.id)}
                  className="h-6 text-xs"
                />
              ) : (
                <button
                  type="button"
                  className="min-w-0 flex-1 text-left"
                  onClick={() => onSelect(s.id)}
                  title={s.preview}
                >
                  <div className="truncate text-xs font-medium">
                    {s.title || "（空）"}
                  </div>
                  <div className="truncate text-[10px] text-muted-foreground">
                    {s.created_at}
                  </div>
                </button>
              )}
              {renaming !== s.id && (
                <div className="hidden shrink-0 gap-0.5 group-hover:flex">
                  <Button
                    variant="ghost"
                    size="sm"
                    className="size-6 p-0"
                    aria-label="重命名"
                    onClick={() => {
                      setRenaming(s.id);
                      setRenameText(s.title);
                    }}
                  >
                    <Pencil className="size-3" />
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="size-6 p-0"
                    aria-label="删除"
                    onClick={() => handleDelete(s.id)}
                  >
                    <Trash2 className="size-3" />
                  </Button>
                </div>
              )}
            </div>
          ))}
          {filtered.length === 0 && (
            <div className="p-3 text-xs text-muted-foreground">暂无会话</div>
          )}
        </div>
      </ScrollArea>
    </div>
  );
}
