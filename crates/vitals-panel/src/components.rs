//! Shared painting helpers: surfaces, logo, charts, chips and app icons.

use std::path::Path;

use vitals_core::{file_stem, initials, now_ms, PointF, RectF, ProcessState};
use vitals_ui::{
    app_icon, Brush, Color, Ctx, Interpolation, Painter, PathBuilder, StrokeStyle, TextStyle, Theme, Weight,
};

pub const PURPLE: Color = Color::rgb8(0xBF, 0x5A, 0xF2);
pub const ORANGE: Color = Color::rgb8(0xFF, 0x9F, 0x0A);
pub const TEAL: Color = Color::rgb8(0x64, 0xD2, 0xFF);

const AVATAR_COLORS: [Color; 8] = [
    Color::rgb8(0x0A, 0x84, 0xFF),
    Color::rgb8(0x5E, 0x5C, 0xE6),
    Color::rgb8(0xBF, 0x5A, 0xF2),
    Color::rgb8(0xFF, 0x37, 0x5F),
    Color::rgb8(0xFF, 0x9F, 0x0A),
    Color::rgb8(0x30, 0xD1, 0x58),
    Color::rgb8(0x64, 0xD2, 0xFF),
    Color::rgb8(0x8E, 0x8E, 0x93),
];

/// Normal window background. Mica shows through on screen; previews get the opaque palette colour.
/// Soft aurora blooms give the flat window depth without competing with the content.
pub fn paint_window_background(cx: &Ctx, p: &mut Painter, theme: &Theme) {
    let bounds = p.bounds();
    if cx.is_offscreen() {
        p.fill_rect(bounds, theme.window_background);
    }
    let (tint, top, blue, violet, teal, warm) = if theme.is_dark() {
        (
            Color::rgba(0.015, 0.020, 0.045, 0.30),
            Color::rgba(0.010, 0.016, 0.040, 0.30),
            0.135,
            0.075,
            0.050,
            0.038,
        )
    } else {
        (
            Color::rgba(1.0, 1.0, 1.0, 0.22),
            Color::rgba(1.0, 1.0, 1.0, 0.34),
            0.085,
            0.052,
            0.038,
            0.028,
        )
    };
    p.fill_rect(bounds, tint);
    p.fill_rect(bounds, Brush::vertical(bounds, top, Color::TRANSPARENT));
    p.fill_rect(
        bounds,
        Brush::glow(
            PointF::new(bounds.w * 0.90, -bounds.h * 0.06),
            bounds.w * 0.72,
            theme.accent.multiply_alpha(blue),
        ),
    );
    p.fill_rect(
        bounds,
        Brush::glow(
            PointF::new(bounds.w * 0.02, bounds.h * 0.38),
            bounds.w * 0.56,
            PURPLE.multiply_alpha(violet),
        ),
    );
    p.fill_rect(
        bounds,
        Brush::glow(
            PointF::new(bounds.w * 0.52, bounds.h * 1.04),
            bounds.w * 0.62,
            TEAL.multiply_alpha(teal),
        ),
    );
    p.fill_rect(
        bounds,
        Brush::glow(
            PointF::new(bounds.w * 0.98, bounds.h * 0.92),
            bounds.w * 0.44,
            Color::rgb8(0xFF, 0x7A, 0x59).multiply_alpha(warm),
        ),
    );
}

/// A slightly raised card used for summaries and charts.
pub fn card(p: &mut Painter, rect: RectF, radius: f32, raised: bool) {
    let theme = p.theme().clone();
    let rect = p.snap_rect(rect);
    if raised {
        p.shadow(rect, radius, &theme.shadows[1].scaled_alpha(0.7));
    }
    let (top, bottom) = if theme.is_dark() {
        (Color::rgba(1.0, 1.0, 1.0, 0.085), Color::rgba(1.0, 1.0, 1.0, 0.032))
    } else {
        (Color::rgba(1.0, 1.0, 1.0, 0.78), Color::rgba(1.0, 1.0, 1.0, 0.56))
    };
    p.fill_round_rect(rect, radius, Brush::vertical(rect, top, bottom));
    p.hairline_round_rect(rect, radius, theme.hairline, true);
    // A short specular line across the flat top edge catches the light like glass.
    let glare = if theme.is_dark() { 0.13 } else { 0.55 };
    let line = RectF::new(rect.x + radius * 0.7, rect.y + 0.5, (rect.w - radius * 1.4).max(0.0), 1.0);
    p.fill_rect(
        line,
        Brush::linear(
            PointF::new(line.x, line.y),
            PointF::new(line.right(), line.y),
            &[(0.0, Color::TRANSPARENT), (0.5, Color::rgba(1.0, 1.0, 1.0, glare)), (1.0, Color::TRANSPARENT)],
        ),
    );
}

