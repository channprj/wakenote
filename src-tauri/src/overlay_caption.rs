use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::settings::{FloatingOverlayCaptionStyle, FloatingOverlayPosition};

pub const OVERLAY_CAPTION_FINAL_HOLD: Duration = Duration::from_secs(5);
pub const OVERLAY_CAPTION_UPDATED_EVENT: &str = "overlay-caption-updated";
pub const OVERLAY_CAPTION_HIDDEN_EVENT: &str = "overlay-caption-hidden";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayCaptionPhase {
    Idle,
    Partial,
    Refining,
    Final,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverlayCaptionSnapshot {
    pub generation: u64,
    pub visible: bool,
    pub phase: OverlayCaptionPhase,
    pub chunk_id: Option<u64>,
    pub audio_path: Option<String>,
    pub text: String,
    pub position: FloatingOverlayPosition,
    pub final_hold_ms: Option<u64>,
    pub style: FloatingOverlayCaptionStyle,
}

#[derive(Debug)]
pub struct OverlayCaptionRuntime {
    generation: u64,
    visible: bool,
    phase: OverlayCaptionPhase,
    chunk_id: Option<u64>,
    audio_path: Option<String>,
    text: String,
    position: FloatingOverlayPosition,
    style: FloatingOverlayCaptionStyle,
    hide_at: Option<Instant>,
}

impl Default for OverlayCaptionRuntime {
    fn default() -> Self {
        Self {
            generation: 0,
            visible: false,
            phase: OverlayCaptionPhase::Idle,
            chunk_id: None,
            audio_path: None,
            text: String::new(),
            position: FloatingOverlayPosition::Off,
            style: default_overlay_caption_style(),
            hide_at: None,
        }
    }
}

impl OverlayCaptionRuntime {
    pub fn snapshot(&self) -> OverlayCaptionSnapshot {
        OverlayCaptionSnapshot {
            generation: self.generation,
            visible: self.visible,
            phase: self.phase,
            chunk_id: self.chunk_id,
            audio_path: self.audio_path.clone(),
            text: self.text.clone(),
            position: self.position,
            final_hold_ms: self
                .hide_at
                .map(|_| OVERLAY_CAPTION_FINAL_HOLD.as_millis() as u64),
            style: self.style.clone(),
        }
    }

    pub fn start_chunk(
        &mut self,
        chunk_id: u64,
        position: FloatingOverlayPosition,
        style: FloatingOverlayCaptionStyle,
    ) {
        if self.chunk_id != Some(chunk_id) {
            self.generation = self.generation.saturating_add(1);
        }
        self.visible = false;
        self.phase = OverlayCaptionPhase::Idle;
        self.chunk_id = Some(chunk_id);
        self.audio_path = None;
        self.text.clear();
        self.position = position;
        self.style = style;
        self.hide_at = None;
    }

    pub fn show_partial(
        &mut self,
        chunk_id: u64,
        text: impl AsRef<str>,
        position: FloatingOverlayPosition,
        style: FloatingOverlayCaptionStyle,
    ) {
        if matches!(position, FloatingOverlayPosition::Off) {
            self.hide();
            return;
        }

        let text = normalize_caption_text(text.as_ref());
        if text.is_empty() {
            return;
        }

        if self.chunk_id != Some(chunk_id) {
            self.generation = self.generation.saturating_add(1);
        }
        self.visible = true;
        self.phase = OverlayCaptionPhase::Partial;
        self.chunk_id = Some(chunk_id);
        self.text = text;
        self.position = position;
        self.style = style;
        self.hide_at = None;
    }

    pub fn mark_committed(&mut self, chunk_id: u64, audio_path: PathBuf, will_transcribe: bool) {
        if self.chunk_id.is_some() && self.chunk_id != Some(chunk_id) {
            return;
        }
        self.chunk_id = Some(chunk_id);
        self.audio_path = Some(audio_path.to_string_lossy().to_string());

        if self.text.is_empty() {
            self.visible = false;
            self.hide_at = None;
            return;
        }

        if will_transcribe {
            self.visible = true;
            self.phase = OverlayCaptionPhase::Refining;
            self.hide_at = None;
        } else {
            self.visible = true;
            self.phase = OverlayCaptionPhase::Final;
            self.hide_at = Some(Instant::now() + OVERLAY_CAPTION_FINAL_HOLD);
        }
    }

    pub fn show_final(
        &mut self,
        chunk_id: Option<u64>,
        audio_path: PathBuf,
        text: impl AsRef<str>,
    ) -> bool {
        self.show_final_at(
            chunk_id,
            audio_path,
            text,
            self.position,
            self.style.clone(),
        )
    }

    pub fn show_final_at(
        &mut self,
        chunk_id: Option<u64>,
        audio_path: PathBuf,
        text: impl AsRef<str>,
        position: FloatingOverlayPosition,
        style: FloatingOverlayCaptionStyle,
    ) -> bool {
        if matches!(position, FloatingOverlayPosition::Off) {
            self.hide();
            return true;
        }

        if !self.matches_result(chunk_id, &audio_path) {
            return false;
        }

        let text = normalize_caption_text(text.as_ref());
        if text.is_empty() {
            self.hide();
            return true;
        }

        let next_chunk_id = chunk_id.or(self.chunk_id);
        if self.chunk_id != next_chunk_id {
            self.generation = self.generation.saturating_add(1);
        }
        self.visible = true;
        self.phase = OverlayCaptionPhase::Final;
        self.chunk_id = next_chunk_id;
        self.audio_path = Some(audio_path.to_string_lossy().to_string());
        self.text = text;
        self.position = position;
        self.style = style;
        self.hide_at = Some(Instant::now() + OVERLAY_CAPTION_FINAL_HOLD);
        true
    }

    pub fn hide(&mut self) {
        if self.visible
            || !self.text.is_empty()
            || self.chunk_id.is_some()
            || self.audio_path.is_some()
        {
            self.generation = self.generation.saturating_add(1);
        }
        self.visible = false;
        self.phase = OverlayCaptionPhase::Idle;
        self.chunk_id = None;
        self.audio_path = None;
        self.text.clear();
        self.position = FloatingOverlayPosition::Off;
        self.hide_at = None;
    }

    pub fn hide_if_generation(&mut self, generation: u64) -> bool {
        if self.generation != generation {
            return false;
        }
        self.hide();
        true
    }

    pub fn set_position(&mut self, position: FloatingOverlayPosition) {
        if matches!(position, FloatingOverlayPosition::Off) {
            self.hide();
            return;
        }
        self.position = position;
    }

    pub fn set_style(&mut self, style: FloatingOverlayCaptionStyle) {
        self.style = style;
    }

    pub fn due_hide_generation(&self, now: Instant) -> Option<u64> {
        self.hide_at
            .filter(|hide_at| *hide_at <= now)
            .map(|_| self.generation)
    }

    fn matches_result(&self, chunk_id: Option<u64>, audio_path: &Path) -> bool {
        if let (Some(current), Some(incoming)) = (self.chunk_id, chunk_id) {
            return current == incoming;
        }
        if let Some(current_path) = self.audio_path.as_ref() {
            return current_path == &audio_path.to_string_lossy();
        }
        true
    }
}

fn normalize_caption_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn default_overlay_caption_style() -> FloatingOverlayCaptionStyle {
    crate::settings::AppSettings::default().floating_overlay_caption_style()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style() -> FloatingOverlayCaptionStyle {
        default_overlay_caption_style()
    }

    #[test]
    fn partial_text_makes_caption_visible() {
        let mut runtime = OverlayCaptionRuntime::default();
        runtime.show_partial(
            7,
            "  지금 말하는 내용입니다  ",
            FloatingOverlayPosition::Top,
            style(),
        );

        let snapshot = runtime.snapshot();
        assert!(snapshot.visible);
        assert_eq!(snapshot.phase, OverlayCaptionPhase::Partial);
        assert_eq!(snapshot.chunk_id, Some(7));
        assert_eq!(snapshot.text, "지금 말하는 내용입니다");
        assert_eq!(snapshot.position, FloatingOverlayPosition::Top);
        assert_eq!(snapshot.final_hold_ms, None);
    }

    #[test]
    fn commit_keeps_partial_visible_as_refining() {
        let mut runtime = OverlayCaptionRuntime::default();
        runtime.show_partial(7, "초안 자막", FloatingOverlayPosition::Top, style());
        runtime.mark_committed(7, PathBuf::from("/tmp/chunk.wav"), true);

        let snapshot = runtime.snapshot();
        assert!(snapshot.visible);
        assert_eq!(snapshot.phase, OverlayCaptionPhase::Refining);
        assert_eq!(snapshot.text, "초안 자막");
        assert_eq!(snapshot.audio_path.as_deref(), Some("/tmp/chunk.wav"));
    }

    #[test]
    fn commit_without_refinement_schedules_hold_for_partial_caption() {
        let mut runtime = OverlayCaptionRuntime::default();
        runtime.show_partial(7, "짧은 자막", FloatingOverlayPosition::Top, style());
        runtime.mark_committed(7, PathBuf::from("/tmp/chunk.wav"), false);

        let snapshot = runtime.snapshot();
        assert!(snapshot.visible);
        assert_eq!(snapshot.phase, OverlayCaptionPhase::Final);
        assert_eq!(snapshot.text, "짧은 자막");
        assert_eq!(snapshot.final_hold_ms, Some(5_000));
    }

    #[test]
    fn final_text_replaces_partial_and_schedules_hold() {
        let mut runtime = OverlayCaptionRuntime::default();
        runtime.show_partial(7, "초안 자막", FloatingOverlayPosition::Top, style());
        runtime.mark_committed(7, PathBuf::from("/tmp/chunk.wav"), true);
        runtime.show_final(Some(7), PathBuf::from("/tmp/chunk.wav"), "최종 자막");

        let snapshot = runtime.snapshot();
        assert!(snapshot.visible);
        assert_eq!(snapshot.phase, OverlayCaptionPhase::Final);
        assert_eq!(snapshot.text, "최종 자막");
        assert_eq!(snapshot.final_hold_ms, Some(5_000));
    }

    #[test]
    fn stale_hide_generation_cannot_hide_newer_caption() {
        let mut runtime = OverlayCaptionRuntime::default();
        runtime.show_final(Some(7), PathBuf::from("/tmp/old.wav"), "이전 자막");
        let stale_generation = runtime.snapshot().generation;
        runtime.show_partial(8, "새 자막", FloatingOverlayPosition::Top, style());

        assert!(!runtime.hide_if_generation(stale_generation));
        assert!(runtime.snapshot().visible);
        assert!(runtime.hide_if_generation(runtime.snapshot().generation));
        assert!(!runtime.snapshot().visible);
    }

    #[test]
    fn stale_final_result_does_not_replace_newer_caption() {
        let mut runtime = OverlayCaptionRuntime::default();
        runtime.show_partial(8, "새 자막", FloatingOverlayPosition::Top, style());

        assert!(!runtime.show_final(
            Some(7),
            PathBuf::from("/tmp/old.wav"),
            "늦게 도착한 이전 자막",
        ));
        assert_eq!(runtime.snapshot().chunk_id, Some(8));
        assert_eq!(runtime.snapshot().text, "새 자막");
        assert_eq!(runtime.snapshot().phase, OverlayCaptionPhase::Partial);
    }

    #[test]
    fn position_change_moves_visible_caption_or_hides_when_off() {
        let mut runtime = OverlayCaptionRuntime::default();
        runtime.show_partial(7, "자막", FloatingOverlayPosition::Top, style());
        runtime.set_position(FloatingOverlayPosition::Bottom);

        assert!(runtime.snapshot().visible);
        assert_eq!(runtime.snapshot().position, FloatingOverlayPosition::Bottom);

        runtime.set_position(FloatingOverlayPosition::Off);
        assert!(!runtime.snapshot().visible);
    }
}
