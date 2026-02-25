use design_reasoning::Explanation;
use eframe::egui;
use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct DetailIssue {
    pub axis: Option<String>,
    #[serde(rename = "type")]
    pub issue_type: Option<String>,
    pub span: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct DetailPayload {
    pub mode: Option<String>,
    pub fallback_reason: Option<String>,
    pub overall_state: Option<String>,
    pub next_priority_axis: Option<String>,
    #[serde(default)]
    pub issues: Vec<DetailIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IssueCard {
    axis: String,
    issue_type: String,
    summary: String,
    detail: String,
    span: Option<String>,
    reason: Option<String>,
}

pub fn parse_detail_payload(detail_json: &str) -> Option<DetailPayload> {
    serde_json::from_str::<DetailPayload>(detail_json).ok()
}

pub fn debug_mode_enabled() -> bool {
    match std::env::var("DESIGN_GUI_DEBUG_DETAIL") {
        Ok(v) => matches!(
            v.to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on" | "debug"
        ),
        Err(_) => false,
    }
}

pub fn render_explanation(
    ui: &mut egui::Ui,
    explanation: &Explanation,
    source_text: &str,
    debug_mode: bool,
) {
    const SECTION_GAP: f32 = 10.0;
    let summary_line = explanation
        .summary
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("要約はありません。");

    let Some(payload) = parse_detail_payload(&explanation.detail) else {
        ui.label(summary_line);
        ui.weak("detail parse failed (non-fatal)");
        return;
    };

    let overall_state = payload.overall_state.as_deref().unwrap_or("UNKNOWN");
    if let Some(state) = payload.overall_state.as_deref() {
        let color = overall_state_color(state);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("状態").strong());
            ui.label(egui::RichText::new(state).color(color).strong());
        });
    }
    ui.add_space(SECTION_GAP);

    if should_show_ready_completion(&payload) {
        let frame = egui::Frame::group(ui.style())
            .fill(egui::Color32::from_rgb(26, 52, 34))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(90, 160, 110)));
        frame.show(ui, |ui| {
            ui.add_space(8.0);
            ui.horizontal_centered(|ui| {
                ui.label(egui::RichText::new("✓").color(egui::Color32::LIGHT_GREEN).size(14.0));
                ui.label(
                    egui::RichText::new("設計は実装可能な水準に達しています。")
                        .strong()
                        .color(egui::Color32::LIGHT_GREEN),
                );
            });
            ui.add_space(8.0);
        });
        ui.add_space(16.0);
    } else {
        ui.label(egui::RichText::new(summary_line).strong().size(13.0));
        ui.add_space(SECTION_GAP);
    }

    if overall_state == "PARTIAL_READY" {
        ui.label(
            egui::RichText::new("設計は概ね整理されています。以下の点を明確にすると安定します。")
                .small()
                .color(egui::Color32::GRAY),
        );
        ui.add_space(SECTION_GAP);
    }

    if should_show_next_step_card(&payload) {
        let axis = payload.next_priority_axis.as_deref().unwrap_or("UNKNOWN_AXIS");
        render_next_step_card(ui, axis_display_name(axis));
        ui.add_space(SECTION_GAP);
    }

    if should_render_issues_section(&payload) {
        ui.separator();
        ui.label(egui::RichText::new("課題").strong().size(14.0));
        let cards = build_issue_cards(&payload);
        if cards.is_empty() {
            ui.weak("確認すべき課題はありません。");
        } else {
            for (idx, card) in cards.iter().enumerate() {
                render_issue_card(ui, card, source_text, idx == 0, idx);
            }
        }
        ui.add_space(SECTION_GAP);
    }

    if debug_mode {
        ui.separator();
        if let Some(mode) = payload.mode.as_deref() {
            ui.weak(format!("mode: {mode}"));
        }
        if let Some(reason) = payload.fallback_reason.as_deref() {
            ui.colored_label(
                egui::Color32::from_rgb(255, 180, 80),
                format!("Explanation generated via RuleBased (fallback: {reason})"),
            );
        }
    }
}

