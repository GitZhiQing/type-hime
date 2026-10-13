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
  CircleIcon,
  ClockIcon,
  WrenchIcon,
  XCircleIcon,
} from "lucide-react";
import type { ComponentProps, ReactNode } from "react";
import { isValidElement } from "react";

/**
 * 改编自 Vercel AI Elements 的 Tool 组件（源码复制路线，R2 spike）：
 * - Collapsible 基座 radix → base-ui（data-state=open → data-open）
 * - `ai` 包的 ToolUIPart 类型 → 本地 ToolState 联合
 * - CodeBlock（shiki 高亮）→ 轻量 pre
 */

export type ToolProps = ComponentProps<typeof Collapsible>;

export const Tool = ({ className, ...props }: ToolProps) => (
  <Collapsible
    className={cn("group not-prose w-full rounded-md border", className)}
    {...props}
  />
);

/** 工具调用状态（与 AgentEvent 的 tool_call/tool_result 对应；approval-* 为 R4 预留） */
export type ToolState =
  | "input-streaming"
  | "input-available"
  | "output-available"
  | "output-error"
  | "approval-requested";

const statusLabels: Record<ToolState, string> = {
  "approval-requested": "待审批",
  "input-available": "运行中",
  "input-streaming": "等待",
  "output-available": "完成",
  "output-error": "出错",
};

const statusIcons: Record<ToolState, ReactNode> = {
  "approval-requested": <ClockIcon className="size-3.5 text-yellow-600" />,
  "input-available": <ClockIcon className="size-3.5 animate-pulse" />,
  "input-streaming": <CircleIcon className="size-3.5" />,
  "output-available": <CheckCircleIcon className="size-3.5 text-green-600" />,
  "output-error": <XCircleIcon className="size-3.5 text-red-600" />,
};

export const getStatusBadge = (status: ToolState) => (
  <Badge className="gap-1 rounded-full text-xs" variant="secondary">
    {statusIcons[status]}
    {statusLabels[status]}
  </Badge>
);

export type ToolHeaderProps = {
  title?: string;
  state: ToolState;
  className?: string;
} & Omit<ComponentProps<typeof CollapsibleTrigger>, "title">;

export const ToolHeader = ({
  className,
  title,
  state,
  ...props
}: ToolHeaderProps) => (
  <CollapsibleTrigger
    className={cn(
      "group flex w-full items-center justify-between gap-4 px-3 py-2",
      className,
    )}
    {...props}
  >
    <div className="flex items-center gap-2">
      <WrenchIcon className="size-4 text-muted-foreground" />
      <span className="text-sm font-medium">{title}</span>
      {getStatusBadge(state)}
    </div>
    <ChevronDownIcon className="size-4 text-muted-foreground transition-transform group-data-[panel-open]:rotate-180" />
  </CollapsibleTrigger>
);

export type ToolContentProps = ComponentProps<typeof CollapsibleContent>;

export const ToolContent = ({ className, ...props }: ToolContentProps) => (
  <CollapsibleContent className={cn("space-y-3 p-3 text-sm", className)} {...props} />
);

export type ToolInputProps = ComponentProps<"div"> & {
  input: unknown;
};

export const ToolInput = ({ className, input, ...props }: ToolInputProps) => (
  <div className={cn("space-y-1.5 overflow-hidden", className)} {...props}>
    <h4 className="text-xs font-medium tracking-wide text-muted-foreground uppercase">
      参数
    </h4>
    <div className="rounded-md bg-muted/50">
      <Code pre={JSON.stringify(input, null, 2)} />
    </div>
  </div>
);

export type ToolOutputProps = ComponentProps<"div"> & {
  output: unknown;
  errorText?: string | null;
};

export const ToolOutput = ({
  className,
  output,
  errorText,
  ...props
}: ToolOutputProps) => {
  if (!(output || errorText)) {
    return null;
  }

  let Output = <div>{output as ReactNode}</div>;
  if (typeof output === "object" && output !== null && !isValidElement(output)) {
    Output = <Code pre={JSON.stringify(output, null, 2)} />;
  } else if (typeof output === "string") {
    Output = <Code pre={output} />;
  }

  return (
    <div className={cn("space-y-1.5", className)} {...props}>
      <h4 className="text-xs font-medium tracking-wide text-muted-foreground uppercase">
        {errorText ? "错误" : "结果"}
      </h4>
      <div
        className={cn(
          "overflow-x-auto rounded-md text-xs",
          errorText
            ? "bg-destructive/10 text-destructive"
            : "bg-muted/50 text-foreground",
        )}
      >
        {errorText && <div className="p-2">{errorText}</div>}
        {Output}
      </div>
    </div>
  );
};

/** 轻量代码块（替代 AI Elements 的 shiki CodeBlock） */
export function Code({ pre, className }: { pre: string; className?: string }) {
  return (
    <pre
      className={cn(
        "max-h-60 overflow-auto p-2 font-mono text-xs leading-relaxed whitespace-pre-wrap break-all",
        className,
      )}
    >
      {pre}
    </pre>
  );
}
