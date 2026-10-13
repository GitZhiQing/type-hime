import {
  createContext,
  useContext,
  type ComponentProps,
  type HTMLAttributes,
  type ReactNode,
} from "react";
import { Button } from "@/components/ui/button";
import { cn } from "cn";
import { ArrowUpIcon, Loader2 } from "lucide-react";

/**
 * 同构自研版 AI Elements PromptInput（R2 spike 结论：源码复制需拖入
 * command/hover-card/input-group/spinner 等 radix 组件，与 base-ui 基座冲突，
 * 故按其 API 与命名自研精简子集；attachments/screenshot/引用源等暂不实现）。
 *
 * 用法：
 * <PromptInput value setValue onSubmit isLoading>
 *   <PromptInputBody>
 *     <PromptInputTextarea placeholder />
 *   </PromptInputBody>
 *   <PromptInputToolbar>
 *     <PromptInputTools>…</PromptInputTools>
 *     <PromptInputSubmit />
 *   </PromptInputToolbar>
 * </PromptInput>
 */

interface PromptInputContextValue {
  value: string;
  setValue: (v: string) => void;
  isLoading: boolean;
  submit: () => void;
}

const PromptInputContext = createContext<PromptInputContextValue | null>(null);

export function usePromptInput() {
  const ctx = useContext(PromptInputContext);
  if (!ctx) throw new Error("usePromptInput 必须在 PromptInput 内使用");
  return ctx;
}

export type PromptInputProps = HTMLAttributes<HTMLDivElement> & {
  value: string;
  setValue: (v: string) => void;
  onSubmit?: () => void;
  onStop?: () => void;
  isLoading?: boolean;
};

export const PromptInput = ({
  className,
  value,
  setValue,
  onSubmit,
  onStop,
  isLoading = false,
  children,
  ...props
}: PromptInputProps) => {
  const submit = () => {
    if (isLoading) return;
    if (!value.trim()) return;
    onSubmit?.();
  };
  return (
    <PromptInputContext.Provider
      value={{ value, setValue, isLoading, submit }}
    >
      <div
        data-slot="prompt-input"
        className={cn(
          "flex w-full flex-col gap-2 rounded-lg border bg-background p-2 focus-within:border-ring/50 focus-within:ring-[3px] focus-within:ring-ring/20",
          className,
        )}
        {...props}
      >
        {children}
      </div>
    </PromptInputContext.Provider>
  );
};

export type PromptInputBodyProps = HTMLAttributes<HTMLDivElement>;

/** 输入主体区（textarea + 悬浮元素如补全菜单的锚定容器） */
export const PromptInputBody = ({
  className,
  ...props
}: PromptInputBodyProps) => (
  <div
    data-slot="prompt-input-body"
    className={cn("relative flex min-h-14 flex-col", className)}
    {...props}
  />
);

export type PromptInputTextareaProps = Omit<
  ComponentProps<"textarea">,
  "value" | "onChange"
> & {
  /** R2-③ 补全菜单等外部按键拦截（内部处理之后调用） */
  onKeyDownCaptureExternal?: (e: React.KeyboardEvent<HTMLTextAreaElement>) => void;
};

export const PromptInputTextarea = ({
  className,
  onKeyDown,
  ...props
}: PromptInputTextareaProps) => {
  const { value, setValue, isLoading, submit } = usePromptInput();
  return (
    <textarea
      data-slot="prompt-input-textarea"
      value={value}
      onChange={(e) => setValue(e.currentTarget.value)}
      disabled={isLoading}
      onKeyDown={(e) => {
        // 外部（补全菜单）先处理；preventDefault 表示已消费
        onKeyDown?.(e);
        if (e.defaultPrevented) return;
        if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
          e.preventDefault();
          submit();
        }
      }}
      className={cn(
        "field-sizing-content max-h-48 min-h-14 w-full resize-none bg-transparent px-2 py-1.5 text-sm outline-none placeholder:text-muted-foreground disabled:opacity-50",
        className,
      )}
      {...props}
    />
  );
};

export type PromptInputToolbarProps = HTMLAttributes<HTMLDivElement>;

export const PromptInputToolbar = ({
  className,
  ...props
}: PromptInputToolbarProps) => (
  <div
    data-slot="prompt-input-toolbar"
    className={cn("flex items-center gap-1", className)}
    {...props}
  />
);

export type PromptInputToolsProps = HTMLAttributes<HTMLDivElement>;

export const PromptInputTools = ({
  className,
  ...props
}: PromptInputToolsProps) => (
  <div
    data-slot="prompt-input-tools"
    className={cn("flex items-center gap-1", className)}
    {...props}
  />
);

export type PromptInputButtonProps = ComponentProps<typeof Button> & {
  tooltip?: ReactNode;
};

export const PromptInputButton = ({
  className,
  variant = "ghost",
  size = "sm",
  tooltip,
  ...props
}: PromptInputButtonProps) => (
  <Button
    data-slot="prompt-input-button"
    variant={variant}
    size={size}
    title={typeof tooltip === "string" ? tooltip : undefined}
    className={cn("size-8 shrink-0 p-0", className)}
    {...props}
  />
);

export type PromptInputSubmitProps = ComponentProps<typeof Button>;

export const PromptInputSubmit = ({
  className,
  ...props
}: PromptInputSubmitProps) => {
  const { value, isLoading, submit } = usePromptInput();
  const canSubmit = !isLoading && !!value.trim();
  return (
    <Button
      data-slot="prompt-input-submit"
      size="sm"
      className={cn("ml-auto size-8 shrink-0 p-0", className)}
      disabled={!canSubmit}
      onClick={submit}
      aria-label="发送"
      {...props}
    >
      {isLoading ? (
        <Loader2 className="size-4 animate-spin" />
      ) : (
        <ArrowUpIcon className="size-4" />
      )}
    </Button>
  );
};
