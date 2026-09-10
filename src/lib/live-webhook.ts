import type { LiveTranscriptionWebhookSettings } from "./types";

export function webhookUrlError(
  settings: Pick<LiveTranscriptionWebhookSettings, "enabled" | "endpoint_url" | "payload_format">,
): string | null {
  if (!settings.enabled) return null;
  const value = settings.endpoint_url.trim();
  try {
    const url = new URL(value);
    if (
      !/^https?:\/\//i.test(value) ||
      /\s/.test(value) ||
      !["http:", "https:"].includes(url.protocol) ||
      !url.hostname
    ) {
      return "Enter a valid http:// or https:// webhook URL.";
    }
    return null;
  } catch {
    return "Enter a valid http:// or https:// webhook URL.";
  }
}
