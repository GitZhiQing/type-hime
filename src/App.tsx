import { BrowserRouter, Route, Routes } from "react-router-dom";
import { Toaster } from "@/components/ui/sonner";
import { AppRail } from "@/components/AppRail";
import { Launcher } from "@/pages/Launcher";
import { Workbench } from "@/pages/Workbench";
import { Settings } from "@/pages/Settings";
import { GlobalChat } from "@/pages/GlobalChat";

function App() {
  return (
    <BrowserRouter>
      <Toaster position="top-center" />
      <div className="flex h-screen">
        <AppRail />
        <div className="h-screen min-w-0 flex-1">
          <Routes>
            <Route path="/" element={<Launcher />} />
            <Route path="/chat" element={<GlobalChat />} />
            <Route path="/work" element={<Workbench />} />
            <Route path="/settings" element={<Settings />} />
          </Routes>
        </div>
      </div>
    </BrowserRouter>
  );
}

export default App;
