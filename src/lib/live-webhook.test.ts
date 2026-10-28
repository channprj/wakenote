import { describe, expect, it } from "vitest";
import { webhookUrlError } from "./live-webhook";

describe("webhook URL validation", () => {
  it("rejects malformed or non-http(s) URLs while enabled", () => {
    for (const endpoint_url of [
      "",
      "example.com",
      "file:///tmp/hook",
      "ftp://example.com",
      "http://",
      "https://user@",
      "https://exa mple.com",
      "https:example.com",
    ]) {
      expect(
        webhookUrlError({
          enabled: true,
          endpoint_url,
          payload_format: "text_only",
        }),
        JSON.stringify(endpoint_url),
      ).not.toBeNull();
    }
  });
  it("accepts valid http(s) URLs", () => {
    for (const endpoint_url of [
      "https://example.com/hook?token=test",
      "http://127.0.0.1:8080",
      "http://[::1]/hook",
      "https://예시.한국/전사",
      "HTTPS://example.com/hook",
    ]) {
      expect(
        webhookUrlError({ enabled: true, endpoint_url, payload_format: "json" }),
        JSON.stringify(endpoint_url),
      ).toBeNull();
    }
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
