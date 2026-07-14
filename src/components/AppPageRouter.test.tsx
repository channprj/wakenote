import type { ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { PrimaryRoute } from "@/lib/navigation";
import { AppPageRouter } from "./AppPageRouter";

const routes: PrimaryRoute[] = [
  "capture",
  "meetings",
  "transcripts",
  "reports",
  "activity",
  "settings",
];

const pages = Object.fromEntries(
  routes.map((route) => [route, <span key={route}>{route}-page</span>]),
) as Record<PrimaryRoute, ReactNode>;

describe("AppPageRouter", () => {
  it.each(routes)("renders only the %s workspace", (route) => {
    const markup = renderToStaticMarkup(
      <AppPageRouter route={route} pages={pages} />,
    );

    expect(markup).toContain(`${route}-page`);
    for (const otherRoute of routes.filter((candidate) => candidate !== route)) {
      expect(markup).not.toContain(`${otherRoute}-page`);
    }
  });
});
