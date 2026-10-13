import { ShieldAlert, ShieldCheck, ShieldX } from "lucide-react";
import { Button } from "@/components/ui/button";

/**
 * 工具审批卡（R4：run_flow/bash 等敏感操作逐条确认；AI Elements Confirmation 同构最小版）。
 * 形态来源：goose 会话窗口 Allow/Deny 卡片、ZCode 工具权限块审批流。
 */

export interface ApprovalView {
  id: number;
  tool: string;
  title: string;
  detail: string;
  status: "pending" | "allowed" | "denied";
  note?: string | null;
}

export function ConfirmationCard({
  approval,
  onDecide,
  onGrantFullAccess,
}: {
  approval: ApprovalView;
  onDecide: (id: number, allow: boolean) => void;
  /** 允许并切本会话为「完全访问」（ZCode grantPermissionFullAccess 同款语义） */
  onGrantFullAccess?: (id: number) => void;
}) {
  const { status } = approval;
  return (
    <div className="rounded-lg border border-amber-500/40 bg-amber-500/5 p-3 text-sm">
      <div className="flex items-center gap-2">
        <ShieldAlert className="size-4 shrink-0 text-amber-600" />
        <span className="font-medium">需要确认</span>
        <span className="text-xs text-muted-foreground">{approval.tool}</span>
        {status !== "pending" && (
          <span
            className={`ml-auto flex items-center gap-1 text-xs ${
              status === "allowed" ? "text-emerald-600" : "text-red-600"
            }`}
          >
            {status === "allowed" ? (
              <ShieldCheck className="size-3.5" />
            ) : (
              <ShieldX className="size-3.5" />
            )}
            {status === "allowed" ? "已允许" : "已拒绝"}
          </span>
        )}
      </div>
      <div className="mt-2 overflow-x-auto rounded bg-muted px-2 py-1 font-mono text-[12px]">
        {approval.title}
      </div>
      {approval.detail && (
        <div className="mt-1 whitespace-pre-wrap text-xs text-muted-foreground">
          {approval.detail}
        </div>
      )}
      {approval.note && <div className="mt-1 text-xs text-amber-600">{approval.note}</div>}
      {status === "pending" && (
        <div className="mt-2 flex items-center gap-2">
          <Button size="sm" className="h-7 px-3 text-xs" onClick={() => onDecide(approval.id, true)}>
            允许
          </Button>
          <Button
            size="sm"
            variant="outline"
            className="h-7 px-3 text-xs"
            onClick={() => onDecide(approval.id, false)}
          >
            拒绝
          </Button>
          {onGrantFullAccess && (
            <Button
              size="sm"
              variant="ghost"
              className="ml-auto h-7 px-2 text-xs text-amber-600"
              title="允许本次并将会话切为「完全访问」：后续白名单命令直通、提议自动应用"
              onClick={() => onGrantFullAccess(approval.id)}
            >
              允许并本会话完全访问
            </Button>
          )}
        </div>
      )}
    </div>
  );
}
