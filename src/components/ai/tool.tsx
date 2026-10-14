import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { Badge } from "@/components/ui/badge";
import { cn } from "cn";
import {
  CheckCircleIcon,
  ChevronDownIcon,
  ClockIcon,
  WrenchIcon,
  XCircleIcon,
} from "lucide-react";
import type { ComponentProps } from "react";

/**
 * 工具调用气泡（改编自 Vercel AI Elements 的 Tool 组件，D26 路线；R3.5 优化）：
 * - 中文工具名 + 关键参数摘要常显（如「读取文件 · outline/总纲.md」）
 * - 展开查看参数（键值列表，超大字段折叠为字数提示）与结果（限高滚动）
 * - 历史会话（load_session_view 的结构化 tool_calls）与流式事件共用本组件
 */

export interface ToolCallData {
  name: string;
  /** JSON 字符串（与流式 AgentEvent.tool_call.args 同构） */
  args: string;
  /** null = 尚无结果（运行中） */
  result: string | null;
  isError?: boolean;
  /** rig call id（R8-③：并行同名工具精确配对；旧数据无此字段） */
  callId?: string | null;
}

const TOOL_LABELS: Record<string, string> = {
  read_file: "读取文件",
  propose_edit: "提交修改提议",
  propose_write: "提交新建提议",
  bash: "执行命令",
  run_flow: "执行流程",
  get_ledger: "读取账本",
  list_issues: "读取问题清单",
  list_lore: "查询设定库",
  check_visibility_leak: "泄漏检查",
  preflight_chapter: "修为预检",
};

export function toolLabel(name: string): string {
  return TOOL_LABELS[name] ?? name;
}

/** 大字段不直接渲染（展示字数摘要） */
const LARGE_FIELDS = new Set(["new_content", "content"]);
const MAX_INLINE = 300;

function safeParse(raw: string): unknown {
  try {
    return JSON.parse(raw);
  } catch {
    return raw;
  }
}

/** 参数摘要行：优先展示 path，附上 reason 等短文本 */
export function toolSummary(args: string): string {
  const parsed = safeParse(args);
  if (typeof parsed !== "object" || parsed === null) return "";
  const o = parsed as Record<string, unknown>;
  const parts: string[] = [];
  for (const key of ["path", "rel_path", "command", "kind"]) {
    const v = o[key];
    if (typeof v === "string" && v) parts.push(v);
  }
  for (const key of ["reason", "description"]) {
    const v = o[key];
    if (typeof v === "string" && v) parts.push(v);
  }
  return parts.join(" · ");
}

/** 参数键值行（跳过超大字段，标注字数） */
function argEntries(args: string): { key: string; value: string; note?: string }[] {
  const parsed = safeParse(args);
  if (typeof parsed !== "object" || parsed === null) {
    return [{ key: "原始参数", value: String(parsed).slice(0, MAX_INLINE) }];
  }
  return Object.entries(parsed as Record<string, unknown>).map(([key, v]) => {
    if (LARGE_FIELDS.has(key) && typeof v === "string") {
      return { key, value: "", note: `全文 ${v.length} 字（已折叠，见对应提议卡片）` };
    }
    const s = typeof v === "string" ? v : JSON.stringify(v);
    return { key, value: s.length > MAX_INLINE ? s.slice(0, MAX_INLINE) + "…" : s };
  });
}

export type ToolCallCardProps = {
  call: ToolCallData;
  /** 流式运行中（结果未到）：默认展开参数 */
  className?: string;
};

export function ToolCallCard({ call, className }: ToolCallCardProps) {
  const running = call.result === null;
  const error = !running && !!call.isError;
  const summary = toolSummary(call.args);
  const entries = argEntries(call.args);

  return (
    <Collapsible
      defaultOpen={running}
      className={cn("group w-full rounded-md border bg-muted/20", className)}
    >
      <CollapsibleTrigger className="flex w-full items-center gap-2 px-2.5 py-1.5 text-left">
        <WrenchIcon className="size-3.5 shrink-0 text-muted-foreground" />
        <span className="shrink-0 text-xs font-medium">{toolLabel(call.name)}</span>
        {summary && (
          <span className="min-w-0 truncate font-mono text-[11px] text-muted-foreground">
            {summary}
          </span>
        )}
        <span className="ml-auto shrink-0">
          {running ? (
            <Badge className="gap-1 rounded-full text-[10px]" variant="secondary">
              <ClockIcon className="size-3 animate-pulse" />
              运行中
            </Badge>
          ) : error ? (
            <Badge className="gap-1 rounded-full text-[10px] text-red-600" variant="secondary">
              <XCircleIcon className="size-3" />
              出错
            </Badge>
          ) : (
            <Badge className="gap-1 rounded-full text-[10px]" variant="secondary">
              <CheckCircleIcon className="size-3 text-green-600" />
              完成
            </Badge>
          )}
        </span>
        <ChevronDownIcon className="size-3.5 shrink-0 text-muted-foreground transition-transform group-data-[panel-open]:rotate-180" />
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="space-y-2 border-t px-2.5 py-2">
          <div>
            <div className="mb-1 text-[10px] font-medium tracking-wide text-muted-foreground uppercase">
              参数
            </div>
            <div className="space-y-0.5">
              {entries.map((e) => (
                <div key={e.key} className="flex gap-2 text-[11px]">
                  <span className="shrink-0 font-mono text-muted-foreground">{e.key}</span>
                  {e.note ? (
                    <span className="text-muted-foreground/70 italic">{e.note}</span>
                  ) : (
                    <span className="min-w-0 break-all whitespace-pre-wrap">{e.value}</span>
                  )}
                </div>
              ))}
            </div>
          </div>
          {!running && (
            <div>
              <div className="mb-1 text-[10px] font-medium tracking-wide text-muted-foreground uppercase">
                结果
              </div>
              <div
                className={cn(
                  "max-h-52 overflow-auto rounded-sm p-1.5",
                  error ? "bg-destructive/10 text-destructive" : "bg-muted/50",
                )}
              >
                <Code pre={prettyResult(call.result)} />
              </div>
            </div>
          )}
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}

/** 结果美化：JSON 字符串则缩进展示，否则原样 */
function prettyResult(result: string | null): string {
  if (result === null) return "";
  const parsed = safeParse(result);
  if (typeof parsed === "string") return parsed;
  return JSON.stringify(parsed, null, 2);
}

/** 轻量代码块 */
export function Code({
  pre,
  className,
  ...props
}: { pre: string } & ComponentProps<"pre">) {
  return (
    <pre
      className={cn(
        "font-mono text-[11px] leading-relaxed whitespace-pre-wrap break-all",
        className,
      )}
      {...props}
    >
      {pre}
    </pre>
  );
}
