use mermaid_rs_renderer::{LayoutConfig, Theme, compute_layout, parse_mermaid, render_svg};

fn render(source: &str, theme: &Theme) -> String {
    let graph = parse_mermaid(source).unwrap().graph;
    let config = LayoutConfig::default();
    render_svg(&compute_layout(&graph, theme, &config), theme, &config)
}

fn text_element<'a>(svg: &'a str, label: &str) -> &'a str {
    svg.split("<text ")
        .filter_map(|part| part.split_once("</text>"))
        .map(|(element, _)| element)
        .find(|element| {
            element.contains(&format!(">{label}<")) || element.ends_with(&format!(">{label}"))
        })
        .unwrap_or_else(|| panic!("missing text: {label}"))
}

fn themes() -> [Theme; 6] {
    let custom = Theme {
        primary_text_color: "#ffffff".into(),
        text_color: "#ffeecc".into(),
        background: "#19222a".into(),
        ..Theme::modern()
    };
    [
        Theme::modern(),
        Theme::mermaid_default(),
        Theme::dark(),
        Theme::forest(),
        Theme::neutral(),
        custom,
    ]
}

fn foreground(svg: &str, label: &str) -> String {
    text_element(svg, label)
        .split("fill=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap()
        .to_owned()
}

fn contrast(foreground: &str, background: &str) -> f32 {
    fn luminance(color: &str) -> f32 {
        let channel = |offset| {
            let value = u8::from_str_radix(&color[offset..offset + 2], 16).unwrap() as f32 / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5)
    }
    let a = luminance(foreground);
    let b = luminance(background);
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[test]
fn quadrant_axes_follow_theme_and_preserve_escaped_multiline_labels() {
    for theme in themes() {
        let source =
            "quadrantChart\n x-axis Left --> Right\n y-axis Bottom<br/>Second --> Top<br/>Fourth";
        let graph = parse_mermaid(source).unwrap().graph;
        let config = LayoutConfig::default();
        let mut layout = compute_layout(&graph, &theme, &config);
        let mermaid_rs_renderer::layout::DiagramData::Quadrant(quadrant) = &mut layout.diagram
        else {
            panic!("expected quadrant chart");
        };
        quadrant.y_axis_top.as_mut().unwrap().lines[1] = "Fourth & <fifth>".into();
        let svg = render_svg(&layout, &theme, &config);
        for label in [
            "Left",
            "Right",
            "Bottom",
            "Second",
            "Top",
            "Fourth &amp; &lt;fifth&gt;",
        ] {
            assert!(
                text_element(&svg, label)
                    .contains(&format!("fill=\"{}\"", theme.primary_text_color)),
                "axis label does not follow theme: {label}"
            );
        }
        assert!(!svg.contains("<fifth>"));
    }
}

#[test]
fn timeline_and_journey_text_contrast_with_fixed_pastel_backgrounds() {
    for theme in themes() {
        let source = "timeline\n title History\n 2020 : First\n 2021 : Second\n 2022 : Third\n 2023 : Fourth\n 2024 : Fifth\n 2025 : Sixth";
        let timeline = render(source, &theme);
        for ((year, label), background) in [
            ("2020", "First"),
            ("2021", "Second"),
            ("2022", "Third"),
            ("2023", "Fourth"),
            ("2024", "Fifth"),
            ("2025", "Sixth"),
        ]
        .into_iter()
        .zip([
            "#ECECFF", "#FFE6CC", "#D5E8D4", "#F8CECC", "#FFF2CC", "#E1D5E7",
        ]) {
            for text in [year, label] {
                assert!(
                    contrast(&foreground(&timeline, text), background) >= 4.5,
                    "low contrast for {text}"
                );
            }
        }
        assert_eq!(foreground(&timeline, "History"), theme.primary_text_color);
        let graph = parse_mermaid("journey\n title Reading\n One: 1: Reader\n Two: 2: Reader\n Three: 3: Reader\n Four: 4: Reader\n Five: 5: Reader").unwrap().graph;
        let config = LayoutConfig::default();
        let layout = compute_layout(&graph, &theme, &config);
        let mermaid_rs_renderer::layout::DiagramData::Journey(journey) = &layout.diagram else {
            panic!("expected journey");
        };
        let svg = render_svg(&layout, &theme, &config);
        for task in &journey.tasks {
            let label = format!("{:.0}", task.score.unwrap());
            assert!(
                contrast(&foreground(&svg, &label), &task.score_color) >= 4.5,
                "low contrast for score {label}"
            );
        }
        assert_eq!(foreground(&svg, "Reading"), theme.primary_text_color);
    }
}
