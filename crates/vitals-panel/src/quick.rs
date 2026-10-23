//! The compact panel summoned with Ctrl+Shift+Esc: live totals and the busiest apps.

use vitals_core::{format_bytes, format_count, format_percent, ProcessRow, Snapshot};
use vitals_ui::widgets::{Button, ButtonStyle, IconButton, Interaction, Presence, Response};
use vitals_ui::{
    Brush, Color, Ctx, Cursor, Event, HitArea, Icon, Key, Painter, PointF, RectF, SizeF, TextAlign, TextStyle, View,
    Weight,
};

use crate::components;
use crate::intent::PanelIntent;

pub const WINDOW_SIZE: SizeF = SizeF::new(432.0, 600.0);
const MARGIN: f32 = 16.0;
const ROW_H: f32 = 46.0;
const ROW_STEP: f32 = 50.0;

pub struct QuickView {
    snapshot: Snapshot,
    paused: bool,
    selected_pid: Option<u32>,
    close: IconButton,
    pause: IconButton,
    open: Button,
    settings: IconButton,
    open_all: Interaction,
    hover_row: Option<usize>,
    presence: Presence,
    card: RectF,
    rows: Vec<RectF>,
    stat_rects: [RectF; 3],
    see_all: RectF,
    footer: RectF,
}

impl QuickView {
    pub fn new(snapshot: Snapshot, paused: bool) -> Self {
        let mut view = Self {
            snapshot,
            paused,
            selected_pid: None,
            close: IconButton::new(Icon::X).tooltip("Close", Some("Esc")),
            pause: IconButton::new(Icon::Pause).tooltip("Pause sampling", None),
            open: Button::new("Open Dashboard", ButtonStyle::Primary).icon(Icon::AppWindow),
            settings: IconButton::new(Icon::Settings).tooltip("Settings", Some("Ctrl+,")),
            open_all: Interaction::new(),
            hover_row: None,
            presence: Presence::new(false),
            card: RectF::default(),
            rows: Vec::new(),
            stat_rects: [RectF::default(); 3],
            see_all: RectF::default(),
            footer: RectF::default(),
        };
        view.sync_pause();
        view.ensure_selection();
        view
    }

    pub fn set_snapshot(&mut self, snapshot: Snapshot) {
        self.snapshot = snapshot;
        self.ensure_selection();
    }

    fn ensure_selection(&mut self) {
        let processes = self.visible_processes();
        if processes.is_empty() {
            self.selected_pid = None;
            return;
        }
        if !processes.iter().any(|process| Some(process.pid) == self.selected_pid) {
            self.selected_pid = Some(processes[0].pid);
        }
    }

    fn selected_index(&self) -> Option<usize> {
        self.visible_processes()
            .iter()
            .position(|process| Some(process.pid) == self.selected_pid)
    }

    fn move_selection(&mut self, delta: isize) {
        let processes = self.visible_processes();
        if processes.is_empty() {
            self.selected_pid = None;
            return;
        }
        let current = self.selected_index().unwrap_or(0) as isize;
        let next = (current + delta).clamp(0, processes.len() as isize - 1) as usize;
        self.selected_pid = Some(processes[next].pid);
    }

    fn open_selected(&self, cx: &mut Ctx, index: Option<usize>) -> bool {
        let processes = self.visible_processes();
        let Some(process) = index.and_then(|index| processes.get(index)).copied() else {
            return false;
        };
        let pid = process.pid;
        cx.post(PanelIntent::HideQuickView);
        cx.post(PanelIntent::ShowProcess { pid });
        true
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        self.sync_pause();
    }

    pub fn set_shown(&mut self, shown: bool) {
        self.presence.set_shown(shown);
    }

    fn sync_pause(&mut self) {
        self.pause.icon = if self.paused { Icon::Play } else { Icon::Pause };
        self.pause.tooltip = Some(if self.paused { "Resume sampling".into() } else { "Pause sampling".into() });
        self.pause.tint = if self.paused { Some(components::ORANGE) } else { None };
    }

