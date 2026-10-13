import { memo } from "react";
import { Streamdown } from "streamdown";
import { cjk } from "@streamdown/cjk";
import { code } from "@streamdown/code";
import { cn } from "cn";

/**
 * 改编自 Vercel AI Elements 的 MessageResponse（源码复制路线，D26）：
 * 用 Streamdown 渲染 AI 消息的 markdown——专为流式设计，增量到达的不完整
 * 语法（未闭合的 **、列表项）不会闪烁或误渲染；块级 memo 避免整段重渲。
 *
 * 插件：cjk（中文换行/标点断行，中文小说场景必须）+ code（代码块高亮）。
 * math/mermaid 插件（AI Elements 默认带）小说场景不需要，尚未引入，需要时再加。
 * 样式：紧凑排版（对话气泡内），首末元素去外边距。
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
      // 下载按钮对聊天场景冗余；复制保留（紧凑版样式见 index.css .md-response 节）
      controls={{ code: { download: false } }}
      codeBlockMaxHeight={320}
      {...props}
    />
  ),
  (prevProps, nextProps) => prevProps.children === nextProps.children,
);
