import { useState } from "react";
import { Check, Hand, ShieldAlert } from "lucide-react";
import { toast } from "sonner";

import { api, type PermissionMode, type ToolOverride } from "@/lib/tauri";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { cn } from "cn";

/**
 * 权限档位下拉（R4-③ · D30）：输入框左下角，选择并展示当前档位。
 * 形态来源 ZCode 会话模式控件（图标+名称+描述行+当前项打勾）；ZCode 模式标签文案见调研笔记。
 */

const MODES: {
  value: PermissionMode;
  label: string;
  description: string;
  icon: typeof Hand;
}[] = [
  {
    value: "confirm",
    label: "变更前确认",
    description: "改文件前先问我；白名单命令逐条确认。",
    icon: Hand,
  },
  {
    value: "full_access",
    label: "完全访问",
    description: "提议自动应用；白名单命令直通。",
    icon: ShieldAlert,
  },
];

export function PermissionSelect({
  workPath,
  mode,
  onChange,
}: {
  workPath: string;
  mode: PermissionMode;
  onChange: (mode: PermissionMode) => void;
}) {
  const current = MODES.find((m) => m.value === mode) ?? MODES[0];
  const CurrentIcon = current.icon;
  const [settingsOpen, setSettingsOpen] = useState(false);

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <Button
              variant="ghost"
              size="sm"
              className={cn(
                "h-7 gap-1.5 px-2 text-xs",
                mode === "full_access" ? "text-amber-600" : "text-muted-foreground",
              )}
              title="权限档位（会话级）"
            />
          }
        >
          <CurrentIcon className="size-3.5" />
          {current.label}
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="w-72">
          {/* base-ui 约束：GroupLabel 必须包在 Menu.Group 内（否则运行时报 MenuGroupContext is missing） */}
          <DropdownMenuGroup>
            <DropdownMenuLabel className="text-xs text-muted-foreground">
              权限档位（会话级）
            </DropdownMenuLabel>
            {MODES.map((m) => {
              const Icon = m.icon;
              return (
                <DropdownMenuItem
                  key={m.value}
                  onClick={() => m.value !== mode && onChange(m.value)}
                  className="items-start gap-2"
                >
                  <Icon className="mt-0.5 size-4 shrink-0" />
                  <span className="flex flex-col">
                    <span className="text-sm">{m.label}</span>
                    <span className="text-xs text-muted-foreground">{m.description}</span>
                  </span>
                  {m.value === mode && <Check className="mt-0.5 ml-auto size-4 shrink-0" />}
                </DropdownMenuItem>
              );
            })}
          </DropdownMenuGroup>
          <DropdownMenuSeparator />
          <DropdownMenuItem className="text-xs" onClick={() => setSettingsOpen(true)}>
            权限例外设置…（按工具 / 命令模式覆盖）
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <PermissionSettingsDialog
        workPath={workPath}
        open={settingsOpen}
        onOpenChange={setSettingsOpen}
      />
    </>
  );
}

const TOOL_OPTIONS = ["bash", "propose_edit", "propose_write", "run_flow"];

/** 权限例外设置（作品级覆盖表，存 project.json；pattern 尾 * 通配）。 */
function PermissionSettingsDialog({
  workPath,
  open,
  onOpenChange,
}: {
  workPath: string;
  open: boolean;
  onOpenChange: (v: boolean) => void;
}) {
  const [rows, setRows] = useState<ToolOverride[] | null>(null);
  const [saving, setSaving] = useState(false);

  // 打开时加载
  if (open && rows === null) {
    api
      .getPermissionConfig(workPath)
      .then((c) => setRows(c.overrides))
      .catch((e) => {
        toast.error(String(e));
        setRows([]);
      });
  }

  function update(i: number, patch: Partial<ToolOverride>) {
    setRows((r) => (r ? r.map((row, j) => (j === i ? { ...row, ...patch } : row)) : r));
  }

  async function save() {
    if (!rows) return;
    setSaving(true);
    try {
      await api.setPermissionConfig(workPath, rows);
      toast.success("权限例外已保存（下一轮对话生效）");
      onOpenChange(false);
      setRows(null);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setSaving(false);
    }
  }

  const list = rows ?? [];
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>权限例外设置</DialogTitle>
        </DialogHeader>
        <div className="flex flex-col gap-3 text-sm">
          <p className="text-xs text-muted-foreground">
            按工具覆盖会话档位：bash 的 pattern 匹配命令（如 <code>git log*</code>）；
            propose_* 的 pattern 匹配目标路径（如 <code>ledger/*</code> = 仅账本自动）。
            更具体的 pattern 优先；留空表示该工具全部。
          </p>
          {list.length === 0 && (
            <p className="text-xs text-muted-foreground">（当前无例外）</p>
          )}
          {list.map((row, i) => (
            <div key={i} className="flex items-center gap-2">
              <Select
                items={Object.fromEntries(TOOL_OPTIONS.map((t) => [t, t]))}
                value={row.tool}
                onValueChange={(v) => v && update(i, { tool: v })}
              >
                <SelectTrigger className="h-8 w-36">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {TOOL_OPTIONS.map((t) => (
                    <SelectItem key={t} value={t}>
                      {t}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <Input
                className="h-8 flex-1"
                placeholder="模式（可空）：git log* 或 ledger/*"
                value={row.pattern ?? ""}
                onChange={(e) => update(i, { pattern: e.target.value || null })}
              />
              <Select
                items={{ confirm: "变更前确认", full_access: "完全访问" }}
                value={row.mode}
                onValueChange={(v) => v && update(i, { mode: v as PermissionMode })}
              >
                <SelectTrigger className="h-8 w-32">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="confirm">变更前确认</SelectItem>
                  <SelectItem value="full_access">完全访问</SelectItem>
                </SelectContent>
              </Select>
              <Button
                variant="ghost"
                size="sm"
                className="h-8 px-2 text-xs text-muted-foreground"
                onClick={() => setRows((r) => (r ? r.filter((_, j) => j !== i) : r))}
              >
                删除
              </Button>
            </div>
          ))}
          <Button
            variant="outline"
            size="sm"
            className="w-fit"
            onClick={() =>
              setRows((r) => [...(r ?? []), { tool: "bash", pattern: null, mode: "full_access" }])
            }
          >
            添加一条例外
          </Button>
          <Label className="text-xs text-muted-foreground">
            注意：可见性设置、删除/恢复文件、跳过阶段、装配切全量四类操作不受档位与例外影响，永远人工。
          </Label>
        </div>
        <DialogFooter>
          <Button size="sm" onClick={save} disabled={saving || rows === null}>
            保存
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
