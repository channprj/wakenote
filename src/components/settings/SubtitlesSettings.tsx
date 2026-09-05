import { Field, FieldContent, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import type { AppSnapshot } from "@/lib/types";
import {
  SettingNumberInput,
  SettingSelect,
  SettingSlider,
  SettingSwitch,
  SettingsCard,
  SettingsGrid,
} from "./settings-controls";
import type { SettingsActions } from "./types";
import { TranslationControls } from "./TranslationControls";

export function SubtitlesSettings({
  snapshot,
  actions,
}: {
  snapshot: AppSnapshot;
  actions: SettingsActions;
}) {
  const { settings } = snapshot;
  const [error, setError] = useState<string | null>(null);

  async function updateSubtitle(
    patch: Parameters<SettingsActions["onPatch"]>[0],
    preview = true,
  ) {
    setError(null);
    try {
      await (actions.onSavePatch ?? actions.onPatch)(patch);
      if (preview) await actions.onPreviewSubtitle(patch);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }

  return (
    <SettingsGrid maxColumns={2}>
      <SettingsCard
        title="Subtitles"
        description="Show Live transcription, Dictation, or both as configurable subtitles above other windows."
      >
        {error ? (
          <p role="alert" className="text-sm text-destructive">
            {error}
          </p>
        ) : null}
        <TranslationControls
          scope="subtitle"
          settings={settings}
          onPatch={actions.onPatch}
        />
        <SettingSwitch
          label="Show subtitles"
          checked={settings.show_floating_overlay}
          onCheckedChange={(show_floating_overlay) =>
            updateSubtitle({ show_floating_overlay }, false)
          }
        />
        <SettingSelect
          label="Subtitle source"
          value={settings.subtitle_source_mode}
          disabled={!settings.show_floating_overlay}
          title={
            settings.show_floating_overlay
              ? "Choose which transcription workflows appear as subtitles."
              : "Enable subtitles before choosing their source."
          }
          options={[
            { value: "live_transcription", label: "Live transcription" },
            { value: "dictation", label: "Dictation" },
            { value: "both", label: "Both" },
          ]}
          onValueChange={(subtitle_source_mode) =>
            updateSubtitle({ subtitle_source_mode }, false)
          }
        />
        <SettingSelect
          label="Subtitle position"
          value={settings.floating_overlay_position}
          disabled={!settings.show_floating_overlay}
          title={
            settings.show_floating_overlay
              ? "Choose where subtitles appear."
              : "Enable subtitles before choosing their position."
          }
          options={[
            { value: "off", label: "Off" },
            { value: "top_left", label: "Top left" },
            { value: "top", label: "Top center" },
            { value: "top_right", label: "Top right" },
            { value: "bottom_left", label: "Bottom left" },
            { value: "bottom", label: "Bottom center" },
            { value: "bottom_right", label: "Bottom right" },
          ]}
          onValueChange={(floating_overlay_position) =>
            updateSubtitle({ floating_overlay_position })
          }
        />
        <SettingSlider
          label="Subtitle font size"
          description="A short example appears on screen while you adjust the size."
          value={settings.floating_overlay_font_size_px}
          min={10}
          max={48}
          suffix=" px"
          onValueChange={(floating_overlay_font_size_px) =>
            updateSubtitle({ floating_overlay_font_size_px })
          }
        />
        <ColorSetting
          label="Subtitle text color"
          value={settings.floating_overlay_text_color}
          onChange={(floating_overlay_text_color) =>
            updateSubtitle({ floating_overlay_text_color })
          }
        />
        <SettingSelect
          label="Subtitle background"
          value={settings.floating_overlay_background_color}
          options={[
            { value: "#000000", label: "Black" },
            { value: "#ffffff", label: "White" },
          ]}
          onValueChange={(floating_overlay_background_color) =>
            updateSubtitle({
              floating_overlay_background_color,
              floating_overlay_text_color:
                floating_overlay_background_color === "#ffffff"
                  ? "#000000"
                  : "#ffffff",
            })
          }
        />
        <SettingSlider
          label="Subtitle background opacity"
          value={settings.floating_overlay_background_opacity}
          min={0}
          max={100}
          suffix="%"
          onValueChange={(floating_overlay_background_opacity) =>
            updateSubtitle({ floating_overlay_background_opacity })
          }
        />
        <SettingSlider
          label="Minimum subtitle duration"
          description="Short subtitles use this duration. Longer subtitles stay visible for their estimated reading time, up to 30 seconds."
          value={settings.subtitle_duration_seconds}
          min={1}
          max={10}
          suffix=" sec"
          onValueChange={(subtitle_duration_seconds) =>
            updateSubtitle({ subtitle_duration_seconds })
          }
        />
        <SettingSelect
          label="Subtitle animation"
          value={settings.subtitle_animation}
          options={[
            { value: "instant", label: "Instant" },
            { value: "fade", label: "Fade" },
            { value: "dissolve", label: "Dissolve" },
          ]}
          onValueChange={(subtitle_animation) =>
            updateSubtitle({ subtitle_animation })
          }
        />
      </SettingsCard>

      <SettingsCard
        title="Subtitle layout & border"
        description="Enter exact pixel values for the caption box."
      >
        <SettingNumberInput
          label="Subtitle horizontal padding"
          value={settings.subtitle_padding_horizontal_px}
          min={0}
          max={64}
          suffix="px"
          onValueChange={(subtitle_padding_horizontal_px) =>
            updateSubtitle({ subtitle_padding_horizontal_px })
          }
        />
        <SettingNumberInput
          label="Subtitle vertical padding"
          value={settings.subtitle_padding_vertical_px}
          min={0}
          max={64}
          suffix="px"
          onValueChange={(subtitle_padding_vertical_px) =>
            updateSubtitle({ subtitle_padding_vertical_px })
          }
        />
        <SettingNumberInput
          label="Subtitle border width"
          value={settings.subtitle_border_width_px}
          min={0}
          max={8}
          suffix="px"
          onValueChange={(subtitle_border_width_px) =>
            updateSubtitle({ subtitle_border_width_px })
          }
        />
        <ColorSetting
          label="Subtitle border color"
          value={settings.subtitle_border_color}
          onChange={(subtitle_border_color) =>
            updateSubtitle({ subtitle_border_color })
          }
        />
        <SettingNumberInput
          label="Subtitle corner radius"
          value={settings.subtitle_border_radius_px}
          min={0}
          max={48}
          suffix="px"
          onValueChange={(subtitle_border_radius_px) =>
            updateSubtitle({ subtitle_border_radius_px })
          }
        />
        <SettingNumberInput
          label="Subtitle maximum width"
          value={settings.subtitle_max_width_px}
          min={0}
          max={1600}
          suffix="px"
          onValueChange={(subtitle_max_width_px) =>
            updateSubtitle({ subtitle_max_width_px })
          }
        />
        <SettingNumberInput
          label="Subtitle minimum height"
          value={settings.subtitle_min_height_px}
          min={0}
          max={1200}
          suffix="px"
          onValueChange={(subtitle_min_height_px) =>
            updateSubtitle({ subtitle_min_height_px })
          }
        />
        <SettingNumberInput
          label="Subtitle maximum height"
          value={settings.subtitle_max_height_px}
          min={0}
          max={1200}
          suffix="px"
          onValueChange={(subtitle_max_height_px) =>
            updateSubtitle({ subtitle_max_height_px })
          }
        />
      </SettingsCard>
    </SettingsGrid>
  );
}

function ColorSetting({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
}) {
  return (
    <Field orientation="responsive" className="settings-row">
      <FieldContent>
        <FieldLabel>{label}</FieldLabel>
      </FieldContent>
      <Input
        aria-label={label}
        type="color"
        className="w-16 p-1"
        value={value}
        onChange={(event) => onChange(event.currentTarget.value)}
      />
    </Field>
  );
}
import { useState } from "react";
