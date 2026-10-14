import { useCallback, useEffect, useState } from "react";
import { api, AgentMeta } from "@/lib/tauri";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Textarea } from "@/components/ui/textarea";
import { Input } from "@/components/ui/input";
import { ArrowLeft, Bot, Loader2, Plus, Save, Trash2 } from "lucide-react";
import { MessageResponse } from "@/components/ai/response";
import { useNavigate } from "react-router-dom";
import { cn } from "cn";
import { toast } from "sonner";

/**
 * 子 Agent 管理页（R8-②/⑤ · 多 Agent 团队）：
 * master-detail（仿技能文档）——左侧团队清单（内置/自定义），右侧编辑。
 * 定义 = md + frontmatter（name/description/model/tools/skills/max_turns），正文即系统提示词。
 * 保存落用户级（app data/agents/*.md，跨作品共享）；内置 general 可覆盖但不可删。
 */

const NEW_AGENT_TEMPLATE = (name: string) =>
  `---\nname: ${name}\ndescription: 一句话说明何时委派这个子 Agent\nmax_turns: 12\n---\n\n你是 type-hime 的子 Agent（${name}）。\n\n职责：\n- （描述这个子 Agent 做什么、遵循什么规程）\n\n要求：\n- 一切作品文件修改走 propose_edit / propose_write 提议；\n- 结论与清单写在最终回复里（根 Agent 只读你的文本）。\n`;

