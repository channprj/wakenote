import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { TooltipProvider } from "./components/ui/tooltip";
import { devFixtures } from "./lib/dev-fixtures";
import { isTauriRuntime, seedBrowserFixtures } from "./lib/tauri-client";
import "./styles.css";

// Outside Tauri the backend is a mock that starts empty, which leaves every
// screen blank. Seed it so `pnpm dev` shows the app with real content.
if (!isTauriRuntime()) {
  seedBrowserFixtures(devFixtures());
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <TooltipProvider>
      <App />
    </TooltipProvider>
  </StrictMode>,
);