    fn layout(&mut self, size: SizeF) {
        let card = RectF::new(MARGIN, MARGIN, size.w - 2.0 * MARGIN, size.h - 2.0 * MARGIN);
        self.card = card;
        let inner_x = card.x + 20.0;
        let inner_w = card.w - 40.0;
        let header_y = card.y + 18.0;
        self.close.set_rect(RectF::new(card.right() - 46.0, header_y - 3.0, 32.0, 32.0));
        self.pause.set_rect(RectF::new(card.right() - 82.0, header_y - 3.0, 32.0, 32.0));
        let stat_y = card.y + 66.0;
        let stat_w = (inner_w - 16.0) / 3.0;
        self.stat_rects = [
            RectF::new(inner_x, stat_y, stat_w, 86.0),
            RectF::new(inner_x + stat_w + 8.0, stat_y, stat_w, 86.0),
            RectF::new(inner_x + 2.0 * (stat_w + 8.0), stat_y, stat_w, 86.0),
        ];
        let row_start = card.y + 202.0;
        self.rows = (0..6)
            .map(|index| RectF::new(inner_x, row_start + index as f32 * ROW_STEP, inner_w, ROW_H))
            .collect();
        self.see_all = RectF::new(card.right() - 102.0, card.y + 172.0, 84.0, 24.0);
        self.footer = RectF::new(inner_x, card.bottom() - 54.0, inner_w, 38.0);
        self.open.set_rect(RectF::new(self.footer.x, self.footer.y + 2.0, (self.footer.w - 46.0).max(120.0), 34.0));
        self.settings.set_rect(RectF::new(self.footer.right() - 38.0, self.footer.y + 2.0, 32.0, 32.0));
    }

    fn visible_processes(&self) -> Vec<&ProcessRow> {
        let mut processes: Vec<&ProcessRow> = self.snapshot.processes.iter().collect();
        processes.sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then_with(|| b.memory.cmp(&a.memory)));
        processes.truncate(6);
        processes
    }

    fn hide(&self, cx: &mut Ctx) {
        cx.post(PanelIntent::HideQuickView);
    }
}

impl View for QuickView {
    fn event(&mut self, cx: &mut Ctx, event: &Event) -> bool {
        self.layout(cx.size());
        let hover_before = self.hover_row;
        if matches!(event, Event::PointerDown(_) | Event::PointerUp(_)) {
            cx.request_paint();
        }
        match event {
            Event::Shown => {
                self.set_shown(true);
                self.selected_pid = self.visible_processes().first().map(|process| process.pid);
                cx.request_paint();
                return true;
            }
            Event::Focus(false) => {
                self.hide(cx);
                return true;
            }
            Event::CloseRequested => {
                self.hide(cx);
                return true;
            }
            Event::KeyDown(key) if key.key == Key::Escape => {
                self.hide(cx);
                return true;
            }
            Event::KeyDown(key) => match key.key {
                Key::Down => {
                    self.move_selection(1);
                    cx.request_paint();
                    return true;
                }
                Key::Up => {
                    self.move_selection(-1);
                    cx.request_paint();
                    return true;
                }
                Key::Home => {
                    self.selected_pid = self.visible_processes().first().map(|process| process.pid);
                    cx.request_paint();
                    return true;
                }
                Key::End => {
                    self.selected_pid = self.visible_processes().last().map(|process| process.pid);
                    cx.request_paint();
                    return true;
                }
                Key::Enter => {
                    let index = self.selected_index();
                    if self.open_selected(cx, index) {
                        return true;
                    }
                }
                _ => {}
            },
            _ => {}
        }

        if self.close.event(cx, event).action().is_some() {
            self.hide(cx);
            return true;
        }
        if self.pause.event(cx, event).action().is_some() {
            cx.post(PanelIntent::SetPaused(!self.paused));
            return true;
        }
        if self.settings.event(cx, event).action().is_some() {
            cx.post(PanelIntent::HideQuickView);
            cx.post(PanelIntent::ShowSettings);
            return true;
        }
        if self.open.event(cx, event).action().is_some() {
            cx.post(PanelIntent::HideQuickView);
            cx.post(PanelIntent::ShowDashboard);
            return true;
        }
        if let Response::Action(()) = self.open_all.update(event, self.see_all, true) {
            cx.post(PanelIntent::HideQuickView);
            cx.post(PanelIntent::ShowTab(crate::intent::Tab::Processes));
            return true;
        }

        if let Some(pos) = event.pointer_pos() {
            let visible = self.visible_processes().len();
            self.hover_row = self.rows.iter().position(|row| row.contains(pos)).filter(|index| *index < visible);
            let processes = self.visible_processes();
            let clickable = self.rows.iter().any(|row| row.contains(pos))
                || self.card.contains(pos);
            if clickable {
                cx.set_cursor(Cursor::Hand);
            }
            if let Event::PointerDown(pointer) = event
                && pointer.button == Some(vitals_ui::MouseButton::Left)
                && let Some(index) = self.hover_row
                && index < processes.len()
            {
                self.selected_pid = Some(processes[index].pid);
                if self.open_selected(cx, Some(index)) {
                    return true;
                }
            }
        }
        if matches!(event, Event::PointerLeave) {
            self.hover_row = None;
        }
        if self.hover_row != hover_before {
            cx.request_paint();
        }
        false
    }

    fn paint(&mut self, cx: &mut Ctx, p: &mut Painter) {
        self.layout(cx.size());
        let presence = self.presence.clone();
        presence.paint(p, self.card.center(), |p| self.paint_card(p));
    }

