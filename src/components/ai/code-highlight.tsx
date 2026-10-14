import { useMemo, type ReactNode } from "react";
import { cn } from "cn";

/**
 * 轻量代码/JSON 高亮（D51，无新依赖）：
 * 工具参数与结果直接展示原始文本——JSON 可解析时做 token 着色（键/字符串/数字/字面量/标点），
 * 非 JSON 原样等宽展示。消息内 fenced 代码块由 streamdown（shiki 插件）高亮，不走这里。
 */

const TOKEN_RE =
  /("(?:\\.|[^"\\])*")(\s*:)?|\b(true|false|null)\b|(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)/g;

const CLS = {
  key: "text-sky-700 dark:text-sky-300",
  string: "text-emerald-700 dark:text-emerald-300",
  number: "text-amber-700 dark:text-amber-300",
  literal: "text-violet-700 dark:text-violet-300",
  punct: "text-muted-foreground",
};

/** JSON 文本 → 着色 spans（解析失败返回 null，由调用方回落原文） */
function tokenizeJson(text: string): ReactNode[] | null {
  try {
    JSON.parse(text);
  } catch {
    return null;
  }
  const out: ReactNode[] = [];
  let last = 0;
  let m: RegExpExecArray | null;
  let key = 0;
  TOKEN_RE.lastIndex = 0;
  while ((m = TOKEN_RE.exec(text)) !== null) {
    if (m.index > last) {
      out.push(
        <span key={`p${last}`} className={CLS.punct}>
          {text.slice(last, m.index)}
        </span>,
      );
    }
    if (m[1] !== undefined) {
      const isKey = m[2] !== undefined;
      out.push(
        <span key={`s${m.index}`} className={isKey ? CLS.key : CLS.string}>
          {m[1]}
        </span>,
      );
      if (isKey) {
        out.push(
          <span key={`c${m.index}`} className={CLS.punct}>
            {m[2]}
          </span>,
        );
      }
    } else if (m[3] !== undefined) {
      out.push(
        <span key={`l${m.index}`} className={CLS.literal}>
          {m[3]}
        </span>,
      );
    } else if (m[4] !== undefined) {
      out.push(
        <span key={`n${m.index}`} className={CLS.number}>
          {m[4]}
        </span>,
      );
    }
    last = TOKEN_RE.lastIndex;
    key += 1;
  }
  if (last < text.length) {
    out.push(
      <span key={`t${last}`} className={CLS.punct}>
        {text.slice(last)}
      </span>,
    );
  }
  return out;
}

/** 可解析 JSON 则缩进美化（否则原文） */
export function prettyMaybeJson(text: string): string {
  try {
    return JSON.stringify(JSON.parse(text), null, 2);
  } catch {
    return text;
  }
}

/** 代码文本高亮块：JSON 着色 / 普通文本原样 */
export function CodeHighlight({
  text,
  className,
}: {
  text: string;
  className?: string;
}) {
  const nodes = useMemo(() => tokenizeJson(text), [text]);
  return (
    <pre
      className={cn(
        "font-mono text-[11px] leading-relaxed whitespace-pre-wrap break-all",
        className,
      )}
    >
      {nodes ?? text}
    </pre>
  );
}
