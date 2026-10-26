import { BrowserRouter, Routes, Route } from "react-router-dom";
import { Toaster } from "sonner";
import { AppProvider } from "./context/AppContext";
import { ThemeProvider, useThemeContext } from "./context/ThemeContext";
import { HelpDialog } from "./components/HelpDialog";
import { CloseActionGuard } from "./components/CloseActionGuard";
import { Layout } from "./components/Layout";
import { Dashboard } from "./views/Dashboard";
import { Library } from "./views/Library";
import { AgentsView } from "./views/AgentsView";
import { ProjectView } from "./views/ProjectView";
import { Marketplace } from "./views/Marketplace";
import { Settings } from "./views/Settings";
import { applyTextSize } from "./lib/textScale";

// Pure UI preference, applied before the first paint of the views.
applyTextSize(localStorage.getItem("skills-manager.textSize") ?? "default");

function ThemedToaster() {
  const { resolvedTheme } = useThemeContext();
  return (
    <Toaster
      theme={resolvedTheme}
      position="bottom-right"
      toastOptions={{
        style: {
          background: "var(--color-surface)",
          border: "1px solid var(--color-border)",
          color: "var(--color-text-primary)",
        },
      }}
    />
  );
}

function App() {
  return (
    <ThemeProvider>
      <AppProvider>
        <BrowserRouter>
          <Routes>
            <Route element={<Layout />}>
              <Route path="/" element={<Dashboard />} />
              <Route path="/library" element={<Library />} />
              <Route path="/agents" element={<AgentsView />} />
              <Route path="/project" element={<ProjectView />} />
              <Route path="/marketplace" element={<Marketplace />} />
              <Route path="/settings" element={<Settings />} />
            </Route>
          </Routes>
          <HelpDialog />
          <CloseActionGuard />
        </BrowserRouter>
        <ThemedToaster />
      </AppProvider>
    </ThemeProvider>
  );
}

export default App;
