//! Settings window: appearance, shortcut ownership, sampling and about, in a calm inset-card layout.

use vitals_core::{Settings, ThemeMode};
use vitals_ui::widgets::{Button, ButtonStyle, Interaction, Response, Segment, Segmented, SegmentedStyle, Toggle};
use vitals_ui::{
    Brush, Color, Ctx, Cursor, Event, HitArea, Icon, Key, Painter, PointF, RectF, SizeF, TextAlign, TextStyle, View,
    Weight,
};

use crate::components;
use crate::intent::PanelIntent;

pub const WINDOW_SIZE: SizeF = SizeF::new(760.0, 680.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SettingsTab {
    General,
    Shortcuts,
    Performance,
    About,
}

impl SettingsTab {
    const ALL: [SettingsTab; 4] = [
        SettingsTab::General,
        SettingsTab::Shortcuts,
        SettingsTab::Performance,
        SettingsTab::About,
    ];

    fn label(self) -> &'static str {
        match self {
            SettingsTab::General => "General",
            SettingsTab::Shortcuts => "Shortcuts",
            SettingsTab::Performance => "Performance",
            SettingsTab::About => "About",
        }
    }

    fn icon(self) -> Icon {
        match self {
            SettingsTab::General => Icon::Sliders,
            SettingsTab::Shortcuts => Icon::Keyboard,
            SettingsTab::Performance => Icon::Activity,
            SettingsTab::About => Icon::Info,
        }
    }
}

pub struct SettingsView {
    settings: Settings,
    version: String,
    tab: SettingsTab,
    tabs: Segmented,
    theme: Segmented,
    sampling: Segmented,
    launch: Toggle,
    minimized: Toggle,
    close_tray: Toggle,
    confirm: Toggle,
    task_manager: Toggle,
    legacy: Toggle,
    data_folder: Button,
    license: Button,
    data_hover: Interaction,
    license_hover: Interaction,
    tabs_rect: RectF,
    theme_rect: RectF,
    sampling_rect: RectF,
}

impl SettingsView {
    pub fn new(settings: Settings, version: String) -> Self {
        let tabs = Segmented::new(
            SettingsTab::ALL
                .iter()
                .map(|tab| Segment::icon_label(tab.icon(), tab.label()))
                .collect(),
            0,
        )
        .style(SegmentedStyle::Track);
        let theme = Segmented::new(
            vec![Segment::label("System"), Segment::label("Light"), Segment::label("Dark")],
            theme_index(settings.theme),
        )
        .style(SegmentedStyle::Track);
        let sampling = Segmented::new(
            vec![Segment::label("0.5 s"), Segment::label("1 s"), Segment::label("2 s")],
            sampling_index(settings.sampling_ms),
        )
        .style(SegmentedStyle::Track);
        let mut view = Self {
            settings,
            version,
            tab: SettingsTab::General,
            tabs,
            theme,
            sampling,
            launch: Toggle::new(false),
            minimized: Toggle::new(false),
            close_tray: Toggle::new(false),
            confirm: Toggle::new(true),
            task_manager: Toggle::new(true),
            legacy: Toggle::new(true),
            data_folder: Button::new("Open data folder", ButtonStyle::Secondary).icon(Icon::Folder),
            license: Button::new("Copy version", ButtonStyle::Plain).icon(Icon::Copy),
            data_hover: Interaction::new(),
            license_hover: Interaction::new(),
            tabs_rect: RectF::default(),
            theme_rect: RectF::default(),
            sampling_rect: RectF::default(),
        };
        view.sync();
        view
    }

    pub fn set_settings(&mut self, settings: Settings) {
        self.settings = settings;
        self.sync();
    }

    pub fn set_version(&mut self, version: String) {
        self.version = version;
    }

    /// Selects a settings tab by index (0 General, 1 Shortcuts, 2 Performance, 3 About); used by previews and tests.
    pub fn select_tab(&mut self, index: usize) {
        if let Some(tab) = SettingsTab::ALL.get(index).copied() {
            self.tab = tab;
            self.tabs.set_selected(index);
        }
    }

