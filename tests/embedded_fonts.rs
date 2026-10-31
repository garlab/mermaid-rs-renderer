use fontdb::{Database, FaceInfo, ID, Language, Source, Stretch, Style, Weight};
use mermaid_rs_renderer::{LayoutConfig, Theme, compute_layout, parse_mermaid, set_font_database};
use std::{fs, process::Command, sync::Arc, time::SystemTime};

fn embedded_database() -> Database {
    let mut database = Database::new();
    database.push_face_info(FaceInfo {
        id: ID::dummy(),
        source: Source::Binary(Arc::new(include_bytes!("fixtures/fonts/demo.ttf").to_vec())),
        index: 0,
        families: vec![("Embedded Test Sans".into(), Language::English_UnitedStates)],
        post_script_name: "EmbeddedTestSans".into(),
        style: Style::Normal,
        weight: Weight::NORMAL,
        stretch: Stretch::Normal,
        monospaced: false,
    });
    database
}

fn label_width() -> f32 {
    let graph = parse_mermaid("flowchart LR\n Node[AAAAAAAAAAAAAAAA]")
        .unwrap()
        .graph;
    let theme = Theme {
        font_family: "Embedded Test Sans".into(),
        font_size: 100.0,
        ..Theme::modern()
    };
    let config = LayoutConfig {
        fast_text_metrics: false,
        ..LayoutConfig::default()
    };
    compute_layout(&graph, &theme, &config).nodes["Node"]
        .label
        .width
}

#[test]
fn replacing_embedded_fonts_clears_hits_and_misses_without_disk_cache() {
    // Use a child process to isolate process-wide configuration and environment
    // from the test harness, including when other tests run in parallel.
    if std::env::var_os("MERMAID_FONT_TEST_CHILD").is_some() {
        set_font_database(Database::new());
        let fallback = label_width();
        set_font_database(embedded_database());
        let embedded = label_width();
        // The fixture's narrow A glyph must affect layout even after a cached
        // missing-family lookup. Allow the layout's conservative width padding.
        assert!(embedded.is_finite() && embedded > 0.0);
        assert!(embedded < fallback - 1.0, "{embedded} vs {fallback}");
        assert_eq!(label_width(), embedded);
        set_font_database(Database::new());
        assert_eq!(label_width(), fallback);
        return;
    }

    let nonce = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let cache = std::env::temp_dir().join(format!("mermaid-fonts-{}-{nonce}", std::process::id()));
    fs::create_dir(&cache).unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "replacing_embedded_fonts_clears_hits_and_misses_without_disk_cache",
            "--nocapture",
        ])
        .env("MERMAID_FONT_TEST_CHILD", "1")
        .env("XDG_CACHE_HOME", &cache)
        .output()
        .unwrap();
    let cache_is_empty = fs::read_dir(&cache).unwrap().next().is_none();
    fs::remove_dir_all(&cache).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(cache_is_empty, "embedded fonts must not write a disk cache");
}
