//! Named colour themes for the interface (Preferences › Appearance › Theme).
//! Aurora's own; the Legacy Viewer has skins of its own, whose colours two
//! of the presets here follow.
//!
//! A preset is a stable id and a full set of the skin's colours: colours
//! only, never the layout, the radii, the glass or the font scale (they stay
//! `Theme::default`'s). The chosen preset is the skin's base: the palette is
//! built from it exactly as it is from `theme.json`, so everything after it
//! is unchanged: the follow-the-system choice, the accent, background and
//! text of `crate::interface_colors`, high contrast (which ignores all of
//! them) and the colour blindness adaptation.
//!
//! A `theme.json` of one's own still wins: a preset applies only to a skin
//! whose colours are Aurora's (no `theme.json`, or one that leaves the
//! colours alone), see `resolve`.
//!
//! Where the colours come from:
//! - `aurora`: the brand palette (`Theme::default`).
//! - `legacy-dark`: the Legacy Viewer's default skin (indra/newview/skins/
//!   default/colors.xml): `DkGray` floaters (#202020), `LtGray` text
//!   (#E6E6E6) and the `EmphasisColor` orange (#F2692C) as the accent.
//! - `legacy-vintage`: the Legacy Viewer's vintage skin (indra/newview/
//!   skins/vintage/colors.xml): the mid greys (`TitleBarFocusColor`
//!   #555555, `DkGray` #3E3E3E) and the blue-grey of its selection
//!   (`UIControlBGSelected` #8D91C2) and labels (`LabelTextColor`
//!   #93A9D5), moved in lightness to read on the panel.
//! - `dragon`, `graphite`, `light`, `ocean`, `forest`, `ember`, `rose`:
//!   made up for Aurora. `dragon` is a charcoal and ember-red theme in the
//!   spirit of the owner's own viewer; it is not Black Dragon's palette
//!   (its colours could not be found).
//!
//! Every preset satisfies the readability rules of `interface_colors`
//! (`tests`).

use crate::theme::{Theme, hex};
use std::borrow::Cow;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The preset a new profile starts with: the Aurora look.
pub const DEFAULT_ID: &str = "aurora";

