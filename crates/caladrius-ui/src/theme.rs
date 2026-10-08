//! Theme tokens (golden rule 7): colours, radii, spacing and font sizes are data, read from
//! `theme.json` (light and dark). No colour, radius or spacing is written anywhere else in the UI
//! code; the code asks the tokens by name.

use egui::{Color32, CornerRadius, FontFamily, FontId, Margin, Stroke, TextStyle};
use serde::de::{self, Deserializer};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

/// The token file, embedded in the binary.
const TOKEN_FILE: &str = include_str!("theme.json");

/// An RGBA colour written as `#rrggbb` or `#rrggbbaa`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgba(pub [u8; 4]);

impl Rgba {
    /// The colour for egui.
    pub fn color(self) -> Color32 {
        let [r, g, b, a] = self.0;
        Color32::from_rgba_unmultiplied(r, g, b, a)
    }

    fn parse(text: &str) -> Option<Rgba> {
        let hex = text.strip_prefix('#')?;
        if !(hex.len() == 6 || hex.len() == 8) || !hex.is_ascii() {
            return None;
        }
        let byte = |i: usize| {
            hex.get(i..i + 2)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        };
        Some(Rgba([
            byte(0)?,
            byte(2)?,
            byte(4)?,
            if hex.len() == 8 { byte(6)? } else { 255 },
        ]))
    }
}

impl Serialize for Rgba {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let [r, g, b, a] = self.0;
        serializer.serialize_str(&format!("#{r:02x}{g:02x}{b:02x}{a:02x}"))
    }
}

impl<'de> Deserialize<'de> for Rgba {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Rgba::parse(&text)
            .ok_or_else(|| de::Error::custom(format!("`{text}` is not a colour like #1a2b3c")))
    }
}

/// Every colour of the interface.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Colors {
    pub background: Rgba,
    pub panel: Rgba,
    pub panel_alt: Rgba,
    pub text: Rgba,
    pub text_muted: Rgba,
    pub accent: Rgba,
    pub accent_text: Rgba,
    pub border: Rgba,
    pub hover: Rgba,
    pub selected: Rgba,
    pub ok: Rgba,
    pub warning: Rgba,
    pub error: Rgba,
    pub stale: Rgba,
    pub stale_background: Rgba,
    pub plot_background: Rgba,
    pub plot_grid: Rgba,
    pub series_observed: Rgba,
    pub series_selected: Rgba,
    pub series_fit: Rgba,
    pub series_replaced: Rgba,
    pub series_other: Rgba,
}

/// Corner radii.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Radius {
    pub small: f32,
    pub medium: f32,
    pub large: f32,
}

/// Distances.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spacing {
    pub small: f32,
    pub medium: f32,
    pub large: f32,
    pub xlarge: f32,
    pub panel_margin: f32,
    pub tree_indent: f32,
}

/// Sizes of panels, cells and plot parts, in points.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sizes {
    pub tree_width: f32,
    pub tree_min_width: f32,
    pub tree_max_width: f32,
    pub plot_panel_width: f32,
    pub plot_panel_min_width: f32,
    pub cell_width: f32,
    pub row_height: f32,
    pub row_number_width: f32,
    pub text_box_width: f32,
    pub list_max_height: f32,
    pub plot_min_height: f32,
    /// What the plot panel keeps for what is under the plot (the note, the candidate table).
    pub plot_reserved_height: f32,
    pub marker_small: f32,
    pub marker_medium: f32,
    pub marker_large: f32,
    /// How near a click must be to a point to select it.
    pub hit_radius: f32,
}

/// Line widths.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Strokes {
    pub thin: f32,
    pub medium: f32,
    pub thick: f32,
}

/// Text sizes in points.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fonts {
    pub small: f32,
    pub body: f32,
    pub heading: f32,
    pub title: f32,
    pub display: f32,
    pub monospace: f32,
}

/// The tokens of one theme.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tokens {
    pub colors: Colors,
    pub radius: Radius,
    pub spacing: Spacing,
    pub size: Sizes,
    pub stroke: Strokes,
    pub font: Fonts,
}

/// Light or dark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    /// Dark text on a light background.
    #[default]
    Light,
    /// Light text on a dark background.
    Dark,
}

