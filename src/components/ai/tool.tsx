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
import { CodeHighlight, prettyMaybeJson } from "@/components/ai/code-highlight";

/**
 * 工具调用气泡（改编自 Vercel AI Elements 的 Tool 组件，D26 路线）：
 * - 中文工具名 + 关键参数摘要常显（如「读取文件 · outline/总纲.md」）
 * - **默认折叠**（含运行中）；展开后直接展示**原始 JSON 参数**
 *   （JSON 语法高亮；空参数显示「无参数」）与结果（限高滚动）
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
  spawn_agent: "委派子 Agent",
  get_pipeline: "查询流水线",
  record_issues: "落盘问题清单",
  git_commit: "git 提交",
  get_ledger: "读取账本",
  list_issues: "读取问题清单",
  list_lore: "查询设定库",
  check_visibility_leak: "泄漏检查",
  preflight_chapter: "修为预检",
};

export function toolLabel(name: string): string {
  return TOOL_LABELS[name] ?? name;
}

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

/** 参数是否为空（无参数工具 / 空 JSON 对象）：展示「无参数」而非空块 */
export function isEmptyArgs(args: string): boolean {
  const t = args.trim();
  if (t === "" || t === "{}" || t === "null" || t === "[]") return true;
  const parsed = safeParse(t);
  return (
    typeof parsed === "object" &&
    parsed !== null &&
    !Array.isArray(parsed) &&
    Object.keys(parsed as Record<string, unknown>).length === 0
  );
}

export type ToolCallCardProps = {
  call: ToolCallData;
  className?: string;
};

export function ToolCallCard({ call, className }: ToolCallCardProps) {
  const running = call.result === null;
  const error = !running && !!call.isError;
  const summary = toolSummary(call.args);
  const emptyArgs = isEmptyArgs(call.args);

  return (
    <Collapsible
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
              <ClockIcon className="size-3" />
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
            {emptyArgs ? (
              <div className="text-[11px] text-muted-foreground/70 italic">无参数</div>
            ) : (
              <div className="max-h-52 overflow-auto rounded-sm bg-muted/50 p-1.5">
                <CodeHighlight text={prettyMaybeJson(call.args)} />
              </div>
            )}
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
                {error ? (
                  <Code pre={call.result ?? ""} />
                ) : (
                  <CodeHighlight text={prettyMaybeJson(call.result ?? "")} />
                )}
              </div>
            </div>
          )}
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}

/** 轻量代码块（非 JSON 结果/错误文本的等宽展示；JSON 走 CodeHighlight） */
export function Code({
  pre,
  className,
  ...props
}: { pre: string } & React.ComponentProps<"pre">) {
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
