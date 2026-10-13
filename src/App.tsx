import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import { Badge } from "@/components/ui/badge";
import { BookOpen, MessageSquare, FileText } from "lucide-react";

function App() {
  return (
    <div className="flex h-screen flex-col bg-background text-foreground">
      <header className="flex h-12 shrink-0 items-center gap-3 border-b px-4">
        <span className="text-sm font-semibold tracking-wide">type-hime</span>
        <Badge variant="secondary">骨架</Badge>
        <div className="ml-auto text-xs text-muted-foreground">
          尚未打开作品
        </div>
      </header>

      <ResizablePanelGroup orientation="horizontal" className="flex-1">
        <ResizablePanel defaultSize={22} minSize={14}>
          <aside className="flex h-full flex-col">
            <div className="flex h-9 items-center gap-2 border-b px-3 text-xs font-medium text-muted-foreground">
              <BookOpen className="size-3.5" />
              资源树
            </div>
            <ScrollArea className="flex-1">
              <div className="p-3 text-xs text-muted-foreground">
                打开作品后这里显示设定 / 大纲 / 章节 / 账本
              </div>
            </ScrollArea>
          </aside>
        </ResizablePanel>

        <ResizableHandle withHandle />

        <ResizablePanel defaultSize={48} minSize={30}>
          <main className="flex h-full flex-col">
            <div className="flex h-9 items-center gap-2 border-b px-3 text-xs font-medium text-muted-foreground">
              <MessageSquare className="size-3.5" />
              会话
            </div>
            <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
              Agent 会话区（切片 2 起启用）
            </div>
          </main>
        </ResizablePanel>

        <ResizableHandle withHandle />

        <ResizablePanel defaultSize={30} minSize={18}>
          <section className="flex h-full flex-col">
            <div className="flex h-9 items-center gap-2 border-b px-3 text-xs font-medium text-muted-foreground">
              <FileText className="size-3.5" />
              编辑器
            </div>
            <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
              文档编辑 / diff 审阅（切片 5 起启用）
            </div>
            <Separator />
          </section>
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  );
}

export default App;
