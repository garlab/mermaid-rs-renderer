//! Public parser/layout checks for the native Gantt contract.
use mermaid_rs_renderer::layout::DiagramData;
use mermaid_rs_renderer::{LayoutConfig, Theme, compute_layout, parse_mermaid_strict, render_svg};

#[test]
fn combined_tags_survive_parsing_and_milestones_use_the_interval_midpoint() {
    let parsed = parse_mermaid_strict("gantt\nA: crit, done, a, 2026-01-01, 4d\nM: crit, milestone, 2026-01-01, 4d\nZ: milestone, after a, 0d").unwrap();
    let status = parsed.graph.gantt_tasks[0].status.unwrap();
    assert!(status.critical && status.done && !status.milestone);
    let config = LayoutConfig::default();
    for theme in [Theme::modern(), Theme::dark()] {
        let layout = compute_layout(&parsed.graph, &theme, &config);
        let DiagramData::Gantt(chart) = &layout.diagram else {
            panic!("expected Gantt")
        };
        assert!((chart.tasks[1].x - (chart.chart_x + chart.chart_width / 2.)).abs() < 0.01);
        assert_eq!(chart.tasks[2].duration, 0.);
        assert_eq!(chart.tasks[2].start, chart.tasks[0].start + 4.);
        let svg = render_svg(&layout, &theme, &config);
        assert_eq!(svg.matches("<polygon").count(), 2);
        assert!(svg.contains("#e25555"));
    }
}

#[test]
fn multiline_labels_fit_rows_and_date_ticks_are_unique() {
    let source = "gantt\naxisFormat %d %b\nsection A section with a long descriptive label\nA particularly long task description with accents éàï: 2026-01-01, 1d\nB: 1d";
    let parsed = parse_mermaid_strict(source).unwrap();
    let config = LayoutConfig {
        max_label_width_chars: 18,
        ..Default::default()
    };
    let layout = compute_layout(&parsed.graph, &Theme::modern(), &config);
    let DiagramData::Gantt(chart) = &layout.diagram else {
        panic!("expected Gantt")
    };
    for task in &chart.tasks {
        assert!(task.label.height < chart.row_height);
    }
    assert_eq!(chart.ticks.len(), 3);
    assert_eq!(
        chart
            .ticks
            .iter()
            .map(|t| t.label.as_str())
            .collect::<Vec<_>>(),
        ["01 Jan", "02 Jan", "03 Jan"]
    );
    let svg = render_svg(&layout, &Theme::modern(), &config);
    for word in ["particularly", "description", "accents", "éàï"] {
        assert!(svg.contains(word), "missing wrapped label text: {word}");
    }
}

#[test]
fn programmatic_invalid_schedules_return_an_error_layout() {
    let mut graph = parse_mermaid_strict("gantt\nA: 2026-01-01, 1d")
        .unwrap()
        .graph;
    graph.gantt_tasks[0].duration = Some("30q".into());
    let layout = compute_layout(&graph, &Theme::modern(), &LayoutConfig::default());
    assert!(matches!(layout.diagram, DiagramData::Error(_)));
}

#[test]
fn millisecond_bars_and_milestones_keep_relative_positions_at_year_9999() {
    let graph = parse_mermaid_strict("gantt\ndateFormat YYYY-MM-DD HH:mm:ss.SSS\naxisFormat %S.%L\nA: a, 9999-01-01 12:00:00.000, 1ms\nB: b, after a, 2ms\nC: milestone, after b, 0ms").unwrap().graph;
    let config = LayoutConfig::default();
    let layout = compute_layout(&graph, &Theme::modern(), &config);
    let DiagramData::Gantt(chart) = &layout.diagram else {
        panic!("expected Gantt")
    };
    assert!((chart.tasks[0].width - chart.chart_width / 3.).abs() < 0.001);
    assert!((chart.tasks[1].x - chart.chart_x - chart.chart_width / 3.).abs() < 0.001);
    assert!((chart.tasks[2].x - chart.chart_x - chart.chart_width).abs() < 0.001);
    assert!((chart.tasks[0].duration * 86_400_000. - 1.).abs() < 1e-12);
    assert_eq!(
        chart
            .ticks
            .iter()
            .map(|t| t.label.as_str())
            .collect::<Vec<_>>(),
        ["00.000", "00.001", "00.002", "00.003"]
    );
    assert!(mermaid_rs_renderer::layout::validate_layout_invariants(&layout).is_ok());
}

#[test]
fn automatic_clock_axis_distinguishes_overnight_days_without_inventing_a_date() {
    let graph = parse_mermaid_strict("gantt\ndateFormat HH:mm\nNight: 23:30, 90m")
        .unwrap()
        .graph;
    let layout = compute_layout(&graph, &Theme::modern(), &LayoutConfig::default());
    let DiagramData::Gantt(chart) = &layout.diagram else {
        panic!("expected Gantt")
    };
    assert!(chart.ticks.iter().any(|t| t.label.starts_with("+0d ")));
    assert!(chart.ticks.iter().any(|t| t.label.starts_with("+1d ")));
    assert!(chart.ticks.iter().all(|t| !t.label.contains("1970")));
    assert!((chart.tasks[0].duration * 24. - 1.5).abs() < 1e-12);
}

#[test]
fn automatic_axes_expose_seconds_and_milliseconds_and_compact_lanes_stay_exact() {
    for (format, start, duration, needle) in [
        ("HH:mm:ss", "09:00:00", "2s", "09:00:01"),
        ("HH:mm:ss.SSS", "09:00:00.000", "2ms", "09:00:00.002"),
    ] {
        let graph = parse_mermaid_strict(&format!("---\ndisplayMode: compact\n---\ngantt\ndateFormat {format}\nA: a, {start}, {duration}\nB: after a, {duration}")).unwrap().graph;
        let layout = compute_layout(&graph, &Theme::modern(), &LayoutConfig::default());
        let DiagramData::Gantt(chart) = &layout.diagram else {
            panic!("expected Gantt")
        };
        assert_eq!(chart.tasks[0].y, chart.tasks[1].y);
        assert!(
            chart.ticks.iter().any(|t| t.label.contains(needle)),
            "missing {needle}: {:?}",
            chart.ticks
        );
    }
}

#[test]
fn default_axis_keeps_labels_for_unaligned_times_and_end_of_calendar_milestones() {
    for task in [
        "dateFormat YYYY-MM-DD HH:mm\nA: 2026-11-05 09:01, 58m",
        "dateFormat YYYY-MM-DD HH:mm:ss.SSS\nA: milestone, 9999-12-31 23:59:59.999, 0ms",
    ] {
        let graph = parse_mermaid_strict(&format!("gantt\n{task}"))
            .unwrap()
            .graph;
        let layout = compute_layout(&graph, &Theme::modern(), &LayoutConfig::default());
        let DiagramData::Gantt(chart) = &layout.diagram else {
            panic!("expected Gantt")
        };
        assert!(!chart.ticks.is_empty());
        assert!(chart.ticks.iter().all(|tick| !tick.label.contains("10000")));
        assert!(mermaid_rs_renderer::layout::validate_layout_invariants(&layout).is_ok());
    }
}
