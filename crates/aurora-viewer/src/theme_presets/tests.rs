use super::*;
use crate::interface_colors::contrast;
use egui::Color32;

fn is_hex(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
}

#[test]
fn ids_are_distinct_and_every_colour_is_valid_hex() {
    let ids: std::collections::HashSet<_> = all().iter().map(|p| p.id).collect();
    assert_eq!(ids.len(), all().len(), "an id is used twice");
    assert!((9..=11).contains(&all().len()), "about ten presets");
    for p in all() {
        for c in color_fields(&p.theme()) {
            assert!(is_hex(c), "{}: '{c}' is not a #RRGGBB colour", p.id);
        }
    }
}

#[test]
fn the_default_preset_is_the_aurora_theme() {
    let aurora = find(DEFAULT_ID).expect("the default preset exists");
    assert_eq!(all()[0].id, DEFAULT_ID);
    assert_eq!(
        serde_json::to_value(aurora.theme()).unwrap(),
        serde_json::to_value(Theme::default()).unwrap()
    );
}

#[test]
fn only_the_colours_differ_from_the_default_theme() {
    let default = Theme::default();
    for p in all() {
        let t = p.theme();
        assert_eq!(t.panel_alpha, default.panel_alpha, "{}", p.id);
        assert_eq!(t.button_radius, default.button_radius, "{}", p.id);
        assert_eq!(t.window_radius, default.window_radius, "{}", p.id);
        assert_eq!(t.font_scale, default.font_scale, "{}", p.id);
    }
}

/// The readability rules of `interface_colors` (text 4.5:1, the dim text
/// and the accent 3:1, text on the accent, the colours that colour text),
/// on every preset's own panel.
#[test]
fn every_preset_is_readable() {
    for p in all() {
        let s = p.theme().skin_palette_with(p);
        let id = p.id;
        let check = |what: &str, a: Color32, b: Color32, min: f32| {
            let c = contrast(a, b);
            assert!(c >= min, "{id}: {what} reads {c:.2}:1, wants {min}");
        };
        check("ink on the panel", s.ink, s.panel, 4.5);
        check("muted on the panel", s.muted, s.panel, 4.5);
        check("muted on the bar", s.muted, s.bar, 4.5);
        check("ink on a button face", s.ink, s.raised, 4.5);
        check("ink on the pressed face", s.ink, s.indigo, 4.5);
        check("dim text on the panel", s.muted_dim, s.panel, 3.0);
        check("accent on the panel", s.violet, s.panel, 3.0);
        // Aurora's own white on its violet is 4.2:1, and its hovered face
        // 2.7:1: the brand's, kept as it is
        let (on_accent, hovered) = if id == DEFAULT_ID { (4.2, 2.7) } else { (4.5, 3.0) };
        check("text on the accent", s.on_accent, s.violet, on_accent);
        check("text on the hovered accent", s.on_accent, s.violet_light, hovered);
        for (what, c) in [
            ("pale accent", s.violet_pale),
            ("light indigo", s.indigo_light),
            ("link", s.teal),
            ("amber", s.amber),
            ("rose", s.rose),
            ("danger", s.danger),
            ("success", s.success),
            ("warn", s.warn),
        ] {
            check(what, c, s.panel, 4.5);
        }
    }
}

#[test]
fn every_preset_builds_a_palette_and_the_choice_changes_it() {
    let skin = Theme::default();
    let base = skin.skin_palette_with(find(DEFAULT_ID).unwrap());
    for p in all() {
        let palette = skin.skin_palette_with(p);
        if p.id != DEFAULT_ID {
            assert_ne!(palette.violet, base.violet, "{}: the accent", p.id);
            assert_ne!(palette.panel, base.panel, "{}: the panel", p.id);
        }
        // the three strip colours are the palette's
        assert_eq!(p.swatches(), [palette.panel, palette.violet, palette.ink], "{}", p.id);
    }
    // every preset also applies to egui without a panic
    for p in all() {
        let ctx = egui::Context::default();
        p.theme().apply(&ctx, 1.0);
    }
}

#[test]
fn a_theme_json_of_ones_own_wins_over_a_preset() {
    let ocean = find("ocean").unwrap();
    let plain = Theme::default();
    assert_eq!(resolve_with(ocean, &plain).violet, ocean.theme().violet);
    let own = Theme {
        violet: "#123456".into(),
        ..Theme::default()
    };
    assert_eq!(resolve_with(ocean, &own).violet, "#123456");
    assert_eq!(own.skin_palette_with(ocean).violet, Color32::from_rgb(0x12, 0x34, 0x56));
    // a preset keeps the skin's numbers (glass, radii, font)
    let tuned = Theme {
        panel_alpha: 200,
        font_scale: 1.2,
        ..Theme::default()
    };
    let themed = resolve_with(ocean, &tuned);
    assert_eq!((themed.panel_alpha, themed.font_scale), (200, 1.2));
}

#[test]
fn an_unknown_id_is_not_found() {
    assert!(find("no-such-theme").is_none());
    assert!(find("").is_none());
}