/// The colours of a preset, in the order of `Theme`'s fields: violet
/// (the accent), indigo, teal, navy, panel, bar, field, raised,
/// violet_light, violet_pale, ink, muted, muted_dim, amber, rose, danger,
/// success, warn, danger_dim, indigo_light, on_accent.
type Colors = [&'static str; 21];

pub struct Preset {
    /// Stored in the settings: never renamed.
    pub id: &'static str,
    colors: Colors,
}

/// The states' colours of the dark presets (amber, rose, danger, success,
/// warn, danger_dim), Aurora's.
macro_rules! dark_states {
    () => {
        ["#FCD34D", "#F472B6", "#F87171", "#4ADE80", "#FB923C", "#A85252"]
    };
}

/// Aurora's dark surfaces and text, shared by the single accent presets.
macro_rules! aurora_base {
    () => {
        ("#070B1F", "#252830", "#1B1E26", "#181B23", "#363B47")
    };
}

const fn colors(
    accent: [&'static str; 3],
    base: (&'static str, &'static str, &'static str, &'static str, &'static str),
    light_accent: [&'static str; 2],
    text: [&'static str; 3],
    states: [&'static str; 6],
    indigo_light: &'static str,
    on_accent: &'static str,
) -> Colors {
    // accent: violet, indigo, teal (the link colour)
    let (navy, panel, bar, field, raised) = base;
    [
        accent[0],
        accent[1],
        accent[2],
        navy,
        panel,
        bar,
        field,
        raised,
        light_accent[0],
        light_accent[1],
        text[0],
        text[1],
        text[2],
        states[0],
        states[1],
        states[2],
        states[3],
        states[4],
        states[5],
        indigo_light,
        on_accent,
    ]
}

const PRESETS: &[Preset] = &[
    Preset {
        id: DEFAULT_ID,
        colors: colors(
            ["#8B5CF6", "#4F46E5", "#5EEAD4"],
            aurora_base!(),
            ["#A78BFA", "#C4B5FD"],
            ["#E8EDFB", "#9AA6C6", "#6F7BA0"],
            dark_states!(),
            "#818CF8",
            "#FFFFFF",
        ),
    },
    Preset {
        id: "graphite",
        colors: colors(
            ["#7E9BBB", "#3F5A78", "#A9C7E0"],
            ("#0E0F10", "#26282B", "#1C1E20", "#181A1C", "#383B3F"),
            ["#9DB4CE", "#C3D3E4"],
            ["#ECECEC", "#A7ABB0", "#7B8087"],
            dark_states!(),
            "#8FB0D6",
            "#101418",
        ),
    },
    Preset {
        id: "legacy-dark",
        colors: colors(
            ["#F2692C", "#A8461C", "#FFC49B"],
            ("#0A0A0A", "#202020", "#171717", "#121212", "#353535"),
            ["#F58A55", "#FAB28A"],
            ["#E6E6E6", "#A8A8A8", "#7A7A7A"],
            dark_states!(),
            "#818CF8",
            "#1A1A1A",
        ),
    },
    Preset {
        id: "legacy-vintage",
        colors: colors(
            ["#9A9FD6", "#4F5496", "#B7C8F0"],
            ("#242424", "#484848", "#3A3A3A", "#2E2E2E", "#5E5E5E"),
            ["#B4B8E6", "#D2D5F4"],
            ["#F2F2F2", "#C4CCDC", "#9AA3B8"],
            ["#FCD34D", "#F9A8D4", "#FCA5A5", "#4ADE80", "#FDBA74", "#A85252"],
            "#B0B6EE",
            "#1E1E2A",
        ),
    },
    Preset {
        id: "dragon",
        colors: colors(
            ["#EE5340", "#A02A20", "#FFB4A8"],
            ("#0A0809", "#1E1A1C", "#151214", "#110E10", "#322C2F"),
            ["#F57A66", "#FAA89A"],
            ["#F1E9E6", "#B3A6A3", "#857876"],
            dark_states!(),
            "#818CF8",
            "#160D0C",
        ),
    },
    Preset {
        id: "light",
        colors: colors(
            ["#6B46D8", "#C7CCF3", "#0E7490"],
            ("#D0D5DE", "#F3F4F7", "#E4E7EC", "#FFFFFF", "#D9DDE5"),
            ["#7F5CE6", "#5B3BC4"],
            ["#1B1F2A", "#4A5365", "#6B7488"],
            ["#B45309", "#BE185D", "#B91C1C", "#15803D", "#C2410C", "#C2565B"],
            "#4338CA",
            "#FFFFFF",
        ),
    },
    Preset {
        id: "ocean",
        colors: colors(
            ["#3B9EFF", "#1D5FB5", "#67E8F9"],
            aurora_base!(),
            ["#66B3FF", "#A8D2FF"],
            ["#E8EDFB", "#9AA6C6", "#6F7BA0"],
            dark_states!(),
            "#7DB8FF",
            "#07121F",
        ),
    },
    Preset {
        id: "forest",
        colors: colors(
            ["#3FBF73", "#1F7A47", "#5EEAD4"],
            aurora_base!(),
            ["#6AD596", "#A6EBC3"],
            ["#E8EDFB", "#9AA6C6", "#6F7BA0"],
            dark_states!(),
            "#818CF8",
            "#06130B",
        ),
    },
    Preset {
        id: "ember",
        colors: colors(
            ["#F59E0B", "#8A4F08", "#FDE68A"],
            aurora_base!(),
            ["#FBB63A", "#FCD479"],
            ["#E8EDFB", "#9AA6C6", "#6F7BA0"],
            dark_states!(),
            "#818CF8",
            "#1A1100",
        ),
    },
    Preset {
        id: "rose",
        colors: colors(
            ["#EC5B9A", "#A8285F", "#F9A8D4"],
            aurora_base!(),
            ["#F37FB4", "#F9B4D3"],
            ["#E8EDFB", "#9AA6C6", "#6F7BA0"],
            dark_states!(),
            "#818CF8",
            "#1A0A12",
        ),
    },
];

/// The presets, in the order of the Preferences list.
pub fn all() -> &'static [Preset] {
    PRESETS
}

/// The preset of an id, `None` for one this build does not know.
pub fn find(id: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.id == id)
}

impl Preset {
    /// The name shown in the interface.
    pub fn name(&self) -> &'static str {
        use aurora_i18n::tr;
        match self.id {
            "graphite" => tr!("ui-theme-graphite"),
            "legacy-dark" => tr!("ui-theme-legacy-dark"),
            "legacy-vintage" => tr!("ui-theme-legacy-vintage"),
            "dragon" => tr!("ui-theme-dragon"),
            "light" => tr!("ui-theme-light"),
            "ocean" => tr!("ui-theme-ocean"),
            "forest" => tr!("ui-theme-forest"),
            "ember" => tr!("ui-theme-ember"),
            "rose" => tr!("ui-theme-rose"),
            _ => tr!("ui-theme-aurora"),
        }
    }

    /// The skin, the colours of the preset and everything else
    /// `Theme::default`'s.
    pub fn theme(&self) -> Theme {
        let [
            violet,
            indigo,
            teal,
            navy,
            panel,
            bar,
            field,
            raised,
            violet_light,
            violet_pale,
            ink,
            muted,
            muted_dim,
            amber,
            rose,
            danger,
            success,
            warn,
            danger_dim,
            indigo_light,
            on_accent,
        ] = self.colors.map(String::from);
        Theme {
            violet,
            indigo,
            teal,
            navy,
            panel,
            bar,
            field,
            raised,
            violet_light,
            violet_pale,
            ink,
            muted,
            muted_dim,
            amber,
            rose,
            danger,
            success,
            warn,
            danger_dim,
            indigo_light,
            on_accent,
            ..Theme::default()
        }
    }

    /// The three colours of the preview strip: the panel, the accent and
    /// the text.
    pub fn swatches(&self) -> [egui::Color32; 3] {
        [hex(self.colors[4]), hex(self.colors[0]), hex(self.colors[10])]
    }
}

/// The index of the chosen preset in `PRESETS`.
static CURRENT: AtomicUsize = AtomicUsize::new(0);

/// The theme from now on (the palette is built again by its users). An id
/// this build does not know keeps the Aurora look, with a line in the log.
pub fn set(id: &str) {
    let index = match PRESETS.iter().position(|p| p.id == id) {
        Some(i) => i,
        None => {
            if !id.is_empty() && id != DEFAULT_ID && id != last_unknown().as_str() {
                log::warn!("unknown interface theme '{id}': using '{DEFAULT_ID}'");
            }
            remember_unknown(id);
            0
        }
    };
    CURRENT.store(index, Ordering::Relaxed);
}

fn unknown() -> &'static std::sync::Mutex<String> {
    static UNKNOWN: std::sync::OnceLock<std::sync::Mutex<String>> = std::sync::OnceLock::new();
    UNKNOWN.get_or_init(Default::default)
}

fn last_unknown() -> String {
    unknown().lock().map(|s| s.clone()).unwrap_or_default()
}

/// So that a wrong id is logged once, not at every change of the settings.
fn remember_unknown(id: &str) {
    if let Ok(mut s) = unknown().lock() {
        id.clone_into(&mut s);
    }
}

pub fn current() -> &'static Preset {
    PRESETS.get(CURRENT.load(Ordering::Relaxed)).unwrap_or(&PRESETS[0])
}

/// `AURORA_DEMO_THEME=<id>` (demo mode only): this run's theme, never
/// saved. An unknown id is logged and the saved theme stays.
pub fn demo_override() -> Option<&'static str> {
    let raw = std::env::var("AURORA_DEMO_THEME").ok()?;
    let id = raw.trim();
    let found = find(id).map(|p| p.id);
    if found.is_none() {
        log::warn!("AURORA_DEMO_THEME: unknown theme '{id}', keeping the saved one");
    }
    found
}