fn should_show_ready_completion(payload: &DetailPayload) -> bool {
    payload.overall_state.as_deref() == Some("READY") && payload.issues.is_empty()
}

fn should_show_next_step_card(payload: &DetailPayload) -> bool {
    payload.overall_state.as_deref() != Some("READY") && payload.next_priority_axis.is_some()
}

fn should_render_issues_section(payload: &DetailPayload) -> bool {
    !should_show_ready_completion(payload)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LayoutSection {
    OverallState,
    Summary,
    NextStep,
    Issues,
}

fn layout_sections(payload: &DetailPayload) -> Vec<LayoutSection> {
    let mut sections = vec![LayoutSection::OverallState, LayoutSection::Summary];
    if should_show_next_step_card(payload) {
        sections.push(LayoutSection::NextStep);
    }
    if should_render_issues_section(payload) {
        sections.push(LayoutSection::Issues);
    }
    sections
}

fn axis_display_name(axis: &str) -> &'static str {
    match axis {
        "PROBLEM_DEFINITION" => "課題定義",
        "TARGET_USER" => "対象ユーザー",
        "VALUE_PROPOSITION" => "提供価値",
        "SUCCESS_METRIC" => "成功指標",
        "SCOPE_BOUNDARY" => "対象範囲",
        "CONSTRAINT" => "制約条件",
        "TECHNICAL_STRATEGY" => "技術方針",
        "RISK_ASSUMPTION" => "リスク前提",
        _ => "未定義項目",
    }
}

fn render_next_step_card(ui: &mut egui::Ui, axis_label: &str) {
    let desired = egui::vec2(ui.available_width(), 58.0);
    let (rect, response) = ui.allocate_at_least(desired, egui::Sense::click());
    let base = egui::Color32::from_rgba_unmultiplied(255, 191, 0, 24);
    let hover = egui::Color32::from_rgba_unmultiplied(255, 191, 0, 44);
    ui.painter()
        .rect_filled(rect, 6.0, if response.hovered() { hover } else { base });
    if response.hovered() {
        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
    }
    let x = rect.left() + 10.0;
    ui.painter().text(
        egui::pos2(x, rect.top() + 9.0),
        egui::Align2::LEFT_TOP,
        "▶ 次に整理すると安定する項目",
        egui::FontId::proportional(11.0),
        egui::Color32::from_gray(170),
    );
    ui.painter().text(
        egui::pos2(x + 14.0, rect.top() + 28.0),
        egui::Align2::LEFT_TOP,
        axis_label,
        egui::FontId::proportional(16.0),
        egui::Color32::from_rgb(245, 220, 150),
    );
    ui.add_space(8.0);
}

fn overall_state_color(state: &str) -> egui::Color32 {
    match state {
        "READY" => egui::Color32::LIGHT_GREEN,
        "PARTIAL_READY" => egui::Color32::from_rgb(255, 191, 0),
        "INSUFFICIENT" => egui::Color32::LIGHT_RED,
        _ => egui::Color32::GRAY,
    }
}

fn build_issue_cards(payload: &DetailPayload) -> Vec<IssueCard> {
    payload
        .issues
        .iter()
        .take(5)
        .map(|issue| {
            let axis = issue.axis.clone().unwrap_or_else(|| "UNKNOWN_AXIS".to_string());
            let issue_type = issue
                .issue_type
                .clone()
                .unwrap_or_else(|| "UNKNOWN_TYPE".to_string());
            let summary = issue_summary(issue);
            let detail = issue_detail(issue);
            IssueCard {
                axis,
                issue_type,
                summary,
                detail,
                span: issue
                    .span
                    .as_ref()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
                reason: issue.reason.clone(),
            }
        })
        .collect()
}

fn issue_summary(issue: &DetailIssue) -> String {
    let axis = issue.axis.as_deref().unwrap_or("UNKNOWN_AXIS");
    match issue.issue_type.as_deref().unwrap_or("UNKNOWN_TYPE") {
        "MISSING" => format!("{axis} が未定義です。"),
        "AMBIGUOUS" => format!("{axis} が曖昧です。"),
        "WEAK" => format!("{axis} の根拠が弱いです。"),
        "MINOR" => format!("{axis} の補足が必要です。"),
        _ => format!("{axis} に確認事項があります。"),
    }
}