    fn hit_test(&self, _pos: PointF) -> HitArea {
        HitArea::Client
    }

    fn interactive_region(&self) -> Option<Vec<RectF>> {
        Some(vec![self.card])
    }
}

impl QuickView {
    fn paint_card(&mut self, p: &mut Painter) {
        let theme = p.theme().clone();
        let card = self.card;
        // The card fill is translucent, so use the exterior-only shadow; a normal box shadow would show its
        // solid interior through the glass and produce a hard rectangular band.
        p.shadow_outside(card, 24.0, &theme.shadows[0].scaled_alpha(1.30));
        p.fill_round_rect(card, 24.0, theme.glass_fill);
        p.clip_round_rect(card, 24.0, |p| {
            p.fill_rect(
                card,
                Brush::vertical(
                    card,
                    if theme.is_dark() { Color::rgba(1.0, 1.0, 1.0, 0.050) } else { Color::rgba(1.0, 1.0, 1.0, 0.28) },
                    Color::TRANSPARENT,
                ),
            );
            p.fill_rect(
                card,
                Brush::glow(
                    PointF::new(card.right() - 24.0, card.y + 28.0),
                    300.0,
                    theme.accent.multiply_alpha(if theme.is_dark() { 0.12 } else { 0.07 }),
                ),
            );
            p.fill_rect(
                card,
                Brush::glow(
                    PointF::new(card.x + 18.0, card.bottom() - 18.0),
                    250.0,
                    components::PURPLE.multiply_alpha(if theme.is_dark() { 0.060 } else { 0.035 }),
                ),
            );
        });
        p.hairline_round_rect(card, 24.0, theme.hairline.multiply_alpha(1.25), true);

        components::logo(p, PointF::new(card.x + 34.0, card.y + 34.0), 30.0);
        p.text(
            "Vitals",
            &TextStyle::new(18.0).weight(Weight::Semibold),
            theme.text,
            RectF::new(card.x + 58.0, card.y + 11.0, 160.0, 24.0),
        );
        let live_color = if self.paused { components::ORANGE } else { theme.success };
        p.fill_ellipse(PointF::new(card.x + 60.0, card.y + 42.0), 2.7, 2.7, live_color);
        p.text(
            if self.paused { "Sampling paused" } else { "Live system pulse" },
            &TextStyle::new(10.0).weight(Weight::Medium),
            if self.paused { components::ORANGE } else { theme.text_tertiary },
            RectF::new(card.x + 68.0, card.y + 34.0, 150.0, 16.0),
        );
        self.pause.paint(p);
        self.close.paint(p);

        let cpu = self.snapshot.system.cpu_percent;
        let memory = self.snapshot.system.memory_percent();
        let processes = self.snapshot.system.process_count;
        self.paint_stat(p, self.stat_rects[0], Icon::Cpu, "CPU", &format_percent(cpu), &format!("{} threads", format_count(self.snapshot.system.thread_count as usize)), theme.accent, Some(cpu / 100.0));
        self.paint_stat(p, self.stat_rects[1], Icon::Memory, "Memory", &format_bytes(self.snapshot.system.memory_used), &format!("of {}", format_bytes(self.snapshot.system.memory_total)), components::PURPLE, Some(memory / 100.0));
        self.paint_stat(p, self.stat_rects[2], Icon::ListTree, "Processes", &format_count(processes), "running now", components::TEAL, None);

        p.text(
            "Top processes",
            &TextStyle::new(13.5).weight(Weight::Semibold),
            theme.text,
            RectF::new(card.x + 20.0, card.y + 172.0, 180.0, 22.0),
        );
        let hovered_link = self.open_all.hover_amount();
        p.text(
            "See all",
            &TextStyle::new(10.5).weight(Weight::Medium).align(TextAlign::Trailing),
            theme.accent.lerp(&theme.text, hovered_link * 0.25),
            RectF::new(self.see_all.x, self.see_all.y, self.see_all.w - 14.0, self.see_all.h),
        );
        p.icon(
            Icon::ChevronRight,
            PointF::new(self.see_all.right() - 7.0, self.see_all.center().y),
            11.0,
            theme.accent,
        );

        let processes = self.visible_processes();
        let selected = self.selected_index();
        for (index, row) in self.rows.iter().enumerate() {
            let Some(process) = processes.get(index) else { break };
            let is_selected = Some(index) == selected;
            if is_selected {
                p.fill_round_rect(
                    *row,
                    12.0,
                    Brush::linear(
                        PointF::new(row.x, row.y),
                        PointF::new(row.right(), row.y),
                        &[
                            (0.0, theme.accent.multiply_alpha(if theme.is_dark() { 0.20 } else { 0.13 })),
                            (1.0, theme.accent.multiply_alpha(if theme.is_dark() { 0.09 } else { 0.06 })),
                        ],
                    ),
                );
                p.stroke_round_rect(*row, 12.0, theme.accent.multiply_alpha(0.55), 1.3);
            } else {
                p.fill_round_rect(
                    *row,
                    12.0,
                    if theme.is_dark() {
                        Color::rgba(1.0, 1.0, 1.0, if Some(index) == self.hover_row { 0.070 } else { 0.040 })
                    } else {
                        Color::rgba(1.0, 1.0, 1.0, if Some(index) == self.hover_row { 0.92 } else { 0.62 })
                    },
                );
                p.hairline_round_rect(*row, 12.0, theme.hairline.multiply_alpha(0.55), true);
            }
            if is_selected {
                p.fill_round_rect(RectF::new(row.x + 3.0, row.center().y - 9.0, 2.5, 18.0), 1.25, theme.accent);
            }
            components::app_avatar(
                p,
                RectF::new(row.x + 12.0, row.center().y - 13.0, 26.0, 26.0),
                process.path.as_deref(),
                &components::process_title(&process.name, &process.display_name),
            );
            p.text(
                &components::process_title(&process.name, &process.display_name),
                &TextStyle::new(12.5).weight(Weight::Semibold),
                theme.text,
                RectF::new(row.x + 48.0, row.y + 5.0, row.w - 116.0, 18.0),
            );
            p.text(
                &format!("PID {}", process.pid),
                &TextStyle::new(9.5),
                theme.text_tertiary,
                RectF::new(row.x + 48.0, row.y + 23.0, row.w - 116.0, 14.0),
            );
            p.text(
                &format_percent(process.cpu),
                &TextStyle::new(11.5).weight(Weight::Semibold).tabular().align(TextAlign::Trailing),
                if is_selected { theme.text } else { theme.text_secondary },
                RectF::new(row.right() - 66.0, row.y + 7.0, 54.0, 16.0),
            );
            p.text(
                &format_bytes(process.memory),
                &TextStyle::new(9.0).tabular().align(TextAlign::Trailing),
                theme.text_tertiary,
                RectF::new(row.right() - 66.0, row.y + 24.0, 54.0, 13.0),
            );
        }
        p.fill_rect(
            RectF::new(card.x + 20.0, self.footer.y - 10.0, card.w - 40.0, 1.0),
            theme.separator.multiply_alpha(0.45),
        );
        self.open.paint(p);
        self.settings.paint(p);
    }

