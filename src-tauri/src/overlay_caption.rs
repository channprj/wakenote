use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::caption_layout::{CaptionPageUpdate, StableCaptionPager};
use crate::settings::{FloatingOverlayCaptionStyle, FloatingOverlayPosition};

pub const OVERLAY_CAPTION_UPDATED_EVENT: &str = "overlay-caption-updated";
pub const OVERLAY_CAPTION_HIDDEN_EVENT: &str = "overlay-caption-hidden";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayCaptionSource {
    #[default]
    LiveTranscription,
    Dictation,
    Preview,
}

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
    #[serde(default)]
    pub source: OverlayCaptionSource,
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
    source: OverlayCaptionSource,
    phase: OverlayCaptionPhase,
    chunk_id: Option<u64>,
    audio_path: Option<String>,
    raw_text: String,
    text: String,
    pager: StableCaptionPager,
    effective_max_width_px: Option<f64>,
    position: FloatingOverlayPosition,
    style: FloatingOverlayCaptionStyle,
    hide_at: Option<Instant>,
    scheduled_generation: Option<u64>,
}

impl Default for OverlayCaptionRuntime {
    fn default() -> Self {
        Self {
            generation: 0,
            visible: false,
            source: OverlayCaptionSource::default(),
            phase: OverlayCaptionPhase::Idle,
            chunk_id: None,
            audio_path: None,
            raw_text: String::new(),
            text: String::new(),
            pager: StableCaptionPager::default(),
            effective_max_width_px: None,
            position: FloatingOverlayPosition::Off,
            style: default_overlay_caption_style(),
            hide_at: None,
            scheduled_generation: None,
        }
    }
}

impl OverlayCaptionRuntime {
    pub fn snapshot(&self) -> OverlayCaptionSnapshot {
        OverlayCaptionSnapshot {
            generation: self.generation,
            visible: self.visible,
            source: self.source,
            phase: self.phase,
            chunk_id: self.chunk_id,
            audio_path: self.audio_path.clone(),
            text: self.text.clone(),
            position: self.position,
            final_hold_ms: self
                .hide_at
                .map(|_| self.final_hold_duration().as_millis() as u64),
            style: self.style.clone(),
        }
    }

    pub fn start_chunk(
        &mut self,
        chunk_id: u64,
        position: FloatingOverlayPosition,
        style: FloatingOverlayCaptionStyle,
    ) {
        let _ = self.start_chunk_for_source(
            OverlayCaptionSource::LiveTranscription,
            chunk_id,
            position,
            style,
        );
    }

    pub fn start_chunk_for_source(
        &mut self,
        source: OverlayCaptionSource,
        chunk_id: u64,
        position: FloatingOverlayPosition,
        style: FloatingOverlayCaptionStyle,
    ) -> bool {
        if !self.source_can_replace(source) {
            return false;
        }
        if self.source != source || self.chunk_id != Some(chunk_id) {
            self.generation = self.generation.saturating_add(1);
        }
        self.visible = false;
        self.source = source;
        self.phase = OverlayCaptionPhase::Idle;
        self.chunk_id = Some(chunk_id);
        self.audio_path = None;
        self.raw_text.clear();
        self.text.clear();
        self.pager.clear();
        self.position = position;
        self.style = style;
        self.hide_at = None;
        true
    }

    pub fn show_partial(
        &mut self,
        chunk_id: u64,
        text: impl AsRef<str>,
        position: FloatingOverlayPosition,
        style: FloatingOverlayCaptionStyle,
    ) {
        self.show_partial_at(chunk_id, text, position, style, Instant::now());
    }

    pub fn show_partial_for_source(
        &mut self,
        source: OverlayCaptionSource,
        chunk_id: u64,
        text: impl AsRef<str>,
        position: FloatingOverlayPosition,
        style: FloatingOverlayCaptionStyle,
    ) -> bool {
        self.show_partial_for_source_at(source, chunk_id, text, position, style, Instant::now())
    }

    fn show_partial_at(
        &mut self,
        chunk_id: u64,
        text: impl AsRef<str>,
        position: FloatingOverlayPosition,
        style: FloatingOverlayCaptionStyle,
        now: Instant,
    ) {
        let _ = self.show_partial_for_source_at(
            OverlayCaptionSource::LiveTranscription,
            chunk_id,
            text,
            position,
            style,
            now,
        );
    }

