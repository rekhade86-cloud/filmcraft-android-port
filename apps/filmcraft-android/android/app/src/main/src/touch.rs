//! Touch ergonomics: finger-sized controls and a phone workspace.

use filmcraft_ui_egui::dock::{DockNode, PanelKind, SplitSize};
use std::sync::Arc;

/// Re-applied after every pass, so a theme change (which rewrites spacing and fonts) keeps the
/// touch sizes. Cheap: it only sets a handful of fields.
pub fn install(ctx: &egui::Context) {
    apply(ctx);
    ctx.on_end_pass("android-touch-style", Arc::new(|ui| apply(ui.ctx())));
}

fn apply(ctx: &egui::Context) {
    use egui::{FontFamily, FontId, TextStyle, vec2};
    ctx.global_style_mut(|s| {
        // egui points are dp on Android, so 40 is a comfortable fingertip target (Material: 48).
        s.spacing.interact_size = vec2(40.0, 40.0);
        s.spacing.button_padding = vec2(12.0, 8.0);
        s.spacing.item_spacing = vec2(10.0, 8.0);
        s.spacing.icon_width = 22.0;
        s.spacing.scroll.bar_width = 14.0;
        s.interaction.resize_grab_radius_side = 14.0;
        s.interaction.interact_radius = 12.0;
        s.text_styles.insert(TextStyle::Body, FontId::new(15.0, FontFamily::Proportional));
        s.text_styles.insert(TextStyle::Button, FontId::new(15.0, FontFamily::Proportional));
        s.text_styles.insert(TextStyle::Small, FontId::new(13.0, FontFamily::Proportional));
        s.text_styles.insert(TextStyle::Heading, FontId::new(18.0, FontFamily::Name("semibold".into())));
        s.text_styles.insert(TextStyle::Monospace, FontId::new(14.0, FontFamily::Monospace));
    });
}

/// Landscape phone workspace: Program monitor beside the working panels on top, Timeline with a
/// slim tool strip underneath. Everything else stays one tab away.
pub fn phone_layout() -> DockNode {
    use PanelKind::*;
    use SplitSize::*;
    let tabs = |p: &[PanelKind]| DockNode::Tabs { panels: p.to_vec(), active: 0 };
    let split = |vertical: bool, size: SplitSize, a: DockNode, b: DockNode| DockNode::Split { vertical, size, a: Box::new(a), b: Box::new(b) };
    split(
        true,
        Ratio(0.48),
        split(
            false,
            Ratio(0.55),
            tabs(&[Program, Source]),
            tabs(&[Project, MediaBrowser, EffectControls, Effects, Properties, EssentialGraphics, Text, LumetriColor, AudioTrackMixer, Info, Markers, History]),
        ),
        split(false, FixedA(56.0), tabs(&[Tools]), tabs(&[Timeline])),
    )
}
