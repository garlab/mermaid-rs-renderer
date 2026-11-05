use super::*;

fn gantt_palette(theme: &Theme) -> Vec<String> {
    vec![
        theme.primary_border_color.clone(),
        "#0ea5e9".to_string(), // sky-500
        "#10b981".to_string(), // emerald-500
        "#6366f1".to_string(), // indigo-500
        "#f97316".to_string(), // orange-500
    ]
}

fn shift_color(color: &str, target_s: f32, target_l: f32, strength: f32) -> String {
    let Some((_h, s, l)) = parse_color_to_hsl(color) else {
        return color.to_string();
    };
    let delta_s = (target_s - s) * strength;
    let delta_l = (target_l - l) * strength;
    adjust_color(color, 0.0, delta_s, delta_l)
}

fn gantt_section_palette(theme: &Theme, sections: &[String]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if sections.is_empty() {
        return map;
    }
    let base = theme.primary_border_color.as_str();
    let step = 360.0 / sections.len().max(1) as f32;
    for (idx, name) in sections.iter().enumerate() {
        let hue_shift = step * idx as f32;
        let mut color = adjust_color(base, hue_shift, 0.0, 0.0);
        color = shift_color(&color, 60.0, 55.0, 0.4);
        map.insert(name.clone(), color);
    }
    map
}

fn gantt_task_color(status: Option<crate::ir::GanttStatus>, base: &str, fallback: &str) -> String {
    let base = if parse_color_to_hsl(base).is_some() {
        base.to_string()
    } else {
        fallback.to_string()
    };
    let status = status.unwrap_or_default();
    let mut color = if status.critical {
        "#e25555".to_string()
    } else if status.milestone {
        "#dba644".to_string()
    } else {
        base
    };
    if status.done {
        color = shift_color(&color, 25.0, 70.0, 0.6);
    } else if status.active {
        color = shift_color(&color, 70.0, 52.0, 0.6);
    }
    color
}

