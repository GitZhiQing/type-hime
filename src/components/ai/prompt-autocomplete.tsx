import { cn } from "cn";

/**
 * 输入框补全菜单（R2）：`/` 命令与 `@` 素材引用共用。
 * 渲染在 PromptInputBody 内（绝对定位锚定），键盘导航由宿主处理。
 */

export interface AutocompleteItem {
  /** 选中后替换触发词的文本，如 "/正文" 或 "@林晚" */
  replace: string;
  label: string;
  hint?: string;
}

interface PromptAutocompleteProps {
  items: AutocompleteItem[];
  activeIndex: number;
  onPick: (item: AutocompleteItem) => void;
  onHover?: (index: number) => void;
}

export function PromptAutocomplete({
  items,
  activeIndex,
  onPick,
  onHover,
}: PromptAutocompleteProps) {
  if (items.length === 0) return null;
  return (
    <div
      data-slot="prompt-autocomplete"
      role="listbox"
      className="absolute bottom-full left-0 z-20 mb-1 max-h-64 w-full min-w-64 overflow-auto rounded-md border bg-popover p-1 text-popover-foreground shadow-md"
    >
      {items.map((item, i) => (
        <button
          key={item.replace}
          type="button"
          role="option"
          aria-selected={i === activeIndex}
          className={cn(
            "flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-xs hover:bg-accent",
            i === activeIndex && "bg-accent",
          )}
          onMouseDown={(e) => {
            // 阻止 mousedown 抢焦点导致 textarea blur
            e.preventDefault();
            onPick(item);
          }}
          onMouseEnter={() => onHover?.(i)}
        >
          <span className="font-mono font-medium">{item.label}</span>
          {item.hint && (
            <span className="truncate text-muted-foreground">{item.hint}</span>
          )}
        </button>
      ))}
    </div>
  );
}
