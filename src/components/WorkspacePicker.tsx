import { useEffect, useState } from "react";
import { api, type DirListing } from "@/lib/tauri";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ArrowUp, Folder, FolderOpen, HardDrive, Loader2 } from "lucide-react";

/**
 * 应用内文件夹选择器（D56）：导航式浏览——当前位置 + 上级 + 子目录列表 + 选择此文件夹。
 * 两个通道（Tauri / 浏览器）行为一致，不依赖原生弹窗（用户反馈：不接受手输路径）。
 * current 为空串 = Windows「盘符列表」虚拟根；parent=Some("") 表示返回盘符列表。
 */
export function WorkspacePicker({
  open,
  onOpenChange,
  initialPath,
  onPick,
}: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
  /** 打开时的起始目录（不存在则回退盘符/主目录） */
  initialPath?: string;
  onPick: (path: string) => void;
}) {
  const [listing, setListing] = useState<DirListing | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  async function navigate(path: string | null) {
    setLoading(true);
    setError("");
    try {
      const l = await api.listDirs(path);
      setListing(l);
    } catch (e) {
      setError(String(e));
      // 起始目录不存在等：回退顶层（盘符列表/主目录）
      if (path) {
        try {
          setListing(await api.listDirs(null));
        } catch {
          /* 顶层也失败则保持错误态 */
        }
      }
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    if (!open) return;
    setListing(null);
    void navigate(initialPath && initialPath.trim() ? initialPath.trim() : null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const atVirtualRoot = listing?.current === "";

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>选择文件夹</DialogTitle>
          <DialogDescription>
            浏览并选择工作区文件夹；也可以直接使用文本框中已有的路径。
          </DialogDescription>
        </DialogHeader>

        {/* 当前位置 + 上级 + 此电脑快捷（D56） */}
        <div className="flex items-center gap-2">
          <Button
            type="button"
            variant="outline"
            size="icon"
            aria-label="此电脑"
            title="此电脑（盘符列表）"
            disabled={!listing || atVirtualRoot || loading}
            onClick={() => void navigate(null)}
          >
            <HardDrive className="size-4" />
          </Button>
          <Button
            type="button"
            variant="outline"
            size="icon"
            aria-label="上级目录"
            title="上级目录"
            disabled={!listing || listing.parent === null || loading}
            onClick={() => void navigate(listing?.parent === "" ? null : (listing?.parent ?? null))}
          >
            <ArrowUp className="size-4" />
          </Button>
          <div
            className="flex min-w-0 flex-1 items-center gap-2 rounded-md border bg-muted/40 px-2.5 py-1.5"
            title={listing?.current || "此电脑"}
          >
            {atVirtualRoot ? (
              <HardDrive className="size-4 shrink-0 text-muted-foreground" />
            ) : (
              <FolderOpen className="size-4 shrink-0 text-muted-foreground" />
            )}
            <span className="truncate font-mono text-xs">
              {atVirtualRoot ? "此电脑" : listing?.current ?? "…"}
            </span>
          </div>
        </div>

        {/* 目录列表 */}
        <div className="h-64 overflow-y-auto rounded-md border">
          {loading && (
            <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
              <Loader2 className="mr-2 size-4 animate-spin" /> 读取目录…
            </div>
          )}
          {!loading && listing && listing.dirs.length === 0 && (
            <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
              没有子文件夹
            </div>
          )}
          {!loading &&
            listing?.dirs.map((d) => (
              <button
                key={d.path}
                type="button"
                className="flex w-full items-center gap-2 border-b px-2.5 py-1.5 text-left text-sm transition-colors last:border-b-0 hover:bg-accent"
                title={d.path}
                onClick={() => void navigate(d.path)}
              >
                {atVirtualRoot ? (
                  <HardDrive className="size-3.5 shrink-0 text-muted-foreground" />
                ) : (
                  <Folder className="size-3.5 shrink-0 text-muted-foreground" />
                )}
                <span className="truncate">{d.name}</span>
              </button>
            ))}
        </div>
        {error && <p className="text-xs text-destructive">{error}</p>}

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            取消
          </Button>
          <Button
            disabled={!listing || atVirtualRoot || loading}
            onClick={() => {
              if (listing && listing.current) {
                onPick(listing.current);
                onOpenChange(false);
              }
            }}
          >
            选择此文件夹
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
