//! Deterministic offscreen previews of every surface, used instead of screenshots while developing.

use std::{fs, path::Path};

use anyhow::{Context, Result};
use vitals_core::{SizeF, ThemeMode};
use vitals_panel::{DashboardView, QuickView, SettingsView, Tab};
use vitals_panel::demo::{demo_settings, demo_snapshot, demo_startup};
use vitals_ui::{Brush, Color, Gfx, OffscreenSpec, Painter, PointF, Theme, View, render_offscreen, render_view_offscreen};

use crate::image_png::write_png;

fn preview_scale() -> f32 {
    1.25
}

/// A smooth, abstract desktop behind the quick panel. It stands in for the user's wallpaper so the preview shows the
/// same translucency the real acrylic window gets from the desktop.
fn paint_wallpaper(p: &mut Painter, theme: &Theme) {
    let b = p.bounds();
    let (base, top, blue, violet, warm, teal, sheen) = if theme.is_dark() {
        (
            Color::rgb8(0x0A, 0x10, 0x20),
            Color::rgb8(0x11, 0x1B, 0x37),
            Color::rgb8(0x2F, 0x6B, 0xFF),
            Color::rgb8(0x8B, 0x5C, 0xF6),
            Color::rgb8(0xFF, 0x8A, 0x5C),
            Color::rgb8(0x2D, 0xD4, 0xBF),
            0.075,
        )
    } else {
        (
            Color::rgb8(0xE9, 0xEE, 0xFA),
            Color::rgb8(0xDC, 0xE7, 0xFB),
            Color::rgb8(0x5B, 0x8D, 0xFF),
            Color::rgb8(0xA7, 0x83, 0xF4),
            Color::rgb8(0xFF, 0xB2, 0x8A),
            Color::rgb8(0x5A, 0xD6, 0xC6),
            0.35,
        )
    };
    p.fill_rect(b, base);
    p.fill_rect(b, Brush::vertical(b, top, Color::TRANSPARENT));
    p.fill_rect(
        b,
        Brush::glow(
            PointF::new(b.w * 0.80, b.h * 0.16),
            b.w * 0.88,
            blue.multiply_alpha(if theme.is_dark() { 0.72 } else { 0.48 }),
        ),
    );
    p.fill_rect(
        b,
        Brush::glow(
            PointF::new(b.w * 0.06, b.h * 0.54),
            b.w * 0.76,
            violet.multiply_alpha(if theme.is_dark() { 0.54 } else { 0.34 }),
        ),
    );
    p.fill_rect(
        b,
        Brush::glow(
            PointF::new(b.w * 0.72, b.h * 0.94),
            b.w * 0.58,
            warm.multiply_alpha(if theme.is_dark() { 0.30 } else { 0.20 }),
        ),
    );
    p.fill_rect(
        b,
        Brush::glow(
            PointF::new(b.w * 0.18, b.h * 0.99),
            b.w * 0.62,
            teal.multiply_alpha(if theme.is_dark() { 0.30 } else { 0.18 }),
        ),
    );
    p.fill_rect(
        b,
        Brush::glow(
            PointF::new(b.w * 0.44, b.h * 0.44),
            b.w * 0.46,
            Color::rgb8(0xFF, 0x5C, 0x8A).multiply_alpha(if theme.is_dark() { 0.12 } else { 0.08 }),
        ),
    );
    p.fill_rect(
        b,
        Brush::linear(
            PointF::new(0.0, 0.0),
            PointF::new(b.w, b.h),
            &[
                (0.0, Color::rgba(1.0, 1.0, 1.0, sheen)),
                (0.42, Color::TRANSPARENT),
                (1.0, Color::rgba(0.0, 0.0, 0.0, sheen * 0.55)),
            ],
        ),
    );
}

/// Releases GPU icon cache entries before the Direct2D device they belong to is dropped.
struct IconCacheGuard;
impl Drop for IconCacheGuard {
    fn drop(&mut self) {
        vitals_ui::app_icon::clear_cache();
    }
}

pub fn run(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let gfx = Gfx::new().context("creating Direct2D device")?;
    let _icons = IconCacheGuard;
    let snapshot = demo_snapshot();
    let settings = demo_settings();
    let startup = demo_startup();
    let version = env!("CARGO_PKG_VERSION").to_string();

    for (mode, theme) in [("dark", Theme::dark()), ("light", Theme::light())] {
        for tab in Tab::ALL {
            let mut view = DashboardView::new(snapshot.clone(), settings.clone(), false);
            view.set_startup(startup.clone());
            view.set_tab(tab);
            let mut spec = OffscreenSpec::new(SizeF::new(1180.0, 820.0), preview_scale(), theme.clone());
            spec.background = theme.window_background;
            let image = render_view_offscreen(&gfx, &mut view, &spec)?;
            let name = format!("dashboard-{}-{mode}.png", tab.label().to_lowercase());
            write_png(&dir.join(name), &image)?;
        }

        let mut quick = QuickView::new(snapshot.clone(), false);
        quick.set_shown(true);
        let spec = OffscreenSpec::new(SizeF::new(432.0, 600.0), preview_scale(), theme.clone());
        let image = render_offscreen(&gfx, &spec, |cx, p| {
            paint_wallpaper(p, &theme);
            quick.paint(cx, p);
        })?;
        write_png(&dir.join(format!("quick-{mode}.png")), &image)?;

        for (index, tab) in ["general", "shortcuts", "performance", "about"].iter().enumerate() {
            let mut view = SettingsView::new(settings.clone(), version.clone());
            view.select_tab(index);
            let mut spec = OffscreenSpec::new(SizeF::new(760.0, 680.0), preview_scale(), theme.clone());
            spec.background = theme.window_background;
            let image = render_view_offscreen(&gfx, &mut view, &spec)?;
            write_png(&dir.join(format!("settings-{tab}-{mode}.png")), &image)?;
        }
    }

    // One deliberately tiny render keeps the layout arithmetic honest at the minimum window size.
    let mut view = DashboardView::new(snapshot, settings, false);
    view.set_startup(startup);
    view.set_tab(Tab::Processes);
    let spec = OffscreenSpec::new(SizeF::new(960.0, 620.0), 1.0, Theme::dark()).background(Theme::dark().window_background);
    let image = render_view_offscreen(&gfx, &mut view, &spec)?;
    write_png(&dir.join("dashboard-compact-dark.png"), &image)?;

    drop(_icons);
    let _ = ThemeMode::System;
    log::info!("wrote previews to {}", dir.display());
    println!("Previews written to {}", dir.display());
    Ok(())
}
