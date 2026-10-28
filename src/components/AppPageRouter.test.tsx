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
  "webhooks",
  "costs",
  "settings",
];

const pages = Object.fromEntries(
  routes.map((route) => [route, <span key={route}>{route}-page</span>]),
) as Record<PrimaryRoute, ReactNode>;

describe("AppPageRouter", () => {
  it("renders only the active workspace for every route", () => {
    for (const route of routes) {
      const markup = renderToStaticMarkup(
        <AppPageRouter route={route} pages={pages} />,
      );

      expect(markup, route).toContain(`${route}-page`);
      for (const otherRoute of routes.filter(
        (candidate) => candidate !== route,
      )) {
        expect(markup, `${route} route`).not.toContain(`${otherRoute}-page`);
      }
    }
  });
});
