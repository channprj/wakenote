import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { RecordingOverlay } from "./RecordingOverlay";
import "./overlay.css";

createRoot(document.getElementById("overlay-root")!).render(
  <StrictMode>
    <RecordingOverlay />
  </StrictMode>,
);
