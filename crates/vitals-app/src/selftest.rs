//! Non-interactive smoke tests: model math, settings, live sampling and the renderer.
//!
//! `--selftest` is intentionally safe: it never ends, suspends or reprioritizes a process.

use std::{collections::HashSet, thread, time::Duration};

use anyhow::{Result, bail};
use vitals_core::{Image, Settings, SizeF, SortKey, group_processes};
use vitals_panel::demo::{demo_settings, demo_snapshot, demo_startup};
use vitals_panel::intent::command_executable;
use vitals_panel::{DashboardView, QuickView, SettingsView, Tab};
use vitals_proc::Sampler;
use vitals_ui::{Gfx, OffscreenSpec, Theme, render_view_offscreen};

#[derive(Default)]
struct Checks {
    failures: Vec<String>,
    passed: usize,
}

impl Checks {
    fn check(&mut self, condition: bool, label: impl Into<String>) {
        if condition {
            self.passed += 1;
        } else {
            self.failures.push(label.into());
        }
    }

    fn finish(self) -> Result<()> {
        if self.failures.is_empty() {
            println!("Selftest: {} checks passed.", self.passed);
            Ok(())
        } else {
            for failure in &self.failures {
                println!("FAIL: {failure}");
            }
            bail!("{} of {} checks failed", self.failures.len(), self.passed + self.failures.len());
        }
    }
}

/// Releases GPU icon cache entries before the Direct2D device they belong to is dropped.
struct IconCacheGuard;
impl Drop for IconCacheGuard {
    fn drop(&mut self) {
        vitals_ui::app_icon::clear_cache();
    }
}

fn colors(image: &Image) -> usize {
    let mut seen = HashSet::new();
    for y in (0..image.height as usize).step_by(9) {
        for x in (0..image.width as usize).step_by(9) {
            let i = y * image.width as usize * 4 + x * 4;
            if i + 3 < image.data.len() {
                seen.insert(u32::from_le_bytes([
                    image.data[i],
                    image.data[i + 1],
                    image.data[i + 2],
                    image.data[i + 3],
                ]));
            }
        }
    }
    seen.len()
}

fn check_image(checks: &mut Checks, image: &Image, width: u32, height: u32, label: &str) {
    checks.check(
        image.width == width && image.height == height,
        format!("{label}: unexpected size {}x{}", image.width, image.height),
    );
    checks.check(image.data.len() == width as usize * height as usize * 4, format!("{label}: buffer length mismatch"));
    checks.check(image.data.chunks_exact(4).all(|p| p[3] == 255), format!("{label}: background was not opaque"));
    let unique = colors(image);
    checks.check(unique > 24, format!("{label}: looked blank ({unique} sampled colours)"));
}

pub fn run() -> Result<()> {
    let mut checks = Checks::default();

    let demo = demo_snapshot();
    checks.check(demo.processes.len() >= 10, "demo snapshot has too few processes");
    checks.check(demo.system.memory_total > 0, "demo memory total is zero");
    checks.check(demo.system.cpu_history.len() > 2, "demo CPU history is empty");
    let groups = group_processes(&demo.processes, SortKey::Cpu, true);
    checks.check(!groups.is_empty(), "grouping produced no groups");
    checks.check(groups.iter().map(|g| g.count()).sum::<usize>() == demo.processes.len(), "grouping lost processes");
    checks.check(groups.windows(2).all(|pair| pair[0].cpu >= pair[1].cpu), "CPU groups are not descending");
    checks.check(demo.system.process_count > 0, "demo process count is zero");

    let settings_on = Settings { confirm_end_task: false, ..demo_settings() };
    let json = serde_json::to_string(&settings_on)?;
    let settings_back: Settings = serde_json::from_str(&json)?;
    checks.check(settings_on == settings_back, "settings did not round-trip through JSON");

    checks.check(demo_startup().len() >= 4, "demo startup list is too short");
    checks.check(
        command_executable(r#""C:\Program Files\App\app.exe" --quiet"#).is_some(),
        "quoted command parsing failed",
    );
    checks.check(command_executable("relative.exe").is_none(), "relative command should not be executable");

    // A real sample proves the Win32 path still works; a second sample gives CPU deltas.
    let mut sampler = Sampler::new();
    let warm = sampler.sample();
    thread::sleep(Duration::from_millis(240));
    let live = sampler.sample();
    checks.check(
        live.processes.len() >= warm.processes.len().saturating_sub(4).max(1),
        "live sampler lost most processes",
    );
    checks.check(live.system.memory_total > 0, "live memory total is zero");
    checks.check(
        live.processes.iter().any(|p| p.pid == std::process::id()),
        "live sampler did not find Vitals itself",
    );
    checks.check(
        live.system.cpu_percent.is_finite() && (0.0..=100.0).contains(&live.system.cpu_percent),
        "live CPU percent was out of range",
    );
    checks.check(
        live.processes.iter().all(|p| p.cpu.is_finite() && (0.0..=100.0).contains(&p.cpu)),
        "a process CPU value was out of range",
    );

    let gfx = Gfx::new()?;
    let _icons = IconCacheGuard;
    let theme = Theme::dark();
    let mut dashboard = DashboardView::new(demo.clone(), demo_settings(), false);
    dashboard.set_startup(demo_startup());
    dashboard.set_tab(Tab::Performance);
    let spec = OffscreenSpec::new(SizeF::new(1180.0, 820.0), 1.0, theme.clone()).background(theme.window_background);
    let image = render_view_offscreen(&gfx, &mut dashboard, &spec)?;
    check_image(&mut checks, &image, 1180, 820, "dashboard render");

    let mut quick = QuickView::new(demo.clone(), false);
    quick.set_shown(true);
    let spec = OffscreenSpec::new(SizeF::new(432.0, 600.0), 1.25, theme.clone()).background(theme.window_background);
    let image = render_view_offscreen(&gfx, &mut quick, &spec)?;
    check_image(&mut checks, &image, 540, 750, "quick view render");

    let mut settings = SettingsView::new(demo_settings(), "0.1.0".into());
    settings.select_tab(2);
    let spec = OffscreenSpec::new(SizeF::new(760.0, 680.0), 1.0, Theme::light()).background(Theme::light().window_background);
    let image = render_view_offscreen(&gfx, &mut settings, &spec)?;
    check_image(&mut checks, &image, 760, 680, "settings render");

    drop(_icons);
    checks.finish()
}
