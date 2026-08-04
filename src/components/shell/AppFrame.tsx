import type { ReactNode } from "react";
import type { PrimaryRoute } from "@/lib/navigation";
import { AppSidebar } from "./AppSidebar";

export function AppFrame({
  activeRoute,
  queueAttentionCount,
  queueAttentionTone,
  onNavigate,
  children,
  statusRail,
  theme,
}: {
  activeRoute: PrimaryRoute;
  queueAttentionCount: number;
  queueAttentionTone: "warning" | "danger";
  onNavigate: (route: PrimaryRoute) => void;
  children: ReactNode;
  statusRail?: ReactNode;
  theme?: "light" | "dark";
}) {
  return (
    <div className="app-frame" data-theme={theme}>
      <AppSidebar
        activeRoute={activeRoute}
        queueAttentionCount={queueAttentionCount}
        queueAttentionTone={queueAttentionTone}
        onNavigate={onNavigate}
      />
      <main className="app-viewport">
        <div className="app-page">{children}</div>
        {statusRail}
      </main>
    </div>
  );
}
