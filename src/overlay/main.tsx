import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { installDiagnosticErrorLogging } from "../lib/diagnostic-log";
import { RecordingOverlay } from "./RecordingOverlay";
import "./overlay.css";

installDiagnosticErrorLogging();

createRoot(document.getElementById("overlay-root")!).render(
  <StrictMode>
    <RecordingOverlay />
  </StrictMode>,
);
