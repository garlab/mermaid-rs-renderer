//! Public parser/layout checks for the native day-based Gantt contract.
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
    graph.gantt_tasks[0].duration = Some("30m".into());
    let layout = compute_layout(&graph, &Theme::modern(), &LayoutConfig::default());
    assert!(matches!(layout.diagram, DiagramData::Error(_)));
}