    fn show_partial_for_source_at(
        &mut self,
        source: OverlayCaptionSource,
        chunk_id: u64,
        text: impl AsRef<str>,
        position: FloatingOverlayPosition,
        style: FloatingOverlayCaptionStyle,
        now: Instant,
    ) -> bool {
        if matches!(position, FloatingOverlayPosition::Off) {
            return self.hide_source(source);
        }
        if !self.source_can_replace(source)
            || (self.source == source
                && self.chunk_id == Some(chunk_id)
                && self.phase == OverlayCaptionPhase::Final)
        {
            return false;
        }

        let text = normalize_caption_text(text.as_ref());
        if text.is_empty() {
            return false;
        }

        let source_or_chunk_changed = self.source != source || self.chunk_id != Some(chunk_id);
        if source_or_chunk_changed {
            self.generation = self.generation.saturating_add(1);
            self.pager.clear();
        }
        self.visible = true;
        self.source = source;
        self.phase = OverlayCaptionPhase::Partial;
        self.chunk_id = Some(chunk_id);
        self.position = position;
        self.style = style;
        self.raw_text = text;
        let update = self.refresh_display_text();
        if update.page_turned {
            self.generation = self.generation.saturating_add(1);
        }
        self.hide_at = Some(now + self.final_hold_duration());
        true
    }

    pub fn mark_committed(&mut self, chunk_id: u64, audio_path: PathBuf, will_transcribe: bool) {
        self.mark_committed_at(chunk_id, audio_path, will_transcribe, Instant::now());
    }

    pub fn mark_committed_for_source(
        &mut self,
        source: OverlayCaptionSource,
        chunk_id: u64,
        audio_path: PathBuf,
        will_transcribe: bool,
    ) -> bool {
        self.mark_committed_for_source_at(
            source,
            chunk_id,
            audio_path,
            will_transcribe,
            Instant::now(),
        )
    }

    fn mark_committed_at(
        &mut self,
        chunk_id: u64,
        audio_path: PathBuf,
        will_transcribe: bool,
        now: Instant,
    ) {
        let _ = self.mark_committed_for_source_at(
            OverlayCaptionSource::LiveTranscription,
            chunk_id,
            audio_path,
            will_transcribe,
            now,
        );
    }

    fn mark_committed_for_source_at(
        &mut self,
        source: OverlayCaptionSource,
        chunk_id: u64,
        audio_path: PathBuf,
        will_transcribe: bool,
        now: Instant,
    ) -> bool {
        if !self.source_can_replace(source) || (self.visible && self.source != source) {
            return false;
        }
        if self.chunk_id.is_some() && self.chunk_id != Some(chunk_id) {
            return false;
        }
        self.source = source;
        self.chunk_id = Some(chunk_id);
        self.audio_path = Some(audio_path.to_string_lossy().to_string());

        if self.raw_text.is_empty() {
            self.visible = false;
            self.hide_at = None;
            return true;
        }

        if will_transcribe {
            self.visible = true;
            self.phase = OverlayCaptionPhase::Refining;
            self.hide_at = Some(now + self.final_hold_duration());
        } else {
            self.visible = true;
            self.phase = OverlayCaptionPhase::Final;
            self.hide_at = Some(now + self.final_hold_duration());
        }
        true
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
        self.show_final_for_source(
            OverlayCaptionSource::LiveTranscription,
            chunk_id,
            audio_path,
            text,
            position,
            style,
        )
    }

    pub fn show_final_for_source(
        &mut self,
        source: OverlayCaptionSource,
        chunk_id: Option<u64>,
        audio_path: PathBuf,
        text: impl AsRef<str>,
        position: FloatingOverlayPosition,
        style: FloatingOverlayCaptionStyle,
    ) -> bool {
        if matches!(position, FloatingOverlayPosition::Off) {
            return self.hide_source(source);
        }
        if !self.source_can_replace(source) {
            return false;
        }

        if self.source == source && !self.matches_result(chunk_id, &audio_path) {
            return false;
        }

        let text = normalize_caption_text(text.as_ref());
        if text.is_empty() {
            return self.hide_source(source);
        }

        let next_chunk_id = chunk_id.or((self.source == source).then_some(self.chunk_id).flatten());
        let source_or_chunk_changed = self.source != source || self.chunk_id != next_chunk_id;
        if source_or_chunk_changed {
            self.generation = self.generation.saturating_add(1);
            self.pager.clear();
        }
        self.visible = true;
        self.source = source;
        self.phase = OverlayCaptionPhase::Final;
        self.chunk_id = next_chunk_id;
        self.audio_path = Some(audio_path.to_string_lossy().to_string());
        self.position = position;
        self.style = style;
        self.raw_text = text;
        let update = self.refresh_display_text();
        if update.page_turned {
            self.generation = self.generation.saturating_add(1);
        }
        self.hide_at = Some(Instant::now() + self.final_hold_duration());
        true
    }