    fn sync(&mut self) {
        self.launch.set_on(self.settings.launch_at_login);
        self.minimized.set_on(self.settings.start_minimized);
        self.close_tray.set_on(self.settings.close_to_tray);
        self.confirm.set_on(self.settings.confirm_end_task);
        self.task_manager.set_on(self.settings.replace_task_manager);
        self.legacy.set_on(self.settings.replace_legacy_task_manager);
        self.theme.set_selected(theme_index(self.settings.theme));
        self.sampling.set_selected(sampling_index(self.settings.sampling_ms));
    }

    fn layout(&mut self, gfx: &vitals_ui::Gfx, size: SizeF) {
        let tabs_rect = RectF::new(40.0, 124.0, size.w - 80.0, 32.0);
        self.tabs_rect = tabs_rect;
        self.tabs.layout(gfx, tabs_rect);
        self.theme_rect = RectF::new(60.0, 274.0, 270.0, 28.0);
        self.theme.layout(gfx, self.theme_rect);
        self.sampling_rect = RectF::new(size.w - 40.0 - 20.0 - 300.0, 246.0, 300.0, 28.0);
        self.sampling.layout(gfx, self.sampling_rect);
        let toggle_x = size.w - 40.0 - 20.0 - 40.0;
        self.launch.set_origin(PointF::new(toggle_x, 427.0));
        self.minimized.set_origin(PointF::new(toggle_x, 487.0));
        self.close_tray.set_origin(PointF::new(toggle_x, 547.0));
        self.confirm.set_origin(PointF::new(toggle_x, 465.0));
        self.task_manager.set_origin(PointF::new(toggle_x, 249.0));
        self.legacy.set_origin(PointF::new(toggle_x, 309.0));
        self.data_folder.set_rect(RectF::new(64.0, 398.0, 158.0, 30.0));
        self.license.set_rect(RectF::new(232.0, 398.0, 110.0, 30.0));
    }

    fn post_settings(&self, cx: &mut Ctx) {
        cx.post(PanelIntent::SettingsChanged(Box::new(self.settings.clone())));
    }

    fn hide(&self, cx: &mut Ctx) {
        cx.post(PanelIntent::HideSettings);
    }

    fn switch(&mut self, tab: SettingsTab, cx: &mut Ctx) {
        if self.tab != tab {
            self.tab = tab;
            cx.request_paint();
        }
    }
}

