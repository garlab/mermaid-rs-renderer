use std::collections::BTreeMap;

use crate::config::LayoutConfig;
use crate::ir::Graph;
use crate::theme::Theme;

use super::text::measure_label;
use super::{DiagramData, Layout, QuadrantLayout, QuadrantPointLayout, TextBlock};

fn quadrant_palette(_theme: &Theme) -> Vec<String> {
    vec![
        "#6366f1".to_string(), // indigo
        "#f59e0b".to_string(), // amber
        "#10b981".to_string(), // emerald
        "#ef4444".to_string(), // red
        "#8b5cf6".to_string(), // violet
        "#06b6d4".to_string(), // cyan
    ]
}

fn finite_unit_interval(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

pub(super) fn compute_quadrant_layout(
    graph: &Graph,
    theme: &Theme,
    config: &LayoutConfig,
) -> Layout {
    let padding = theme.font_size * 1.6;
    let grid_size = 360.0;
    // Measure title
    let title = graph
        .quadrant
        .title
        .as_ref()
        .map(|t| measure_label(t, theme, config));
    let title_height = title.as_ref().map(|t| t.height + padding).unwrap_or(0.0);

    // Measure axis labels
    let x_left = graph
        .quadrant
        .x_axis_left
        .as_ref()
        .map(|t| measure_label(t, theme, config));
    let x_right = graph
        .quadrant
        .x_axis_right
        .as_ref()
        .map(|t| measure_label(t, theme, config));
    let y_bottom = graph
        .quadrant
        .y_axis_bottom
        .as_ref()
        .map(|t| measure_label(t, theme, config));
    let y_top = graph
        .quadrant
        .y_axis_top
        .as_ref()
        .map(|t| measure_label(t, theme, config));

    // Measure quadrant labels
    let q_labels: [Option<TextBlock>; 4] = [
        graph.quadrant.quadrant_labels[0]
            .as_ref()
            .map(|t| measure_label(t, theme, config)),
        graph.quadrant.quadrant_labels[1]
            .as_ref()
            .map(|t| measure_label(t, theme, config)),
        graph.quadrant.quadrant_labels[2]
            .as_ref()
            .map(|t| measure_label(t, theme, config)),
        graph.quadrant.quadrant_labels[3]
            .as_ref()
            .map(|t| measure_label(t, theme, config)),
    ];

    let y_axis_label_width = y_bottom
        .as_ref()
        .map(|t| t.width)
        .unwrap_or(0.0)
        .max(y_top.as_ref().map(|t| t.width).unwrap_or(0.0));
    let y_axis_width = if y_axis_label_width > 0.0 {
        y_axis_label_width + padding
    } else {
        padding
    };
    let x_axis_height = x_left
        .as_ref()
        .map(|t| t.height + padding)
        .unwrap_or(padding);

    let base_grid_x = y_axis_width + padding;
    let base_grid_y = title_height + padding;

    // Measure points before fixing the canvas bounds. QuadrantPointLayout has a
    // single anchor shared by the marker and its label, so reserve any label
    // overflow by translating/expanding the canvas rather than clamping that
    // anchor and changing the point's data mapping.
    let palette = quadrant_palette(theme);
    let measured_points: Vec<(f32, f32, TextBlock, String)> = graph
        .quadrant
        .points
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let x = finite_unit_interval(p.x);
            let y = finite_unit_interval(p.y);
            let label = measure_label(&p.label, theme, config);
            (x, y, label, palette[i % palette.len()].clone())
        })
        .collect();

    let base_width = base_grid_x + grid_size + padding * 2.0;
    let base_height = base_grid_y + grid_size + x_axis_height + padding;
    let mut left_overflow = 0.0f32;
    let mut right_overflow = 0.0f32;
    let mut top_overflow = 0.0f32;
    let mut bottom_overflow = 0.0f32;
    for (x, y, label, _) in &measured_points {
        let center_x = base_grid_x + x * grid_size;
        let center_y = base_grid_y + (1.0 - y) * grid_size;
        left_overflow = left_overflow.max(label.width / 2.0 - center_x);
        right_overflow = right_overflow.max(center_x + label.width / 2.0 - base_width);
        top_overflow = top_overflow.max(label.height / 2.0 - center_y);
        // Also contain the SVG label, whose center is below its data marker.
        bottom_overflow = bottom_overflow
            .max(center_y + QuadrantPointLayout::LABEL_OFFSET_Y + label.height / 2.0 - base_height);
    }
    let grid_x = base_grid_x + left_overflow;
    let grid_y = base_grid_y + top_overflow;

    let points: Vec<QuadrantPointLayout> = measured_points
        .into_iter()
        .map(|(x, y, label, color)| QuadrantPointLayout {
            label,
            x: grid_x + x * grid_size,
            y: grid_y + (1.0 - y) * grid_size,
            color,
        })
        .collect();

    let width = base_width + left_overflow + right_overflow;
    let height = base_height + top_overflow + bottom_overflow;

    Layout {
        kind: graph.kind,
        nodes: BTreeMap::new(),
        edges: Vec::new(),
        subgraphs: Vec::new(),
        width,
        height,
        diagram: DiagramData::Quadrant(QuadrantLayout {
            title,
            title_y: title_height / 2.0 + top_overflow,
            x_axis_left: x_left,
            x_axis_right: x_right,
            y_axis_bottom: y_bottom,
            y_axis_top: y_top,
            quadrant_labels: q_labels,
            points,
            grid_x,
            grid_y,
            grid_width: grid_size,
            grid_height: grid_size,
        }),
    }
}
