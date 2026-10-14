import { useCallback, useEffect, useState } from "react";
import { api, IssueRecord } from "@/lib/tauri";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Check, Loader2, RefreshCw, ShieldCheck, X } from "lucide-react";
import { toast } from "sonner";

/**
 * 问题清单制品（R5-② · REDESIGN §5.5）：issues.json 结构化渲染，
 * 裁决按钮从 GeneratePane 迁入（fixed/dismissed 是保留的人工入口）。
 */

interface IssuesViewProps {
  workPath: string;
  /** 会话/一轮结束 +1 触发刷新 */
  refreshKey?: number;
}

export function IssuesView({ workPath, refreshKey = 0 }: IssuesViewProps) {
  const [issues, setIssues] = useState<IssueRecord[] | null>(null);
  const [onlyOpen, setOnlyOpen] = useState(true);

  const reload = useCallback(async () => {
    try {
      setIssues(await api.listIssues(workPath));
    } catch {
      setIssues([]); // 无清单文件
    }
  }, [workPath]);

  useEffect(() => {
    reload();
  }, [reload, refreshKey]);

  // 聊天内联问题卡裁决后经 window 事件刷新（R5-③ 发）
  useEffect(() => {
    const onChanged = () => reload();
    window.addEventListener("issues:changed", onChanged);
    return () => window.removeEventListener("issues:changed", onChanged);
  }, [reload]);

  async function rule(id: number, status: "fixed" | "dismissed" | "needs_ruling") {
    try {
      await api.recordIssueRuling(workPath, id, status);
      await reload();
    } catch (e) {
      toast.error(String(e));
    }
  }

  if (issues === null) {
    return (
      <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
        <Loader2 className="mr-2 size-4 animate-spin" /> 加载问题清单…
      </div>
    );
  }

  const openCount = issues.filter(
    (i) => i.status === "open" || i.status === "needs_ruling",
  ).length;
  const shown = onlyOpen
    ? issues.filter((i) => i.status === "open" || i.status === "needs_ruling")
    : issues;

  return (
    <div className="flex h-full min-w-0 flex-col">
      <div className="flex h-9 shrink-0 items-center gap-2 border-b px-3 text-xs text-muted-foreground">
        <ShieldCheck className="size-3.5" />
        <span className="font-medium text-foreground/80">问题清单</span>
        <span>
          {openCount} 未决 / 共 {issues.length} 条
        </span>
        <label className="ml-2 flex cursor-pointer items-center gap-1 text-[11px]">
          <input
            type="checkbox"
            checked={onlyOpen}
            onChange={(e) => setOnlyOpen(e.currentTarget.checked)}
          />
          只看未决
        </label>
        <Button
          variant="ghost"
          size="sm"
          className="ml-auto h-6 px-2 text-[11px]"
          onClick={reload}
        >
          <RefreshCw data-icon="inline-start" />
          刷新
        </Button>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto p-3">
        {shown.length === 0 ? (
          <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
            {issues.length === 0
              ? "还没有问题清单——一致性检查 / 复检 / 审卷后按章落盘到这里"
              : "没有未决问题——关掉只看未决即可回看裁决档案"}
          </div>
        ) : (
          <div className="flex flex-col gap-2">
            {shown.map((iss) => (
              <IssueCard key={iss.id} issue={iss} onRule={rule} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

export function IssueCard({
  issue,
  onRule,
}: {
  issue: IssueRecord;
  onRule: (id: number, status: "fixed" | "dismissed" | "needs_ruling") => void;
}) {
  const statusBadge = () => {
    switch (issue.status) {
      case "fixed":
        return <Badge className="bg-emerald-500/15 text-emerald-700">已修复</Badge>;
      case "dismissed":
        return <Badge variant="secondary">已驳回</Badge>;
      case "needs_ruling":
        return <Badge className="bg-amber-500/15 text-amber-700">需裁决</Badge>;
      default:
        return <Badge variant="outline">待处理</Badge>;
    }
  };
  return (
    <div className="rounded-md border p-2 text-xs">
      <div className="flex items-center gap-2">
        <span className="font-mono text-muted-foreground">{issue.chapter}</span>
        <span className="text-[10px] text-muted-foreground/70">{issue.source}</span>
        {statusBadge()}
        {(issue.status === "open" || issue.status === "needs_ruling") && (
          <span className="ml-auto flex gap-1">
            <Button
              variant="outline"
              size="sm"
              className="h-6 px-2 text-[11px]"
              onClick={() => onRule(issue.id, "fixed")}
            >
              <Check data-icon="inline-start" />
              确认已修复
            </Button>
            <Button
              variant="outline"
              size="sm"
              className="h-6 px-2 text-[11px]"
              onClick={() => onRule(issue.id, "dismissed")}
            >
              <X data-icon="inline-start" />
              驳回
            </Button>
          </span>
        )}
      </div>
      <p className="mt-1 whitespace-pre-wrap leading-relaxed text-muted-foreground">
        {issue.text.length > 300 ? `${issue.text.slice(0, 300)}…` : issue.text}
      </p>
      {issue.ruling && (
        <p className="mt-1 text-[11px] text-muted-foreground/70">裁决：{issue.ruling}</p>
      )}
    </div>
  );
}