impl View for SettingsView {
    fn event(&mut self, cx: &mut Ctx, event: &Event) -> bool {
        self.layout(cx.gfx(), cx.size());
        match event {
            Event::CloseRequested => {
                self.hide(cx);
                return true;
            }
            Event::KeyDown(key) if key.key == Key::Escape => {
                self.hide(cx);
                return true;
            }
            Event::KeyDown(key) if key.mods.ctrl => match key.key {
                Key::Char('1') => {
                    self.switch(SettingsTab::General, cx);
                    return true;
                }
                Key::Char('2') => {
                    self.switch(SettingsTab::Shortcuts, cx);
                    return true;
                }
                Key::Char('3') => {
                    self.switch(SettingsTab::Performance, cx);
                    return true;
                }
                Key::Char('4') => {
                    self.switch(SettingsTab::About, cx);
                    return true;
                }
                Key::Char('W') => {
                    self.hide(cx);
                    return true;
                }
                _ => {}
            },
            Event::Shown => {
                cx.request_paint();
                return true;
            }
            _ => {}
        }

        if let Some(index) = self.tabs.event(cx, event).action() {
            self.switch(SettingsTab::ALL[index], cx);
            return true;
        }

        let pointer = event.pointer_pos();
        match self.tab {
            SettingsTab::General => {
                if let Some(index) = self.theme.event(cx, event).action() {
                    self.settings.theme = [ThemeMode::System, ThemeMode::Light, ThemeMode::Dark][index];
                    self.post_settings(cx);
                    cx.request_paint();
                    return true;
                }
                if self.launch.event(cx, event).action().is_some() {
                    self.settings.launch_at_login = self.launch.is_on();
                    self.post_settings(cx);
                    return true;
                }
                if self.minimized.event(cx, event).action().is_some() {
                    self.settings.start_minimized = self.minimized.is_on();
                    self.post_settings(cx);
                    return true;
                }
                if self.close_tray.event(cx, event).action().is_some() {
                    self.settings.close_to_tray = self.close_tray.is_on();
                    self.post_settings(cx);
                    return true;
                }
            }
            SettingsTab::Shortcuts => {
                if self.task_manager.event(cx, event).action().is_some() {
                    self.settings.replace_task_manager = self.task_manager.is_on();
                    self.post_settings(cx);
                    return true;
                }
                if self.legacy.event(cx, event).action().is_some() {
                    self.settings.replace_legacy_task_manager = self.legacy.is_on();
                    self.post_settings(cx);
                    return true;
                }
            }
            SettingsTab::Performance => {
                if let Some(index) = self.sampling.event(cx, event).action() {
                    self.settings.sampling_ms = [500, 1000, 2000][index];
                    self.post_settings(cx);
                    cx.request_paint();
                    return true;
                }
                if self.confirm.event(cx, event).action().is_some() {
                    self.settings.confirm_end_task = self.confirm.is_on();
                    self.post_settings(cx);
                    return true;
                }
            }
            SettingsTab::About => {
                if self.data_folder.event(cx, event).action().is_some() {
                    cx.post(PanelIntent::OpenDataFolder);
                    return true;
                }
                if self.license.event(cx, event).action().is_some() {
                    cx.post(PanelIntent::CopyText(format!("Vitals {}", self.version)));
                    return true;
                }
                if let Response::Action(()) = self.data_hover.update(event, self.data_folder_rect(), true) {
                    cx.post(PanelIntent::OpenDataFolder);
                    return true;
                }
                if let Response::Action(()) = self.license_hover.update(event, self.license_rect(), true) {
                    cx.post(PanelIntent::CopyText(format!("Vitals {}", self.version)));
                    return true;
                }
            }
        }

        if let Some(pos) = pointer {
            if self.toggle_rects().iter().any(|rect| rect.contains(pos))
                || self.tabs_rect.contains(pos)
                || (self.tab == SettingsTab::General && self.theme_rect.contains(pos))
                || (self.tab == SettingsTab::Performance && self.sampling_rect.contains(pos))
                || (self.tab == SettingsTab::About && (self.data_folder_rect().contains(pos) || self.license_rect().contains(pos)))
            {
                cx.set_cursor(Cursor::Hand);
            }
        }
        false
    }

    fn paint(&mut self, cx: &mut Ctx, p: &mut Painter) {
        self.layout(cx.gfx(), cx.size());
        let theme = p.theme().clone();
        components::paint_window_background(cx, p, &theme);
        let bounds = p.bounds();

        p.fill_rect(RectF::new(0.0, 0.0, bounds.w, 48.0), if theme.is_dark() {
            Color::rgba(1.0, 1.0, 1.0, 0.018)
        } else {
            Color::rgba(1.0, 1.0, 1.0, 0.24)
        });
        p.fill_rect(RectF::new(0.0, 47.0, bounds.w, 1.0), theme.separator.multiply_alpha(0.5));
        components::logo(p, PointF::new(30.0, 24.0), 28.0);
        p.text(
            "Vitals",
            &TextStyle::new(16.0).weight(Weight::Semibold),
            theme.text,
            RectF::new(52.0, 7.0, 120.0, 22.0),
        );
        p.text("Settings", &TextStyle::new(9.5), theme.text_tertiary, RectF::new(52.0, 25.0, 120.0, 14.0));

        p.text(
            "Settings",
            &TextStyle::new(31.0).weight(Weight::Semibold),
            theme.text,
            RectF::new(40.0, 56.0, 300.0, 38.0),
        );
        p.text(
            "Changes apply immediately and are remembered.",
            &TextStyle::new(12.5),
            theme.text_secondary,
            RectF::new(40.0, 96.0, 460.0, 20.0),
        );
        self.tabs.paint(p);

        match self.tab {
            SettingsTab::General => self.paint_general(p),
            SettingsTab::Shortcuts => self.paint_shortcuts(p),
            SettingsTab::Performance => self.paint_performance(p),
            SettingsTab::About => self.paint_about(p),
        }

        p.fill_rect(
            RectF::new(40.0, bounds.h - 44.0, bounds.w - 80.0, 1.0),
            theme.separator.multiply_alpha(0.45),
        );
        p.text(
            "Settings save automatically",
            &TextStyle::new(10.0),
            theme.text_tertiary,
            RectF::new(40.0, bounds.h - 28.0, 280.0, 16.0),
        );
        p.text(
            &format!("Vitals {}", self.version),
            &TextStyle::new(10.0).align(TextAlign::Trailing),
            theme.text_tertiary,
            RectF::new(bounds.w - 200.0, bounds.h - 28.0, 160.0, 16.0),
        );
    }