impl ThemeMode {
    /// The other mode.
    pub fn toggled(self) -> ThemeMode {
        match self {
            ThemeMode::Light => ThemeMode::Dark,
            ThemeMode::Dark => ThemeMode::Light,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TokenFile {
    #[allow(dead_code)]
    version: u32,
    light: Tokens,
    dark: Tokens,
}

impl Tokens {
    /// The tokens of `mode` from the embedded file.
    pub fn embedded(mode: ThemeMode) -> Result<Tokens, String> {
        Tokens::from_json(TOKEN_FILE, mode)
    }

    /// The tokens of `mode` from a token file's text.
    pub fn from_json(text: &str, mode: ThemeMode) -> Result<Tokens, String> {
        let file: TokenFile =
            serde_json::from_str(text).map_err(|e| format!("the theme file is invalid: {e}"))?;
        Ok(match mode {
            ThemeMode::Light => file.light,
            ThemeMode::Dark => file.dark,
        })
    }

    /// Sets egui's visuals, spacing and text sizes from the tokens.
    pub fn apply(&self, ctx: &egui::Context, mode: ThemeMode) {
        let c = &self.colors;
        let mut style = (*ctx.style()).clone();
        let mut visuals = match mode {
            ThemeMode::Light => egui::Visuals::light(),
            ThemeMode::Dark => egui::Visuals::dark(),
        };
        visuals.override_text_color = Some(c.text.color());
        visuals.panel_fill = c.panel.color();
        visuals.window_fill = c.panel.color();
        visuals.extreme_bg_color = c.plot_background.color();
        visuals.faint_bg_color = c.panel_alt.color();
        visuals.code_bg_color = c.panel_alt.color();
        visuals.hyperlink_color = c.accent.color();
        visuals.warn_fg_color = c.warning.color();
        visuals.error_fg_color = c.error.color();
        visuals.selection.bg_fill = c.selected.color();
        visuals.selection.stroke = Stroke::new(1.0_f32, c.accent.color());
        let radius = CornerRadius::same(self.radius.medium.round() as u8);
        visuals.window_corner_radius = CornerRadius::same(self.radius.large.round() as u8);
        visuals.menu_corner_radius = radius;
        let border = Stroke::new(1.0_f32, c.border.color());
        for widget in [
            &mut visuals.widgets.noninteractive,
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
            &mut visuals.widgets.open,
        ] {
            widget.corner_radius = radius;
            widget.fg_stroke.color = c.text.color();
        }
        visuals.widgets.noninteractive.bg_stroke = border;
        visuals.widgets.noninteractive.bg_fill = c.panel.color();
        visuals.widgets.inactive.bg_fill = c.panel_alt.color();
        visuals.widgets.inactive.weak_bg_fill = c.panel_alt.color();
        visuals.widgets.inactive.bg_stroke = border;
        visuals.widgets.hovered.bg_fill = c.hover.color();
        visuals.widgets.hovered.weak_bg_fill = c.hover.color();
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, c.accent.color());
        visuals.widgets.active.bg_fill = c.hover.color();
        visuals.widgets.active.weak_bg_fill = c.hover.color();
        // Strong text takes the colour of an active widget: keep it the text colour.
        visuals.widgets.active.fg_stroke.color = c.text.color();
        visuals.widgets.open.bg_fill = c.hover.color();
        style.visuals = visuals;
        style.spacing.item_spacing = egui::vec2(self.spacing.medium, self.spacing.small + 2.0);
        style.spacing.button_padding = egui::vec2(self.spacing.medium, self.spacing.small);
        style.spacing.indent = self.spacing.tree_indent;
        style.text_styles.insert(
            TextStyle::Body,
            FontId::new(self.font.body, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Button,
            FontId::new(self.font.body, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Small,
            FontId::new(self.font.small, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Heading,
            FontId::new(self.font.heading, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Monospace,
            FontId::new(self.font.monospace, FontFamily::Monospace),
        );
        ctx.set_style(style);
    }

    /// The frame of a panel body: the token margin around a flat panel colour.
    pub fn panel_frame(&self, fill: Rgba) -> egui::Frame {
        egui::Frame::new()
            .fill(fill.color())
            .inner_margin(Margin::same(self.spacing.panel_margin.round() as i8))
    }

    /// The main action of a screen: accent fill, accent text.
    pub fn primary_button(&self, text: &str) -> egui::Button<'static> {
        egui::Button::new(
            egui::RichText::new(text)
                .strong()
                .color(self.colors.accent_text.color()),
        )
        .fill(self.colors.accent.color())
    }

    /// A rounded, bordered box (a card): panel colour, border, medium radius.
    pub fn card_frame(&self) -> egui::Frame {
        egui::Frame::new()
            .fill(self.colors.panel.color())
            .stroke(Stroke::new(1.0_f32, self.colors.border.color()))
            .corner_radius(CornerRadius::same(self.radius.medium.round() as u8))
            .inner_margin(Margin::same(self.spacing.medium.round() as i8))
    }

    /// A banner box with its own fill and text colour (the stale notice).
    pub fn banner_frame(&self, fill: Rgba, edge: Rgba) -> egui::Frame {
        egui::Frame::new()
            .fill(fill.color())
            .stroke(Stroke::new(1.0_f32, edge.color()))
            .corner_radius(CornerRadius::same(self.radius.medium.round() as u8))
            .inner_margin(Margin::same(self.spacing.medium.round() as i8))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_file_holds_both_themes() {
        let light = Tokens::embedded(ThemeMode::Light).unwrap();
        let dark = Tokens::embedded(ThemeMode::Dark).unwrap();
        assert_ne!(light.colors.background, dark.colors.background);
        assert_ne!(light.colors.text, dark.colors.text);
        assert!(light.font.body > 0.0 && light.spacing.medium > 0.0 && light.radius.medium > 0.0);
        assert!(
            light.size.cell_width > 0.0 && light.size.row_height > 0.0 && light.stroke.thin > 0.0
        );
        assert!(
            light.size.tree_min_width < light.size.tree_width
                && light.size.tree_width < light.size.tree_max_width
        );
        // Every colour is set (not the all-zero default) in both themes.
        for tokens in [&light, &dark] {
            let json = serde_json::to_value(&tokens.colors).unwrap();
            for (name, value) in json.as_object().unwrap() {
                assert_ne!(value.as_str(), Some("#00000000"), "{name}");
            }
        }
    }

    #[test]
    fn colours_parse_and_round_trip() {
        assert_eq!(Rgba::parse("#1a2b3c"), Some(Rgba([0x1a, 0x2b, 0x3c, 255])));
        assert_eq!(
            Rgba::parse("#1a2b3c80"),
            Some(Rgba([0x1a, 0x2b, 0x3c, 0x80]))
        );
        for bad in ["1a2b3c", "#12", "#gg0000", "#1a2b3c8", "#é2b3c", ""] {
            assert_eq!(Rgba::parse(bad), None, "{bad}");
        }
        let tokens = Tokens::embedded(ThemeMode::Dark).unwrap();
        let text = serde_json::to_string(&tokens).unwrap();
        let back: Tokens = serde_json::from_str(&text).unwrap();
        assert_eq!(back, tokens);
    }

    #[test]
    fn a_bad_theme_file_says_what_is_wrong() {
        let e = Tokens::from_json("{\"version\":1}", ThemeMode::Light).unwrap_err();
        assert!(e.contains("theme file is invalid"), "{e}");
        let e = Tokens::from_json("nonsense", ThemeMode::Light).unwrap_err();
        assert!(e.contains("invalid"), "{e}");
    }

    #[test]
    fn text_has_enough_contrast_on_its_backgrounds() {
        // WCAG relative luminance contrast, at least 4.5 for body text on panel and background.
        fn luminance(c: Rgba) -> f64 {
            let f = |v: u8| {
                let s = f64::from(v) / 255.0;
                if s <= 0.03928 {
                    s / 12.92
                } else {
                    ((s + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * f(c.0[0]) + 0.7152 * f(c.0[1]) + 0.0722 * f(c.0[2])
        }
        fn contrast(a: Rgba, b: Rgba) -> f64 {
            let (x, y) = (luminance(a), luminance(b));
            (x.max(y) + 0.05) / (x.min(y) + 0.05)
        }
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            let t = Tokens::embedded(mode).unwrap();
            let c = &t.colors;
            for (fg, bg, name) in [
                (c.text, c.background, "text on background"),
                (c.text, c.panel, "text on panel"),
                (c.text_muted, c.panel, "muted text on panel"),
                (c.accent_text, c.accent, "accent text on accent"),
                (c.text, c.selected, "text on a selected item"),
                (
                    c.stale,
                    c.stale_background,
                    "stale text on stale background",
                ),
                (c.error, c.panel, "error on panel"),
                (c.warning, c.panel, "warning on panel"),
                (c.ok, c.panel, "ok on panel"),
            ] {
                let ratio = contrast(fg, bg);
                assert!(ratio >= 4.5, "{mode:?} {name}: {ratio:.2}");
            }
        }
    }
}
