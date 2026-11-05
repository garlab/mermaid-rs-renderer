use anyhow::{Result, bail, ensure};

use super::{ParseOutput, extract_frontmatter_value, preprocess_input};
use crate::gantt::time::{DateFormat, format_timestamp, looks_like_duration, parse_date};
use crate::gantt::{MAX_TASKS, schedule};
use crate::ir::{DiagramKind, Direction, Edge, GanttStatus, GanttTask, Graph, NodeShape, Subgraph};

pub(super) fn parse(input: &str) -> Result<ParseOutput> {
    let mut graph = Graph::new();
    graph.kind = DiagramKind::Gantt;
    graph.direction = Direction::LeftRight;
    let (lines, init_config) = preprocess_input(input)?;
    graph.gantt_display_mode = extract_frontmatter_value(input, "displayMode");
    ensure!(
        graph
            .gantt_display_mode
            .as_deref()
            .is_none_or(|s| s == "compact"),
        "unsupported Gantt display mode"
    );
    let mut section = None;
    for line in lines {
        let (keyword, value) = line.split_once(char::is_whitespace).unwrap_or((&line, ""));
        let value = value.trim();
        match keyword.to_ascii_lowercase().as_str() {
            "gantt" => {
                ensure!(value.is_empty(), "invalid Gantt header");
            }
            "title" => {
                graph.gantt_title = Some(value.to_string());
            }
            "dateformat" => {
                DateFormat::parse(value)?;
                graph.gantt_date_format = Some(value.to_string());
            }
            "axisformat" => {
                ensure!(!value.is_empty(), "empty Gantt axisFormat");
                format_timestamp(0, value, true)?;
                graph.gantt_axis_format = Some(value.to_string());
            }
            "todaymarker" => {
                ensure!(value == "off", "only todayMarker off is supported");
            }
            "excludes" => {
                ensure!(!value.is_empty(), "empty Gantt excludes");
                for token in value
                    .split(|c: char| c == ',' || c.is_whitespace())
                    .filter(|s| !s.is_empty())
                {
                    if token.eq_ignore_ascii_case("weekends") {
                        graph.gantt_excludes_weekends = true;
                    } else {
                        parse_date(token)?;
                        ensure!(
                            graph.gantt_excluded_dates.len() < 128,
                            "too many Gantt excluded dates"
                        );
                        graph.gantt_excluded_dates.push(token.to_string());
                    }
                }
            }
            "section" => {
                ensure!(
                    !value.is_empty() && graph.gantt_sections.len() < 128,
                    "invalid or excessive Gantt sections"
                );
                section = Some(value.to_string());
                graph.gantt_sections.push(value.to_string());
                graph.subgraphs.push(Subgraph {
                    id: Some(format!("section_{}", graph.subgraphs.len())),
                    label: value.to_string(),
                    nodes: Vec::new(),
                    direction: None,
                    icon: None,
                });
            }
            "tickinterval" | "includes" | "weekend" | "weekday" | "click" | "inclusiveenddates"
            | "topaxis" => {
                bail!("unsupported Gantt option: {keyword}");
            }
            _ => {
                ensure!(graph.gantt_tasks.len() < MAX_TASKS, "too many Gantt tasks");
                let (label, meta) = line
                    .split_once(':')
                    .ok_or_else(|| anyhow::anyhow!("invalid Gantt statement"))?;
                ensure!(!label.trim().is_empty(), "empty Gantt task label");
                let task =
                    parse_task(label.trim(), meta, graph.gantt_tasks.len(), section.clone())?;
                graph.ensure_node(
                    &task.id,
                    Some(task.label.clone()),
                    Some(NodeShape::Rectangle),
                );
                if section.is_some() {
                    graph
                        .subgraphs
                        .last_mut()
                        .unwrap()
                        .nodes
                        .push(task.id.clone());
                }
                graph.gantt_tasks.push(task);
            }
        }
    }
    // Validate the complete schedule before any caller can render a chart.
    // Unrecognized options and unresolved dates must never become guessed bars.
    schedule(&graph)?;
    for (i, task) in graph.gantt_tasks.iter().enumerate() {
        let dependencies: Vec<_> = if let Some(after) = &task.after {
            after.split_whitespace().collect()
        } else if task.start.is_none() && i > 0 {
            vec![graph.gantt_tasks[i - 1].id.as_str()]
        } else {
            Vec::new()
        };
        for from in dependencies {
            graph.edges.push(Edge {
                from: from.to_string(),
                to: task.id.clone(),
                label: None,
                start_label: None,
                end_label: None,
                directed: true,
                arrow_start: false,
                arrow_end: true,
                arrow_start_kind: None,
                arrow_end_kind: None,
                start_decoration: None,
                end_decoration: None,
                style: crate::ir::EdgeStyle::Solid,
            });
        }
    }
    Ok(ParseOutput { graph, init_config })
}

fn parse_task(label: &str, meta: &str, index: usize, section: Option<String>) -> Result<GanttTask> {
    let mut fields: Vec<_> = meta.split(',').map(str::trim).collect();
    ensure!(
        fields.iter().all(|s| !s.is_empty()),
        "empty Gantt task field"
    );
    let mut status = GanttStatus::default();
    let mut tags = 0;
    for token in &fields {
        match token.to_ascii_lowercase().as_str() {
            "done" => status.done = true,
            "active" => status.active = true,
            "crit" => status.critical = true,
            "milestone" => status.milestone = true,
            _ => break,
        }
        tags += 1;
    }
    fields.drain(..tags);
    ensure!((1..=3).contains(&fields.len()), "invalid Gantt task fields");
    let id = if fields.len() == 3 {
        let id = fields.remove(0);
        ensure!(
            id.bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-')),
            "invalid Gantt task ID"
        );
        id.to_string()
    } else {
        format!("__gantt_{index}")
    };
    let (mut start, mut after) = (None, None);
    if fields.len() == 2 {
        let first = fields.remove(0);
        if let Some((keyword, ids)) = first.split_once(char::is_whitespace)
            && keyword.eq_ignore_ascii_case("after")
        {
            ensure!(!ids.trim().is_empty(), "empty Gantt dependency");
            after = Some(ids.trim().to_string());
        } else {
            start = Some(first.to_string());
        }
    }
    let end_token = fields[0];
    let (end, duration) = if looks_like_duration(end_token) {
        (None, Some(end_token.to_string()))
    } else {
        (Some(end_token.to_string()), None)
    };
    Ok(GanttTask {
        id,
        label: label.to_string(),
        start,
        end,
        duration,
        after,
        section,
        status: (tags > 0).then_some(status),
    })
}
