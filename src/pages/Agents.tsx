import { useCallback, useEffect, useState } from "react";
import { api, AgentMeta, AgentProfile, ToolMeta } from "@/lib/tauri";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Textarea } from "@/components/ui/textarea";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { ArrowLeft, Bot, ChevronDown, ChevronRight, Loader2, Plus, Save, Trash2 } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { cn } from "cn";
import { toast } from "sonner";
import {
  AGENT_COLOR_KEYS,
  AGENT_DOT,
  resolveAgentColor,
} from "@/lib/agent-colors";

/**
 * 子 Agent 管理页（R8-②/⑤，R9 结构化表单 · ZCode 子智能体配置同构）：
 * 左侧团队清单（色点+内置徽标），右侧卡片表单——
 * 名称 / 颜色标记 / 模型（继承默认或自定义）/ 描述 / 可用工具（多选，全选=默认全部）/ 系统提示词；
 * 「源码」切换保留 md 直接编辑（高级）。存储仍为 md+frontmatter（单一格式源在后端 format_profile）。
 */

const DEFAULT_BODY =
  "你是码字姬的子 Agent。\n\n职责：\n- （描述这个子 Agent 做什么、遵循什么规程）\n\n要求：\n- 一切作品文件修改走 propose_edit / propose_write 提议；\n- 结论与清单写在最终回复里（根 Agent 只读你的文本）。";

function emptyProfile(): AgentProfile {
  return {
    name: "",
    description: "",
    model: null,
    color: null,
    tools: [],
    skills: [],
    max_turns: 12,
    body: DEFAULT_BODY,
  };
}

