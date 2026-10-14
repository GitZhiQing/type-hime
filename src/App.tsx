import { useEffect, useState } from "react";
import { BrowserRouter, Route, Routes } from "react-router-dom";
import { Toaster } from "@/components/ui/sonner";
import { AppRail } from "@/components/AppRail";
import { Launcher } from "@/pages/Launcher";
import { Workbench } from "@/pages/Workbench";
import { Settings } from "@/pages/Settings";
import { GlobalChat } from "@/pages/GlobalChat";
import { Agents } from "@/pages/Agents";
import { getStoredTheme, resolveTheme } from "@/lib/theme";

function App() {
  // 主题跟随（R11）：Toaster 与 rail 同步（theme:changed 广播）
  const [resolved, setResolved] = useState<"light" | "dark">(() => resolveTheme(getStoredTheme()));
  useEffect(() => {
    const onChanged = (e: Event) => {
      const detail = (e as CustomEvent<{ resolved: "light" | "dark" }>).detail;
      if (detail?.resolved) setResolved(detail.resolved);
    };
    window.addEventListener("theme:changed", onChanged);
    return () => window.removeEventListener("theme:changed", onChanged);
  }, []);

  return (
    <BrowserRouter>
      <Toaster position="top-center" theme={resolved} />
      <div className="flex h-screen">
        <AppRail />
        <div className="h-screen min-w-0 flex-1">
          <Routes>
            <Route path="/" element={<Launcher />} />
            <Route path="/chat" element={<GlobalChat />} />
            <Route path="/work" element={<Workbench />} />
            <Route path="/settings" element={<Settings />} />
            <Route path="/agents" element={<Agents />} />
          </Routes>
        </div>
      </div>
    </BrowserRouter>
  );
}

export default App;
