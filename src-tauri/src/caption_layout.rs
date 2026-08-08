use crate::settings::FloatingOverlayCaptionStyle;

const ASCII_WIDTH: f64 = 0.58;
const NON_ASCII_WIDTH: f64 = 1.0;
const AVERAGE_GLYPH_WIDTH_RATIO: f64 = 0.56;
const EXTRA_LINE_PENALTY_RATIO: f64 = 0.2;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaptionLineMetrics {
    pub widest_line_width: f64,
    pub line_count: usize,
}

pub fn balanced_caption_text(raw: &str, style: &FloatingOverlayCaptionStyle) -> String {
    let normalized = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let capacity = caption_line_capacity(style);
    if normalized.is_empty() || weighted_width(&normalized) <= capacity {
        return normalized;
    }

    let tokens = normalized.split(' ').collect::<Vec<_>>();
    if tokens.len() == 1 {
        return normalized;
    }
    let token_widths = tokens
        .iter()
        .map(|token| weighted_width(token))
        .collect::<Vec<_>>();
    let minimum_lines = minimum_line_count(&token_widths, capacity);
    let Some(mut best) = layout_for_line_count(&tokens, &token_widths, capacity, minimum_lines)
    else {
        return normalized;
    };

    if minimum_lines < tokens.len()
        && let Some(mut candidate) =
            layout_for_line_count(&tokens, &token_widths, capacity, minimum_lines + 1)
    {
        candidate.score += capacity.powi(2) * EXTRA_LINE_PENALTY_RATIO;
        if candidate.score < best.score {
            best = candidate;
        }
    }

    best.lines.join("\n")
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

fn minimum_line_count(token_widths: &[f64], capacity: f64) -> usize {
    let mut line_count = 1;
    let mut current_width = 0.0;

    for &token_width in token_widths {
        let next_width = if current_width == 0.0 {
            token_width
        } else {
            current_width + ASCII_WIDTH + token_width
        };
        if current_width > 0.0 && next_width > capacity {
            line_count += 1;
            current_width = token_width;
        } else {
            current_width = next_width;
        }
    }

    line_count
}

#[derive(Debug)]
struct Layout {
    score: f64,
    lines: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
struct LayoutNode {
    score: f64,
    previous_end: usize,
}

fn layout_for_line_count(
    tokens: &[&str],
    token_widths: &[f64],
    capacity: f64,
    line_count: usize,
) -> Option<Layout> {
    let target = target_width(token_widths, line_count);
    let mut nodes = vec![vec![None::<LayoutNode>; tokens.len() + 1]; line_count + 1];
    nodes[0][0] = Some(LayoutNode {
        score: 0.0,
        previous_end: 0,
    });

    for line_number in 1..=line_count {
        for start in 0..tokens.len() {
            let Some(previous) = nodes[line_number - 1][start] else {
                continue;
            };
            let mut width = 0.0;
            for end in (start + 1)..=tokens.len() {
                if end > start + 1 {
                    width += ASCII_WIDTH;
                }
                width += token_widths[end - 1];

                let single_oversized_token = end == start + 1 && token_widths[start] > capacity;
                if width > capacity && !single_oversized_token {
                    break;
                }
                let remaining_lines = line_count - line_number;
                let remaining_tokens = tokens.len() - end;
                if remaining_tokens < remaining_lines
                    || (remaining_lines == 0 && remaining_tokens != 0)
                {
                    continue;
                }

                let mut score = previous.score + (target - width).powi(2);
                if end < tokens.len() {
                    score -= break_bonus(tokens[end - 1]);
                } else if width < target * 0.55 {
                    score += (target * 0.55 - width).powi(2) * 4.0 + target.powi(2) * 0.2;
                }

                let replace = nodes[line_number][end]
                    .map(|current| score < current.score)
                    .unwrap_or(true);
                if replace {
                    nodes[line_number][end] = Some(LayoutNode {
                        score,
                        previous_end: start,
                    });
                }
            }
        }
    }

    let final_node = nodes[line_count][tokens.len()]?;
    let mut ranges = Vec::with_capacity(line_count);
    let mut end = tokens.len();
    for line_number in (1..=line_count).rev() {
        let node = nodes[line_number][end]?;
        ranges.push((node.previous_end, end));
        end = node.previous_end;
    }
    ranges.reverse();

    Some(Layout {
        score: final_node.score,
        lines: ranges
            .into_iter()
            .map(|(start, end)| tokens[start..end].join(" "))
            .collect(),
    })
}

fn target_width(token_widths: &[f64], line_count: usize) -> f64 {
    let token_width = token_widths.iter().sum::<f64>();
    let retained_spaces = token_widths.len().saturating_sub(line_count) as f64 * ASCII_WIDTH;
    (token_width + retained_spaces) / line_count as f64
}

fn break_bonus(token: &str) -> f64 {
    match token.chars().last() {
        Some('.' | '?' | '!' | '。' | '？' | '！') => 6.0,
        Some(',' | ';' | ':' | '，' | '；' | '：') => 2.0,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LONG_KOREAN_MEMO: &str = "그리고 장문의 그 서브타이틀일 경우에는 사실 듀레이션이 어느 정도 길게 있어야 할 것 같습니다. 왜냐하면 어쨌건 읽는 데 시간이 또 어느 정도 필요로 하기 때문에, 대부분은 읽다가 이게 서브타이틀이 사라지는 그런 불편함을 마주하게 될 것 같아요. 그래서 이런 기능이 이제 수정, 개선되어야 하고, 또 줄바꿈이 그 문장의 어떤 끝맺음이나, 아니면 일정 그 길이 이상일 때 이루어지도록 그 처리가 되어야 할 것 같습니다. 그렇지 않으면 이제 사용, 실제로 사용하는 데 좀 불편함을 많이 느낄 것 같고, 음, 음.";

    fn caption_style() -> FloatingOverlayCaptionStyle {
        crate::settings::AppSettings::default().floating_overlay_caption_style()
    }

    #[test]
    fn long_korean_memo_forms_visually_balanced_lines_within_capacity() {
        let style = caption_style();
        let capacity = caption_line_capacity(&style);
        let balanced = balanced_caption_text(LONG_KOREAN_MEMO, &style);
        let lines = balanced.lines().collect::<Vec<_>>();

        assert!(lines.len() >= 3);
        assert!(lines.iter().all(|line| weighted_width(line) <= capacity));
        let widths = lines
            .iter()
            .map(|line| weighted_width(line))
            .collect::<Vec<_>>();
        let widest = widths.iter().copied().fold(0.0, f64::max);
        let narrowest = widths.iter().copied().fold(f64::MAX, f64::min);
        assert!(widest - narrowest < 12.0, "line widths: {widths:?}");
    }

    #[test]
    fn short_korean_and_english_captions_remain_unchanged() {
        let style = caption_style();

        assert_eq!(balanced_caption_text("짧은 자막", &style), "짧은 자막");
        assert_eq!(
            balanced_caption_text("A short subtitle", &style),
            "A short subtitle"
        );
    }

    #[test]
    fn sentence_endings_are_a_soft_preference_between_balanced_candidates() {
        let mut style = caption_style();
        style.max_width_px = 160;

        let balanced = balanced_caption_text("alpha beta. gamma delta", &style);
        assert_eq!(balanced, "alpha beta.\ngamma delta");

        let unbalanced = balanced_caption_text("a. balanced subtitle continues here", &style);
        assert_ne!(unbalanced.lines().next(), Some("a."));
    }

    #[test]
    fn oversized_url_is_preserved_as_one_unmodified_token() {
        let mut style = caption_style();
        style.max_width_px = 240;
        let url = "https://example.com/a/very/long/unbreakable/subtitle/token";
        let raw = format!("링크 확인 {url} 완료");

        let balanced = balanced_caption_text(&raw, &style);
        assert!(balanced.contains(url));
        assert_eq!(balanced.replace('\n', " "), raw);
    }

    #[test]
    fn line_metrics_count_explicit_and_emergency_rows() {
        let metrics = caption_line_metrics("첫 줄\n두 번째로 긴 줄", 3.0);

        assert_eq!(metrics.line_count, 4);
        assert_eq!(metrics.widest_line_width, 3.0);
    }
}
