import { describe, expect, it } from "vitest";
import { webhookUrlError } from "./live-webhook";

describe("webhook URL validation", () => {
  it.each([
    "",
    "example.com",
    "file:///tmp/hook",
    "ftp://example.com",
    "http://",
    "https://user@",
    "https://exa mple.com",
    "https:example.com",
  ])("rejects %s while enabled", (endpoint_url) => {
    expect(
      webhookUrlError({
        enabled: true,
        endpoint_url,
        payload_format: "text_only",
      }),
    ).not.toBeNull();
  });
  it.each([
    "https://example.com/hook?token=test",
    "http://127.0.0.1:8080",
    "http://[::1]/hook",
    "https://예시.한국/전사",
    "HTTPS://example.com/hook",
  ])("accepts %s", (endpoint_url) => {
    expect(
      webhookUrlError({ enabled: true, endpoint_url, payload_format: "json" }),
    ).toBeNull();
  });
  it("always permits disabling", () => {
    expect(
      webhookUrlError({
        enabled: false,
        endpoint_url: "invalid",
        payload_format: "text_only",
      }),
    ).toBeNull();
  });
});
