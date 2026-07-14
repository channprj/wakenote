import type { ReactNode } from "react";
import type { PrimaryRoute } from "@/lib/navigation";
import { AppSidebar } from "./AppSidebar";

export function AppFrame({
  activeRoute,
  queueAttentionCount,
  onNavigate,
  children,
  statusRail,
}: {
  activeRoute: PrimaryRoute;
  queueAttentionCount: number;
  onNavigate: (route: PrimaryRoute) => void;
  children: ReactNode;
  statusRail?: ReactNode;
}) {
  return (
    <div className="app-frame">
      <AppSidebar
        activeRoute={activeRoute}
        queueAttentionCount={queueAttentionCount}
        onNavigate={onNavigate}
      />
      <main className="app-viewport">
        <div className="app-page">{children}</div>
        {statusRail}
      </main>
    </div>
  );
}