pub(super) fn compute_gantt_layout(graph: &Graph, theme: &Theme, config: &LayoutConfig) -> Layout {
    let padding = theme.font_size * 1.25;
    let mut row_height = (theme.font_size * 1.5).max(theme.font_size + 8.0);
    let label_gap = theme.font_size * 1.05;
    let Ok(schedule) = crate::gantt::schedule(graph) else {
        return super::error::compute_error_layout(graph, config);
    };

    let title = graph
        .gantt_title
        .as_ref()
        .map(|t| measure_label(t, theme, config));
    let title_height = title.as_ref().map(|t| t.height + padding).unwrap_or(0.0);

    let mut task_label_width = 0.0_f32;
    let mut section_label_width = 0.0_f32;
    for task in &graph.gantt_tasks {
        let label = measure_label(&task.label, theme, config);
        task_label_width = task_label_width.max(label.width);
        row_height = row_height.max(label.height + 8.0);
        if let Some(section) = task.section.as_ref() {
            let section_label = measure_label(section, theme, config);
            section_label_width = section_label_width.max(section_label.width);
            row_height = row_height.max(section_label.height + 8.0);
        }
    }
    task_label_width = task_label_width.max(theme.font_size * 6.5);

    let label_x = padding;
    let section_task_gap = if section_label_width > 0.0 {
        theme.font_size * 0.8
    } else {
        0.0
    };
    let label_width = section_label_width + section_task_gap + task_label_width;
    let section_label_x = label_x;
    let task_label_x = label_x + section_label_width + section_task_gap;
    let chart_y = title_height + padding;
    let chart_width =
        (theme.font_size * 26.0).max(title.as_ref().map_or(0.0, |t| t.width + padding * 2.0));

    let time_start = schedule.iter().map(|t| t.start).min().unwrap() as f32;
    let time_end = schedule.iter().map(|t| t.end).max().unwrap() as f32;
    let time_end = time_end.max(time_start + 1.0);
    let time_span = time_end - time_start;
    let axis_format = graph.gantt_axis_format.as_deref().unwrap_or("%Y-%m-%d");
    let tick_font = theme.font_size * 0.8;
    let mut max_tick_width = 0.0_f32;
    // Measure the candidate labels before choosing tick density. Long date
    // formats should reduce tick count instead of overlapping their neighbors.
    for i in 0..=4 {
        let day = (time_start + time_span * i as f32 / 4.0).round() as i32;
        let Ok(label) = crate::gantt::format_date(day, axis_format) else {
            return super::error::compute_error_layout(graph, config);
        };
        max_tick_width = max_tick_width.max(
            measure_label_with_font_size(&label, tick_font, config, false, &theme.font_family)
                .width,
        );
    }
    let chart_width = chart_width.max(max_tick_width + padding);
    let chart_x = (padding + label_width + label_gap).max(max_tick_width / 2.0 + padding);
    let time_scale = chart_width / time_span;
    let tick_count = ((chart_width / (max_tick_width + padding)).floor() as i32)
        .clamp(1, 4)
        .min(time_span as i32)
        .max(1);
    let mut ticks = Vec::new();
    for i in 0..=tick_count {
        let day = (time_start + time_span * i as f32 / tick_count as f32).round() as i32;
        let Ok(label) = crate::gantt::format_date(day, axis_format) else {
            return super::error::compute_error_layout(graph, config);
        };
        ticks.push(GanttTick {
            x: chart_x + (day as f32 - time_start) * time_scale,
            label,
        });
    }

    let compact = graph
        .gantt_display_mode
        .as_deref()
        .map(|m| m.eq_ignore_ascii_case("compact"))
        .unwrap_or(false);

    let palette = gantt_palette(theme);
    let section_palette = gantt_section_palette(theme, &graph.gantt_sections);
    let mut current_section: Option<String> = None;
    let mut current_section_idx: Option<usize> = None;
    let mut sections: Vec<GanttSectionLayout> = Vec::new();
    let mut tasks: Vec<GanttTaskLayout> = Vec::new();
    let mut y = chart_y;

    // Compact mode: pack non-overlapping tasks into the same row.
    // Each lane is y_position, end_time
    let mut lanes: Vec<(f32, f32)> = Vec::new();

    for (idx, task) in graph.gantt_tasks.iter().enumerate() {
        let timing = schedule[idx];
        let (label, status, section) = (&task.label, task.status, &task.section);
        let start = timing.start as f32;
        let duration = (timing.end - timing.start) as f32;
        if section != &current_section {
            if let Some(sec) = section.as_ref() {
                if let Some(prev_idx) = current_section_idx {
                    let height = (y - sections[prev_idx].y).max(row_height);
                    sections[prev_idx].height = height;
                }
                lanes.clear();
                let base_color = section_palette
                    .get(sec)
                    .cloned()
                    .unwrap_or_else(|| palette[idx % palette.len()].clone());
                let band_color = shift_color(&base_color, 20.0, 92.0, 0.7);
                sections.push(GanttSectionLayout {
                    label: measure_label(sec, theme, config),
                    y,
                    height: 0.0,
                    color: base_color,
                    band_color,
                });
                current_section_idx = Some(sections.len() - 1);
            } else if let Some(prev_idx) = current_section_idx {
                let height = (y - sections[prev_idx].y).max(row_height);
                sections[prev_idx].height = height;
                current_section_idx = None;
                lanes.clear();
            }
            current_section = section.clone();
        }

        let milestone_offset = if status.is_some_and(|s| s.milestone) {
            duration / 2.0
        } else {
            0.0
        };
        let bar_x = chart_x + (start + milestone_offset - time_start) * time_scale;
        let bar_width = ((timing.render_end - timing.start) as f32 * time_scale).max(1.0);
        let base_color = if let Some(sec) = section.as_ref() {
            section_palette
                .get(sec)
                .cloned()
                .unwrap_or_else(|| palette[idx % palette.len()].clone())
        } else {
            palette[idx % palette.len()].clone()
        };
        let color = gantt_task_color(status, &base_color, &palette[0]);

        let task_end = start + duration;
        let task_y = if compact {
            if let Some(lane) = lanes.iter_mut().find(|(_, end)| start >= *end) {
                let ly = lane.0;
                lane.1 = task_end;
                ly
            } else {
                let ly = y;
                lanes.push((ly, task_end));
                y += row_height;
                ly
            }
        } else {
            let ly = y;
            y += row_height;
            ly
        };

        tasks.push(GanttTaskLayout {
            label: measure_label(label, theme, config),
            x: bar_x,
            y: task_y,
            width: bar_width,
            height: row_height,
            color,
            start,
            duration,
            status,
        });
    }
    if let Some(prev_idx) = current_section_idx {
        let height = (y - sections[prev_idx].y).max(row_height);
        sections[prev_idx].height = height;
    }

    let max_tick_half_width = ticks
        .iter()
        .map(|tick| {
            measure_label_with_font_size(
                tick.label.as_str(),
                tick_font,
                config,
                false,
                theme.font_family.as_str(),
            )
            .width
                / 2.0
        })
        .fold(0.0_f32, f32::max);
    let axis_pad = row_height * 0.9 + theme.font_size;
    let height = y + padding + axis_pad;
    let label_overflow = if compact {
        padding + task_label_width
    } else {
        0.0
    };
    let right_padding = (max_tick_half_width + padding * 0.4)
        .max(label_overflow)
        .max(padding);
    let width = chart_x + chart_width + right_padding;

    Layout {
        kind: graph.kind,
        nodes: BTreeMap::new(),
        edges: Vec::new(),
        subgraphs: Vec::new(),
        diagram: DiagramData::Gantt(GanttLayout {
            title,
            sections,
            tasks,
            time_start,
            time_end,
            chart_x,
            chart_y,
            chart_width,
            chart_height: y - chart_y,
            row_height,
            label_x,
            label_width,
            section_label_x,
            section_label_width,
            task_label_x,
            task_label_width,
            title_y: chart_y - row_height * 0.6,
            ticks,
            compact,
        }),
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use crate::ir::{DiagramKind, GanttStatus, GanttTask, Graph};
    use crate::layout::LayoutConfig;
    use crate::layout::types::DiagramData;
    use crate::theme::Theme;

    use super::compute_gantt_layout;

    fn make_graph(display_mode: Option<&str>, tasks: Vec<GanttTask>) -> Graph {
        let mut graph = Graph::new();
        graph.kind = DiagramKind::Gantt;
        graph.gantt_display_mode = display_mode.map(|s| s.to_string());
        let mut sections = Vec::new();
        for t in &tasks {
            if let Some(sec) = &t.section
                && !sections.contains(sec)
            {
                sections.push(sec.clone());
            }
        }
        graph.gantt_sections = sections;
        graph.gantt_tasks = tasks;
        graph
    }

    fn task(id: &str, section: &str, start: &str, dur: &str) -> GanttTask {
        GanttTask {
            id: id.to_string(),
            label: id.to_string(),
            start: Some(start.to_string()),
            duration: Some(dur.to_string()),
            end: None,
            after: None,
            section: Some(section.to_string()),
            status: None,
        }
    }

    fn milestone(id: &str, section: &str, start: &str) -> GanttTask {
        GanttTask {
            id: id.to_string(),
            label: id.to_string(),
            start: Some(start.to_string()),
            duration: Some("0d".to_string()),
            end: None,
            after: None,
            section: Some(section.to_string()),
            status: Some(GanttStatus {
                milestone: true,
                ..Default::default()
            }),
        }
    }

    fn extract_task_ys(graph: &Graph) -> Vec<f32> {
        let theme = Theme::modern();
        let config = LayoutConfig::default();
        let layout = compute_gantt_layout(graph, &theme, &config);
        match &layout.diagram {
            DiagramData::Gantt(g) => g.tasks.iter().map(|t| t.y).collect(),
            _ => panic!("expected Gantt layout"),
        }
    }

    #[test]
    fn compact_non_overlapping_tasks_share_row() {
        let graph = make_graph(
            Some("compact"),
            vec![
                milestone("m1", "MacOS", "2025-09-01"),
                milestone("m2", "MacOS", "2026-09-01"),
                milestone("m3", "MacOS", "2027-09-01"),
            ],
        );
        let ys = extract_task_ys(&graph);
        assert_eq!(ys[0], ys[1], "m1 and m2 should share a row");
        assert_eq!(ys[1], ys[2], "m2 and m3 should share a row");
    }

    #[test]
    fn compact_overlapping_tasks_get_separate_rows() {
        let graph = make_graph(
            Some("compact"),
            vec![
                task("hw", "support", "2025-09-01", "1y"),
                task("sw", "support", "2025-09-01", "2y"),
            ],
        );
        let ys = extract_task_ys(&graph);
        assert_ne!(ys[0], ys[1], "overlapping tasks must be on different rows");
    }

    #[test]
    fn non_compact_always_separate_rows() {
        let graph = make_graph(
            None,
            vec![
                milestone("m1", "MacOS", "2025-09-01"),
                milestone("m2", "MacOS", "2026-09-01"),
                milestone("m3", "MacOS", "2027-09-01"),
            ],
        );
        let ys = extract_task_ys(&graph);
        assert_ne!(ys[0], ys[1]);
        assert_ne!(ys[1], ys[2]);
    }
}