export function Agents() {
  const navigate = useNavigate();
  const [agents, setAgents] = useState<AgentMeta[] | null>(null);
  const [active, setActive] = useState<string | null>(null);
  const [toolMeta, setToolMeta] = useState<ToolMeta[]>([]);
  const [view, setView] = useState<"form" | "source">("form");
  const [isNew, setIsNew] = useState(false);

  // 表单态（结构化）
  const [form, setForm] = useState<AgentProfile | null>(null);
  const [formSource, setFormSource] = useState<"builtin" | "user">("builtin");
  const [toolsOpen, setToolsOpen] = useState(false);
  const [customModel, setCustomModel] = useState(false);
  const [saving, setSaving] = useState(false);

  // 源码态
  const [srcContent, setSrcContent] = useState("");
  const [srcSaved, setSrcSaved] = useState("");

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
      if (list.length > 0) setActive((cur) => cur ?? list[0].name);
    });
    api.listTools().then(setToolMeta).catch(() => {});
  }, [reloadList]);

  // 选中项 → 读结构化表单 + 源码原文
  useEffect(() => {
    if (!active || isNew) return;
    setView("form");
    setToolsOpen(false);
    Promise.all([api.getAgentProfile(active), api.readAgent(active)])
      .then(([p, raw]) => {
        setForm(p);
        setFormSource(raw.source);
        setCustomModel(!!p.model);
        setSrcContent(raw.content);
        setSrcSaved(raw.content);
      })
      .catch((e) => toast.error(String(e)));
  }, [active, isNew]);

  function startNew() {
    setIsNew(true);
    setActive(null);
    setView("form");
    setToolsOpen(false);
    setCustomModel(false);
    setForm(emptyProfile());
  }

  function cancelForm() {
    if (isNew) {
      setIsNew(false);
      setForm(null);
      reloadList().then((list) => {
        if (list.length > 0) setActive(list[0].name);
      });
      return;
    }
    // 回滚到初始值
    if (active) {
      api.getAgentProfile(active).then((p) => {
        setForm(p);
        setCustomModel(!!p.model);
      });
    }
  }

  async function saveForm() {
    if (!form || saving) return;
    setSaving(true);
    try {
      await api.saveAgentProfile({ ...form, name: form.name.trim() });
      toast.success(
        isNew
          ? `已创建子 Agent ${form.name}（用户级定义，跨作品共享）`
          : `已保存 ${form.name}（根 Agent 下次委派即用新配置）`,
      );
      setIsNew(false);
      await reloadList();
      setActive(form.name.trim());
    } catch (e) {
      toast.error(String(e));
    } finally {
      setSaving(false);
    }
  }

  async function saveSource() {
    if (!active || srcContent === srcSaved) return;
    setSaving(true);
    try {
      await api.writeAgent(active, srcContent);
      setSrcSaved(srcContent);
      await reloadList();
      toast.success(`已保存源码（${active}.md）`);
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
      setActive(null);
      setForm(null);
      const list = await reloadList();
      if (list.length > 0) setActive(list[0].name);
    } catch (e) {
      toast.error(String(e));
    }
  }

  /** 勾选集合（全选归一化为空 = 默认全部工具） */
  const checkedTools = form
    ? form.tools.length === 0
      ? toolMeta.map((t) => t.name)
      : form.tools
    : [];

  function toggleTool(name: string) {
    if (!form) return;
    const cur = new Set(checkedTools);
    if (cur.has(name)) cur.delete(name);
    else cur.add(name);
    const all = toolMeta.map((t) => t.name);
    const next = all.every((n) => cur.has(n)) ? [] : all.filter((n) => cur.has(n));
    setForm({ ...form, tools: next });
  }

  const formDirty = !!form && form.name.trim() !== "" && !saving;

  return (
    <div className="flex h-screen flex-col bg-background text-foreground">
      <header className="flex h-12 shrink-0 items-center gap-3 border-b px-3">
        <Button variant="ghost" size="sm" className="h-8 px-2" onClick={() => navigate(-1)}>
          <ArrowLeft data-icon="inline-start" />
          返回
        </Button>
        <Bot className="size-4 text-muted-foreground" />
        <span className="text-sm font-semibold">子 Agent 团队</span>
        <span className="text-xs text-muted-foreground">
          根 Agent 通过 spawn_agent 委派它们干活；定义存用户级，跨作品共享
        </span>
        <Button
          variant="outline"
          size="sm"
          className="ml-auto h-8 px-2 text-xs"
          onClick={startNew}
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
                    active === a.name && !isNew && "bg-accent",
                  )}
                  onClick={() => {
                    setIsNew(false);
                    setActive(a.name);
                  }}
                >
                  <div className="flex items-center gap-1.5">
                    <span
                      className={cn(
                        "size-2 shrink-0 rounded-full",
                        AGENT_DOT[resolveAgentColor(a.name, a.color)],
                      )}
                    />
                    <span className="truncate font-medium">{a.name}</span>
                    {a.source === "builtin" && (
                      <Badge variant="secondary" className="shrink-0 text-[10px]">
                        内置
                      </Badge>
                    )}
                  </div>
                  <div className="truncate pl-3.5 text-[10px] text-muted-foreground" title={a.description}>
                    {a.description}
                  </div>
                </button>
              ))
            )}
          </div>
        </aside>

        {/* 右区：表单 / 源码 */}
        <div className="min-h-0 min-w-0 flex-1 overflow-y-auto">
          {form ? (
            <div className="mx-auto max-w-2xl p-6">
              {/* 卡片头：标题 + 视图切换 */}
              <div className="mb-1 flex items-center gap-2">
                <h2 className="text-base font-semibold">
                  {isNew ? "新建子 Agent" : `编辑 ${form.name}`}
                </h2>
                {!isNew && formSource === "user" && (
                  <Badge variant="secondary" className="text-[10px]">
                    用户级定义
                  </Badge>
                )}
                {!isNew && formSource === "builtin" && (
                  <span className="text-[10px] text-muted-foreground">
                    内置 · 保存即创建用户级覆盖
                  </span>
                )}
                <span className="ml-auto flex gap-1">
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-7 px-2 text-xs"
                    onClick={() => setView(view === "form" ? "source" : "form")}
                  >
                    {view === "form" ? "源码" : "表单"}
                  </Button>
                  {!isNew && formSource === "user" && (
                    <Button
                      variant="ghost"
                      size="sm"
                      className="h-7 px-2 text-xs text-destructive"
                      onClick={() => remove(form.name)}
                    >
                      <Trash2 data-icon="inline-start" />
                      删除
                    </Button>
                  )}
                </span>
              </div>
              <p className="mb-4 text-xs text-muted-foreground">
                {view === "form"
                  ? "填写名称、工具与系统提示词；保存后返回列表。"
                  : "直接编辑 md 源码（frontmatter + 正文=系统提示词）；保存后与表单视图共享同一文件。"}
              </p>

              {view === "form" ? (
                <div className="rounded-lg border p-4">
                  {/* 行 1：名称 / 颜色标记 / 模型 */}
                  <div className="grid grid-cols-[1fr_auto_1fr] gap-4">
                    <label className="block">
                      <span className="mb-1.5 block text-sm">名称</span>
                      <Input
                        value={form.name}
                        onChange={(e) =>
                          setForm({ ...form, name: e.currentTarget.value.replace(/[^a-zA-Z0-9_-]/g, "") })
                        }
                        placeholder="writer / editor / general"
                        readOnly={!isNew}
                        className={cn("font-mono", !isNew && "bg-muted/40")}
                      />
                      {!isNew && (
                        <span className="mt-1 block text-[10px] text-muted-foreground">
                          名称即文件名，创建后不可改（改用新建）
                        </span>
                      )}
                    </label>
                    <div>
                      <span className="mb-1.5 block text-sm">颜色标记</span>
                      <div className="flex items-center gap-1.5 rounded-md border px-2 py-1.5">
                        {AGENT_COLOR_KEYS.map((k) => (
                          <button
                            key={k}
                            type="button"
                            aria-label={`颜色 ${k}`}
                            className={cn(
                              "size-4 rounded-full transition-transform",
                              AGENT_DOT[k],
                              form.color === k
                                ? "ring-2 ring-foreground/60 ring-offset-1"
                                : "hover:scale-110",
                            )}
                            onClick={() => setForm({ ...form, color: form.color === k ? null : k })}
                          />
                        ))}
                      </div>
                    </div>
                    <label className="block">
                      <span className="mb-1.5 block text-sm">模型</span>
                      <Select
                        items={{
                          __default__: "继承默认",
                          ...(form.model ? { [form.model]: form.model } : {}),
                          __custom__: "自定义…",
                        }}
                        value={form.model || "__default__"}
                        onValueChange={(v) => {
                          if (!v) return;
                          if (v === "__default__") {
                            setCustomModel(false);
                            setForm({ ...form, model: null });
                          } else if (v === "__custom__") {
                            setCustomModel(true);
                            setForm({ ...form, model: "" });
                          } else {
                            setCustomModel(false);
                            setForm({ ...form, model: v });
                          }
                        }}
                      >
                        <SelectTrigger>
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value="__default__">继承默认</SelectItem>
                          {form.model && <SelectItem value={form.model}>{form.model}</SelectItem>}
                          <SelectItem value="__custom__">自定义…</SelectItem>
                        </SelectContent>
                      </Select>
                      {customModel && (
                        <Input
                          value={form.model ?? ""}
                          onChange={(e) => setForm({ ...form, model: e.currentTarget.value })}
                          placeholder="模型 id（如 deepseek-chat）"
                          className="mt-1.5 font-mono text-xs"
                        />
                      )}
                    </label>
                  </div>

                  {/* 描述 */}
                  <label className="mt-4 block">
                    <span className="mb-1.5 block text-sm">描述</span>
                    <Input
                      value={form.description}
                      onChange={(e) => setForm({ ...form, description: e.currentTarget.value })}
                      placeholder="展示给根 Agent 的简短说明（决定何时委派它）"
                    />
                  </label>

                  {/* 可用工具（折叠面板多选；全选=默认全部） */}
                  <div className="mt-4">
                    <span className="mb-1.5 block text-sm">可用工具</span>
                    <button
                      type="button"
                      className="flex w-full items-center gap-2 rounded-md border px-3 py-2 text-left text-sm transition-colors hover:bg-accent/50"
                      onClick={() => setToolsOpen((o) => !o)}
                      aria-expanded={toolsOpen}
                    >
                      {toolsOpen ? (
                        <ChevronDown className="size-3.5 text-muted-foreground" />
                      ) : (
                        <ChevronRight className="size-3.5 text-muted-foreground" />
                      )}
                      {form.tools.length === 0
                        ? "默认（全部工具）"
                        : `已选 ${form.tools.length} / ${toolMeta.length} 项`}
                      <span className="ml-auto text-[11px] text-muted-foreground">
                        控制该子 Agent 可以调用的工具范围；全选即默认全部
                      </span>
                    </button>
                    {toolsOpen && (
                      <ul className="mt-1 divide-y rounded-md border">
                        {toolMeta.map((t) => {
                          const on = checkedTools.includes(t.name);
                          return (
                            <li key={t.name}>
                              <label className="flex cursor-pointer items-center gap-2.5 px-3 py-1.5 transition-colors hover:bg-accent/40">
                                <input type="checkbox" checked={on} onChange={() => toggleTool(t.name)} />
                                <span className="w-20 shrink-0 text-xs font-medium">{t.label}</span>
                                <span className="min-w-0 flex-1 truncate text-[11px] text-muted-foreground">
                                  {t.summary}
                                </span>
                                <span className="shrink-0 font-mono text-[10px] text-muted-foreground/70">
                                  {t.name}
                                </span>
                              </label>
                            </li>
                          );
                        })}
                      </ul>
                    )}
                  </div>

                  {/* 系统提示词 */}
                  <label className="mt-4 block">
                    <span className="mb-1.5 block text-sm">系统提示词</span>
                    <Textarea
                      value={form.body}
                      onChange={(e) => setForm({ ...form, body: e.currentTarget.value })}
                      placeholder="描述这个子 Agent 的角色、边界和规则…"
                      className="min-h-44 font-mono text-[12px] leading-relaxed"
                    />
                  </label>

                  {/* footer */}
                  <div className="mt-4 flex justify-end gap-2">
                    <Button variant="ghost" size="sm" onClick={cancelForm} disabled={saving}>
                      取消
                    </Button>
                    <Button
                      size="sm"
                      onClick={saveForm}
                      disabled={!formDirty || !form.description.trim() || !form.body.trim()}
                    >
                      {saving ? (
                        <Loader2 data-icon="inline-start" className="animate-spin" />
                      ) : (
                        <Save data-icon="inline-start" />
                      )}
                      {isNew ? "创建" : "保存"}
                    </Button>
                  </div>
                </div>
              ) : (
                <div className="rounded-lg border">
                  <div className="flex h-9 items-center gap-2 border-b px-3 text-xs text-muted-foreground">
                    <span className="font-mono text-foreground/80">{form.name}.md</span>
                    <Button
                      variant="ghost"
                      size="sm"
                      className="ml-auto h-7 px-2 text-xs"
                      disabled={srcContent === srcSaved || saving}
                      onClick={saveSource}
                    >
                      保存源码
                    </Button>
                  </div>
                  <Textarea
                    value={srcContent}
                    onChange={(e) => setSrcContent(e.currentTarget.value)}
                    spellCheck={false}
                    className="min-h-80 resize-y rounded-none border-0 font-mono text-[12px] leading-relaxed focus-visible:ring-0"
                  />
                  <div className="border-t px-3 py-2 text-[10px] text-muted-foreground">
                    提示：表单保存会用 format 归一化 frontmatter；此处手写容忍逗号串等宽松格式。
                  </div>
                </div>
              )}
            </div>
          ) : (
            <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
              左侧选择子 Agent，或右上角新建
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
