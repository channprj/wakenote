import { useState } from "react";
import { AudioUploadPanel } from "./AudioUploadPanel";
import { MeetingTranscriptionPanel } from "./MeetingTranscriptionPanel";

type AudioTab = "upload" | "meeting";

/**
 * The Audio tab hosts two separate features behind a segmented toggle:
 * the existing realtime upload/player (`upload`) and the long-form meeting
 * batch transcription (`meeting`). Default is `upload` so existing behavior is
 * unchanged for users who don't switch.
 */
export function AudioWorkspace() {
  const [tab, setTab] = useState<AudioTab>("upload");
  return (
    <div className="audio-workspace">
      <div className="audio-tabs" role="tablist" aria-label="Audio mode">
        <button
          type="button"
          role="tab"
          aria-selected={tab === "upload"}
          data-active={tab === "upload"}
          className="audio-tabs__tab"
          onClick={() => setTab("upload")}
        >
          Realtime upload
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={tab === "meeting"}
          data-active={tab === "meeting"}
          className="audio-tabs__tab"
          onClick={() => setTab("meeting")}
        >
          Meeting
        </button>
      </div>
      {tab === "upload" ? <AudioUploadPanel /> : <MeetingTranscriptionPanel />}
    </div>
  );
}