    fn hit_test(&self, pos: PointF) -> HitArea {
        if pos.y < 48.0 { HitArea::Caption } else { HitArea::Client }
    }
}

impl SettingsView {
    fn paint_general(&mut self, p: &mut Painter) {
        let theme = p.theme().clone();
        let appearance = RectF::new(40.0, 174.0, p.bounds().w - 80.0, 164.0);
        components::card_tinted(p, appearance, 18.0, components::PURPLE, 0.12);
        self.paint_section_header(
            p,
            appearance,
            Icon::Sparkles,
            components::PURPLE,
            "Appearance",
            "Follow Windows or choose a look for Vitals.",
        );
        p.text("Theme", &TextStyle::body(), theme.text, RectF::new(appearance.x + 20.0, appearance.y + 78.0, 120.0, 24.0));
        self.theme.paint(p);
        let preview = RectF::new(appearance.right() - 248.0, appearance.y + 16.0, 228.0, 112.0);
        self.paint_theme_preview(p, preview);

        let behavior = RectF::new(40.0, 352.0, p.bounds().w - 80.0, 262.0);
        components::card_tinted(p, behavior, 18.0, theme.accent, 0.09);
        self.paint_section_header(
            p,
            behavior,
            Icon::Monitor,
            theme.accent,
            "Behavior",
            "Keep Vitals close without getting in the way.",
        );
        self.paint_row(p, behavior, 72.0, "Launch at login", "Start Vitals with Windows, quietly.", self.launch.rect());
        self.paint_row(p, behavior, 132.0, "Start minimized", "Open in the notification area instead of the dashboard.", self.minimized.rect());
        self.paint_row(p, behavior, 192.0, "Close window to tray", "Keep monitoring when the dashboard closes.", self.close_tray.rect());
        self.launch.paint(p);
        self.minimized.paint(p);
        self.close_tray.paint(p);
    }

    fn paint_section_header(
        &self,
        p: &mut Painter,
        card: RectF,
        icon: Icon,
        tint: Color,
        title: &str,
        subtitle: &str,
    ) {
        let theme = p.theme().clone();
        components::icon_tile(p, RectF::new(card.x + 20.0, card.y + 16.0, 32.0, 32.0), icon, tint);
        p.text(
            title,
            &TextStyle::new(13.5).weight(Weight::Semibold),
            theme.text,
            RectF::new(card.x + 62.0, card.y + 15.0, 260.0, 20.0),
        );
        p.text(
            subtitle,
            &TextStyle::new(10.5),
            theme.text_tertiary,
            RectF::new(card.x + 62.0, card.y + 34.0, card.w - 330.0, 16.0),
        );
    }