/// The colours of the skin as it is read: the chosen preset's when the
/// skin's own are Aurora's, else the skin's (a `theme.json` of one's own
/// wins over the preset).
pub fn resolve_with<'a>(preset: &Preset, skin: &'a Theme) -> Cow<'a, Theme> {
    if preset.id == DEFAULT_ID || !has_default_colors(skin) {
        return Cow::Borrowed(skin);
    }
    let mut themed = preset.theme();
    themed.panel_alpha = skin.panel_alpha;
    themed.button_radius = skin.button_radius;
    themed.window_radius = skin.window_radius;
    themed.font_scale = skin.font_scale;
    Cow::Owned(themed)
}

fn has_default_colors(skin: &Theme) -> bool {
    let default = Theme::default();
    color_fields(skin)
        .iter()
        .zip(color_fields(&default))
        .all(|(a, b)| a.eq_ignore_ascii_case(b))
}

/// A skin's colours, in the order of `Colors`.
fn color_fields(t: &Theme) -> [&str; 21] {
    [
        &t.violet,
        &t.indigo,
        &t.teal,
        &t.navy,
        &t.panel,
        &t.bar,
        &t.field,
        &t.raised,
        &t.violet_light,
        &t.violet_pale,
        &t.ink,
        &t.muted,
        &t.muted_dim,
        &t.amber,
        &t.rose,
        &t.danger,
        &t.success,
        &t.warn,
        &t.danger_dim,
        &t.indigo_light,
        &t.on_accent,
    ]
}

#[cfg(test)]
mod tests;