/// A card warmed by one of the accent colours: a soft corner bloom behind the content.
pub fn card_tinted(p: &mut Painter, rect: RectF, radius: f32, color: Color, strength: f32) {
    let theme = p.theme().clone();
    let snapped = p.snap_rect(rect);
    card(p, rect, radius, false);
    p.clip_round_rect(snapped, radius, |p| {
        p.fill_rect(
            snapped,
            Brush::glow(
                PointF::new(snapped.right() - snapped.w * 0.10, snapped.y + snapped.h * 0.04),
                snapped.w * 0.62,
                color.multiply_alpha(if theme.is_dark() { strength } else { strength * 0.72 }),
            ),
        );
        p.fill_rect(
            snapped,
            Brush::linear(
                PointF::new(snapped.x, snapped.y),
                PointF::new(snapped.right(), snapped.bottom()),
                &[(0.0, color.multiply_alpha(if theme.is_dark() { 0.045 } else { 0.030 })), (0.65, Color::TRANSPARENT)],
            ),
        );
    });
    p.hairline_round_rect(snapped, radius, color.multiply_alpha(if theme.is_dark() { 0.20 } else { 0.16 }), true);
}

/// A small raised key cap, e.g. the three keys in the quick-view hint.
pub fn keycap(p: &mut Painter, rect: RectF, label: &str) {
    let theme = p.theme().clone();
    let rect = p.snap_rect(rect);
    let (fill, base, edge) = if theme.is_dark() {
        (Color::rgba(1.0, 1.0, 1.0, 0.11), Color::rgba(0.0, 0.0, 0.0, 0.35), Color::rgba(1.0, 1.0, 1.0, 0.13))
    } else {
        (Color::WHITE, Color::rgba(0.0, 0.0, 0.0, 0.13), Color::rgba(0.0, 0.0, 0.0, 0.08))
    };
    let radius = (rect.h * 0.24).clamp(4.0, 7.0);
    p.fill_round_rect(rect.offset(0.0, 1.0), radius, base);
    p.fill_round_rect(rect, radius, fill);
    p.hairline_round_rect(rect, radius, edge, true);
    p.text(
        label,
        &TextStyle::new((rect.h * 0.35).clamp(8.0, 11.5)).weight(Weight::Medium).tabular().centered(),
        theme.text_secondary,
        rect,
    );
}

/// The Vitals mark: a blue-violet rounded square with a heartbeat line.
pub fn logo(p: &mut Painter, center: PointF, size: f32) {
    let rect = RectF::new(center.x - size / 2.0, center.y - size / 2.0, size, size);
    let radius = size * 0.29;
    let colors = (Color::rgb8(0x0A, 0x84, 0xFF), Color::rgb8(0x7B, 0x5C, 0xF6));
    p.shadow(rect, radius, &vitals_ui::Shadow::new(size * 0.10, size * 0.26, Color::rgba(0.08, 0.18, 0.65, 0.36)));
    p.fill_round_rect(
        rect,
        radius,
        Brush::linear(
            PointF::new(rect.x, rect.y),
            PointF::new(rect.right(), rect.bottom()),
            &[(0.0, colors.0), (1.0, colors.1)],
        ),
    );
    p.hairline_round_rect(rect, radius, Color::rgba(1.0, 1.0, 1.0, 0.24), true);
    let unit = size / 28.0;
    let points = [
        PointF::new(center.x - 8.4 * unit, center.y + 0.4 * unit),
        PointF::new(center.x - 3.6 * unit, center.y + 0.4 * unit),
        PointF::new(center.x - 1.7 * unit, center.y - 5.2 * unit),
        PointF::new(center.x + 1.5 * unit, center.y + 5.7 * unit),
        PointF::new(center.x + 3.6 * unit, center.y + 0.4 * unit),
        PointF::new(center.x + 8.4 * unit, center.y + 0.4 * unit),
    ];
    p.polyline(&points, Color::rgba(1.0, 1.0, 1.0, 0.96), 1.85 * unit, &StrokeStyle::round(), false);
}

/// Filled history graph with a matching line. `max` is the top of the scale.
pub fn sparkline(p: &mut Painter, rect: RectF, values: &[f32], max: f32, color: Color) {
    if values.len() < 2 || rect.w <= 2.0 || rect.h <= 2.0 {
        return;
    }
    let max = max.max(0.001);
    let step = rect.w / (values.len() - 1) as f32;
    let point = |index: usize, value: f32| {
        PointF::new(rect.x + index as f32 * step, rect.bottom() - (value / max).clamp(0.0, 1.0) * rect.h)
    };
    let points: Vec<PointF> = values.iter().enumerate().map(|(i, &v)| point(i, v)).collect();
    let mut area = PathBuilder::new();
    area.move_to(PointF::new(rect.x, rect.bottom()));
    for point in &points {
        area.line_to(*point);
    }
    area.line_to(PointF::new(rect.right(), rect.bottom()));
    area.close();
    if let Ok(path) = area.build(p.gfx()) {
        p.fill_path(&path, Brush::vertical(rect, color.with_alpha(0.26), color.with_alpha(0.0)));
    }
    p.polyline(&points, color, 1.6, &StrokeStyle::round(), false);
    if let Some(last) = points.last() {
        p.fill_ellipse(*last, 5.0, 5.0, color.multiply_alpha(0.16));
        p.fill_ellipse(*last, 1.8, 1.8, color);
    }
}