    fn paint_theme_preview(&self, p: &mut Painter, rect: RectF) {
        let theme = p.theme().clone();
        let rect = p.snap_rect(rect);
        p.shadow(rect, 14.0, &theme.shadows[1].scaled_alpha(0.55));
        p.clip_round_rect(rect, 12.0, |p| {
            p.fill_rect(rect, theme.window_background);
            p.fill_rect(
                rect,
                Brush::glow(
                    PointF::new(rect.right() - 24.0, rect.y + 8.0),
                    rect.w * 0.8,
                    theme.accent.multiply_alpha(if theme.is_dark() { 0.20 } else { 0.12 }),
                ),
            );
            p.fill_rect(
                rect,
                Brush::glow(
                    PointF::new(rect.x + 8.0, rect.bottom() - 4.0),
                    rect.w * 0.55,
                    components::PURPLE.multiply_alpha(if theme.is_dark() { 0.10 } else { 0.06 }),
                ),
            );
            let side = RectF::new(rect.x, rect.y, 54.0, rect.h);
            p.fill_rect(side, if theme.is_dark() { Color::rgba(0.0, 0.0, 0.0, 0.18) } else { Color::rgba(1.0, 1.0, 1.0, 0.34) });
            p.fill_rect(RectF::new(side.right() - 1.0, side.y, 1.0, side.h), theme.separator.multiply_alpha(0.55));
            p.fill_round_rect(RectF::new(side.x + 9.0, side.y + 9.0, 11.0, 11.0), 3.0, theme.accent);
            p.fill_round_rect(RectF::new(side.x + 25.0, side.y + 12.0, 18.0, 5.0), 2.5, theme.text.multiply_alpha(0.8));
            for index in 0..3 {
                let y = side.y + 30.0 + index as f32 * 14.0;
                let pill = RectF::new(side.x + 7.0, y, 40.0, 10.0);
                p.fill_round_rect(
                    pill,
                    4.0,
                    if index == 0 {
                        theme.accent.multiply_alpha(if theme.is_dark() { 0.24 } else { 0.16 })
                    } else {
                        Color::rgba(1.0, 1.0, 1.0, if theme.is_dark() { 0.04 } else { 0.50 })
                    },
                );
                p.fill_round_rect(RectF::new(pill.x + 5.0, pill.center().y - 2.0, 4.0, 4.0), 1.0, if index == 0 { theme.accent } else { theme.text_tertiary });
                p.fill_round_rect(RectF::new(pill.x + 12.0, pill.center().y - 1.5, 18.0, 3.0), 1.5, theme.text_tertiary.multiply_alpha(if index == 0 { 0.9 } else { 0.55 }));
            }
            let content_x = side.right() + 10.0;
            p.fill_round_rect(RectF::new(content_x, rect.y + 13.0, 54.0, 6.0), 3.0, theme.text.multiply_alpha(0.85));
            p.fill_round_rect(RectF::new(content_x, rect.y + 23.0, 82.0, 3.0), 1.5, theme.text_tertiary.multiply_alpha(0.6));
            let stat_w = (rect.right() - content_x - 10.0 - 6.0) / 2.0;
            for index in 0..2 {
                let stat = RectF::new(content_x + index as f32 * (stat_w + 6.0), rect.y + 34.0, stat_w, 27.0);
                p.fill_round_rect(stat, 6.0, if theme.is_dark() { Color::rgba(1.0, 1.0, 1.0, 0.075) } else { Color::rgba(1.0, 1.0, 1.0, 0.72) });
                p.hairline_round_rect(stat, 6.0, theme.hairline.multiply_alpha(0.8), true);
                p.fill_round_rect(RectF::new(stat.x + 7.0, stat.y + 8.0, 6.0, 6.0), 2.0, if index == 0 { theme.accent } else { components::PURPLE });
                p.fill_round_rect(RectF::new(stat.x + 17.0, stat.y + 8.0, 22.0, 3.0), 1.5, theme.text_tertiary.multiply_alpha(0.65));
                p.fill_round_rect(RectF::new(stat.x + 7.0, stat.y + 17.0, 30.0, 5.0), 2.5, theme.text.multiply_alpha(0.85));
            }
            for index in 0..3 {
                let row = RectF::new(content_x, rect.y + 68.0 + index as f32 * 14.0, rect.right() - content_x - 10.0, 11.0);
                p.fill_round_rect(
                    row,
                    4.0,
                    if index == 0 {
                        theme.accent.multiply_alpha(if theme.is_dark() { 0.18 } else { 0.12 })
                    } else {
                        Color::rgba(1.0, 1.0, 1.0, if theme.is_dark() { 0.045 } else { 0.56 })
                    },
                );
                if index == 0 {
                    p.hairline_round_rect(row, 4.0, theme.accent.multiply_alpha(0.45), true);
                }
                p.fill_round_rect(
                    RectF::new(row.x + 5.0, row.center().y - 3.5, 7.0, 7.0),
                    2.0,
                    if index == 0 { theme.accent } else { theme.text_tertiary.multiply_alpha(0.45) },
                );
                p.fill_round_rect(RectF::new(row.x + 17.0, row.center().y - 2.0, 40.0, 3.0), 1.5, theme.text_secondary.multiply_alpha(0.75));
                p.fill_round_rect(RectF::new(row.right() - 25.0, row.center().y - 1.0, 17.0, 2.0), 1.0, theme.text_tertiary.multiply_alpha(0.55));
            }
        });
        p.hairline_round_rect(rect, 12.0, theme.hairline.multiply_alpha(1.2), true);
    }

