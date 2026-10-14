import { memo, useState, isValidElement, cloneElement, type ReactElement, type ReactNode } from "react";
import { Streamdown } from "streamdown";
import { cjk } from "@streamdown/cjk";
import { code } from "@streamdown/code";
import { Check, ChevronDown, Copy } from "lucide-react";
import { toast } from "sonner";
import { cn } from "cn";
import { installCodeCopyEnhancer, writeClipboard } from "@/components/ai/code-copy";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";

// 代码块复制增强（clipboard 失败回退 + 统一反馈）；幂等，模块加载时安装一次
installCodeCopyEnhancer();

/**
 * 代码块复制按钮（折叠卡片 header 用）：剪贴板带降级 + 勾选/toast 反馈。
 */
function CodeCopyButton({ code }: { code: string }) {
  const [ok, setOk] = useState(false);
  return (
    <button
      type="button"
      aria-label="复制代码"
      className="rounded p-1 text-muted-foreground/70 transition-colors hover:bg-accent hover:text-foreground"
      onClick={() => {
        void writeClipboard(code).then((done) => {
          if (done) {
            setOk(true);
            toast.success("已复制到剪贴板");
            window.setTimeout(() => setOk(false), 1500);
          } else {
            toast.error("复制失败：剪贴板不可用");
          }
        });
      }}
    >
      {ok ? <Check className="size-3.5 text-emerald-600" /> : <Copy className="size-3.5" />}
    </button>
  );
}

/**
 * 代码块折叠卡片（用户反馈批）：streamdown 的代码块卡片由 code 组件链渲染（pre 只是透传壳），
 * 官方 controls 无自定义控件槽——在 pre 层包一层：自建 header（语言 + 复制 + 折叠）+
 * Collapsible 内容（默认展开）。原生浮动按钮经 controls 关闭避免双复制。
 * 关键：子元素必须补 data-block:"true"，否则整块退化为行内代码（streamdown 以此判块级）。
 */
function CollapsibleCodePre({ children }: { children?: ReactNode }) {
  if (!isValidElement(children)) return <>{children}</>;
  const props = children.props as { className?: string; children?: unknown };
  const language = props.className?.match(/language-([\w+-]+)/)?.[1] ?? "";
  const raw = typeof props.children === "string" ? props.children : "";
  return (
    <Collapsible
      defaultOpen
      className="group/code-card my-2 overflow-hidden rounded-md border bg-background"
      data-collapsible-code=""
    >
      <div className="flex items-center gap-2 border-b bg-muted/40 px-2 py-1">
        <CollapsibleTrigger
          render={
            <button
              type="button"
              className="flex min-w-0 items-center gap-1.5 text-left"
              aria-label={language ? `折叠代码块（${language}）` : "折叠代码块"}
            />
          }
        >
          <ChevronDown className="size-3.5 shrink-0 text-muted-foreground transition-transform group-data-[panel-open]/code-card:rotate-180" />
          <span className="truncate font-mono text-[11px] lowercase text-muted-foreground">
            {language || "代码"}
          </span>
        </CollapsibleTrigger>
        {raw && (
          <div className="ml-auto shrink-0">
            <CodeCopyButton code={raw} />
          </div>
        )}
      </div>
      <CollapsibleContent>
        {cloneElement(children as ReactElement, { "data-block": "true" } as Record<string, unknown>)}
      </CollapsibleContent>
    </Collapsible>
  );
}

/** 模块级稳定引用：避免 components 每渲染变 Identity 导致折叠状态重置 */
const mdComponents = { pre: CollapsibleCodePre };

/**
 * 改编自 Vercel AI Elements 的 MessageResponse（源码复制路线，D26）：
 * 用 Streamdown 渲染 AI 消息的 markdown——专为流式设计，增量到达的不完整
 * 语法（未闭合的 **、列表项）不会闪烁或误渲染；块级 memo 避免整段重渲。
 *
 * 插件：cjk（中文换行/标点断行，中文小说场景必须）+ code（代码块高亮）。
 * math/mermaid 插件（AI Elements 默认带）小说场景不需要，尚未引入，需要时再加。
 * 样式：紧凑排版（对话气泡内），首末元素去外边距；代码块为折叠卡片（header 带复制/折叠）。
 */

const streamdownPlugins = { cjk, code };

export type MessageResponseProps = React.ComponentProps<typeof Streamdown>;

export const MessageResponse = memo(
  ({ className, ...props }: MessageResponseProps) => (
    <Streamdown
      className={cn(
        "md-response text-sm leading-relaxed",
        "[&>*:first-child]:mt-0 [&>*:last-child]:mb-0",
        "[&_p]:my-1.5",
        "[&_ul]:my-1.5 [&_ul]:list-disc [&_ul]:pl-5",
        "[&_ol]:my-1.5 [&_ol]:list-decimal [&_ol]:pl-5",
        "[&_li]:my-0.5",
        "[&_h1]:my-2 [&_h1]:text-base [&_h1]:font-semibold",
        "[&_h2]:my-2 [&_h2]:text-sm [&_h2]:font-semibold",
        "[&_h3]:my-1.5 [&_h3]:text-sm [&_h3]:font-medium",
        "[&_blockquote]:border-l-2 [&_blockquote]:pl-3 [&_blockquote]:text-muted-foreground",
        "[&_table]:block [&_table]:overflow-x-auto [&_table]:text-xs",
        "[&_a]:underline [&_a]:underline-offset-2",
        className,
      )}
      plugins={streamdownPlugins}
      // 原生复制/下载已由折叠卡片 header 承接（CodeCopyButton），关闭避免双按钮
      controls={{ code: { copy: false, download: false } }}
      codeBlockMaxHeight={320}
      components={mdComponents}
      {...props}
    />
  ),
  (prevProps, nextProps) => prevProps.children === nextProps.children,
);