/// Vertical bars for a small distribution (top processes).
pub fn mini_bars(p: &mut Painter, rect: RectF, values: &[f32], color: Color) {
    if values.is_empty() || rect.w <= 2.0 || rect.h <= 2.0 {
        return;
    }
    let max = values.iter().copied().fold(0.0f32, f32::max).max(1.0);
    let gap = 4.0;
    let width = ((rect.w - gap * (values.len() as f32 - 1.0)) / values.len() as f32).max(2.0);
    for (index, value) in values.iter().enumerate() {
        let height = (value / max).clamp(0.06, 1.0) * rect.h;
        let bar = RectF::new(rect.x + index as f32 * (width + gap), rect.bottom() - height, width, height);
        p.fill_round_rect(bar, width / 2.0, color.multiply_alpha(0.42 + 0.58 * value / max));
    }
}

/// Rounded progress rail.
pub fn progress(p: &mut Painter, rect: RectF, fraction: f32, color: Color) {
    let radius = rect.h / 2.0;
    p.fill_round_rect(rect, radius, p.theme().control_track);
    let filled = RectF::new(rect.x, rect.y, rect.w * fraction.clamp(0.0, 1.0), rect.h);
    if filled.w > 0.5 {
        p.fill_round_rect(
            filled,
            radius,
            Brush::linear(
                PointF::new(filled.x, filled.y),
                PointF::new(filled.x, filled.bottom()),
                &[(0.0, color.lerp(&Color::WHITE.with_alpha(color.a), 0.18)), (1.0, color)],
            ),
        );
    }
}

/// Small icon tile used in summary cards and settings.
pub fn icon_tile(p: &mut Painter, rect: RectF, icon: vitals_ui::Icon, color: Color) {
    let rect = p.snap_rect(rect);
    let radius = rect.h * 0.30;
    p.fill_round_rect(rect, radius, color.multiply_alpha(0.15));
    p.clip_round_rect(rect, radius, |p| {
        p.fill_rect(rect, Brush::glow(rect.center(), rect.h * 0.9, color.multiply_alpha(0.16)));
    });
    p.hairline_round_rect(rect, radius, color.multiply_alpha(0.22), true);
    p.icon(icon, rect.center(), rect.h * 0.56, color);
}

/// Capsule label, e.g. `Suspended`, `On`, `All users`.
pub fn badge(p: &mut Painter, rect: RectF, text: &str, color: Color) {
    let style = TextStyle::caption().weight(Weight::Medium).centered();
    p.fill_round_rect(rect, rect.h / 2.0, color.multiply_alpha(0.14));
    p.hairline_round_rect(rect, rect.h / 2.0, color.multiply_alpha(0.24), true);
    p.text(text, &style, color, rect);
}

pub fn state_color(theme: &Theme, state: ProcessState) -> Color {
    match state {
        ProcessState::Running => theme.success,
        ProcessState::Suspended => ORANGE,
        ProcessState::NotResponding => theme.destructive,
    }
}

/// A real shell icon when the executable exists, otherwise a deterministic coloured monogram.
pub fn app_avatar(p: &mut Painter, rect: RectF, path: Option<&Path>, name: &str) {
    if let Some(path) = path
        && path.is_file()
        && let Some(icon) = app_icon::shell_icon(path, 32)
    {
        p.bitmap(&icon, rect, None, 1.0, Interpolation::Cubic);
        return;
    }
    let color = avatar_color(name);
    let lighter = color.lerp(&Color::WHITE.with_alpha(color.a), 0.16);
    let darker = color.lerp(&Color::BLACK.with_alpha(color.a), 0.12);
    p.fill_round_rect(
        rect,
        rect.h * 0.26,
        Brush::linear(PointF::new(rect.x, rect.y), PointF::new(rect.right(), rect.bottom()), &[(0.0, lighter), (1.0, darker)]),
    );
    p.hairline_round_rect(rect, rect.h * 0.26, Color::rgba(1.0, 1.0, 1.0, 0.22), true);
    let text = initials(name);
    let style = TextStyle::new((rect.h * 0.36).max(8.0)).weight(Weight::Semibold).centered();
    p.text(&text, &style, Color::WHITE, rect);
}

fn avatar_color(name: &str) -> Color {
    let hash = name.bytes().fold(0u32, |hash, byte| hash.wrapping_mul(31).wrapping_add(byte as u32));
    AVATAR_COLORS[hash as usize % AVATAR_COLORS.len()]
}

pub fn process_title(name: &str, display: &str) -> String {
    if display.trim().is_empty() { file_stem(name) } else { display.to_string() }
}

pub fn updated_label(captured_ms: i64) -> String {
    let elapsed = (now_ms() - captured_ms).max(0) as f64 / 1000.0;
    if elapsed < 1.5 {
        "Updated just now".into()
    } else if elapsed < 60.0 {
        format!("Updated {elapsed:.0}s ago")
    } else {
        format!("Updated {:.0}m ago", elapsed / 60.0)
    }
}
