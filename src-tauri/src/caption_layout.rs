use crate::settings::FloatingOverlayCaptionStyle;

const ASCII_WIDTH: f64 = 0.58;
const NON_ASCII_WIDTH: f64 = 1.0;
const AVERAGE_GLYPH_WIDTH_RATIO: f64 = 0.56;

pub const MAX_CAPTION_LINES: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaptionLineMetrics {
    pub widest_line_width: f64,
    pub line_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CaptionLayoutSignature {
    font_size_px: u32,
    padding_horizontal_px: u32,
    border_width_px: u32,
    max_width_px: u32,
}

impl From<&FloatingOverlayCaptionStyle> for CaptionLayoutSignature {
    fn from(style: &FloatingOverlayCaptionStyle) -> Self {
        Self {
            font_size_px: style.font_size_px,
            padding_horizontal_px: style.padding_horizontal_px,
            border_width_px: style.border_width_px,
            max_width_px: style.max_width_px,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DisplayUnit {
    text: String,
    leading_space: bool,
}

impl DisplayUnit {
    fn render_into(&self, rendered: &mut String) {
        if self.leading_space {
            rendered.push(' ');
        }
        rendered.push_str(&self.text);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptionPageUpdate {
    pub text: String,
    pub page_turned: bool,
}

#[derive(Debug, Clone, Default)]
pub struct StableCaptionPager {
    normalized: String,
    units: Vec<DisplayUnit>,
    page_start: usize,
    line_ends: Vec<usize>,
    text: String,
    signature: Option<CaptionLayoutSignature>,
}

impl StableCaptionPager {
    pub fn update(&mut self, raw: &str, style: &FloatingOverlayCaptionStyle) -> CaptionPageUpdate {
        let normalized = normalize(raw);
        if normalized.is_empty() {
            self.clear();
            return CaptionPageUpdate {
                text: String::new(),
                page_turned: false,
            };
        }

        let signature = CaptionLayoutSignature::from(style);
        if self.normalized == normalized && self.signature.as_ref() == Some(&signature) {
            return CaptionPageUpdate {
                text: self.text.clone(),
                page_turned: false,
            };
        }

        let capacity = caption_line_capacity(style);
        let new_units = display_units(&normalized, capacity);
        let mut page_turned = false;

        if self
            .signature
            .as_ref()
            .is_some_and(|current| current != &signature)
        {
            self.page_start = 0;
            self.line_ends.clear();
            page_turned = !self.text.is_empty();
        } else if self.units.is_empty() {
            self.page_start = 0;
            self.line_ends.clear();
        } else {
            let (prefix, suffix) = matching_prefix_and_suffix(&self.units, &new_units);
            let old_changed_end = self.units.len() - suffix;
            let new_changed_end = new_units.len() - suffix;
            let delta = new_units.len() as isize - self.units.len() as isize;

            if self.page_start >= old_changed_end {
                self.page_start = shifted_index(self.page_start, delta);
                self.line_ends = self
                    .line_ends
                    .iter()
                    .map(|&end| shifted_index(end, delta))
                    .collect();
            } else if prefix < self.page_start || old_changed_end > self.page_start {
                let retained = self
                    .line_ends
                    .iter()
                    .copied()
                    .filter_map(|end| {
                        if end <= prefix {
                            Some(end)
                        } else if end >= old_changed_end {
                            Some(shifted_index(end, delta))
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();
                if retained.len() != self.line_ends.len() || prefix == 0 {
                    page_turned |= !self.text.is_empty() && prefix == 0 && self.page_start > 0;
                }
                self.line_ends = retained;
                if prefix == 0 {
                    self.page_start = 0;
                }
            }

            self.page_start = self.page_start.min(new_units.len());
            self.line_ends.retain(|&end| {
                end > self.page_start && end <= new_changed_end.max(prefix).max(new_units.len())
            });
        }

        self.units = new_units;
        self.normalized = normalized;
        self.signature = Some(signature);

        if let Some(reset_start) = self.first_overflowing_anchor(capacity) {
            self.page_start = reset_start;
            self.line_ends.clear();
            page_turned = true;
        }

        page_turned |= self.greedy_complete_page(capacity);
        self.text = self.render();
        CaptionPageUpdate {
            text: self.text.clone(),
            page_turned,
        }
    }

    pub fn clear(&mut self) {
        self.normalized.clear();
        self.units.clear();
        self.page_start = 0;
        self.line_ends.clear();
        self.text.clear();
        self.signature = None;
    }

    fn first_overflowing_anchor(&self, capacity: f64) -> Option<usize> {
        let mut start = self.page_start;
        for &end in &self.line_ends {
            if end <= start || end > self.units.len() {
                return Some(start);
            }
            if units_width(&self.units[start..end]) > capacity {
                return Some(start);
            }
            start = end;
        }
        None
    }

    fn greedy_complete_page(&mut self, capacity: f64) -> bool {
        let mut page_turned = false;
        'page: loop {
            self.line_ends
                .retain(|&end| end > self.page_start && end <= self.units.len());
            let mut line_start = self.line_ends.last().copied().unwrap_or(self.page_start);
            if self.line_ends.len() == MAX_CAPTION_LINES && line_start < self.units.len() {
                self.page_start = line_start;
                self.line_ends.clear();
                page_turned = true;
                continue;
            }
            let mut width = 0.0;
            let mut index = line_start;

            while index < self.units.len() {
                let unit_width = unit_width_in_line(&self.units[index], index == line_start);
                if index > line_start && width + unit_width > capacity {
                    self.line_ends.push(index);
                    if self.line_ends.len() == MAX_CAPTION_LINES {
                        self.page_start = index;
                        self.line_ends.clear();
                        page_turned = true;
                        continue 'page;
                    }
                    line_start = index;
                    width = 0.0;
                    continue;
                }
                width += unit_width;
                index += 1;
            }

            if index > line_start {
                self.line_ends.push(index);
            }
            return page_turned;
        }
    }

    fn render(&self) -> String {
        if self.page_start >= self.units.len() {
            return String::new();
        }
        let mut rendered = String::new();
        let mut start = self.page_start;
        for end in self
            .line_ends
            .iter()
            .copied()
            .chain(std::iter::once(self.units.len()))
        {
            if end <= start {
                continue;
            }
            if !rendered.is_empty() {
                rendered.push('\n');
            }
            for unit in &self.units[start..end] {
                unit.render_into(&mut rendered);
            }
            start = end;
        }
        rendered
    }
}

pub fn caption_line_capacity(style: &FloatingOverlayCaptionStyle) -> f64 {
    caption_line_capacity_for_width(style, style.max_width_px as f64)
}

pub(crate) fn caption_line_capacity_for_width(
    style: &FloatingOverlayCaptionStyle,
    max_width: f64,
) -> f64 {
    let font_size = (style.font_size_px as f64).clamp(10.0, 48.0);
    let content_width =
        max_width - style.padding_horizontal_px as f64 * 2.0 - style.border_width_px as f64 * 2.0;

    (content_width / (font_size * AVERAGE_GLYPH_WIDTH_RATIO)).max(1.0)
}

pub fn caption_line_metrics(text: &str, capacity: f64) -> CaptionLineMetrics {
    let capacity = capacity.max(1.0);
    let mut widest_line_width = 0.0_f64;
    let mut line_count = 0_usize;

    for line in text.split('\n') {
        let width = weighted_width(line);
        widest_line_width = widest_line_width.max(width.min(capacity));
        line_count += (width / capacity).ceil().max(1.0) as usize;
    }

    CaptionLineMetrics {
        widest_line_width,
        line_count: line_count.max(1),
    }
}

fn normalize(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn display_units(normalized: &str, capacity: f64) -> Vec<DisplayUnit> {
    normalized
        .split(' ')
        .enumerate()
        .flat_map(|(word_index, word)| split_word(word, word_index > 0, capacity))
        .collect()
}

fn split_word(word: &str, leading_space: bool, capacity: f64) -> Vec<DisplayUnit> {
    let mut units = Vec::new();
    let mut fragment = String::new();
    let mut fragment_leading_space = leading_space;
    let mut width = if leading_space { ASCII_WIDTH } else { 0.0 };

    for character in word.chars() {
        let character_width = if character.is_ascii() {
            ASCII_WIDTH
        } else {
            NON_ASCII_WIDTH
        };
        if !fragment.is_empty() && width + character_width > capacity {
            units.push(DisplayUnit {
                text: std::mem::take(&mut fragment),
                leading_space: fragment_leading_space,
            });
            fragment_leading_space = false;
            width = 0.0;
        }
        fragment.push(character);
        width += character_width;
    }
    if !fragment.is_empty() {
        units.push(DisplayUnit {
            text: fragment,
            leading_space: fragment_leading_space,
        });
    }
    units
}

fn matching_prefix_and_suffix(old: &[DisplayUnit], new: &[DisplayUnit]) -> (usize, usize) {
    let prefix = old
        .iter()
        .zip(new)
        .take_while(|(left, right)| left == right)
        .count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(left, right)| left == right)
        .count();
    (prefix, suffix)
}

fn shifted_index(index: usize, delta: isize) -> usize {
    index.saturating_add_signed(delta)
}

fn units_width(units: &[DisplayUnit]) -> f64 {
    units
        .iter()
        .enumerate()
        .map(|(index, unit)| unit_width_in_line(unit, index == 0))
        .sum()
}

fn unit_width_in_line(unit: &DisplayUnit, line_start: bool) -> f64 {
    weighted_width(&unit.text)
        + if unit.leading_space && !line_start {
            ASCII_WIDTH
        } else {
            0.0
        }
}

fn weighted_width(text: &str) -> f64 {
    text.chars()
        .map(|character| {
            if character.is_ascii() {
                ASCII_WIDTH
            } else {
                NON_ASCII_WIDTH
            }
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compact_style() -> FloatingOverlayCaptionStyle {
        let mut style = crate::settings::AppSettings::default().floating_overlay_caption_style();
        style.max_width_px = 160;
        style
    }

    fn text_for_rows(style: &FloatingOverlayCaptionStyle, rows: usize) -> String {
        let capacity = caption_line_capacity(style);
        let words_per_row =
            (((capacity + ASCII_WIDTH) / (NON_ASCII_WIDTH + ASCII_WIDTH)).floor() as usize).max(1);
        (0..words_per_row * rows)
            .map(|_| "가")
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn appended_partial_keeps_existing_breaks_and_wraps_near_capacity() {
        let style = compact_style();
        let mut pager = StableCaptionPager::default();
        let first = pager.update("하나 다섯 여섯", &style);
        let first_line = first.text.lines().next().unwrap().to_string();
        let grown = pager.update("하나 다섯 여섯 일곱", &style);
        assert_eq!(grown.text.lines().next(), Some(first_line.as_str()));
        assert!(
            grown
                .text
                .lines()
                .all(|line| weighted_width(line) <= caption_line_capacity(&style))
        );
        assert!(weighted_width(&first_line) > caption_line_capacity(&style) * 0.70);
    }

    #[test]
    fn fourth_row_atomically_starts_a_new_page_with_overflow() {
        let style = compact_style();
        let mut pager = StableCaptionPager::default();
        let three_rows = text_for_rows(&style, 3);
        let before = pager.update(&three_rows, &style);
        assert_eq!(before.text.lines().count(), MAX_CAPTION_LINES);
        let overflow_word = "새페이지";
        let after = pager.update(&format!("{three_rows} {overflow_word}"), &style);
        assert!(after.page_turned);
        assert!(after.text.trim_start().starts_with(overflow_word));
        assert!(after.text.lines().count() <= MAX_CAPTION_LINES);
        assert!(!after.text.contains(before.text.lines().next().unwrap()));
    }

    #[test]
    fn same_length_provider_correction_keeps_line_slots() {
        let style = compact_style();
        let mut pager = StableCaptionPager::default();
        let before = pager.update("alpha beta gamma delta epsilon zeta", &style);
        let before_line_count = before.text.lines().count();
        let corrected = pager.update("alpha beta revised delta epsilon zeta", &style);
        assert!(corrected.text.contains("revised"));
        assert_eq!(corrected.text.lines().count(), before_line_count);
        assert!(!corrected.page_turned);
    }

    #[test]
    fn oversized_token_is_display_split_without_mutating_raw_text() {
        let style = compact_style();
        let raw = "https://example.com/one/very/long/unbroken/path";
        let mut pager = StableCaptionPager::default();
        let page = pager.update(raw, &style);
        assert!(page.text.lines().count() <= MAX_CAPTION_LINES);
        assert!(page.page_turned);
    }

    #[test]
    fn repeated_input_is_idempotent_and_short_captions_stay_plain() {
        let style = compact_style();
        let mut pager = StableCaptionPager::default();
        let first = pager.update("짧은 자막", &style);
        let repeated = pager.update("짧은 자막", &style);
        assert_eq!(first.text, "짧은 자막");
        assert_eq!(repeated, first);
        assert_eq!(
            pager.update("Short subtitle", &style).text,
            "Short subtitle"
        );
    }

    #[test]
    fn radical_correction_resets_to_a_valid_page() {
        let style = compact_style();
        let mut pager = StableCaptionPager::default();
        pager.update(&text_for_rows(&style, 3), &style);
        let corrected = pager.update("completely different corrected transcript", &style);
        assert!(corrected.text.contains("transcript"));
        assert!(corrected.text.lines().count() <= MAX_CAPTION_LINES);
    }

    #[test]
    fn long_single_update_shows_the_newest_complete_page() {
        let style = compact_style();
        let mut pager = StableCaptionPager::default();
        let source = text_for_rows(&style, 4);
        let page = pager.update(&source, &style);
        assert!(page.page_turned);
        assert_eq!(page.text.lines().count(), 1);
        assert!(page.text.trim_start().starts_with("가"));
    }

    #[test]
    fn line_metrics_count_explicit_and_emergency_rows() {
        let metrics = caption_line_metrics("첫 줄\n두 번째로 긴 줄", 3.0);
        assert_eq!(metrics.line_count, 4);
        assert_eq!(metrics.widest_line_width, 3.0);
    }
}
