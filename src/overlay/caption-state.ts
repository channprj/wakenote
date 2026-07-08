export const OVERLAY_FINAL_HOLD_MS = 5_000;

export type OverlayCaptionStatus = "partial" | "refining" | "final";

export interface OverlayCaptionState {
  chunkId: number | null;
  audioPath: string | null;
  status: OverlayCaptionStatus;
  text: string;
  visible: boolean;
  generation: number;
  hideAfterMs: number | null;
}

export type OverlayCaptionEvent =
  | { type: "started"; chunk_id: number; started_at?: string }
  | { type: "partial"; chunk_id: number; text: string }
  | {
      type: "committed";
      chunk_id: number;
      audio_path: string;
      will_transcribe?: boolean;
    }
  | {
      type: "final";
      chunk_id: number | null;
      audio_path: string;
      text: string;
      recorded_at?: string;
    }
  | { type: "failed"; chunk_id: number | null; audio_path?: string; error?: string }
  | { type: "hidden" };

export function initialCaptionState(): OverlayCaptionState {
  return {
    chunkId: null,
    audioPath: null,
    status: "partial",
    text: "",
    visible: false,
    generation: 0,
    hideAfterMs: null,
  };
}

export function reduceCaptionOverlay(
  state: OverlayCaptionState,
  event: OverlayCaptionEvent,
): OverlayCaptionState {
  switch (event.type) {
    case "started":
      return {
        ...initialCaptionState(),
        chunkId: event.chunk_id,
        generation: state.generation + 1,
      };

    case "partial": {
      if (!eventMatchesCurrentChunk(state, event.chunk_id)) {
        return state;
      }
      const text = normalizeCaptionText(event.text);
      if (!text) {
        return state;
      }
      return {
        ...state,
        chunkId: event.chunk_id,
        status: "partial",
        text,
        visible: true,
        hideAfterMs: null,
      };
    }

    case "committed": {
      if (!eventMatchesCurrentChunk(state, event.chunk_id)) {
        return state;
      }
      const willTranscribe = event.will_transcribe !== false;
      const hasText = Boolean(state.text);
      return {
        ...state,
        chunkId: event.chunk_id,
        audioPath: event.audio_path,
        status: willTranscribe ? "refining" : state.status,
        visible: hasText,
        hideAfterMs: willTranscribe ? null : hasText ? OVERLAY_FINAL_HOLD_MS : 0,
      };
    }

    case "final": {
      if (!eventMatchesCurrentResult(state, event.chunk_id, event.audio_path)) {
        return state;
      }
      const text = normalizeCaptionText(event.text);
      if (!text) {
        return resetCaption(state);
      }
      return {
        ...state,
        chunkId: event.chunk_id ?? state.chunkId,
        audioPath: event.audio_path,
        status: "final",
        text,
        visible: true,
        hideAfterMs: OVERLAY_FINAL_HOLD_MS,
      };
    }

    case "failed":
    case "hidden":
      return resetCaption(state);
  }
}

export function hideCaptionForGeneration(
  state: OverlayCaptionState,
  generation: number,
): OverlayCaptionState {
  if (state.generation !== generation) {
    return state;
  }
  return resetCaption(state);
}

function resetCaption(state: OverlayCaptionState): OverlayCaptionState {
  return {
    ...initialCaptionState(),
    generation: state.generation + 1,
  };
}

function normalizeCaptionText(text: string): string {
  return text.replace(/\s+/g, " ").trim();
}

function eventMatchesCurrentChunk(state: OverlayCaptionState, chunkId: number): boolean {
  return state.chunkId == null || state.chunkId === chunkId;
}

function eventMatchesCurrentResult(
  state: OverlayCaptionState,
  chunkId: number | null,
  audioPath: string,
): boolean {
  if (chunkId != null && state.chunkId != null) {
    return chunkId === state.chunkId;
  }
  if (state.audioPath) {
    return state.audioPath === audioPath;
  }
  return true;
}