    pub fn hide_source(&mut self, source: OverlayCaptionSource) -> bool {
        if self.source != source {
            return false;
        }
        self.hide();
        true
    }

    pub fn hide(&mut self) {
        if self.visible
            || !self.raw_text.is_empty()
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
        self.raw_text.clear();
        self.text.clear();
        self.pager.clear();
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

    pub fn take_expiry_generation_to_schedule(&mut self) -> Option<u64> {
        if !self.visible
            || self.hide_at.is_none()
            || self.scheduled_generation == Some(self.generation)
        {
            return None;
        }
        self.scheduled_generation = Some(self.generation);
        Some(self.generation)
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
        let update = self.refresh_display_text();
        if update.page_turned {
            self.generation = self.generation.saturating_add(1);
        }
        if self.hide_at.is_some() {
            self.hide_at = Some(Instant::now() + self.final_hold_duration());
        }
    }

    pub fn set_effective_max_width(&mut self, effective_max_width_px: Option<f64>) {
        let normalized = effective_max_width_px.map(|width| width.max(1.0));
        if self.effective_max_width_px == normalized {
            return;
        }
        self.effective_max_width_px = normalized;
        let update = self.refresh_display_text();
        if update.page_turned {
            self.generation = self.generation.saturating_add(1);
        }
    }

    pub fn due_hide_generation(&self, now: Instant) -> Option<u64> {
        self.hide_at
            .filter(|hide_at| *hide_at <= now)
            .map(|_| self.generation)
    }

    pub fn hide_delay_for_generation(&self, generation: u64, now: Instant) -> Option<Duration> {
        if self.generation != generation {
            return None;
        }
        self.hide_at
            .map(|hide_at| hide_at.saturating_duration_since(now))
    }

    fn final_hold_duration(&self) -> Duration {
        adaptive_caption_hold(&self.raw_text, self.style.duration_seconds)
    }

    fn refresh_display_text(&mut self) -> CaptionPageUpdate {
        let update = self.pager.update_for_max_width(
            &self.raw_text,
            &self.style,
            self.effective_max_width_px
                .unwrap_or(self.style.max_width_px as f64),
        );
        self.text.clone_from(&update.text);
        update
    }

    fn source_can_replace(&self, incoming: OverlayCaptionSource) -> bool {
        !self.visible
            || self.source == incoming
            || self.source != OverlayCaptionSource::Dictation
            || incoming == OverlayCaptionSource::Dictation
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

pub fn adaptive_caption_hold(text: &str, minimum_seconds: u8) -> Duration {
    let reading_chars = text
        .chars()
        .filter(|character| !character.is_whitespace())
        .count() as u64;
    let reading_seconds = reading_chars.div_ceil(8);
    let hold_seconds = reading_seconds
        .max(u64::from(minimum_seconds.clamp(1, 10)))
        .clamp(1, 30);

    Duration::from_secs(hold_seconds)
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
    use crate::caption_layout::MAX_CAPTION_LINES;

    fn style() -> FloatingOverlayCaptionStyle {
        default_overlay_caption_style()
    }

    fn compact_style() -> FloatingOverlayCaptionStyle {
        let mut style = style();
        style.max_width_px = 160;
        style
    }

    fn text_for_rows(rows: usize) -> String {
        (0..rows * 6).map(|_| "가").collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn adaptive_hold_uses_minimum_and_reading_time_with_a_thirty_second_cap() {
        assert_eq!(
            adaptive_caption_hold("짧은 자막", 5),
            Duration::from_secs(5)
        );
        assert_eq!(
            adaptive_caption_hold(&"가".repeat(80), 5),
            Duration::from_secs(10)
        );
        assert_eq!(
            adaptive_caption_hold(&"가".repeat(160), 5),
            Duration::from_secs(20)
        );
        assert_eq!(
            adaptive_caption_hold(&"가".repeat(241), 5),
            Duration::from_secs(30)
        );
    }

    #[test]
    fn whitespace_does_not_inflate_reading_time() {
        assert_eq!(
            adaptive_caption_hold(&format!("{}   \n\t", "가".repeat(80)), 5),
            Duration::from_secs(10)
        );
    }

    #[test]
    fn growing_partial_and_final_restart_hold_from_normalized_raw_text() {
        let mut runtime = OverlayCaptionRuntime::default();
        let partial = "가".repeat(80);
        runtime.show_partial(7, &partial, FloatingOverlayPosition::Top, style());
        let generation = runtime.snapshot().generation;
        assert_eq!(runtime.raw_text, partial);
        assert_eq!(runtime.snapshot().final_hold_ms, Some(10_000));

        let final_text = "나".repeat(160);
        assert!(runtime.show_final(Some(7), PathBuf::from("/tmp/chunk.wav"), &final_text,));
        assert_eq!(runtime.snapshot().generation, generation + 1);
        assert_eq!(runtime.raw_text, final_text);
        assert_eq!(runtime.snapshot().final_hold_ms, Some(20_000));
    }

    #[test]
    fn page_turn_increments_generation_once_but_normal_growth_does_not() {
        let mut runtime = OverlayCaptionRuntime::default();
        let style = compact_style();
        runtime.show_partial(
            7,
            text_for_rows(2),
            FloatingOverlayPosition::Top,
            style.clone(),
        );
        let stable_generation = runtime.snapshot().generation;

        let three_rows = text_for_rows(3);
        runtime.show_partial(7, &three_rows, FloatingOverlayPosition::Top, style.clone());
        assert_eq!(runtime.snapshot().generation, stable_generation);

        runtime.show_partial(
            7,
            format!("{three_rows} 새페이지"),
            FloatingOverlayPosition::Top,
            style,
        );
        assert_eq!(runtime.snapshot().generation, stable_generation + 1);
        assert!(runtime.snapshot().text.lines().count() <= MAX_CAPTION_LINES);
    }

    #[test]
    fn color_only_style_change_keeps_page_while_width_change_resets_it() {
        let mut runtime = OverlayCaptionRuntime::default();
        let initial_style = compact_style();
        runtime.show_partial(
            7,
            text_for_rows(3),
            FloatingOverlayPosition::Top,
            initial_style.clone(),
        );
        let generation = runtime.snapshot().generation;
        let anchors = runtime.snapshot().text.clone();

        let mut color = initial_style;
        color.text_color = "#12abef".to_string();
        runtime.set_style(color.clone());
        assert_eq!(runtime.snapshot().generation, generation);
        assert_eq!(runtime.snapshot().text, anchors);

        let mut narrower = color;
        narrower.max_width_px /= 2;
        runtime.set_style(narrower);
        assert_eq!(runtime.snapshot().generation, generation + 1);
        assert!(runtime.snapshot().text.lines().count() <= MAX_CAPTION_LINES);
    }

    #[test]
    fn live_and_dictation_use_the_same_page_limit_and_long_raw_text_is_retained() {
        let style = compact_style();
        let long_token = "https://example.com/one/very/long/unbroken/path";
        let mut live = OverlayCaptionRuntime::default();
        live.show_partial(7, long_token, FloatingOverlayPosition::Top, style.clone());
        assert_eq!(live.raw_text, long_token);
        assert!(live.snapshot().text.lines().count() <= MAX_CAPTION_LINES);

        let mut dictation = OverlayCaptionRuntime::default();
        assert!(dictation.show_partial_for_source(
            OverlayCaptionSource::Dictation,
            7,
            long_token,
            FloatingOverlayPosition::Top,
            style,
        ));
        assert_eq!(dictation.raw_text, long_token);
        assert!(dictation.snapshot().text.lines().count() <= MAX_CAPTION_LINES);
    }

    #[test]
    fn final_update_retains_completed_anchors_without_a_page_turn() {
        let mut runtime = OverlayCaptionRuntime::default();
        let style = compact_style();
        let partial = "가 가 가 가 가 가 가";
        runtime.show_partial(7, partial, FloatingOverlayPosition::Top, style);
        let generation = runtime.snapshot().generation;
        let first_anchor = runtime.snapshot().text.lines().next().unwrap().to_string();
        assert!(runtime.show_final(
            Some(7),
            PathBuf::from("/tmp/chunk.wav"),
            format!("{partial} 가"),
        ));
        assert_eq!(runtime.snapshot().generation, generation);
        assert_eq!(
            runtime.snapshot().text.lines().next(),
            Some(first_anchor.as_str())
        );
        assert!(runtime.snapshot().text.lines().count() <= MAX_CAPTION_LINES);
    }

    #[test]
    fn stale_pre_page_turn_generation_cannot_hide_the_new_page() {
        let mut runtime = OverlayCaptionRuntime::default();
        let style = compact_style();
        let three_rows = text_for_rows(3);
        runtime.show_partial(7, &three_rows, FloatingOverlayPosition::Top, style.clone());
        let stale_generation = runtime.snapshot().generation;

        runtime.show_partial(
            7,
            format!("{three_rows} 새페이지"),
            FloatingOverlayPosition::Top,
            style,
        );

        assert!(!runtime.hide_if_generation(stale_generation));
        assert!(runtime.snapshot().visible);
        assert!(runtime.snapshot().text.starts_with("새페이지"));
    }

    #[test]
    fn hide_and_new_chunk_clear_multi_page_state() {
        let mut runtime = OverlayCaptionRuntime::default();
        let style = compact_style();
        let three_rows = text_for_rows(3);
        runtime.show_partial(7, &three_rows, FloatingOverlayPosition::Top, style.clone());
        runtime.show_partial(
            7,
            format!("{three_rows} 새페이지"),
            FloatingOverlayPosition::Top,
            style.clone(),
        );
        assert!(runtime.snapshot().text.starts_with("새페이지"));

        runtime.hide();
        runtime.show_partial(8, "새 청크", FloatingOverlayPosition::Top, style);
        assert_eq!(runtime.snapshot().text, "새 청크");
        assert_eq!(runtime.raw_text, "새 청크");
    }

    #[test]
    fn runtime_retraction_and_radical_first_page_replacement_turn_once_without_sticking() {
        let mut runtime = OverlayCaptionRuntime::default();
        let style = compact_style();
        let four_rows = text_for_rows(4);
        runtime.show_partial(7, &four_rows, FloatingOverlayPosition::Top, style.clone());
        let page_generation = runtime.snapshot().generation;

        let retracted = text_for_rows(2);
        runtime.show_partial(7, &retracted, FloatingOverlayPosition::Top, style.clone());
        assert_eq!(runtime.snapshot().generation, page_generation + 1);
        assert!(!runtime.snapshot().text.is_empty());
        runtime.show_partial(
            7,
            format!("{retracted} 가"),
            FloatingOverlayPosition::Top,
            style.clone(),
        );
        assert!(!runtime.snapshot().text.is_empty());

        let before_replacement = runtime.snapshot().generation;
        runtime.show_partial(
            7,
            text_for_rows(3).replace('가', "나"),
            FloatingOverlayPosition::Top,
            style,
        );
        assert_eq!(runtime.snapshot().generation, before_replacement + 1);
    }

    #[test]
    fn runtime_one_line_complete_replacement_advances_generation_once() {
        let mut runtime = OverlayCaptionRuntime::default();
        let style = compact_style();
        runtime.show_partial(7, "first", FloatingOverlayPosition::Top, style.clone());
        let first_generation = runtime.snapshot().generation;

        runtime.show_partial(7, "second", FloatingOverlayPosition::Top, style.clone());
        assert_eq!(runtime.snapshot().generation, first_generation + 1);

        runtime.show_partial(7, "second", FloatingOverlayPosition::Top, style);
        assert_eq!(runtime.snapshot().generation, first_generation + 1);
    }

    #[test]
    fn effective_target_width_repages_before_native_fourth_row() {
        let mut runtime = OverlayCaptionRuntime::default();
        let style = style();
        runtime.set_effective_max_width(Some(110.0));
        runtime.show_partial(
            7,
            (0..40).map(|_| "가").collect::<Vec<_>>().join(" "),
            FloatingOverlayPosition::Top,
            style,
        );
        assert!(runtime.snapshot().text.lines().count() <= MAX_CAPTION_LINES);
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
        assert_eq!(snapshot.final_hold_ms, Some(5_000));
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
        assert_eq!(snapshot.final_hold_ms, Some(5_000));
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
    fn configured_subtitle_duration_controls_the_final_hold() {
        let mut runtime = OverlayCaptionRuntime::default();
        let mut custom_style = style();
        custom_style.duration_seconds = 9;
        runtime.show_partial(
            7,
            "오래 유지되는 자막",
            FloatingOverlayPosition::BottomRight,
            custom_style.clone(),
        );
        assert!(runtime.show_final_at(
            Some(7),
            PathBuf::from("/tmp/chunk.wav"),
            "최종 자막",
            FloatingOverlayPosition::BottomRight,
            custom_style,
        ));

        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.final_hold_ms, Some(9_000));
        assert_eq!(snapshot.position, FloatingOverlayPosition::BottomRight);
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
    fn repeated_partial_updates_restart_one_generation_safe_expiry() {
        let mut runtime = OverlayCaptionRuntime::default();
        let started = Instant::now();
        runtime.show_partial_at(7, "첫 자막", FloatingOverlayPosition::Top, style(), started);
        let generation = runtime.snapshot().generation;
        assert_eq!(
            runtime.take_expiry_generation_to_schedule(),
            Some(generation)
        );
        assert_eq!(runtime.take_expiry_generation_to_schedule(), None);

        runtime.show_partial_at(
            7,
            "갱신된 자막",
            FloatingOverlayPosition::Top,
            style(),
            started + Duration::from_secs(4),
        );

        assert_eq!(runtime.snapshot().generation, generation);
        assert_eq!(runtime.take_expiry_generation_to_schedule(), None);
        assert_eq!(
            runtime.hide_delay_for_generation(generation, started + Duration::from_secs(5)),
            Some(Duration::from_secs(4))
        );
        assert_eq!(
            runtime.due_hide_generation(started + Duration::from_secs(8)),
            None
        );
        assert_eq!(
            runtime.due_hide_generation(started + Duration::from_secs(9)),
            Some(generation)
        );
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
    fn dictation_caption_blocks_live_replacement_until_hidden() {
        let mut runtime = OverlayCaptionRuntime::default();
        assert!(runtime.show_partial_for_source(
            OverlayCaptionSource::Dictation,
            7,
            "딕테이션 자막",
            FloatingOverlayPosition::Top,
            style(),
        ));

        assert!(!runtime.show_partial_for_source(
            OverlayCaptionSource::LiveTranscription,
            8,
            "라이브 자막",
            FloatingOverlayPosition::Top,
            style(),
        ));
        assert_eq!(runtime.snapshot().source, OverlayCaptionSource::Dictation);
        assert_eq!(runtime.snapshot().text, "딕테이션 자막");

        assert!(runtime.hide_source(OverlayCaptionSource::Dictation));
        assert_eq!(runtime.snapshot().source, OverlayCaptionSource::Dictation);
        assert!(!runtime.snapshot().visible);
        assert!(runtime.show_partial_for_source(
            OverlayCaptionSource::LiveTranscription,
            8,
            "라이브 자막",
            FloatingOverlayPosition::Top,
            style(),
        ));
        assert_eq!(
            runtime.snapshot().source,
            OverlayCaptionSource::LiveTranscription
        );
        assert_eq!(runtime.snapshot().text, "라이브 자막");
    }

    #[test]
    fn late_dictation_partial_cannot_replace_same_operation_final() {
        let mut runtime = OverlayCaptionRuntime::default();
        assert!(runtime.show_final_for_source(
            OverlayCaptionSource::Dictation,
            Some(7),
            PathBuf::from("/tmp/dictation.wav"),
            "최종 딕테이션",
            FloatingOverlayPosition::Top,
            style(),
        ));

        assert!(!runtime.show_partial_for_source(
            OverlayCaptionSource::Dictation,
            7,
            "늦은 중간 결과",
            FloatingOverlayPosition::Top,
            style(),
        ));
        assert_eq!(runtime.snapshot().phase, OverlayCaptionPhase::Final);
        assert_eq!(runtime.snapshot().text, "최종 딕테이션");
    }

    #[test]
    fn source_specific_hide_preserves_another_visible_source() {
        let mut runtime = OverlayCaptionRuntime::default();
        assert!(runtime.show_partial_for_source(
            OverlayCaptionSource::Dictation,
            7,
            "딕테이션 자막",
            FloatingOverlayPosition::Top,
            style(),
        ));

        assert!(!runtime.hide_source(OverlayCaptionSource::LiveTranscription));
        assert!(runtime.snapshot().visible);
        assert_eq!(runtime.snapshot().source, OverlayCaptionSource::Dictation);
        assert_eq!(runtime.snapshot().text, "딕테이션 자막");
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
