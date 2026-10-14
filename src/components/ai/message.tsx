import type { HTMLAttributes } from "react";
import { cn } from "cn";

/**
 * 改编自 Vercel AI Elements 的 Message 组件（源码复制路线，D26 同 Tool）：
 * - 保留：Message / MessageContent（is-user/is-assistant 分组气泡）/ MessageHeader / MessageFooter
 * - 文本 = 纯文本 + whitespace-pre-wrap（markdown 渲染由 Response/streamdown 承担）
 * - 未包含：Branch 分支、Action 工具条（需要时再加）
 */

export type MessageProps = HTMLAttributes<HTMLDivElement> & {
  from: "user" | "assistant" | "system";
};

export const Message = ({ className, from, ...props }: MessageProps) => (
  <div
    className={cn(
      "group flex w-full flex-col gap-1",
      // 用户消息：右对齐窄栏（气泡样式）；AI 消息：全宽无气泡（内容直接落底）
      from === "user" ? "is-user ml-auto max-w-[85%] justify-end" : "is-assistant",
      className,
    )}
    data-from={from}
    {...props}
  />
);

export type MessageContentProps = HTMLAttributes<HTMLDivElement>;

export const MessageContent = ({
  children,
  className,
  ...props
}: MessageContentProps) => (
  <div
    className={cn(
      "flex w-fit min-w-0 max-w-full flex-col gap-2 overflow-hidden text-sm",
      // 用户气泡：浅色底 + 大圆角，右对齐（主流 AI Chat 惯例）
      "group-[.is-user]:ml-auto group-[.is-user]:rounded-2xl group-[.is-user]:bg-muted group-[.is-user]:px-3.5 group-[.is-user]:py-2 group-[.is-user]:text-foreground",
      // AI 消息：无背景气泡，内容全宽直排
      "group-[.is-assistant]:w-full group-[.is-assistant]:text-foreground",
      className,
    )}
    {...props}
  >
    {children}
  </div>
);

export type MessageHeaderProps = HTMLAttributes<HTMLDivElement>;

export const MessageHeader = ({
  className,
  ...props
}: MessageHeaderProps) => (
  <div
    className={cn("flex items-center gap-2 text-xs font-medium", className)}
    {...props}
  />
);

export type MessageFooterProps = HTMLAttributes<HTMLDivElement>;

export const MessageFooter = ({
  className,
  ...props
}: MessageFooterProps) => (
  <div
    className={cn("flex items-center gap-2 text-xs text-muted-foreground", className)}
    {...props}
  />
);

/** 消息文本（保留换行） */
export const MessageText = ({ text }: { text: string }) => (
  <div className="whitespace-pre-wrap text-sm leading-relaxed">{text}</div>
);
