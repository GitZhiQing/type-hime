import { useState } from "react";
import { ChatPane } from "@/components/ChatPane";
import { SessionList } from "@/components/SessionList";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable";

/** 全局对话页（rail「对话」）：无作品上下文的 AI 聊天——找灵感、头脑风暴、写作问答。 */
export function GlobalChat() {
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [sessionRefresh, setSessionRefresh] = useState(0);

  function createSession() {
    const id = `s${Date.now().toString(16)}-${Math.random().toString(16).slice(2, 6)}`;
    setSessionId(id);
    return id;
  }

  return (
    <div className="flex h-full flex-col bg-background text-foreground">
      <header className="flex h-12 shrink-0 items-center gap-3 border-b px-4">
        <span className="text-sm font-semibold">对话</span>
        <span className="text-xs text-muted-foreground">
          无作品上下文 · 找灵感 / 头脑风暴 / 写作问答
        </span>
      </header>
      <ResizablePanelGroup orientation="horizontal" className="min-h-0 flex-1">
        <ResizablePanel defaultSize={26} minSize={16}>
          <SessionList
            scope="global"
            sessionId={sessionId}
            onSelect={setSessionId}
            onNew={createSession}
            refreshKey={sessionRefresh}
          />
        </ResizablePanel>
        <ResizableHandle withHandle />
        <ResizablePanel defaultSize={74}>
          <ChatPane
            scope="global"
            sessionId={sessionId}
            onNewSession={createSession}
            onTurnDone={() => setSessionRefresh((k) => k + 1)}
            refreshKey={sessionRefresh}
          />
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  );
}
