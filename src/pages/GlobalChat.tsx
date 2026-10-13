export function GlobalChat() {
  return (
    <div className="flex h-full flex-col bg-background text-foreground">
      <header className="flex h-12 shrink-0 items-center gap-3 border-b px-4">
        <span className="text-sm font-semibold">对话</span>
        <span className="text-xs text-muted-foreground">
          无作品上下文 · 找灵感 / 头脑风暴 / 写作问答
        </span>
      </header>
      <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
        全局对话页建设中（R1-④）
      </div>
    </div>
  );
}