fn issue_detail(issue: &DetailIssue) -> String {
    if let Some(reason) = issue.reason.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        return format!("{reason} 明確化することで設計判断が安定します。");
    }
    "この項目を明確化すると設計の安定性が上がります。".to_string()
}

fn render_issue_card(
    ui: &mut egui::Ui,
    card: &IssueCard,
    source_text: &str,
    default_open: bool,
    issue_index: usize,
) {
    let open_id = ui.id().with(format!("issue_open_{issue_index}_{}", card.axis));
    let mut open = ui
        .ctx()
        .data_mut(|data| data.get_persisted::<bool>(open_id))
        .unwrap_or(default_issue_open(issue_index, default_open));
    let icon = if card.span.is_some() { "[!]" } else { "[i]" };
    ui.horizontal(|ui| {
        ui.label(icon);
        ui.label(egui::RichText::new(&card.summary).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(if open { "▾" } else { "▸" }).clicked() {
                open = !open;
            }
        });
    });
    ui.ctx()
        .data_mut(|data| data.insert_persisted(open_id, open));

    if open {
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.add_space(14.0);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(&card.detail).small().color(egui::Color32::GRAY));
                if let Some(span) = card.span.as_deref() {
                    render_span_section(
                        ui,
                        source_text,
                        span,
                        card.reason.as_deref(),
                        card.issue_type.as_str(),
                        issue_index,
                    );
                }
            });
        });
    }
}

fn render_span_section(
    ui: &mut egui::Ui,
    source_text: &str,
    span: &str,
    reason: Option<&str>,
    issue_type: &str,
    issue_index: usize,
) {
    let highlight_id = ui.id().with(format!("issue_highlight_{issue_index}"));
    let mut emphasize = ui
        .ctx()
        .data_mut(|data| data.get_persisted::<bool>(highlight_id))
        .unwrap_or(false);
    ui.horizontal(|ui| {
        if ui
            .small_button(if emphasize {
                "該当箇所の強調を解除"
            } else {
                "該当箇所を強調"
            })
            .clicked()
        {
            emphasize = !emphasize;
        }
        let has_highlight = !build_highlight_ranges(source_text, &[span.to_string()]).is_empty();
        if has_highlight {
            let preview = span_visual_style(issue_type, false);
            let rich = egui::RichText::new(format!("span: {span}")).color(preview.color);
            let response = ui.label(rich);
            let hovered = response.hovered() || emphasize;
            let style = span_visual_style(issue_type, hovered);
            draw_span_underline(ui, response.rect, style);
            if let Some(r) = reason {
                response.on_hover_text(r);
            }
        } else {
            ui.weak(format!("span: {span}"));
        }
    });
    ui.ctx()
        .data_mut(|data| data.insert_persisted(highlight_id, emphasize));
}

