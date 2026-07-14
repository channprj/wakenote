import {
  ActivityIcon,
  AudioWaveformIcon,
  CalendarClockIcon,
  FilesIcon,
  FileTextIcon,
  Settings2Icon,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import appIcon from "@/assets/wakenote-app.png";
import { PRIMARY_NAV, type PrimaryRoute } from "@/lib/navigation";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { StatusBadge } from "@/components/ui/status-badge";

const NAV_ICONS: Record<(typeof PRIMARY_NAV)[number]["id"], LucideIcon> = {
  capture: AudioWaveformIcon,
  meetings: CalendarClockIcon,
  transcripts: FilesIcon,
  reports: FileTextIcon,
  activity: ActivityIcon,
};

export function AppSidebar({
  activeRoute,
  queueAttentionCount,
  onNavigate,
}: {
  activeRoute: PrimaryRoute;
  queueAttentionCount: number;
  onNavigate: (route: PrimaryRoute) => void;
}) {
  return (
    <aside className="app-sidebar">
      <div className="app-sidebar__brand">
        <span className="app-sidebar__mark" aria-hidden="true">
          <img src={appIcon} alt="" />
        </span>
        <span className="app-sidebar__brand-copy">
          <strong>WakeNote</strong>
          <small>Recorder workspace</small>
        </span>
      </div>

      <nav className="app-sidebar__nav" aria-label="Primary navigation">
        {PRIMARY_NAV.map((item) => {
          const Icon = NAV_ICONS[item.id];
          const active = activeRoute === item.id;
          return (
            <Button
              key={item.id}
              type="button"
              size="sm"
              variant={active ? "secondary" : "ghost"}
              className="app-sidebar__nav-item"
              data-route={item.id}
              data-active={active}
              aria-current={active ? "page" : undefined}
              onClick={() => onNavigate(item.id)}
            >
              <Icon data-icon="inline-start" />
              <span>{item.label}</span>
              {item.id === "activity" && queueAttentionCount > 0 ? (
                <StatusBadge tone="danger">{queueAttentionCount}</StatusBadge>
              ) : null}
            </Button>
          );
        })}
      </nav>

      <div className="app-sidebar__utility">
        <Separator />
        <Button
          type="button"
          size="sm"
          variant={activeRoute === "settings" ? "secondary" : "ghost"}
          className="app-sidebar__nav-item"
          data-route="settings"
          data-active={activeRoute === "settings"}
          aria-current={activeRoute === "settings" ? "page" : undefined}
          onClick={() => onNavigate("settings")}
        >
          <Settings2Icon data-icon="inline-start" />
          <span>Settings</span>
        </Button>
        <span className="app-sidebar__version">v{__APP_VERSION__}</span>
      </div>
    </aside>
  );
}