    fn paint_shortcuts(&mut self, p: &mut Painter) {
        let theme = p.theme().clone();
        let card = RectF::new(40.0, 174.0, p.bounds().w - 80.0, 226.0);
        components::card_tinted(p, card, 18.0, theme.accent, 0.09);
        self.paint_section_header(
            p,
            card,
            Icon::Keyboard,
            theme.accent,
            "Keyboard ownership",
            "While Vitals runs, these shortcuts belong to it.",
        );
        self.paint_row(p, card, 72.0, "Replace Task Manager", "Ctrl+Shift+Esc opens the Vitals quick view.", self.task_manager.rect());
        self.paint_row(p, card, 132.0, "Replace legacy Task Manager", "Also take Ctrl+Alt+Esc.", self.legacy.rect());
        self.task_manager.paint(p);
        self.legacy.paint(p);

        let key = RectF::new(40.0, 416.0, p.bounds().w - 80.0, 144.0);
        components::card_tinted(p, key, 18.0, components::TEAL, 0.10);
        self.paint_section_header(
            p,
            key,
            Icon::Rocket,
            components::TEAL,
            "Global shortcut",
            "Summon Vitals over any app.",
        );
        let key_y = key.y + 72.0;
        components::keycap(p, RectF::new(key.x + 20.0, key_y, 46.0, 30.0), "Ctrl");
        p.text("+", &TextStyle::new(10.0), theme.text_tertiary, RectF::new(key.x + 66.0, key_y, 8.0, 30.0));
        components::keycap(p, RectF::new(key.x + 74.0, key_y, 48.0, 30.0), "Shift");
        p.text("+", &TextStyle::new(10.0), theme.text_tertiary, RectF::new(key.x + 122.0, key_y, 8.0, 30.0));
        components::keycap(p, RectF::new(key.x + 130.0, key_y, 40.0, 30.0), "Esc");
        p.text(
            "Opens without moving your focus, then closes with Esc.",
            &TextStyle::new(11.0),
            theme.text_secondary,
            RectF::new(key.x + 190.0, key_y, key.w - 210.0, 30.0),
        );
        p.text(
            "The shortcut returns to Windows as soon as Vitals quits.",
            &TextStyle::new(10.5),
            theme.text_tertiary,
            RectF::new(key.x + 20.0, key.y + 116.0, key.w - 40.0, 16.0),
        );
    }

    fn paint_performance(&mut self, p: &mut Painter) {
        let theme = p.theme().clone();
        let card = RectF::new(40.0, 174.0, p.bounds().w - 80.0, 200.0);
        components::card_tinted(p, card, 18.0, theme.accent, 0.09);
        self.paint_section_header(
            p,
            card,
            Icon::Gauge,
            theme.accent,
            "Sampling",
            "How often Vitals reads the process table and counters.",
        );
        p.text("Update every", &TextStyle::body(), theme.text, RectF::new(card.x + 20.0, card.y + 72.0, 150.0, 28.0));
        self.sampling.paint(p);
        p.text(
            "Faster updates feel more immediate; slower updates use a little less power.",
            &TextStyle::new(10.5),
            theme.text_tertiary,
            RectF::new(card.x + 20.0, card.y + 124.0, card.w - 40.0, 16.0),
        );

        let safety = RectF::new(40.0, 390.0, p.bounds().w - 80.0, 140.0);
        components::card_tinted(p, safety, 18.0, components::ORANGE, 0.10);
        self.paint_section_header(
            p,
            safety,
            Icon::Shield,
            components::ORANGE,
            "Safety",
            "A guardrail before anything destructive happens.",
        );
        self.paint_row(p, safety, 72.0, "Confirm before ending tasks", "Ask before closing processes and losing unsaved work.", self.confirm.rect());
        self.confirm.paint(p);
        let note = RectF::new(40.0, 550.0, p.bounds().w - 80.0, 40.0);
        p.fill_ellipse(PointF::new(note.x + 3.0, note.y + 8.0), 2.5, 2.5, theme.text_tertiary);
        p.text(
            "Vitals cannot end protected Windows processes. Elevation is required to act on processes owned by other accounts.",
            &TextStyle::new(10.5).wrap().line_height(16.0),
            theme.text_tertiary,
            RectF::new(note.x + 12.0, note.y, note.w - 12.0, note.h),
        );
    }