fn default_issue_open(issue_index: usize, default_open: bool) -> bool {
    if issue_index == 0 {
        true
    } else {
        default_open
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SpanVisualStyle {
    color: egui::Color32,
    solid: bool,
}

fn span_visual_style(issue_type: &str, hovered: bool) -> SpanVisualStyle {
    let (r, g, b) = match issue_type {
        "AMBIGUOUS" => (210, 170, 70),
        "CONSISTENCY" => (215, 90, 90),
        "MISSING" => (150, 150, 150),
        _ => (170, 170, 170),
    };
    let alpha = if hovered { 255 } else { 150 };
    SpanVisualStyle {
        color: egui::Color32::from_rgba_unmultiplied(r, g, b, alpha),
        solid: hovered,
    }
}

fn draw_span_underline(ui: &egui::Ui, rect: egui::Rect, style: SpanVisualStyle) {
    let y = rect.bottom() - 1.0;
    let left = rect.left();
    let right = rect.right();
    let stroke = egui::Stroke::new(1.0, style.color);
    if style.solid {
        ui.painter()
            .line_segment([egui::pos2(left, y), egui::pos2(right, y)], stroke);
        return;
    }
    let mut x = left;
    while x < right {
        let seg_end = (x + 2.0).min(right);
        ui.painter()
            .line_segment([egui::pos2(x, y), egui::pos2(seg_end, y)], stroke);
        x += 4.0;
    }
}

pub fn build_highlight_ranges(source_text: &str, spans: &[String]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::<(usize, usize)>::new();
    if source_text.is_empty() || spans.is_empty() {
        return ranges;
    }
    for span in spans {
        let needle = span.trim();
        if needle.is_empty() {
            continue;
        }
        for (byte_start, _) in source_text.match_indices(needle) {
            let byte_end = byte_start + needle.len();
            if source_text.is_char_boundary(byte_start) && source_text.is_char_boundary(byte_end) {
                let char_start = source_text[..byte_start].chars().count();
                let char_end = source_text[..byte_end].chars().count();
                if char_start < char_end && char_end <= source_text.chars().count() {
                    ranges.push((char_start, char_end));
                }
            }
        }
    }
    merge_ranges(ranges, source_text.chars().count())
}

fn merge_ranges(mut ranges: Vec<(usize, usize)>, source_char_len: usize) -> Vec<(usize, usize)> {
    if ranges.is_empty() {
        return ranges;
    }
    ranges.sort_unstable_by_key(|(start, end)| (*start, *end));
    let mut merged = Vec::<(usize, usize)>::new();
    for (start, end) in ranges {
        if start >= end || end > source_char_len {
            continue;
        }
        if let Some(last) = merged.last_mut() {
            if start <= last.1 {
                last.1 = last.1.max(end);
                continue;
            }
        }
        merged.push((start, end));
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::{
        DetailIssue, DetailPayload, axis_display_name, build_highlight_ranges, build_issue_cards,
        default_issue_open, issue_summary, layout_sections, parse_detail_payload,
        should_render_issues_section, should_show_next_step_card, should_show_ready_completion,
        span_visual_style,
    };

    #[test]
    fn parse_llm_controlled_detail() {
        let detail = r#"{
            "mode":"LLM_CONTROLLED",
            "fallback_reason":null,
            "overall_state":"READY",
            "next_priority_axis":"SUCCESS_METRIC",
            "issues":[{"axis":"SUCCESS_METRIC","type":"MISSING","span":"指標","reason":"不足"}]
        }"#;
        let parsed = parse_detail_payload(detail).expect("must parse");
        assert_eq!(parsed.mode.as_deref(), Some("LLM_CONTROLLED"));
        assert_eq!(parsed.fallback_reason, None);
        assert_eq!(parsed.issues.len(), 1);
    }

    #[test]
    fn parse_rule_based_with_fallback() {
        let detail = r#"{
            "mode":"RULE_BASED",
            "fallback_reason":"TooManySentences",
            "overall_state":"INSUFFICIENT",
            "next_priority_axis":"PROBLEM_DEFINITION",
            "issues":[]
        }"#;
        let parsed = parse_detail_payload(detail).expect("must parse");
        assert_eq!(parsed.mode.as_deref(), Some("RULE_BASED"));
        assert_eq!(parsed.fallback_reason.as_deref(), Some("TooManySentences"));
        assert!(parsed.issues.is_empty());
    }

    #[test]
    fn parse_ignores_unknown_fields() {
        let detail = r#"{
            "mode":"RULE_BASED",
            "fallback_reason":null,
            "overall_state":"PARTIAL_READY",
            "next_priority_axis":"CONSTRAINT",
            "issues":[],
            "unknown":"x",
            "nested":{"a":1}
        }"#;
        let parsed = parse_detail_payload(detail).expect("must parse");
        assert_eq!(parsed.overall_state.as_deref(), Some("PARTIAL_READY"));
    }

    #[test]
    fn highlight_with_span() {
        let src = "日本語テキストで成功指標を定義する";
        let ranges = build_highlight_ranges(src, &[String::from("成功指標")]);
        assert_eq!(ranges.len(), 1);
        assert!(ranges[0].0 < ranges[0].1);
    }

    #[test]
    fn highlight_without_span() {
        let src = "日本語テキストで成功指標を定義する";
        let ranges = build_highlight_ranges(src, &[String::from("存在しない")]);
        assert!(ranges.is_empty());
    }

    #[test]
    fn ready_without_issues_hides_next_priority_and_shows_completion() {
        let payload = DetailPayload {
            mode: Some("LLM_CONTROLLED".to_string()),
            fallback_reason: None,
            overall_state: Some("READY".to_string()),
            next_priority_axis: Some("SUCCESS_METRIC".to_string()),
            issues: vec![],
        };
        assert!(should_show_ready_completion(&payload));
        assert!(!should_show_next_step_card(&payload));
    }

    #[test]
    fn partial_ready_shows_next_priority() {
        let payload = DetailPayload {
            mode: Some("LLM_CONTROLLED".to_string()),
            fallback_reason: None,
            overall_state: Some("PARTIAL_READY".to_string()),
            next_priority_axis: Some("SUCCESS_METRIC".to_string()),
            issues: vec![],
        };
        assert!(!should_show_ready_completion(&payload));
        assert!(should_show_next_step_card(&payload));
    }

    #[test]
    fn issue_card_summary_is_compact() {
        let issue = DetailIssue {
            axis: Some("SUCCESS_METRIC".to_string()),
            issue_type: Some("MISSING".to_string()),
            span: None,
            reason: Some("成功条件が不足".to_string()),
        };
        let summary = issue_summary(&issue);
        assert_eq!(summary, "SUCCESS_METRIC が未定義です。");
    }

    #[test]
    fn issue_cards_keep_order_and_limit() {
        let mut issues = Vec::new();
        for idx in 0..7 {
            issues.push(DetailIssue {
                axis: Some(format!("AXIS_{idx}")),
                issue_type: Some("MISSING".to_string()),
                span: None,
                reason: Some("r".to_string()),
            });
        }
        let payload = DetailPayload {
            mode: Some("LLM_CONTROLLED".to_string()),
            fallback_reason: None,
            overall_state: Some("PARTIAL_READY".to_string()),
            next_priority_axis: Some("AXIS_0".to_string()),
            issues,
        };
        let cards = build_issue_cards(&payload);
        assert_eq!(cards.len(), 5);
        assert_eq!(cards[0].axis, "AXIS_0");
        assert_eq!(cards[1].axis, "AXIS_1");
    }

    #[test]
    fn span_absent_issue_has_no_highlight_target() {
        let payload = DetailPayload {
            mode: Some("LLM_CONTROLLED".to_string()),
            fallback_reason: None,
            overall_state: Some("PARTIAL_READY".to_string()),
            next_priority_axis: Some("SUCCESS_METRIC".to_string()),
            issues: vec![DetailIssue {
                axis: Some("SUCCESS_METRIC".to_string()),
                issue_type: Some("MISSING".to_string()),
                span: None,
                reason: Some("r".to_string()),
            }],
        };
        let cards = build_issue_cards(&payload);
        assert_eq!(cards.len(), 1);
        assert!(cards[0].span.is_none());
    }

    #[test]
    fn only_top_issue_is_open_by_default() {
        assert!(default_issue_open(0, false));
        assert!(!default_issue_open(1, false));
        assert!(!default_issue_open(2, false));
    }

    #[test]
    fn non_hover_span_uses_weak_style() {
        let style = span_visual_style("AMBIGUOUS", false);
        assert!(!style.solid);
        assert_eq!(style.color.a(), 150);
    }

    #[test]
    fn hover_span_uses_strong_style() {
        let style = span_visual_style("AMBIGUOUS", true);
        assert!(style.solid);
        assert_eq!(style.color.a(), 255);
    }

    #[test]
    fn next_step_card_shows_for_partial_ready_with_axis() {
        let payload = DetailPayload {
            mode: Some("LLM_CONTROLLED".to_string()),
            fallback_reason: None,
            overall_state: Some("PARTIAL_READY".to_string()),
            next_priority_axis: Some("SUCCESS_METRIC".to_string()),
            issues: vec![],
        };
        assert!(should_show_next_step_card(&payload));
    }

    #[test]
    fn next_step_card_hidden_for_ready_with_no_issues() {
        let payload = DetailPayload {
            mode: Some("LLM_CONTROLLED".to_string()),
            fallback_reason: None,
            overall_state: Some("READY".to_string()),
            next_priority_axis: Some("SUCCESS_METRIC".to_string()),
            issues: vec![],
        };
        assert!(!should_show_next_step_card(&payload));
    }

    #[test]
    fn next_step_card_shows_for_insufficient_with_axis() {
        let payload = DetailPayload {
            mode: Some("RULE_BASED".to_string()),
            fallback_reason: Some("AxisOutOfScope:TARGET_USER".to_string()),
            overall_state: Some("INSUFFICIENT".to_string()),
            next_priority_axis: Some("TARGET_USER".to_string()),
            issues: vec![DetailIssue {
                axis: Some("TARGET_USER".to_string()),
                issue_type: Some("MISSING".to_string()),
                span: None,
                reason: Some("対象が未定義".to_string()),
            }],
        };
        assert!(should_show_next_step_card(&payload));
    }

    #[test]
    fn axis_mapping_is_human_readable() {
        assert_eq!(axis_display_name("PROBLEM_DEFINITION"), "課題定義");
        assert_eq!(axis_display_name("TARGET_USER"), "対象ユーザー");
        assert_eq!(axis_display_name("VALUE_PROPOSITION"), "提供価値");
        assert_eq!(axis_display_name("SUCCESS_METRIC"), "成功指標");
        assert_eq!(axis_display_name("SCOPE_BOUNDARY"), "対象範囲");
        assert_eq!(axis_display_name("CONSTRAINT"), "制約条件");
        assert_eq!(axis_display_name("TECHNICAL_STRATEGY"), "技術方針");
        assert_eq!(axis_display_name("RISK_ASSUMPTION"), "リスク前提");
    }

    #[test]
    fn ready_completion_hides_issue_section() {
        let payload = DetailPayload {
            mode: Some("LLM_CONTROLLED".to_string()),
            fallback_reason: None,
            overall_state: Some("READY".to_string()),
            next_priority_axis: Some("SUCCESS_METRIC".to_string()),
            issues: vec![],
        };
        assert!(!should_render_issues_section(&payload));
    }

    #[test]
    fn partial_and_insufficient_keep_hierarchy_with_next_before_issues() {
        let partial = DetailPayload {
            mode: Some("LLM_CONTROLLED".to_string()),
            fallback_reason: None,
            overall_state: Some("PARTIAL_READY".to_string()),
            next_priority_axis: Some("SUCCESS_METRIC".to_string()),
            issues: vec![DetailIssue {
                axis: Some("SUCCESS_METRIC".to_string()),
                issue_type: Some("MISSING".to_string()),
                span: None,
                reason: Some("r".to_string()),
            }],
        };
        let insufficient = DetailPayload {
            mode: Some("RULE_BASED".to_string()),
            fallback_reason: None,
            overall_state: Some("INSUFFICIENT".to_string()),
            next_priority_axis: Some("TARGET_USER".to_string()),
            issues: vec![DetailIssue {
                axis: Some("TARGET_USER".to_string()),
                issue_type: Some("MISSING".to_string()),
                span: None,
                reason: Some("r".to_string()),
            }],
        };
        assert_eq!(
            layout_sections(&partial),
            vec![
                super::LayoutSection::OverallState,
                super::LayoutSection::Summary,
                super::LayoutSection::NextStep,
                super::LayoutSection::Issues
            ]
        );
        assert_eq!(
            layout_sections(&insufficient),
            vec![
                super::LayoutSection::OverallState,
                super::LayoutSection::Summary,
                super::LayoutSection::NextStep,
                super::LayoutSection::Issues
            ]
        );
    }
}
