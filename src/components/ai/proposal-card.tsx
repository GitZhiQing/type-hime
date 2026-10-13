import { useState } from "react";
import { api, Proposal } from "@/lib/tauri";
import { collapseContext } from "@/lib/diff";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { cn } from "cn";
import { toast } from "sonner";
import { Check, ChevronDown, FilePlus2, Loader2, MessageSquare, X } from "lucide-react";

/**
 * 聊天内联提议卡片（R2-③）：与右栏审阅 Tab 同源同队列（review:new 广播），
 * 接受/拒绝带评论；拒绝评论回流 Agent 上下文驱动修改重提。
 * 操作后派发 window 事件 `review:changed` 让审阅 Tab 与工作台同步刷新。
 */

export interface InlineProposal {
  proposal: Proposal;
  status: "pending" | "accepted" | "rejected";
}

interface ProposalCardProps {
  item: InlineProposal;
  workPath: string;
  onResolved: (id: number, status: "accepted" | "rejected") => void;
}

export function ProposalCard({ item, workPath, onResolved }: ProposalCardProps) {
  const p = item.proposal;
  const [busy, setBusy] = useState(false);
  const [commenting, setCommenting] = useState(false);
  const [comment, setComment] = useState("");

  async function act(accept: boolean, withComment?: string) {
    setBusy(true);
    try {
      if (accept) {
        const msg = await api.applyProposal(workPath, p.id);
        toast.success(msg);
        onResolved(p.id, "accepted");
      } else {
        await api.rejectProposal(p.id, withComment);
        toast.info(withComment ? "已拒绝，反馈将注入下轮对话" : "已拒绝提议");
        onResolved(p.id, "rejected");
      }
      window.dispatchEvent(new CustomEvent("review:changed"));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
      setCommenting(false);
      setComment("");
    }
  }

  return (
    <div className="w-full rounded-md border">
      <div className="flex items-center gap-2 border-b px-3 py-2">
        {p.is_new_file ? (
          <Badge variant="secondary">
            <FilePlus2 data-icon="inline-start" />
            新建
          </Badge>
        ) : (
          <Badge variant="secondary">修改</Badge>
        )}
        <span className="truncate font-mono text-xs" title={p.rel_path}>
          {p.rel_path}
        </span>
        {p.requires_review && (
          <Badge variant="outline" className="shrink-0 border-amber-500/50 text-amber-600">
            可见性变更 · 必须人工
          </Badge>
        )}
        <span className="ml-auto flex items-center gap-1">
          {item.status === "pending" ? (
            <>
              <Button
                size="sm"
                className="h-7 text-xs"
                disabled={busy}
                onClick={() => act(true)}
              >
                {busy ? (
                  <Loader2 data-icon="inline-start" className="animate-spin" />
                ) : (
                  <Check data-icon="inline-start" />
                )}
                接受
              </Button>
              <Button
                size="sm"
                variant="outline"
                className="h-7 text-xs"
                disabled={busy}
                onClick={() => (commenting ? act(false, comment || undefined) : setCommenting(true))}
                title="拒绝，可附评论反馈给 AI"
              >
                <X data-icon="inline-start" />
                {commenting ? "确认拒绝" : "拒绝"}
              </Button>
            </>
          ) : (
            <Badge variant={item.status === "accepted" ? "default" : "secondary"}>
              {item.status === "accepted" ? "已接受落盘" : "已拒绝"}
            </Badge>
          )}
        </span>
      </div>
      {p.reason && (
        <div className="border-b bg-muted/40 px-3 py-1.5 text-xs text-muted-foreground">
          理由：{p.reason}
        </div>
      )}
      {commenting && (
        <div className="flex items-center gap-2 border-b bg-muted/20 px-3 py-2">
          <MessageSquare className="size-3.5 shrink-0 text-muted-foreground" />
          <Input
            autoFocus
            value={comment}
            onChange={(e) => setComment(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") act(false, comment || undefined);
              if (e.key === "Escape") setCommenting(false);
            }}
            placeholder="拒绝评论（将反馈给 AI 据此修改重提，可留空）"
            className="h-7 text-xs"
          />
        </div>
      )}
      <Collapsible>
        <CollapsibleTrigger className="group flex w-full items-center gap-1 px-3 py-1.5 text-xs text-muted-foreground hover:text-foreground">
          <ChevronDown className="size-3 transition-transform group-data-[panel-open]:rotate-180" />
          查看 diff
        </CollapsibleTrigger>
        <CollapsibleContent>
          <pre className="overflow-x-auto border-t p-2 font-mono text-[12px] leading-relaxed">
            {collapseContext(p.diff).map((line, i) => (
              <div
                key={i}
                className={cn(
                  line.startsWith("+") && !line.startsWith("+++") && "bg-emerald-500/10 text-emerald-700",
                  line.startsWith("-") && !line.startsWith("---") && "bg-red-500/10 text-red-700",
                  (line.startsWith("@@") || line.startsWith("---") || line.startsWith("+++")) &&
                    "text-muted-foreground",
                  line === "⋯" && "text-muted-foreground/50 select-none",
                )}
              >
                {line || " "}
              </div>
            ))}
          </pre>
        </CollapsibleContent>
      </Collapsible>
    </div>
  );
}