export function Agents() {
  const navigate = useNavigate();
  const [agents, setAgents] = useState<AgentMeta[] | null>(null);
  const [active, setActive] = useState<string | null>(null);
  const [content, setContent] = useState("");
  const [saved, setSaved] = useState("");
  const [source, setSource] = useState<"builtin" | "user">("builtin");
  const [saving, setSaving] = useState(false);
  const [preview, setPreview] = useState(false);
  const [newOpen, setNewOpen] = useState(false);
  const [newName, setNewName] = useState("");

  const reloadList = useCallback(async () => {
    try {
      const list = await api.listAgents();
      setAgents(list);
      return list;
    } catch (e) {
      toast.error(String(e));
      setAgents([]);
      return [];
    }
  }, []);

  useEffect(() => {
    reloadList().then((list) => {
      if (list.length > 0 && !active) setActive(list[0].name);
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [reloadList]);

  useEffect(() => {
    if (!active) return;
    setPreview(false);
    api
      .readAgent(active)
      .then((raw) => {
        setContent(raw.content);
        setSaved(raw.content);
        setSource(raw.source);
      })
      .catch((e) => toast.error(String(e)));
  }, [active]);

  async function save() {
    if (!active || saving || content === saved) return;
    setSaving(true);
    try {
      await api.writeAgent(active, content);
      setSaved(content);
      setSource("user");
      await reloadList();
      toast.success(`已保存 ${active}（用户级定义，跨作品生效；根 Agent 下次委派即用新提示词）`);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setSaving(false);
    }
  }

  async function remove(name: string) {
    try {
      await api.deleteAgent(name);
      toast.success(`已删除用户级定义 ${name}`);
      if (active === name) setActive(null);
      const list = await reloadList();
      if (list.length > 0) setActive(list[0].name);
    } catch (e) {
      toast.error(String(e));
    }
  }

  async function create() {
    const name = newName.trim();
    if (!name) return;
    try {
      await api.writeAgent(name, NEW_AGENT_TEMPLATE(name));
      setNewOpen(false);
      setNewName("");
      await reloadList();
      setActive(name);
      toast.success(`已创建子 Agent ${name}（基于通用模板）`);
    } catch (e) {
      toast.error(String(e));
    }
  }

  const dirty = content !== saved;

  return (
    <div className="flex h-screen flex-col bg-background text-foreground">
      <header className="flex h-12 shrink-0 items-center gap-3 border-b px-3">
        <Button variant="ghost" size="sm" className="h-8 px-2" onClick={() => navigate(-1)}>
          <ArrowLeft data-icon="inline-start" />
          返回
        </Button>
        <Bot className="size-4 text-muted-foreground" />
        <span className="text-sm font-semibold">Agents · 子 Agent 团队</span>
        <span className="text-xs text-muted-foreground">
          根 Agent 通过 spawn_agent 委派它们干活；定义存用户级，跨作品共享
        </span>
        <Button
          variant="outline"
          size="sm"
          className="ml-auto h-8 px-2 text-xs"
          onClick={() => setNewOpen(true)}
        >
          <Plus data-icon="inline-start" />
          新建子 Agent
        </Button>
      </header>

      <div className="flex min-h-0 flex-1">
        {/* 团队清单 */}
        <aside className="flex w-64 shrink-0 flex-col border-r">
          <div className="min-h-0 flex-1 overflow-y-auto p-1.5">
            {agents === null ? (
              <div className="flex items-center justify-center py-8 text-muted-foreground">
                <Loader2 className="size-4 animate-spin" />
              </div>
            ) : (
              agents.map((a) => (
                <button
                  key={a.name}
                  type="button"
                  className={cn(
                    "mb-0.5 w-full rounded px-2 py-1.5 text-left text-xs transition-colors hover:bg-accent",
                    active === a.name && "bg-accent",
                  )}
                  onClick={() => setActive(a.name)}
                >
                  <div className="flex items-center gap-1.5">
                    <span className="truncate font-medium">{a.name}</span>
                    {a.source === "builtin" && (
                      <Badge variant="secondary" className="shrink-0 text-[10px]">
                        内置
                      </Badge>
                    )}
                  </div>
                  <div className="truncate text-[10px] text-muted-foreground" title={a.description}>
                    {a.description}
                  </div>
                </button>
              ))
            )}
          </div>
        </aside>

        {/* 编辑区 */}
        <div className="flex min-w-0 flex-1 flex-col">
          {active ? (
            <>
              <div className="flex h-9 shrink-0 items-center gap-2 border-b px-3 text-xs text-muted-foreground">
                <span className="truncate font-mono text-foreground/80">{active}.md</span>
                {source === "user" && (
                  <Badge variant="secondary" className="shrink-0 text-[10px]">
                    用户级定义
                  </Badge>
                )}
                {source === "builtin" && (
                  <span className="text-[10px]">内置 · 保存即创建用户级覆盖</span>
                )}
                <span className="ml-auto flex items-center gap-1">
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-7 px-2 text-xs"
                    disabled={!dirty || saving}
                    onClick={save}
                  >
                    {saving ? (
                      <Loader2 data-icon="inline-start" className="animate-spin" />
                    ) : (
                      <Save data-icon="inline-start" />
                    )}
                    保存
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-7 px-2 text-xs text-muted-foreground"
                    onClick={() => setPreview((p) => !p)}
                  >
                    {preview ? "编辑" : "预览"}
                  </Button>
                  {source === "user" && (
                    <Button
                      variant="ghost"
                      size="sm"
                      className="h-7 px-2 text-xs text-destructive"
                      onClick={() => remove(active)}
                    >
                      <Trash2 data-icon="inline-start" />
                      删除
                    </Button>
                  )}
                </span>
              </div>
              {preview ? (
                <div className="min-h-0 flex-1 overflow-y-auto">
                  <div className="mx-auto max-w-3xl px-6 py-4">
                    <MessageResponse className="text-[13px]">{content}</MessageResponse>
                  </div>
                </div>
              ) : (
                <Textarea
                  value={content}
                  onChange={(e) => setContent(e.currentTarget.value)}
                  spellCheck={false}
                  className="min-h-0 flex-1 resize-none rounded-none border-0 font-mono text-[12px] leading-relaxed focus-visible:ring-0"
                />
              )}
            </>
          ) : (
            <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
              左侧选择子 Agent 查看/编辑，或右上角新建
            </div>
          )}
        </div>
      </div>

      {/* 新建（轻量内联条，避免 Dialog 阻塞） */}
      {newOpen && (
        <div className="absolute inset-x-0 top-12 z-10 mx-auto flex w-[420px] items-center gap-2 rounded-b-lg border border-t-0 bg-popover p-3 shadow-md">
          <Input
            value={newName}
            onChange={(e) => setNewName(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") create();
              if (e.key === "Escape") setNewOpen(false);
            }}
            placeholder="子 Agent 名（字母/数字/-/_，如 writer、editor）"
            autoFocus
          />
          <Button size="sm" onClick={create} disabled={!newName.trim()}>
            创建
          </Button>
          <Button size="sm" variant="ghost" onClick={() => setNewOpen(false)}>
            取消
          </Button>
        </div>
      )}
    </div>
  );
}