    fn paint_stat(
        &self,
        p: &mut Painter,
        rect: RectF,
        icon: Icon,
        label: &str,
        value: &str,
        detail: &str,
        color: Color,
        progress: Option<f32>,
    ) {
        let theme = p.theme().clone();
        let rect = p.snap_rect(rect);
        p.fill_round_rect(
            rect,
            14.0,
            if theme.is_dark() {
                Color::rgba(1.0, 1.0, 1.0, 0.052)
            } else {
                Color::rgba(1.0, 1.0, 1.0, 0.72)
            },
        );
        p.clip_round_rect(rect, 14.0, |p| {
            p.fill_rect(
                rect,
                Brush::glow(
                    PointF::new(rect.right() - 12.0, rect.y + 12.0),
                    rect.w * 0.86,
                    color.multiply_alpha(if theme.is_dark() { 0.14 } else { 0.09 }),
                ),
            );
            p.fill_rect(
                rect,
                Brush::vertical(
                    rect,
                    color.multiply_alpha(if theme.is_dark() { 0.075 } else { 0.045 }),
                    Color::TRANSPARENT,
                ),
            );
        });
        p.hairline_round_rect(rect, 14.0, color.multiply_alpha(if theme.is_dark() { 0.24 } else { 0.18 }), true);
        p.icon(icon, PointF::new(rect.x + 13.0, rect.y + 14.0), 12.0, color);
        p.text(
            label,
            &TextStyle::new(9.0).weight(Weight::Semibold),
            theme.text_tertiary,
            RectF::new(rect.x + 23.0, rect.y + 7.0, rect.w - 30.0, 14.0),
        );
        p.text(
            value,
            &TextStyle::new(19.5).weight(Weight::Semibold).tabular(),
            theme.text,
            RectF::new(rect.x + 12.0, rect.y + 26.0, rect.w - 24.0, 27.0),
        );
        p.text(
            detail,
            &TextStyle::new(9.0),
            theme.text_tertiary,
            RectF::new(rect.x + 12.0, rect.y + 51.0, rect.w - 24.0, 13.0),
        );
        if let Some(fraction) = progress {
            components::progress(p, RectF::new(rect.x + 12.0, rect.bottom() - 12.0, rect.w - 24.0, 3.0), fraction, color);
        }
    }
}
