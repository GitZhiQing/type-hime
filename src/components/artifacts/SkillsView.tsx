import { useCallback, useEffect, useState } from "react";
import { api, SkillMeta } from "@/lib/tauri";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "cn";
import { Loader2, Save, ScrollText } from "lucide-react";
import { toast } from "sonner";

/**
 * 技能文档制品（R5-② · REDESIGN §5.5）：内置/作品级技能查看与编辑。
 * 保存走人类直写通道（works::write_file → 作品级 skills/<name>.md 同名覆盖内置层，git 留痕）。
 */

export function SkillsView({ workPath }: { workPath: string }) {
  const [skills, setSkills] = useState<SkillMeta[] | null>(null);
  const [active, setActive] = useState<string | null>(null);
  const [content, setContent] = useState("");
  const [saved, setSaved] = useState("");
  const [source, setSource] = useState<"builtin" | "work">("builtin");
  const [saving, setSaving] = useState(false);

  const reloadList = useCallback(async () => {
    try {
      const list = await api.listSkills(workPath);
      setSkills(list);
      return list;
    } catch (e) {
      toast.error(String(e));
      setSkills([]);
      return [];
    }
  }, [workPath]);

  useEffect(() => {
    reloadList();
  }, [reloadList]);

  // 选中技能 → 读取原始 md（作品层覆盖优先）
  useEffect(() => {
    if (!active) return;
    api
      .readSkill(workPath, active)
      .then((raw) => {
        setContent(raw.content);
        setSaved(raw.content);
        setSource(raw.source);
      })
      .catch((e) => toast.error(String(e)));
  }, [active, workPath]);

  async function save() {
    if (!active || saving || content === saved) return;
    setSaving(true);
    try {
      await api.writeSkill(workPath, active, content);
      setSaved(content);
      setSource("work");
      await reloadList();
      toast.success(
        `已保存 skills/${active}.md（作品级覆盖，git 留痕；对内置技能即创建覆盖）`,
      );
    } catch (e) {
      toast.error(String(e));
    } finally {
      setSaving(false);
    }
  }

  const dirty = content !== saved;

  return (
    <div className="flex h-full min-w-0">
      {/* 技能清单 */}
      <aside className="flex w-52 shrink-0 flex-col border-r">
        <div className="flex h-9 shrink-0 items-center gap-2 border-b px-3 text-xs text-muted-foreground">
          <ScrollText className="size-3.5" />
          <span className="font-medium text-foreground/80">技能文档</span>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto p-1.5">
          {skills === null ? (
            <div className="flex items-center justify-center py-8 text-muted-foreground">
              <Loader2 className="size-4 animate-spin" />
            </div>
          ) : (
            skills.map((s) => (
              <button
                key={s.name}
                type="button"
                className={cn(
                  "mb-0.5 w-full rounded px-2 py-1.5 text-left text-xs transition-colors hover:bg-accent",
                  active === s.name && "bg-accent",
                )}
                onClick={() => setActive(s.name)}
              >
                <div className="flex items-center gap-1.5">
                  <span className="truncate font-medium">{s.name}</span>
                  {s.source === "work" && (
                    <Badge variant="secondary" className="shrink-0 text-[10px]">
                      作品级
                    </Badge>
                  )}
                </div>
                <div className="truncate text-[10px] text-muted-foreground" title={s.description}>
                  {s.description}
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
              <span className="truncate font-mono text-foreground/80">
                skills/{active}.md
              </span>
              {source === "work" && (
                <Badge variant="secondary" className="shrink-0 text-[10px]">
                  作品级覆盖
                </Badge>
              )}
              {source === "builtin" && (
                <span className="text-[10px]">内置 · 编辑保存即创建作品级覆盖</span>
              )}
              <Button
                variant="ghost"
                size="sm"
                className="ml-auto h-7 px-2 text-xs"
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
            </div>
            <Textarea
              value={content}
              onChange={(e) => setContent(e.currentTarget.value)}
              spellCheck={false}
              className="min-h-0 flex-1 resize-none rounded-none border-0 font-mono text-[12px] leading-relaxed focus-visible:ring-0"
            />
          </>
        ) : (
          <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
            左侧选择技能查看/编辑；编辑内置技能保存后以作品级同名文件覆盖
          </div>
        )}
      </div>
    </div>
  );
}
