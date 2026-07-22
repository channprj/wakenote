import type {
  MeetingFinishedPayload,
  MeetingProgressPayload,
  MeetingSegmentPayload,
} from "./types";

export type MeetingEventListen = <Payload>(
  eventName: string,
  handler: (event: { payload: Payload }) => void,
) => Promise<() => void>;

export interface MeetingEventHandlers {
  onProgress: (payload: MeetingProgressPayload) => void;
  onSegment: (payload: MeetingSegmentPayload) => void;
  onFinished: (payload: MeetingFinishedPayload) => void;
}

/** Install all listeners before the caller hydrates the meeting list. */
export async function subscribeMeetingEvents(
  listen: MeetingEventListen,
  handlers: MeetingEventHandlers,
): Promise<Array<() => void>> {
  const unlisteners: Array<() => void> = [];
  try {
    unlisteners.push(
      await listen<MeetingProgressPayload>("meeting-progress", (event) =>
        handlers.onProgress(event.payload),
      ),
    );
    unlisteners.push(
      await listen<MeetingSegmentPayload>(
        "meeting-segment-committed",
        (event) => handlers.onSegment(event.payload),
      ),
    );
    unlisteners.push(
      await listen<MeetingFinishedPayload>("meeting-finished", (event) =>
        handlers.onFinished(event.payload),
      ),
    );
    return unlisteners;
  } catch (error) {
    for (const unlisten of unlisteners.reverse()) {
      try {
        unlisten();
      } catch {
        // Preserve the registration error while still attempting every cleanup.
      }
    }
    throw error;
  }
}
