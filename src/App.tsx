import { BrowserRouter, Route, Routes } from "react-router-dom";
import { Toaster } from "@/components/ui/sonner";
import { Launcher } from "@/pages/Launcher";
import { Workbench } from "@/pages/Workbench";

function App() {
  return (
    <BrowserRouter>
      <Toaster position="top-center" />
      <Routes>
        <Route path="/" element={<Launcher />} />
        <Route path="/work" element={<Workbench />} />
      </Routes>
    </BrowserRouter>
  );
}

export default App;