    fn paint_about(&mut self, p: &mut Painter) {
        let theme = p.theme().clone();
        let card = RectF::new(40.0, 174.0, p.bounds().w - 80.0, 300.0);
        components::card_tinted(p, card, 18.0, theme.accent, 0.13);
        components::logo(p, PointF::new(card.x + 60.0, card.y + 60.0), 72.0);
        p.text(
            "Vitals",
            &TextStyle::new(31.0).weight(Weight::Semibold),
            theme.text,
            RectF::new(card.x + 128.0, card.y + 26.0, 260.0, 38.0),
        );
        p.text(
            &format!("Version {}", self.version),
            &TextStyle::new(12.0),
            theme.text_secondary,
            RectF::new(card.x + 128.0, card.y + 64.0, 260.0, 18.0),
        );
        components::badge(
            p,
            RectF::new(card.right() - 78.0, card.y + 34.0, 54.0, 24.0),
            "MIT",
            components::TEAL,
        );
        p.text(
            "A calm, fast process manager for Windows 11.",
            &TextStyle::new(13.0),
            theme.text_secondary,
            RectF::new(card.x + 128.0, card.y + 90.0, 440.0, 20.0),
        );
        p.text(
            "Designed around clarity, spring motion, and details that stay out of the way.",
            &TextStyle::new(10.5),
            theme.text_tertiary,
            RectF::new(card.x + 128.0, card.y + 113.0, 470.0, 16.0),
        );
        p.fill_rect(
            RectF::new(card.x + 24.0, card.y + 148.0, card.w - 48.0, 1.0),
            theme.separator.multiply_alpha(0.55),
        );
        p.text(
            "Design language and render kit adapted from Glint and Tuck by joxan2137. MIT licensed.",
            &TextStyle::new(10.5),
            theme.text_tertiary,
            RectF::new(card.x + 24.0, card.y + 162.0, card.w - 48.0, 16.0),
        );
        p.text(
            "Built in Rust with Direct2D, DirectWrite and DirectComposition.",
            &TextStyle::new(10.5),
            theme.text_tertiary,
            RectF::new(card.x + 24.0, card.y + 182.0, card.w - 48.0, 16.0),
        );
        self.data_folder.paint(p);
        self.license.paint(p);
        p.fill_ellipse(PointF::new(47.0, 512.0), 2.5, 2.5, theme.accent);
        p.text(
            "The source is organized as small crates: core model, render kit, sampler, Windows integration, views and app.",
            &TextStyle::new(10.5).wrap().line_height(16.0),
            theme.text_tertiary,
            RectF::new(56.0, 502.0, p.bounds().w - 112.0, 40.0),
        );
    }

    fn paint_row(&self, p: &mut Painter, card: RectF, y: f32, title: &str, subtitle: &str, toggle: RectF) {
        let theme = p.theme().clone();
        let row = RectF::new(card.x + 20.0, card.y + y, card.w - 40.0, 54.0);
        p.text(title, &TextStyle::body().weight(Weight::Medium), theme.text, RectF::new(row.x, row.y, row.w - 90.0, 22.0));
        p.text(subtitle, &TextStyle::caption(), theme.text_tertiary, RectF::new(row.x, row.y + 22.0, row.w - 90.0, 16.0));
        let _ = toggle;
        if y > 54.0 {
            p.fill_rect(RectF::new(row.x, row.y - 4.0, row.w, 1.0), theme.separator.multiply_alpha(0.55));
        }
    }

    fn toggle_rects(&self) -> [RectF; 6] {
        [self.launch.rect(), self.minimized.rect(), self.close_tray.rect(), self.confirm.rect(), self.task_manager.rect(), self.legacy.rect()]
    }

    fn data_folder_rect(&self) -> RectF {
        RectF::new(64.0, 398.0, 158.0, 30.0)
    }

    fn license_rect(&self) -> RectF {
        RectF::new(232.0, 398.0, 110.0, 30.0)
    }
}

fn theme_index(theme: ThemeMode) -> usize {
    match theme {
        ThemeMode::System => 0,
        ThemeMode::Light => 1,
        ThemeMode::Dark => 2,
    }
}

fn sampling_index(ms: u64) -> usize {
    if ms <= 600 { 0 } else if ms >= 1500 { 2 } else { 1 }
}
