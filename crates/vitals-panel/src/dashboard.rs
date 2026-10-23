//! Main dashboard: sidebar, process table, performance charts and startup list.

use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use vitals_core::{
    compact_path, format_bytes, format_count, format_percent, format_rate, format_uptime, group_processes, now_ms,
    Priority, ProcessGroup, ProcessRow, ProcessState, Settings, Snapshot, SortKey,
};
use vitals_ui::widgets::{
    Button, ButtonStyle, IconButton, Interaction, Menu, MenuItem, Response, ScrollView, TextField,
};
use vitals_ui::{
    Animated, Brush, Color, Ctx, Cursor, Event, Gfx, HitArea, Icon, Key, Modifiers, Painter, PointF, RectF, SizeF,
    TextAlign, TextStyle, View, Weight,
};

use crate::components;
use crate::intent::{PanelIntent, StartupRow, Tab};

const SIDEBAR_W: f32 = 224.0;
const TITLEBAR_H: f32 = 48.0;
const CONTENT_PAD: f32 = 30.0;
const BODY_TOP: f32 = 140.0;
const SUMMARY_H: f32 = 104.0;
const TOOLBAR_H: f32 = 36.0;
const TABLE_HEADER_H: f32 = 30.0;
const ACTION_H: f32 = 50.0;
const ROW_H: f32 = 48.0;
const CHILD_H: f32 = 36.0;
const STARTUP_H: f32 = 58.0;
const SCROLL_TOKEN: u64 = 1;
const STARTUP_SCROLL_TOKEN: u64 = 2;
const TOAST_TOKEN: u64 = 3;
const CARD_RADIUS: f32 = 18.0;

#[derive(Clone, Debug, Default)]
struct Layout {
    title: RectF,
    subtitle: RectF,
    status_chip: RectF,
    summary: [RectF; 3],
    group_button: RectF,
    refresh: RectF,
    pause: RectF,
    more: RectF,
    table_card: RectF,
    table_header: RectF,
    columns: Columns,
    header_hits: [RectF; 4],
    list: RectF,
    action_bar: RectF,
    action_end: RectF,
    action_suspend: RectF,
    action_priority: RectF,
    action_reveal: RectF,
    footer: RectF,
    sidebar: [RectF; 3],
    settings_row: RectF,
    quick_card: RectF,
    performance_top: RectF,
    performance_memory: RectF,
    performance_consumers: RectF,
    startup_bar: RectF,
    startup_refresh: RectF,
    startup_settings: RectF,
    startup_list: RectF,
}

#[derive(Clone, Debug, Default)]
struct Columns {
    name: RectF,
    cpu: RectF,
    memory: RectF,
    disk: RectF,
    status: RectF,
}

#[derive(Clone, Debug)]
struct TableRow {
    key: String,
    title: String,
    subtitle: String,
    path: Option<PathBuf>,
    cpu: f32,
    memory: u64,
    disk: f64,
    status: ProcessState,
    priority: Priority,
    pids: Vec<u32>,
    count: usize,
    expandable: bool,
    expanded: bool,
    child: bool,
    system: bool,
    elevated: bool,
}

impl TableRow {
    fn height(&self) -> f32 {
        if self.child { CHILD_H } else { ROW_H }
    }
}

#[derive(Clone, Debug)]
struct ConfirmState {
    title: String,
    body: String,
    confirm_label: String,
    pids: Vec<u32>,
    name: String,
    cancel: Button,
    confirm: Button,
    rect: RectF,
}

impl ConfirmState {
    fn new(name: &str, pids: Vec<u32>) -> Self {
        let count = pids.len();
        Self {
            title: if count == 1 { format!("End \"{name}\"?") } else { format!("End {count} tasks?") },
            body: if count == 1 {
                "This process will be ended immediately. Unsaved work may be lost.".into()
            } else {
                format!("{count} processes for \"{name}\" will be ended immediately. Unsaved work may be lost.")
            },
            confirm_label: if count == 1 { "End task".into() } else { format!("End {count} tasks") },
            pids,
            name: name.into(),
            cancel: Button::new("Cancel", ButtonStyle::Secondary),
            confirm: Button::new("End task", ButtonStyle::Destructive),
            rect: RectF::default(),
        }
    }
}

#[derive(Clone, Debug)]
enum MenuKind {
    Row(String),
    More,
    Priority,
}

#[derive(Clone, Debug)]
struct OpenMenu {
    kind: MenuKind,
    menu: Menu,
    anchor: PointF,
}

#[derive(Clone, Debug)]
struct Notice {
    text: String,
    until_ms: i64,
}

pub struct DashboardView {
    tab: Tab,
    snapshot: Snapshot,
    settings: Settings,
    startup: Vec<StartupRow>,
    paused: bool,
    version: String,

    search: TextField,
    scroll: ScrollView,
    startup_scroll: ScrollView,
    group_button: Button,
    refresh: IconButton,
    pause: IconButton,
    more: IconButton,
    end_button: Button,
    suspend_button: Button,
    priority_button: Button,
    reveal_button: IconButton,
    startup_refresh: IconButton,
    startup_settings: Button,
    nav: Vec<Interaction>,
    settings_nav: Interaction,
    quick_card: Interaction,
    sort_hits: Vec<Interaction>,

    layout: Layout,
    groups: Vec<ProcessGroup>,
    rows: Vec<TableRow>,
    rows_dirty: bool,
    last_query: String,
    grouped: bool,
    sort: SortKey,
    descending: bool,
    expanded: HashSet<String>,
    selected: Option<String>,
    pending_pid: Option<u32>,
    pending_scroll: bool,
    hovered_row: Option<usize>,
    startup_hover: Option<usize>,
    startup_toggle_hover: Option<usize>,
    menu: Option<OpenMenu>,
    confirm: Option<ConfirmState>,
    notice: Option<Notice>,
    cpu_value: Animated<f32>,
    memory_value: Animated<f32>,
    process_value: Animated<f32>,
}

impl DashboardView {
    pub fn new(snapshot: Snapshot, settings: Settings, paused: bool) -> Self {
        let mut search = TextField::new("Search processes").icon(Icon::Search);
        search.set_focused(true);
        let mut view = Self {
            tab: Tab::Processes,
            snapshot,
            settings,
            startup: Vec::new(),
            paused,
            version: env!("CARGO_PKG_VERSION").into(),
            search,
            scroll: ScrollView::new(SCROLL_TOKEN),
            startup_scroll: ScrollView::new(STARTUP_SCROLL_TOKEN),
            group_button: Button::new("Group apps", ButtonStyle::Secondary).icon(Icon::Layers),
            refresh: IconButton::new(Icon::Refresh).tooltip("Refresh now", Some("F5")),
            pause: IconButton::new(Icon::Pause).tooltip("Pause sampling", None),
            more: IconButton::new(Icon::More).tooltip("More", None),
            end_button: Button::new("End task", ButtonStyle::PlainDestructive),
            suspend_button: Button::new("Suspend", ButtonStyle::Plain).icon(Icon::Pause),
            priority_button: Button::new("Priority", ButtonStyle::Plain).icon(Icon::Sliders),
            reveal_button: IconButton::new(Icon::Folder).tooltip("Open file location", None),
            startup_refresh: IconButton::new(Icon::Refresh).tooltip("Refresh startup entries", None),
            startup_settings: Button::new("Manage in Settings", ButtonStyle::Secondary).icon(Icon::ExternalLink),
            nav: (0..3).map(|_| Interaction::new()).collect(),
            settings_nav: Interaction::new(),
            quick_card: Interaction::new(),
            sort_hits: (0..4).map(|_| Interaction::new()).collect(),
            layout: Layout::default(),
            groups: Vec::new(),
            rows: Vec::new(),
            rows_dirty: true,
            last_query: String::new(),
            grouped: true,
            sort: SortKey::Cpu,
            descending: true,
            expanded: HashSet::new(),
            selected: None,
            pending_pid: None,
            pending_scroll: false,
            hovered_row: None,
            startup_hover: None,
            startup_toggle_hover: None,
            menu: None,
            confirm: None,
            notice: None,
            cpu_value: Animated::new(0.0),
            memory_value: Animated::new(0.0),
            process_value: Animated::new(0.0),
        };
        view.cpu_value.snap(snapshot_value(&view.snapshot, true));
        view.memory_value.snap(snapshot_value(&view.snapshot, false));
        view.process_value.snap(view.snapshot.system.process_count as f32);
        view.sync_pause_button();
        view
    }

    pub fn tab(&self) -> Tab {
        self.tab
    }

    pub fn set_tab(&mut self, tab: Tab) {
        self.tab = tab;
        self.menu = None;
        self.hovered_row = None;
        self.startup_hover = None;
        self.startup_toggle_hover = None;
    }

    pub fn focus_process(&mut self, pid: u32) {
        self.tab = Tab::Processes;
        self.search.clear();
        self.search.set_focused(true);
        self.pending_pid = Some(pid);
        self.pending_scroll = true;
        self.rows_dirty = true;
    }

    pub fn set_snapshot(&mut self, snapshot: Snapshot) {
        // Counters update once per sample. Snapping avoids keeping a 60 fps animation alive for most of every
        // second, which was the single largest source of idle CPU in the dashboard.
        self.cpu_value.snap(snapshot.system.cpu_percent);
        self.memory_value.snap(snapshot.system.memory_percent());
        self.process_value.snap(snapshot.system.process_count as f32);
        self.snapshot = snapshot;
        self.rows_dirty = true;
    }

    pub fn set_settings(&mut self, settings: Settings) {
        self.settings = settings;
    }

    pub fn set_startup(&mut self, startup: Vec<StartupRow>) {
        self.startup = startup;
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        self.sync_pause_button();
    }

    pub fn show_notice(&mut self, cx: &mut Ctx, text: &str) {
        self.notice = Some(Notice { text: text.into(), until_ms: now_ms() + 4000 });
        cx.set_timer(Duration::from_secs(4), TOAST_TOKEN);
        cx.request_paint();
    }

    fn sync_pause_button(&mut self) {
        self.pause.icon = if self.paused { Icon::Play } else { Icon::Pause };
        self.pause.tooltip = Some(if self.paused { "Resume sampling".into() } else { "Pause sampling".into() });
        self.pause.tint = if self.paused { Some(components::ORANGE) } else { None };
    }

    fn layout(&mut self, _gfx: &Rc<Gfx>, size: SizeF) {
        let bounds = RectF::new(0.0, 0.0, size.w, size.h);
        let content = RectF::new(SIDEBAR_W + CONTENT_PAD, BODY_TOP, (size.w - SIDEBAR_W - 2.0 * CONTENT_PAD).max(320.0), 0.0);
        let title = RectF::new(content.x, 56.0, (content.w - 170.0).max(200.0), 38.0);
        let subtitle = RectF::new(content.x, 96.0, title.w, 20.0);
        let status_chip = RectF::new(bounds.right() - 158.0 - 80.0, 13.0, 80.0, 22.0);
        let footer = RectF::new(content.x, size.h - 24.0, content.w, 16.0);

        let summary_w = (content.w - 24.0) / 3.0;
        let summary = [
            RectF::new(content.x, BODY_TOP, summary_w, SUMMARY_H),
            RectF::new(content.x + summary_w + 12.0, BODY_TOP, summary_w, SUMMARY_H),
            RectF::new(content.x + 2.0 * (summary_w + 12.0), BODY_TOP, summary_w, SUMMARY_H),
        ];
        let toolbar_y = BODY_TOP + SUMMARY_H + 18.0;
        let toolbar = RectF::new(content.x, toolbar_y, content.w, TOOLBAR_H);
        let search = RectF::new(toolbar.x, toolbar.y, 320.0, TOOLBAR_H);
        let group_button = RectF::new(search.right() + 10.0, toolbar.y, 134.0, TOOLBAR_H);
        let icons_right = toolbar.right();
        let more = RectF::new(icons_right - TOOLBAR_H, toolbar.y, TOOLBAR_H, TOOLBAR_H);
        let pause = RectF::new(more.x - TOOLBAR_H - 6.0, toolbar.y, TOOLBAR_H, TOOLBAR_H);
        let refresh = RectF::new(pause.x - TOOLBAR_H - 6.0, toolbar.y, TOOLBAR_H, TOOLBAR_H);
        let table_card_y = toolbar.bottom() + 12.0;
        let table_header = RectF::new(content.x + 16.0, table_card_y + 12.0, content.w - 32.0, TABLE_HEADER_H);
        let list_bottom = (size.h - 92.0).max(table_header.bottom() + 40.0);
        let list = RectF::new(
            table_header.x,
            table_header.bottom() + 2.0,
            table_header.w,
            (list_bottom - table_header.bottom() - 2.0).max(40.0),
        );
        let table_card = RectF::new(content.x, table_card_y, content.w, (list.bottom() + 10.0 - table_card_y).max(80.0));
        let action_bar = RectF::new(content.x, size.h - 82.0, content.w, ACTION_H);
        let action_end = RectF::new(action_bar.right() - 96.0, action_bar.y + 11.0, 96.0, 28.0);
        let action_suspend = RectF::new(action_end.x - 102.0, action_bar.y + 11.0, 96.0, 28.0);
        let action_priority = RectF::new(action_suspend.x - 150.0, action_bar.y + 11.0, 144.0, 28.0);
        let action_reveal = RectF::new(action_priority.x - 38.0, action_bar.y + 9.0, 32.0, 32.0);

        let columns = columns(table_header, list);

        let nav_top = 102.0;
        let mut sidebar = [RectF::default(); 3];
        for (index, rect) in sidebar.iter_mut().enumerate() {
            *rect = RectF::new(12.0, nav_top + index as f32 * 44.0, SIDEBAR_W - 24.0, 40.0);
        }
        let settings_row = RectF::new(12.0, size.h - 54.0, SIDEBAR_W - 24.0, 38.0);
        let quick_card = RectF::new(14.0, size.h - 158.0, SIDEBAR_W - 28.0, 90.0);

        let body_bottom = size.h - 44.0;
        let perf_top = RectF::new(content.x, BODY_TOP, content.w, 276.0);
        let perf_bottom_y = perf_top.bottom() + 18.0;
        let perf_bottom_h = (body_bottom - perf_bottom_y).max(160.0);
        let memory_w = ((content.w - 16.0) * 0.58).round();
        let performance_memory = RectF::new(content.x, perf_bottom_y, memory_w, perf_bottom_h);
        let performance_consumers =
            RectF::new(content.x + memory_w + 16.0, perf_bottom_y, content.w - memory_w - 16.0, perf_bottom_h);

        let startup_bar = RectF::new(content.x, BODY_TOP, content.w, 36.0);
        let startup_settings = RectF::new(startup_bar.right() - 174.0, startup_bar.y, 174.0, 30.0);
        let startup_refresh = RectF::new(startup_settings.x - 38.0, startup_bar.y - 1.0, 32.0, 32.0);
        let startup_list = RectF::new(content.x, startup_bar.bottom() + 14.0, content.w, (body_bottom - startup_bar.bottom() - 14.0).max(60.0));

        self.layout = Layout {
            title,
            subtitle,
            status_chip,
            summary,
            group_button,
            refresh,
            pause,
            more,
            table_card,
            table_header,
            columns: columns.clone(),
            header_hits: [
                RectF::new(columns.name.x, table_header.y, columns.name.w, table_header.h),
                RectF::new(columns.cpu.x - 12.0, table_header.y, columns.cpu.w + 12.0, table_header.h),
                RectF::new(columns.memory.x - 12.0, table_header.y, columns.memory.w + 12.0, table_header.h),
                RectF::new(columns.disk.x - 12.0, table_header.y, columns.disk.w + 12.0, table_header.h),
            ],
            list,
            action_bar,
            action_end,
            action_suspend,
            action_priority,
            action_reveal,
            footer,
            sidebar,
            settings_row,
            quick_card,
            performance_top: perf_top,
            performance_memory,
            performance_consumers,
            startup_bar,
            startup_refresh,
            startup_settings,
            startup_list,
        };

        self.search.set_rect(search);
        self.group_button.set_rect(group_button);
        self.refresh.set_rect(refresh);
        self.pause.set_rect(pause);
        self.more.set_rect(more);
        self.end_button.set_rect(action_end);
        self.suspend_button.set_rect(action_suspend);
        self.priority_button.set_rect(action_priority);
        self.reveal_button.set_rect(action_reveal);
        self.startup_refresh.set_rect(startup_refresh);
        self.startup_settings.set_rect(startup_settings);
        self.scroll.set_viewport(list);
        self.startup_scroll.set_viewport(startup_list);
        self.startup_scroll.set_content_height(self.startup.len() as f32 * STARTUP_H + 8.0);
        self.group_button.label = if self.grouped { "Grouped" } else { "Group apps" }.into();
        self.action_buttons_visible(self.selected.is_some() && self.tab == Tab::Processes);
    }

    fn action_buttons_visible(&mut self, visible: bool) {
        if !visible {
            self.end_button.set_rect(RectF::default());
            self.suspend_button.set_rect(RectF::default());
            self.priority_button.set_rect(RectF::default());
            self.reveal_button.set_rect(RectF::default());
        } else {
            self.end_button.set_rect(self.layout.action_end);
            self.suspend_button.set_rect(self.layout.action_suspend);
            self.priority_button.set_rect(self.layout.action_priority);
            self.reveal_button.set_rect(self.layout.action_reveal);
        }
    }

    fn ensure_rows(&mut self) {
        if self.tab != Tab::Processes {
            return;
        }
        let query = self.search.text().trim().to_lowercase();
        if query != self.last_query {
            self.last_query = query;
            self.rows_dirty = true;
        }
        if !self.rows_dirty {
            self.sync_action_bar();
            return;
        }
        self.rows_dirty = false;
        let query = self.last_query.clone();
        self.rebuild_rows(&query);
        self.scroll.set_content_height(self.rows.iter().map(TableRow::height).sum::<f32>() + 12.0);

        if let Some(pid) = self.pending_pid.take() {
            if let Some(index) = self.rows.iter().position(|row| row.pids.contains(&pid)) {
                self.selected = Some(self.rows[index].key.clone());
                if self.pending_scroll {
                    self.pending_scroll = false;
                    let top = self.row_top(index);
                    let center = top + self.rows[index].height() / 2.0 - self.layout.list.h / 2.0;
                    self.scroll.snap_to(center.max(0.0));
                }
            }
        }
        if let Some(selected) = &self.selected
            && !self.rows.iter().any(|row| &row.key == selected)
        {
            self.selected = None;
        }
        self.sync_action_bar();
    }

    fn sync_action_bar(&mut self) {
        let Some(row) = self.selected_row().cloned() else {
            self.action_buttons_visible(false);
            return;
        };
        let endable = row.pids.iter().filter(|pid| self.snapshot.process(**pid).is_some_and(|p| p.endable())).count();
        self.end_button.enabled = endable > 0;
        self.end_button.label = match endable {
            0 | 1 => "End task".into(),
            count => format!("End {count} tasks"),
        };
        let resumable = row.status == ProcessState::Suspended;
        self.suspend_button.label = if resumable { "Resume" } else { "Suspend" }.into();
        self.suspend_button.icon = Some(if resumable { Icon::Play } else { Icon::Pause });
        self.suspend_button.enabled = row.pids.iter().any(|pid| self.snapshot.process(*pid).is_some_and(|p| !p.system));
        self.priority_button.enabled = row.pids.len() == 1 && !row.system;
        self.priority_button.label = if self.priority_button.enabled {
            format!("Priority - {}", row.priority.label())
        } else {
            "Priority".into()
        };
        self.reveal_button.enabled = row.path.as_ref().is_some_and(|path| path.is_file());
        self.action_buttons_visible(self.tab == Tab::Processes);
    }

    fn rebuild_rows(&mut self, query: &str) {
        self.rows.clear();
        self.groups.clear();
        if !self.grouped {
            let mut indices: Vec<usize> = self
                .snapshot
                .processes
                .iter()
                .enumerate()
                .filter(|(_, process)| process.matches(query))
                .map(|(index, _)| index)
                .collect();
            sort_process_indices(&mut indices, &self.snapshot.processes, self.sort, self.descending);
            for index in indices {
                let process = &self.snapshot.processes[index];
                self.rows.push(TableRow {
                    key: format!("pid:{}", process.pid),
                    title: components::process_title(&process.name, &process.display_name),
                    subtitle: format!("PID {} - {} threads", process.pid, process.threads),
                    path: process.path.clone(),
                    cpu: process.cpu,
                    memory: process.memory,
                    disk: process.disk_bps(),
                    status: process.state,
                    priority: process.priority,
                    pids: vec![process.pid],
                    count: 1,
                    expandable: false,
                    expanded: false,
                    child: false,
                    system: process.system,
                    elevated: process.elevated,
                });
            }
            return;
        }

        let mut groups = group_processes(&self.snapshot.processes, self.sort, self.descending);
        groups.sort_by(|a, b| {
            let order = match self.sort {
                SortKey::Cpu => a.cpu.total_cmp(&b.cpu),
                SortKey::Memory => a.memory.cmp(&b.memory),
                SortKey::Disk => a.disk_bps.total_cmp(&b.disk_bps),
                SortKey::Pid => a.indices.first().cmp(&b.indices.first()),
                SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            };
            let order = if self.descending { order.reverse() } else { order };
            order.then_with(|| a.name.cmp(&b.name))
        });

        for group in &groups {
            let group_matches = query.is_empty() || group.name.to_lowercase().contains(query);
            let matching_children: Vec<usize> = group
                .indices
                .iter()
                .copied()
                .filter(|&index| query.is_empty() || group_matches || self.snapshot.processes[index].matches(query))
                .collect();
            if matching_children.is_empty() {
                continue;
            }
            self.groups.push(group.clone());
            let processes: Vec<&ProcessRow> = group.indices.iter().map(|&index| &self.snapshot.processes[index]).collect();
            let status = if processes.iter().any(|p| p.state == ProcessState::Suspended) {
                ProcessState::Suspended
            } else if processes.iter().any(|p| p.state == ProcessState::NotResponding) {
                ProcessState::NotResponding
            } else {
                ProcessState::Running
            };
            let first = processes[0];
            let key = format!("group:{}", group.key);
            let expanded = self.expanded.contains(&key);
            let count = processes.len();
            let subtitle = if count == 1 {
                format!("PID {} - {} threads", first.pid, first.threads)
            } else if matching_children.len() < count {
                format!("{} of {} processes", matching_children.len(), count)
            } else {
                format!("{count} processes")
            };
            self.rows.push(TableRow {
                key: key.clone(),
                title: group.name.clone(),
                subtitle,
                path: group.path.clone(),
                cpu: group.cpu,
                memory: group.memory,
                disk: group.disk_bps,
                status,
                priority: if processes.iter().all(|p| p.priority == first.priority) { first.priority } else { Priority::Unknown },
                pids: processes.iter().map(|p| p.pid).collect(),
                count,
                expandable: count > 1,
                expanded,
                child: false,
                system: processes.iter().all(|p| p.system),
                elevated: processes.iter().any(|p| p.elevated),
            });
            if expanded && count > 1 {
                for &index in &matching_children {
                    let process = &self.snapshot.processes[index];
                    self.rows.push(TableRow {
                        key: format!("pid:{}", process.pid),
                        title: components::process_title(&process.name, &process.display_name),
                        subtitle: format!("PID {}", process.pid),
                        path: process.path.clone(),
                        cpu: process.cpu,
                        memory: process.memory,
                        disk: process.disk_bps(),
                        status: process.state,
                        priority: process.priority,
                        pids: vec![process.pid],
                        count: 1,
                        expandable: false,
                        expanded: false,
                        child: true,
                        system: process.system,
                        elevated: process.elevated,
                    });
                }
            }
        }
    }

    fn selected_row(&self) -> Option<&TableRow> {
        let key = self.selected.as_ref()?;
        self.rows.iter().find(|row| &row.key == key)
    }

    fn row_top(&self, index: usize) -> f32 {
        6.0 + self.rows.iter().take(index).map(TableRow::height).sum::<f32>()
    }

    fn row_at(&self, content_pos: PointF) -> Option<usize> {
        let mut y = 6.0;
        for (index, row) in self.rows.iter().enumerate() {
            let height = row.height();
            if content_pos.y >= y && content_pos.y < y + height {
                return Some(index);
            }
            y += height;
        }
        None
    }

    fn startup_row_at(&self, content_pos: PointF) -> Option<usize> {
        let index = ((content_pos.y - 4.0) / STARTUP_H).floor();
        (index >= 0.0 && (index as usize) < self.startup.len()).then_some(index as usize)
    }

    fn startup_toggle_at(&self, content_pos: PointF) -> Option<usize> {
        let index = self.startup_row_at(content_pos)?;
        let row_y = 4.0 + index as f32 * STARTUP_H;
        let local = PointF::new(content_pos.x, content_pos.y - row_y);
        RectF::new(self.layout.startup_list.w - 120.0, 8.0, 50.0, 44.0)
            .contains(local)
            .then_some(index)
    }

    fn startup_folder_at(&self, content_pos: PointF) -> Option<usize> {
        let index = self.startup_row_at(content_pos)?;
        let row_y = 4.0 + index as f32 * STARTUP_H;
        let local = PointF::new(content_pos.x, content_pos.y - row_y);
        RectF::new(self.layout.startup_list.w - 68.0, 2.0, 38.0, STARTUP_H - 4.0)
            .contains(local)
            .then_some(index)
    }

    fn toggle_expanded(&mut self, key: &str) {
        if !self.expanded.remove(key) {
            self.expanded.insert(key.to_string());
        }
        self.rows_dirty = true;
    }

    fn select_row(&mut self, index: usize, cx: &mut Ctx) {
        let Some(row) = self.rows.get(index) else { return };
        self.selected = Some(row.key.clone());
        self.sync_action_bar();
        cx.request_paint();
    }

    fn set_sort(&mut self, sort: SortKey) {
        if self.sort == sort {
            self.descending = !self.descending;
        } else {
            self.sort = sort;
            self.descending = sort != SortKey::Name;
        }
        self.rows_dirty = true;
    }

    fn request_end(&mut self, cx: &mut Ctx) {
        let Some(row) = self.selected_row().cloned() else { return };
        let pids: Vec<u32> =
            row.pids.iter().copied().filter(|pid| self.snapshot.process(*pid).is_some_and(|p| p.endable())).collect();
        if pids.is_empty() {
            self.show_notice(cx, "That process is protected by Windows.");
            return;
        }
        if self.settings.confirm_end_task {
            self.confirm = Some(ConfirmState::new(&row.title, pids));
            self.layout_confirm(cx.size());
            cx.request_paint();
        } else {
            cx.post(PanelIntent::EndTask { pids, name: row.title });
        }
    }

    fn request_suspend(&mut self, cx: &mut Ctx) {
        let Some(row) = self.selected_row().cloned() else { return };
        let pids: Vec<u32> =
            row.pids.iter().copied().filter(|pid| self.snapshot.process(*pid).is_some_and(|p| !p.system)).collect();
        if pids.is_empty() {
            self.show_notice(cx, "Windows processes cannot be suspended.");
            return;
        }
        if row.status == ProcessState::Suspended {
            cx.post(PanelIntent::Resume { pids, name: row.title });
        } else {
            cx.post(PanelIntent::Suspend { pids, name: row.title });
        }
    }

    fn layout_confirm(&mut self, size: SizeF) {
        let Some(confirm) = &mut self.confirm else { return };
        let card = RectF::new((size.w - 448.0) / 2.0, (size.h - 224.0) / 2.0, 448.0, 224.0);
        confirm.rect = card;
        confirm.cancel.set_rect(RectF::new(card.right() - 104.0 - 100.0, card.bottom() - 48.0, 100.0, 28.0));
        confirm.confirm.set_rect(RectF::new(card.right() - 104.0, card.bottom() - 48.0, 104.0, 28.0));
        confirm.confirm.label = confirm.confirm_label.clone();
    }

    fn show_more_menu(&mut self, cx: &mut Ctx) {
        let mut items = vec![
            MenuItem::new("Refresh now").icon(Icon::Refresh).shortcut("F5"),
            if self.paused {
                MenuItem::new("Resume sampling").icon(Icon::Play)
            } else {
                MenuItem::new("Pause sampling").icon(Icon::Pause)
            },
            MenuItem::new("Group processes by app").icon(Icon::Layers).checked(self.grouped),
            MenuItem::separator(),
            MenuItem::new("Open Windows Task Manager").icon(Icon::Gauge),
            MenuItem::new("Settings...").icon(Icon::Settings).shortcut("Ctrl+,"),
            MenuItem::separator(),
            MenuItem::new("Quit Vitals").icon(Icon::Power),
        ];
        if self.paused {
            items[1].checked = true;
        }
        let anchor = self.layout.more;
        let mut menu = Menu::new(items).opaque();
        menu.force_open(cx.gfx(), anchor, cx.size(), None);
        self.menu = Some(OpenMenu { kind: MenuKind::More, menu, anchor: PointF::new(anchor.right(), anchor.bottom()) });
        cx.request_paint();
    }

    fn show_row_menu(&mut self, cx: &mut Ctx, row_key: String, at: PointF) {
        let Some(row) = self.rows.iter().find(|row| row.key == row_key).cloned() else { return };
        let suspend_label = if row.status == ProcessState::Suspended { "Resume" } else { "Suspend" };
        let items = vec![
            MenuItem::new("End task")
                .icon(Icon::X)
                .destructive()
                .disabled_if(row.pids.iter().all(|pid| !self.snapshot.process(*pid).is_some_and(|p| p.endable()))),
            MenuItem::new(suspend_label).icon(if row.status == ProcessState::Suspended { Icon::Play } else { Icon::Pause }),
            MenuItem::new("Priority").icon(Icon::Sliders).disabled_if(row.pids.len() != 1 || row.system),
            MenuItem::separator(),
            MenuItem::new("Open file location").icon(Icon::Folder).disabled_if(row.path.is_none()),
            MenuItem::new("Copy path").icon(Icon::Copy).disabled_if(row.path.is_none()),
        ];
        let mut menu = Menu::new(items).opaque();
        menu.force_open(cx.gfx(), RectF::new(at.x, at.y, 1.0, 1.0), cx.size(), None);
        self.menu = Some(OpenMenu { kind: MenuKind::Row(row_key), menu, anchor: at });
        cx.request_paint();
    }

    fn show_priority_menu(&mut self, cx: &mut Ctx, anchor: PointF) {
        let Some(row) = self.selected_row().cloned() else { return };
        let items = Priority::ALL
            .iter()
            .map(|priority| MenuItem::new(priority.label()).checked(row.priority == *priority))
            .collect::<Vec<_>>();
        let mut menu = Menu::new(items).opaque();
        menu.force_open(cx.gfx(), RectF::new(anchor.x, anchor.y, 1.0, 1.0), cx.size(), None);
        self.menu = Some(OpenMenu { kind: MenuKind::Priority, menu, anchor });
        cx.request_paint();
    }

    fn handle_menu(&mut self, cx: &mut Ctx, index: usize) {
        let Some(open) = self.menu.take() else { return };
        let Some(item) = open.menu.items().get(index).cloned() else { return };
        let selected = self.selected_row().cloned();
        match open.kind {
            MenuKind::Row(key) => {
                self.selected = Some(key.clone());
                match item.label.as_str() {
                    "End task" => self.request_end(cx),
                    "Suspend" => self.request_suspend(cx),
                    "Resume" => self.request_suspend(cx),
                    "Priority" => self.show_priority_menu(cx, open.anchor),
                    "Open file location" => {
                        if let Some(path) = self.rows.iter().find(|row| row.key == key).and_then(|row| row.path.clone()) {
                            cx.post(PanelIntent::OpenLocation(path));
                        }
                    }
                    "Copy path" => {
                        if let Some(path) = self.rows.iter().find(|row| row.key == key).and_then(|row| row.path.clone()) {
                            cx.post(PanelIntent::CopyText(path.display().to_string()));
                        }
                    }
                    _ => {}
                }
            }
            MenuKind::Priority => {
                if let Some(row) = selected {
                    if let Some(priority) = Priority::ALL.iter().find(|p| p.label() == item.label).copied() {
                        cx.post(PanelIntent::SetPriority { pids: row.pids, priority, name: row.title });
                    }
                }
            }
            MenuKind::More => match item.label.as_str() {
                "Refresh now" => cx.post(PanelIntent::Refresh),
                "Pause sampling" => cx.post(PanelIntent::SetPaused(true)),
                "Resume sampling" => cx.post(PanelIntent::SetPaused(false)),
                "Group processes by app" => {
                    self.grouped = !self.grouped;
                    self.rows_dirty = true;
                }
                "Open Windows Task Manager" => cx.post(PanelIntent::OpenTaskManager),
                "Settings..." => cx.post(PanelIntent::ShowSettings),
                "Quit Vitals" => cx.post(PanelIntent::Quit),
                _ => {}
            },
        }
        cx.request_paint();
    }

    fn move_selection(&mut self, cx: &mut Ctx, delta: i32) {
        if self.rows.is_empty() {
            return;
        }
        let current = self.selected.as_ref().and_then(|key| self.rows.iter().position(|row| &row.key == key));
        let next = match (current, delta.signum()) {
            (Some(index), 1) => (index + 1).min(self.rows.len() - 1),
            (Some(index), -1) => index.saturating_sub(1),
            (None, 1) => 0,
            (None, -1) => self.rows.len() - 1,
            _ => return,
        };
        self.selected = Some(self.rows[next].key.clone());
        let top = self.row_top(next);
        self.scroll.reveal(cx, top, top + self.rows[next].height(), 12.0);
        self.sync_action_bar();
        cx.request_paint();
    }

    fn handle_key(&mut self, cx: &mut Ctx, key: &vitals_ui::KeyEvent) -> bool {
        if key.mods.ctrl {
            match key.key {
                Key::Char('1') => {
                    self.set_tab(Tab::Processes);
                    cx.request_paint();
                    return true;
                }
                Key::Char('2') => {
                    self.set_tab(Tab::Performance);
                    cx.request_paint();
                    return true;
                }
                Key::Char('3') => {
                    self.set_tab(Tab::Startup);
                    cx.request_paint();
                    return true;
                }
                Key::Char('F') => {
                    self.search.set_focused(true);
                    self.search.select_all();
                    cx.request_paint();
                    return true;
                }
                Key::Char(',') => {
                    cx.post(PanelIntent::ShowSettings);
                    return true;
                }
                Key::Char('R') => {
                    cx.post(PanelIntent::Refresh);
                    return true;
                }
                Key::Char('W') => {
                    self.request_close(cx);
                    return true;
                }
                _ => {}
            }
        }
        match key.key {
            Key::Escape => {
                if !self.search.is_empty() {
                    self.search.clear();
                    self.rows_dirty = true;
                    cx.request_paint();
                } else if self.selected.is_some() {
                    self.selected = None;
                    self.sync_action_bar();
                    self.action_buttons_visible(false);
                    cx.request_paint();
                } else {
                    self.request_close(cx);
                }
                true
            }
            Key::F(5) => {
                cx.post(PanelIntent::Refresh);
                true
            }
            Key::Down => {
                self.move_selection(cx, 1);
                true
            }
            Key::Up => {
                self.move_selection(cx, -1);
                true
            }
            Key::Right if self.search.is_empty() => {
                if let Some(row) = self.selected_row()
                    && row.expandable
                    && !row.expanded
                {
                    let key = row.key.clone();
                    self.toggle_expanded(&key);
                    cx.request_paint();
                }
                true
            }
            Key::Left if self.search.is_empty() => {
                if let Some(row) = self.selected_row()
                    && row.expandable
                    && row.expanded
                {
                    let key = row.key.clone();
                    self.toggle_expanded(&key);
                    cx.request_paint();
                } else if self.selected.is_some() {
                    self.selected = None;
                    self.sync_action_bar();
                    self.action_buttons_visible(false);
                    cx.request_paint();
                }
                true
            }
            _ => false,
        }
    }

    fn request_close(&mut self, cx: &mut Ctx) {
        if self.settings.close_to_tray {
            cx.post(PanelIntent::HideDashboard);
        } else {
            cx.post(PanelIntent::Quit);
        }
    }

    fn handle_timer(&mut self, cx: &mut Ctx, token: u64) -> bool {
        if token == TOAST_TOKEN {
            if let Some(notice) = &self.notice {
                let remaining = notice.until_ms - now_ms();
                if remaining > 0 {
                    cx.set_timer(Duration::from_millis(remaining as u64), TOAST_TOKEN);
                } else {
                    self.notice = None;
                    cx.request_paint();
                }
            }
            return true;
        }
        false
    }
}

fn columns(header: RectF, list: RectF) -> Columns {
    let status = RectF::new(header.right() - 126.0, header.y, 126.0, header.h);
    let disk = RectF::new(status.x - 92.0, header.y, 92.0, header.h);
    let memory = RectF::new(disk.x - 94.0, header.y, 94.0, header.h);
    let cpu = RectF::new(memory.x - 66.0, header.y, 66.0, header.h);
    let name = RectF::new(header.x, header.y, (cpu.x - header.x - 8.0).max(120.0), header.h);
    Columns {
        name: RectF::new(name.x, list.y, name.w, list.h),
        cpu: RectF::new(cpu.x, list.y, cpu.w, list.h),
        memory: RectF::new(memory.x, list.y, memory.w, list.h),
        disk: RectF::new(disk.x, list.y, disk.w, list.h),
        status: RectF::new(status.x, list.y, status.w, list.h),
    }
}

fn snapshot_value(snapshot: &Snapshot, cpu: bool) -> f32 {
    if cpu { snapshot.system.cpu_percent } else { snapshot.system.memory_percent() }
}

fn sort_process_indices(indices: &mut [usize], processes: &[ProcessRow], sort: SortKey, descending: bool) {
    indices.sort_by(|&a, &b| {
        let (a, b) = (&processes[a], &processes[b]);
        let order = match sort {
            SortKey::Cpu => a.cpu.total_cmp(&b.cpu),
            SortKey::Memory => a.memory.cmp(&b.memory),
            SortKey::Disk => a.disk_bps().total_cmp(&b.disk_bps()),
            SortKey::Pid => a.pid.cmp(&b.pid),
            SortKey::Name => components::process_title(&a.name, &a.display_name)
                .to_lowercase()
                .cmp(&components::process_title(&b.name, &b.display_name).to_lowercase()),
        };
        let order = if descending { order.reverse() } else { order };
        order.then_with(|| a.pid.cmp(&b.pid))
    });
}

// The rest of the view: events and painting.
impl View for DashboardView {
    fn event(&mut self, cx: &mut Ctx, event: &Event) -> bool {
        self.layout(cx.gfx(), cx.size());
        self.ensure_rows();
        self.layout_confirm(cx.size());
        let hover_before = (self.hovered_row, self.startup_hover, self.startup_toggle_hover);
        // Clicks always get a frame, even when a handler posts an intent whose result arrives later. Pointer movement
        // still only repaints when the hovered row changes; a click is one extra frame, not a redraw storm.
        if matches!(event, Event::PointerDown(_) | Event::PointerUp(_)) {
            cx.request_paint();
        }

        if self.confirm.is_some() {
            let mut result = false;
            if let Event::KeyDown(key) = event {
                match key.key {
                    Key::Escape => {
                        self.confirm = None;
                        result = true;
                    }
                    Key::Enter => {
                        if let Some(confirm) = self.confirm.take() {
                            cx.post(PanelIntent::EndTask { pids: confirm.pids, name: confirm.name });
                        }
                        result = true;
                    }
                    _ => {}
                }
            }
            if !result {
                let mut confirm_action = false;
                let mut cancel = false;
                if let Some(confirm) = &mut self.confirm {
                    if confirm.cancel.event(cx, event).action().is_some() {
                        cancel = true;
                    }
                    if confirm.confirm.event(cx, event).action().is_some() {
                        confirm_action = true;
                    }
                }
                if cancel {
                    self.confirm = None;
                    cx.request_paint();
                } else if confirm_action {
                    if let Some(confirm) = self.confirm.take() {
                        cx.post(PanelIntent::EndTask { pids: confirm.pids, name: confirm.name });
                    }
                }
                cx.set_cursor(Cursor::Arrow);
                return true;
            }
            cx.request_paint();
            return true;
        }

        // A menu can close by itself (click outside, Escape, focus loss). If its wrapper stayed in `self.menu`,
        // every later event would keep being swallowed by an invisible menu, which made the whole window dead
        // after a right click. Drop a menu that is no longer open before dispatching anything else.
        if self.menu.as_ref().is_some_and(|open| !open.menu.is_open()) {
            self.menu = None;
            cx.request_paint();
        }
        if let Some(open) = &mut self.menu {
            let response = open.menu.event(cx, event);
            let action = response.action();
            let closed = !open.menu.is_open();
            cx.set_cursor(Cursor::Arrow);
            if let Some(index) = action {
                self.handle_menu(cx, index);
                return true;
            }
            if closed {
                self.menu = None;
                cx.request_paint();
                return true;
            }
            return true;
        }

        if let Event::Timer(token) = event
            && self.handle_timer(cx, *token)
        {
            return true;
        }

        match event {
            Event::CloseRequested => {
                self.request_close(cx);
                return true;
            }
            Event::Focus(focused) => {
                self.search.set_focused(*focused);
                cx.request_paint();
                return false;
            }
            Event::Resized => {
                self.layout(cx.gfx(), cx.size());
                cx.request_paint();
                return true;
            }
            Event::Timer(SCROLL_TOKEN) => {
                self.scroll.event(cx, event);
                return true;
            }
            Event::Timer(STARTUP_SCROLL_TOKEN) => {
                self.startup_scroll.event(cx, event);
                return true;
            }
            Event::Timer(_) => {}
            _ => {}
        }

        if let Event::KeyDown(key) = event {
            if self.handle_key(cx, key) {
                return true;
            }
        }

        if matches!(event, Event::Text(_) | Event::KeyDown(_)) && self.tab == Tab::Processes {
            if !self.search.is_focused() {
                if let Event::Text(_) = event {
                    self.search.set_focused(true);
                }
            }
            if !keyboard_modifiers(event) {
                let response = self.search.event(cx, event);
                if response.consumed() {
                    self.rows_dirty = true;
                    cx.request_paint();
                    return true;
                }
            }
        }

        if self.tab == Tab::Startup {
            if self.startup_scroll.event(cx, event).consumed() {
                return true;
            }
        } else if self.scroll.event(cx, event).consumed() {
            return true;
        }

        let pointer = event.pointer_pos();
        if let Some(pos) = pointer {
            if self.search.rect().contains(pos) {
                if matches!(event, Event::PointerDown(_)) {
                    self.search.set_focused(true);
                    cx.request_paint();
                }
            }
            if self.tab == Tab::Processes {
                if self.layout.list.contains(pos) {
                    let content = self.scroll.to_content(pos);
                    self.hovered_row = self.row_at(content);
                } else {
                    self.hovered_row = None;
                }
            }
            if self.tab == Tab::Startup {
                if self.layout.startup_list.contains(pos) {
                    let content = self.startup_scroll.to_content(pos);
                    self.startup_hover = self.startup_row_at(content);
                    self.startup_toggle_hover = self.startup_toggle_at(content);
                } else {
                    self.startup_hover = None;
                    self.startup_toggle_hover = None;
                }
            }
        }

        // Sidebar.
        for (index, interaction) in self.nav.iter_mut().enumerate() {
            let rect = self.layout.sidebar[index];
            if let Response::Action(()) = interaction.update(event, rect, true) {
                self.set_tab(Tab::ALL[index]);
                self.hovered_row = None;
                cx.request_paint();
                return true;
            }
        }
        if let Response::Action(()) = self.settings_nav.update(event, self.layout.settings_row, true) {
            cx.post(PanelIntent::ShowSettings);
            return true;
        }
        if let Response::Action(()) = self.quick_card.update(event, self.layout.quick_card, true) {
            cx.post(PanelIntent::ShowQuickView);
            return true;
        }

        if self.tab == Tab::Processes {
            if self.group_button.event(cx, event).action().is_some() {
                self.grouped = !self.grouped;
                self.rows_dirty = true;
                cx.request_paint();
                return true;
            }
            if self.refresh.event(cx, event).action().is_some() {
                cx.post(PanelIntent::Refresh);
                return true;
            }
            if self.pause.event(cx, event).action().is_some() {
                cx.post(PanelIntent::SetPaused(!self.paused));
                return true;
            }
            if self.more.event(cx, event).action().is_some() {
                self.show_more_menu(cx);
                return true;
            }

            for (index, hit) in self.sort_hits.iter_mut().enumerate() {
                if let Response::Action(()) = hit.update(event, self.layout.header_hits[index], true) {
                    self.set_sort(match index {
                        0 => SortKey::Name,
                        1 => SortKey::Cpu,
                        2 => SortKey::Memory,
                        _ => SortKey::Disk,
                    });
                    cx.request_paint();
                    return true;
                }
            }

            if self.selected.is_some() {
                if self.end_button.event(cx, event).action().is_some() {
                    self.request_end(cx);
                    return true;
                }
                if self.suspend_button.event(cx, event).action().is_some() {
                    self.request_suspend(cx);
                    return true;
                }
                if self.priority_button.event(cx, event).action().is_some() {
                    let anchor = PointF::new(self.layout.action_priority.x, self.layout.action_priority.bottom());
                    self.show_priority_menu(cx, anchor);
                    return true;
                }
                if self.reveal_button.event(cx, event).action().is_some() {
                    if let Some(path) = self.selected_row().and_then(|row| row.path.clone()) {
                        cx.post(PanelIntent::OpenLocation(path));
                    }
                    return true;
                }
            }

            if let Some(pos) = pointer
                && self.layout.list.contains(pos)
                && let Some(index) = self.row_at(self.scroll.to_content(pos))
            {
                if let Event::PointerDown(e) = event {
                    if e.button == Some(vitals_ui::MouseButton::Right) {
                        let key = self.rows[index].key.clone();
                        self.selected = Some(key.clone());
                        self.sync_action_bar();
                        self.show_row_menu(cx, key, pos);
                        return true;
                    }
                    if e.button == Some(vitals_ui::MouseButton::Left) {
                        let row = self.rows[index].clone();
                        let content = self.scroll.to_content(pos);
                        let chevron = RectF::new(4.0, self.row_top(index) + 8.0, 24.0, row.height() - 16.0);
                        if row.expandable && chevron.contains(content) {
                            self.toggle_expanded(&row.key);
                            cx.request_paint();
                            return true;
                        }
                        self.select_row(index, cx);
                        if e.click_count >= 2 && row.expandable {
                            self.toggle_expanded(&row.key);
                            cx.request_paint();
                        }
                        return true;
                    }
                }
            }
            if let Some(pos) = pointer {
                if self.layout.list.contains(pos) && event.pointer().is_some() {
                    cx.set_cursor(Cursor::Arrow);
                }
            }
            if event.pointer_pos().is_some_and(|pos| {
                self.layout.group_button.contains(pos)
                    || self.layout.refresh.contains(pos)
                    || self.layout.pause.contains(pos)
                    || self.layout.more.contains(pos)
                    || self.layout.action_bar.contains(pos)
                    || self.layout.header_hits.iter().any(|hit| hit.contains(pos))
            }) {
                cx.set_cursor(Cursor::Hand);
            }
        } else if self.tab == Tab::Startup {
            if let Event::PointerDown(e) = event
                && e.button == Some(vitals_ui::MouseButton::Left)
                && self.layout.startup_list.contains(e.pos)
            {
                let content = self.startup_scroll.to_content(e.pos);
                if let Some(index) = self.startup_toggle_at(content) {
                    if let Some(item) = self.startup.get(index) {
                        cx.post(PanelIntent::SetStartupEnabled { id: item.id.clone(), enabled: !item.enabled });
                    }
                    return true;
                }
                if let Some(index) = self.startup_folder_at(content) {
                    if let Some(item) = self.startup.get(index) {
                        cx.post(PanelIntent::OpenStartup { command: item.command.clone() });
                    }
                    return true;
                }
            }
            if self.startup_refresh.event(cx, event).action().is_some() {
                cx.post(PanelIntent::RefreshStartup);
                return true;
            }
            if self.startup_settings.event(cx, event).action().is_some() {
                cx.post(PanelIntent::OpenUrl("ms-settings:startupapps".into()));
                return true;
            }
            if let Some(pos) = event.pointer_pos() {
                let content = self.startup_scroll.to_content(pos);
                let over_row_action = self.layout.startup_list.contains(pos)
                    && (self.startup_toggle_at(content).is_some() || self.startup_folder_at(content).is_some());
                if self.layout.startup_refresh.contains(pos)
                    || self.layout.startup_settings.contains(pos)
                    || over_row_action
                {
                    cx.set_cursor(Cursor::Hand);
                } else if self.layout.startup_list.contains(pos) {
                    cx.set_cursor(Cursor::Arrow);
                }
            }
        }

        if event.pointer_pos().is_some_and(|pos| {
            self.layout.quick_card.contains(pos)
                || self.layout.settings_row.contains(pos)
                || self.layout.sidebar.iter().any(|rect| rect.contains(pos))
        }) {
            cx.set_cursor(Cursor::Hand);
        }
        if let Event::PointerLeave = event {
            self.hovered_row = None;
            self.startup_hover = None;
            self.startup_toggle_hover = None;
        }
        // Pointer moves used to repaint the whole window unconditionally. That turned every mouse move over the
        // dashboard into a 60 fps full-window redraw and made clicks feel unreliable. Only the hovered row needs
        // an immediate repaint; buttons and nav items animate their own hover state through `Interaction`.
        if (self.hovered_row, self.startup_hover, self.startup_toggle_hover) != hover_before {
            cx.request_paint();
        }
        false
    }

    fn paint(&mut self, cx: &mut Ctx, p: &mut Painter) {
        self.layout(cx.gfx(), cx.size());
        self.ensure_rows();
        let size = cx.size();
        let theme = p.theme().clone();
        components::paint_window_background(cx, p, &theme);
        self.paint_sidebar(cx, p);
        self.paint_header(cx, p);
        match self.tab {
            Tab::Processes => self.paint_processes(cx, p),
            Tab::Performance => self.paint_performance(cx, p),
            Tab::Startup => self.paint_startup(cx, p),
        }
        self.paint_footer(cx, p);
        if let Some(mut confirm) = self.confirm.take() {
            self.paint_confirm(p, &mut confirm);
            self.confirm = Some(confirm);
        } else if let Some(open) = &mut self.menu {
            open.menu.paint(p, None);
        }
        self.paint_notice(p);
        let _ = size;
    }

    fn hit_test(&self, pos: PointF) -> HitArea {
        if pos.y < TITLEBAR_H { HitArea::Caption } else { HitArea::Client }
    }
}

impl DashboardView {
    fn paint_sidebar(&mut self, _cx: &mut Ctx, p: &mut Painter) {
        let theme = p.theme().clone();
        let bounds = p.bounds();
        let titlebar = RectF::new(0.0, 0.0, bounds.w, TITLEBAR_H);
        p.fill_rect(titlebar, if theme.is_dark() { Color::rgba(1.0, 1.0, 1.0, 0.018) } else { Color::rgba(1.0, 1.0, 1.0, 0.24) });
        let sidebar = RectF::new(0.0, TITLEBAR_H, SIDEBAR_W, bounds.h - TITLEBAR_H);
        p.clip_rect(sidebar, |p| {
            p.fill_rect(
                sidebar,
                Brush::vertical(
                    sidebar,
                    if theme.is_dark() { Color::rgba(0.03, 0.04, 0.07, 0.26) } else { Color::rgba(1.0, 1.0, 1.0, 0.38) },
                    if theme.is_dark() { Color::rgba(0.01, 0.015, 0.03, 0.34) } else { Color::rgba(1.0, 1.0, 1.0, 0.20) },
                ),
            );
            p.fill_rect(
                sidebar,
                Brush::glow(
                    PointF::new(40.0, 122.0),
                    310.0,
                    theme.accent.multiply_alpha(if theme.is_dark() { 0.10 } else { 0.055 }),
                ),
            );
            p.fill_rect(
                sidebar,
                Brush::glow(
                    PointF::new(12.0, bounds.h * 0.84),
                    260.0,
                    components::PURPLE.multiply_alpha(if theme.is_dark() { 0.055 } else { 0.032 }),
                ),
            );
        });
        p.fill_rect(RectF::new(0.0, TITLEBAR_H - 1.0, bounds.w, 1.0), theme.separator.multiply_alpha(0.5));
        p.fill_rect(RectF::new(SIDEBAR_W - 1.0, TITLEBAR_H, 1.0, bounds.h - TITLEBAR_H), theme.separator.multiply_alpha(0.5));

        components::logo(p, PointF::new(30.0, 24.0), 28.0);
        p.text("Vitals", &TextStyle::new(16.0).weight(Weight::Semibold), theme.text, RectF::new(52.0, 7.0, 120.0, 22.0));
        p.text(
            "Process manager",
            &TextStyle::new(9.5),
            theme.text_tertiary,
            RectF::new(52.0, 25.0, 130.0, 14.0),
        );

        p.text(
            "MONITOR",
            &TextStyle::new(9.5).weight(Weight::Semibold),
            theme.text_tertiary,
            RectF::new(26.0, 74.0, 120.0, 14.0),
        );

        let icons = [Icon::ListTree, Icon::Activity, Icon::Rocket];
        let badges = [
            Some(format_count(self.snapshot.system.process_count)),
            None,
            if self.startup.is_empty() { None } else { Some(self.startup.len().to_string()) },
        ];
        for index in 0..3 {
            let rect = self.layout.sidebar[index];
            let interaction = &self.nav[index];
            let selected = self.tab == Tab::ALL[index];
            if selected {
                p.fill_round_rect(
                    rect,
                    11.0,
                    Brush::vertical(
                        rect,
                        theme.accent.multiply_alpha(if theme.is_dark() { 0.24 } else { 0.17 }),
                        theme.accent.multiply_alpha(if theme.is_dark() { 0.10 } else { 0.07 }),
                    ),
                );
                p.hairline_round_rect(rect, 11.0, theme.accent.multiply_alpha(0.26), true);
                p.fill_round_rect(RectF::new(rect.x + 4.0, rect.center().y - 10.0, 3.0, 20.0), 1.5, theme.accent);
            } else {
                let hover = interaction.hover_amount();
                if hover > 0.001 {
                    p.fill_round_rect(rect, 11.0, theme.hover.with_alpha(theme.hover.a * hover));
                }
            }
            let ink = if selected { theme.text } else { theme.text_secondary };
            let icon_ink = if selected { theme.accent } else { theme.text_secondary };
            p.icon(icons[index], PointF::new(rect.x + 26.0, rect.center().y), 17.0, icon_ink);
            p.text(
                Tab::ALL[index].label(),
                &TextStyle::body().weight(if selected { Weight::Semibold } else { Weight::Regular }),
                ink,
                RectF::new(rect.x + 46.0, rect.y, rect.w - 82.0, rect.h),
            );
            if let Some(badge) = &badges[index] {
                let style = TextStyle::caption().weight(Weight::Medium).tabular();
                let width = (p.measure(badge, &style).w + 14.0).max(24.0);
                let pill = RectF::new(rect.right() - width - 10.0, rect.center().y - 9.0, width, 18.0);
                p.fill_round_rect(
                    pill,
                    9.0,
                    if selected { Color::rgba(1.0, 1.0, 1.0, 0.13) } else { theme.hover },
                );
                p.text(badge, &style.centered(), if selected { theme.text } else { theme.text_secondary }, pill);
            }
        }

        let quick = self.layout.quick_card;
        if let Some(row) = self.selected_row()
            && self.tab == Tab::Processes
        {
            let hint = RectF::new(18.0, quick.y - 26.0, SIDEBAR_W - 36.0, 18.0);
            p.fill_ellipse(PointF::new(hint.x + 3.0, hint.center().y), 2.5, 2.5, theme.accent);
            p.text(
                &format!("{} selected", row.title),
                &TextStyle::new(10.0),
                theme.text_tertiary,
                RectF::new(hint.x + 12.0, hint.y, hint.w - 12.0, hint.h),
            );
        }

        let card_hover = self.quick_card.hover_amount();
        components::card_tinted(p, quick, 16.0, theme.accent, 0.15);
        if card_hover > 0.001 {
            p.clip_round_rect(quick, 16.0, |p| {
                p.fill_rect(quick, Color::rgba(1.0, 1.0, 1.0, 0.05 * card_hover));
            });
        }
        let icon_rect = RectF::new(quick.x + 16.0, quick.y + 16.0, 30.0, 30.0);
        components::icon_tile(p, icon_rect, Icon::Keyboard, theme.accent);
        p.text(
            "Quick view",
            &TextStyle::new(12.5).weight(Weight::Semibold),
            theme.text,
            RectF::new(quick.x + 56.0, quick.y + 13.0, quick.w - 74.0, 18.0),
        );
        p.text(
            "Summon from anywhere",
            &TextStyle::new(9.5),
            theme.text_tertiary,
            RectF::new(quick.x + 56.0, quick.y + 31.0, quick.w - 74.0, 14.0),
        );
        p.icon(
            Icon::ArrowUpRight,
            PointF::new(quick.right() - 19.0, quick.y + 20.0),
            13.0,
            theme.text_tertiary,
        );
        let key_y = quick.y + 60.0;
        components::keycap(p, RectF::new(quick.x + 16.0, key_y, 34.0, 19.0), "Ctrl");
        components::keycap(p, RectF::new(quick.x + 54.0, key_y, 42.0, 19.0), "Shift");
        components::keycap(p, RectF::new(quick.x + 100.0, key_y, 32.0, 19.0), "Esc");

        let settings = self.layout.settings_row;
        let hover = self.settings_nav.hover_amount();
        if hover > 0.001 {
            p.fill_round_rect(settings, 11.0, theme.hover.with_alpha(theme.hover.a * hover));
        }
        p.icon(Icon::Settings, PointF::new(settings.x + 26.0, settings.center().y), 17.0, theme.text_secondary);
        p.text("Settings", &TextStyle::body(), theme.text_secondary, RectF::new(settings.x + 46.0, settings.y, 90.0, settings.h));
        p.text(
            "Ctrl+,",
            &TextStyle::caption().align(TextAlign::Trailing),
            theme.text_tertiary,
            RectF::new(settings.right() - 54.0, settings.y, 44.0, settings.h),
        );
    }

    fn paint_header(&mut self, _cx: &mut Ctx, p: &mut Painter) {
        let theme = p.theme().clone();
        let subtitle = match self.tab {
            Tab::Processes => format!(
                "{} processes \u{00B7} {} threads \u{00B7} {} active",
                format_count(self.snapshot.system.process_count),
                format_count(self.snapshot.system.thread_count as usize),
                self.snapshot.processes.iter().filter(|process| process.cpu > 0.15).count()
            ),
            Tab::Performance => "CPU, memory and the processes behind them".into(),
            Tab::Startup => "What opens when you sign in to Windows".into(),
        };
        p.text(
            self.tab.label(),
            &TextStyle::new(31.0).weight(Weight::Semibold),
            theme.text,
            self.layout.title,
        );
        p.text(&subtitle, &TextStyle::new(12.5), theme.text_secondary, self.layout.subtitle);

        let chip = self.layout.status_chip;
        p.fill_round_rect(chip, 11.0, theme.hover.multiply_alpha(0.72));
        p.hairline_round_rect(chip, 11.0, theme.hairline, true);
        let color = if self.paused { components::ORANGE } else { theme.success };
        let label = if self.paused { "PAUSED" } else { "LIVE" };
        let style = TextStyle::new(9.5).weight(Weight::Semibold);
        let dot_radius = 2.8;
        let dot_gap = 7.0;
        let group_w = dot_radius * 2.0 + dot_gap + p.measure(label, &style).w;
        let group_x = chip.center().x - group_w * 0.5;
        let dot_x = group_x + dot_radius;
        p.fill_ellipse(PointF::new(dot_x, chip.center().y), 5.0, 5.0, color.multiply_alpha(0.16));
        p.fill_ellipse(PointF::new(dot_x, chip.center().y), dot_radius, dot_radius, color);
        p.text(
            label,
            &style,
            if self.paused { components::ORANGE } else { theme.text_secondary },
            RectF::new(dot_x + dot_radius + dot_gap, chip.y, chip.right() - dot_x - dot_radius - dot_gap, chip.h),
        );
    }

    fn paint_processes(&mut self, cx: &mut Ctx, p: &mut Painter) {
        let _ = cx;
        let theme = p.theme().clone();
        let cpu = self.cpu_value.get().max(0.0);
        let memory = self.memory_value.get().max(0.0);
        let process_count = self.process_value.get().max(0.0).round() as usize;
        let active = self.snapshot.processes.iter().filter(|process| process.cpu > 0.15).count();
        let memory_used = self.snapshot.system.memory_used;
        let memory_total = self.snapshot.system.memory_total;
        let detail = format!(
            "{} threads - {} active",
            format_count(self.snapshot.system.thread_count as usize),
            active
        );
        self.paint_summary(
            p,
            self.layout.summary[0],
            Icon::Cpu,
            "CPU",
            &format_percent(cpu),
            "System total",
            theme.accent,
            Some((&self.snapshot.system.cpu_history, 100.0)),
        );
        self.paint_summary(
            p,
            self.layout.summary[1],
            Icon::Memory,
            "Memory",
            &format_bytes(memory_used),
            &format!("of {} - {}%", format_bytes(memory_total), format_percent(memory)),
            components::PURPLE,
            Some((&self.snapshot.system.memory_history, 100.0)),
        );
        let top_cpu: Vec<f32> = {
            let mut values: Vec<f32> = self.snapshot.processes.iter().map(|process| process.cpu).collect();
            values.sort_by(|a, b| b.total_cmp(a));
            values.truncate(7);
            values
        };
        self.paint_summary_bars(
            p,
            self.layout.summary[2],
            Icon::ListTree,
            "Processes",
            &format_count(process_count),
            &detail,
            components::TEAL,
            &top_cpu,
        );

        self.search.paint(p);
        self.group_button.paint(p);
        self.refresh.paint(p);
        self.pause.paint(p);
        self.more.paint(p);
        components::card(p, self.layout.table_card, CARD_RADIUS, false);
        self.paint_table_header(p);
        self.paint_table(p);
        if let Some(row) = self.selected_row().cloned() {
            self.paint_action_bar(p, &row);
        }
    }

    fn paint_summary(
        &self,
        p: &mut Painter,
        rect: RectF,
        icon: Icon,
        label: &str,
        value: &str,
        detail: &str,
        color: Color,
        chart: Option<(&[f32], f32)>,
    ) {
        let theme = p.theme().clone();
        components::card_tinted(p, rect, CARD_RADIUS, color, 0.15);
        components::icon_tile(p, RectF::new(rect.x + 18.0, rect.y + 15.0, 32.0, 32.0), icon, color);
        p.text(
            label,
            &TextStyle::new(11.0).weight(Weight::Semibold),
            theme.text_secondary,
            RectF::new(rect.x + 60.0, rect.y + 16.0, rect.w - 78.0, 18.0),
        );
        p.text(
            value,
            &TextStyle::new(30.0).weight(Weight::Semibold).tabular(),
            theme.text,
            RectF::new(rect.x + 18.0, rect.y + 48.0, rect.w - 36.0, 36.0),
        );
        p.text(
            detail,
            &TextStyle::new(10.5),
            theme.text_tertiary,
            RectF::new(rect.x + 18.0, rect.y + 82.0, rect.w - 36.0, 14.0),
        );
        if let Some((values, max)) = chart {
            let chart_rect = RectF::new(rect.right() - 138.0, rect.y + 46.0, 118.0, 42.0);
            components::sparkline(p, chart_rect, values, max, color);
        }
    }

    fn paint_summary_bars(
        &self,
        p: &mut Painter,
        rect: RectF,
        icon: Icon,
        label: &str,
        value: &str,
        detail: &str,
        color: Color,
        bars: &[f32],
    ) {
        let theme = p.theme().clone();
        components::card_tinted(p, rect, CARD_RADIUS, color, 0.15);
        components::icon_tile(p, RectF::new(rect.x + 18.0, rect.y + 15.0, 32.0, 32.0), icon, color);
        p.text(
            label,
            &TextStyle::new(11.0).weight(Weight::Semibold),
            theme.text_secondary,
            RectF::new(rect.x + 60.0, rect.y + 16.0, rect.w - 78.0, 18.0),
        );
        p.text(
            value,
            &TextStyle::new(30.0).weight(Weight::Semibold).tabular(),
            theme.text,
            RectF::new(rect.x + 18.0, rect.y + 48.0, 150.0, 36.0),
        );
        p.text(
            detail,
            &TextStyle::new(10.5),
            theme.text_tertiary,
            RectF::new(rect.x + 18.0, rect.y + 82.0, rect.w - 36.0, 14.0),
        );
        components::mini_bars(p, RectF::new(rect.right() - 126.0, rect.y + 48.0, 108.0, 38.0), bars, color);
    }

    fn paint_table_header(&mut self, p: &mut Painter) {
        let theme = p.theme().clone();
        let header = self.layout.table_header;
        p.fill_rect(
            RectF::new(header.x, header.bottom() - 1.0, header.w, 1.0),
            theme.separator.multiply_alpha(0.55),
        );
        let labels = [("NAME", self.layout.columns.name, SortKey::Name), ("CPU", self.layout.columns.cpu, SortKey::Cpu), ("MEMORY", self.layout.columns.memory, SortKey::Memory), ("DISK", self.layout.columns.disk, SortKey::Disk)];
        for (index, (label, column, sort)) in labels.iter().enumerate() {
            let hover = self.sort_hits[index].hover_amount();
            let active = self.sort == *sort;
            let ink = if active { theme.text } else { theme.text_tertiary.lerp(&theme.text_secondary, hover * 0.9) };
            let style = TextStyle::new(9.5).weight(if active { Weight::Semibold } else { Weight::Medium });
            if index == 0 {
                // Sit over the app avatars, which start 28 DIP into the name column.
                let width = p.text(label, &style, ink, RectF::new(column.x + 28.0, header.y, column.w - 40.0, header.h));
                if active {
                    p.icon(
                        if self.descending { Icon::ChevronDown } else { Icon::ChevronUp },
                        PointF::new(column.x + 32.0 + width, header.center().y),
                        10.0,
                        theme.accent,
                    );
                }
            } else {
                // Inactive numeric headers share the values' 8 DIP right inset; the active one makes room for its chevron.
                let inset = if active { 14.0 } else { 8.0 };
                p.text(
                    label,
                    &style.align(TextAlign::Trailing),
                    ink,
                    RectF::new(column.x, header.y, column.w - inset, header.h),
                );
                if active {
                    p.icon(
                        if self.descending { Icon::ChevronDown } else { Icon::ChevronUp },
                        PointF::new(column.right() - 6.0, header.center().y),
                        10.0,
                        theme.accent,
                    );
                }
            }
        }
        p.text(
            "STATUS",
            &TextStyle::new(9.5).weight(Weight::Medium),
            theme.text_tertiary,
            // The status label text begins after the state dot, not above the dot itself.
            RectF::new(self.layout.columns.status.x + 21.0, header.y, self.layout.columns.status.w - 29.0, header.h),
        );
    }

    fn paint_table(&mut self, p: &mut Painter) {
        if self.rows.is_empty() {
            let theme = p.theme().clone();
            let list = self.layout.list;
            components::icon_tile(p, RectF::new(list.center().x - 22.0, list.y + 52.0, 44.0, 44.0), Icon::Search, theme.text_tertiary);
            p.text(
                "No matching processes",
                &TextStyle::title(),
                theme.text_secondary,
                RectF::new(list.x, list.y + 112.0, list.w, 22.0),
            );
            p.text(
                "Try a name, PID, publisher, or path.",
                &TextStyle::body(),
                theme.text_tertiary,
                RectF::new(list.x, list.y + 136.0, list.w, 20.0),
            );
            return;
        }
        let left = self.layout.columns.name.x;
        let width = self.layout.columns.status.right() - left;
        let columns = RelativeColumns {
            name: self.layout.columns.name.w,
            cpu: (self.layout.columns.cpu.x - left, self.layout.columns.cpu.w),
            memory: (self.layout.columns.memory.x - left, self.layout.columns.memory.w),
            disk: (self.layout.columns.disk.x - left, self.layout.columns.disk.w),
            status: (self.layout.columns.status.x - left, self.layout.columns.status.w),
        };
        let max_cpu = self.rows.iter().map(|row| row.cpu).fold(0.0f32, f32::max).max(4.0);
        let hovered = self.hovered_row;
        let selected = self.selected.as_deref();
        let rows = &self.rows;
        let paint_rows = |p: &mut Painter| {
            let mut y = 6.0;
            for (index, row) in rows.iter().enumerate() {
                paint_table_row(p, row, y, width, &columns, index == hovered.unwrap_or(usize::MAX), selected == Some(row.key.as_str()), max_cpu);
                y += row.height();
            }
        };
        let list = self.layout.list;
        if self.scroll.can_scroll() {
            // Soften the partially visible row at the bottom edge so the list reads as scrollable rather than clipped.
            let fade = 26.0;
            let solid_until = ((list.h - fade) / list.h.max(1.0)).clamp(0.0, 1.0);
            let mask = Brush::linear(
                PointF::new(list.x, list.y),
                PointF::new(list.x, list.bottom()),
                &[(0.0, Color::WHITE), (solid_until, Color::WHITE), (1.0, Color::TRANSPARENT)],
            );
            p.masked(mask, |p| self.scroll.paint_content(p, paint_rows));
        } else {
            self.scroll.paint_content(p, paint_rows);
        }
        self.scroll.paint_scrollbar(p);
    }

    fn paint_action_bar(&mut self, p: &mut Painter, row: &TableRow) {
        let theme = p.theme().clone();
        let bar = self.layout.action_bar;
        components::card(p, bar, 14.0, true);
        components::app_avatar(p, RectF::new(bar.x + 14.0, bar.center().y - 15.0, 30.0, 30.0), row.path.as_deref(), &row.title);
        p.text(&row.title, &TextStyle::emphasized(), theme.text, RectF::new(bar.x + 54.0, bar.y + 7.0, 250.0, 18.0));
        let subtitle = if row.count > 1 {
            format!("{} processes \u{00B7} {}", row.count, format_bytes(row.memory))
        } else {
            format!("PID {} \u{00B7} {}", row.pids[0], row.priority.label())
        };
        p.text(&subtitle, &TextStyle::caption(), theme.text_tertiary, RectF::new(bar.x + 54.0, bar.y + 25.0, 270.0, 16.0));
        p.fill_rect(
            RectF::new(self.layout.action_reveal.x - 16.0, bar.y + 14.0, 1.0, bar.h - 28.0),
            theme.separator,
        );
        self.reveal_button.paint(p);
        self.priority_button.paint(p);
        self.suspend_button.paint(p);
        self.end_button.paint(p);
    }

    fn paint_performance(&mut self, _cx: &mut Ctx, p: &mut Painter) {
        let theme = p.theme().clone();
        let card = self.layout.performance_top;
        components::card_tinted(p, card, CARD_RADIUS, theme.accent, 0.13);
        components::icon_tile(p, RectF::new(card.x + 20.0, card.y + 17.0, 32.0, 32.0), Icon::Cpu, theme.accent);
        p.text(
            "CPU",
            &TextStyle::new(13.5).weight(Weight::Semibold),
            theme.text,
            RectF::new(card.x + 62.0, card.y + 16.0, 200.0, 18.0),
        );
        p.text(
            "Across all cores",
            &TextStyle::new(10.5),
            theme.text_tertiary,
            RectF::new(card.x + 62.0, card.y + 34.0, 240.0, 14.0),
        );
        p.text(
            &format_percent(self.cpu_value.get().max(0.0)),
            &TextStyle::new(42.0).weight(Weight::Semibold).tabular(),
            theme.text,
            RectF::new(card.x + 20.0, card.y + 48.0, 210.0, 52.0),
        );
        p.text(
            &format!("Uptime {}", format_uptime(self.snapshot.system.uptime_secs)),
            &TextStyle::caption().align(TextAlign::Trailing),
            theme.text_tertiary,
            RectF::new(card.right() - 200.0, card.y + 22.0, 180.0, 16.0),
        );
        if self.paused {
            let chip = RectF::new(card.right() - 92.0, card.y + 15.0, 72.0, 22.0);
            components::badge(p, chip, "Paused", components::ORANGE);
        }

        let plot = RectF::new(card.x + 20.0, card.y + 115.0, card.w - 66.0, card.h - 148.0);
        for fraction in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let y = plot.bottom() - plot.h * fraction;
            p.fill_rect(RectF::new(plot.x, y, plot.w, 1.0), theme.separator.multiply_alpha(0.55));
            p.text(
                &format!("{:.0}%", fraction * 100.0),
                &TextStyle::new(9.5).tabular().align(TextAlign::Trailing),
                theme.text_tertiary,
                RectF::new(plot.right() + 6.0, y - 7.0, 34.0, 14.0),
            );
        }
        components::sparkline(p, plot, &self.snapshot.system.cpu_history, 100.0, theme.accent);
        p.text(
            "60 seconds ago",
            &TextStyle::new(10.0),
            theme.text_tertiary,
            RectF::new(plot.x, plot.bottom() + 8.0, 120.0, 14.0),
        );
        p.text(
            "now",
            &TextStyle::new(10.0).align(TextAlign::Trailing),
            theme.text_tertiary,
            RectF::new(plot.right() - 50.0, plot.bottom() + 8.0, 50.0, 14.0),
        );

        let memory = self.layout.performance_memory;
        components::card_tinted(p, memory, CARD_RADIUS, components::PURPLE, 0.13);
        components::icon_tile(p, RectF::new(memory.x + 20.0, memory.y + 17.0, 32.0, 32.0), Icon::Memory, components::PURPLE);
        p.text(
            "Memory",
            &TextStyle::new(13.5).weight(Weight::Semibold),
            theme.text,
            RectF::new(memory.x + 62.0, memory.y + 16.0, 200.0, 18.0),
        );
        p.text(
            &format!("of {} installed", format_bytes(self.snapshot.system.memory_total)),
            &TextStyle::new(10.5),
            theme.text_tertiary,
            RectF::new(memory.x + 62.0, memory.y + 34.0, 240.0, 14.0),
        );
        components::badge(
            p,
            RectF::new(memory.right() - 84.0, memory.y + 18.0, 64.0, 22.0),
            &format_percent(self.snapshot.system.memory_percent()),
            components::PURPLE,
        );
        p.text(
            &format_bytes(self.snapshot.system.memory_used),
            &TextStyle::new(32.0).weight(Weight::Semibold).tabular(),
            theme.text,
            RectF::new(memory.x + 20.0, memory.y + 48.0, 260.0, 42.0),
        );
        p.text(
            &format!(
                "{} available",
                format_bytes(self.snapshot.system.memory_total.saturating_sub(self.snapshot.system.memory_used))
            ),
            &TextStyle::caption(),
            theme.text_tertiary,
            RectF::new(memory.x + 20.0, memory.y + 87.0, 240.0, 14.0),
        );
        let rail = RectF::new(memory.x + 20.0, memory.y + 108.0, memory.w - 40.0, 8.0);
        components::progress(p, rail, self.snapshot.system.memory_percent() / 100.0, components::PURPLE);
        let plot = RectF::new(memory.x + 20.0, memory.y + 138.0, memory.w - 40.0, (memory.h - 166.0).max(24.0));
        components::sparkline(p, plot, &self.snapshot.system.memory_history, 100.0, components::PURPLE);
        p.text(
            "Memory pressure over the last minute",
            &TextStyle::new(10.0),
            theme.text_tertiary,
            RectF::new(memory.x + 20.0, memory.bottom() - 22.0, memory.w - 40.0, 14.0),
        );

        let consumers = self.layout.performance_consumers;
        components::card_tinted(p, consumers, CARD_RADIUS, components::TEAL, 0.10);
        p.text(
            "Top consumers",
            &TextStyle::new(13.5).weight(Weight::Semibold),
            theme.text,
            RectF::new(consumers.x + 20.0, consumers.y + 16.0, 220.0, 20.0),
        );
        p.text(
            "CPU",
            &TextStyle::new(9.5).weight(Weight::Semibold).align(TextAlign::Trailing),
            theme.text_tertiary,
            RectF::new(consumers.right() - 76.0, consumers.y + 16.0, 56.0, 20.0),
        );
        let mut processes: Vec<&ProcessRow> = self.snapshot.processes.iter().collect();
        processes.sort_by(|a, b| b.cpu.total_cmp(&a.cpu));
        let max = processes.first().map(|process| process.cpu).unwrap_or(1.0).max(1.0);
        let mut y = consumers.y + 50.0;
        for (index, process) in processes.iter().take(5).enumerate() {
            if y + 42.0 > consumers.bottom() - 12.0 {
                break;
            }
            let row = RectF::new(consumers.x + 14.0, y, consumers.w - 28.0, 38.0);
            p.fill_round_rect(
                row,
                10.0,
                if index == 0 {
                    theme.accent.multiply_alpha(if theme.is_dark() { 0.10 } else { 0.07 })
                } else {
                    theme.hover.multiply_alpha(if theme.is_dark() { 0.50 } else { 0.60 })
                },
            );
            components::app_avatar(p, RectF::new(row.x + 8.0, row.center().y - 11.0, 22.0, 22.0), process.path.as_deref(), &components::process_title(&process.name, &process.display_name));
            p.text(
                &components::process_title(&process.name, &process.display_name),
                &TextStyle::new(12.0),
                theme.text,
                RectF::new(row.x + 38.0, row.y + 4.0, row.w - 100.0, 17.0),
            );
            p.text(
                &format_percent(process.cpu),
                &TextStyle::new(11.5).weight(Weight::Medium).tabular().align(TextAlign::Trailing),
                theme.text_secondary,
                RectF::new(row.right() - 60.0, row.y + 4.0, 52.0, 16.0),
            );
            let bar = RectF::new(row.x + 38.0, row.bottom() - 10.0, row.w - 104.0, 3.0);
            p.fill_round_rect(bar, 1.5, theme.control_track.multiply_alpha(0.55));
            let filled = RectF::new(bar.x, bar.y, bar.w * (process.cpu / max).clamp(0.0, 1.0), bar.h);
            if filled.w > 0.5 {
                p.fill_round_rect(filled, 1.5, if index == 0 { theme.accent } else { theme.accent.multiply_alpha(0.65) });
            }
            y += 42.0;
        }
        p.text(
            &format!(
                "{} processes \u{00B7} {} threads",
                format_count(self.snapshot.system.process_count),
                format_count(self.snapshot.system.thread_count as usize)
            ),
            &TextStyle::caption(),
            theme.text_tertiary,
            RectF::new(consumers.x + 20.0, consumers.bottom() - 24.0, consumers.w - 40.0, 14.0),
        );
    }

    fn paint_startup(&mut self, _cx: &mut Ctx, p: &mut Painter) {
        let theme = p.theme().clone();
        let bar = self.layout.startup_bar;
        components::icon_tile(p, RectF::new(bar.x, bar.y + 2.0, 32.0, 32.0), Icon::Rocket, components::TEAL);
        p.text(
            "Startup entries",
            &TextStyle::new(20.0).weight(Weight::Semibold),
            theme.text,
            RectF::new(bar.x + 44.0, bar.y + 2.0, 220.0, 30.0),
        );
        p.text(
            &format!("\u{00B7} {} found", self.startup.len()),
            &TextStyle::new(10.5),
            theme.text_tertiary,
            RectF::new(bar.x + 195.0, bar.y + 7.0, 120.0, 22.0),
        );
        self.startup_refresh.paint(p);
        self.startup_settings.paint(p);

        let list = self.layout.startup_list;
        let shell = RectF::new(list.x, list.y - 8.0, list.w, list.h + 8.0);
        components::card(p, shell, CARD_RADIUS, false);

        if self.startup.is_empty() {
            components::icon_tile(p, RectF::new(list.center().x - 24.0, list.y + 56.0, 48.0, 48.0), Icon::Rocket, theme.text_tertiary);
            p.text("Nothing starts with Windows", &TextStyle::title(), theme.text_secondary, RectF::new(list.x, list.y + 120.0, list.w, 22.0));
            p.text("Startup entries from the registry and startup folders appear here.", &TextStyle::body(), theme.text_tertiary, RectF::new(list.x, list.y + 144.0, list.w, 20.0));
            return;
        }

        let rows = &self.startup;
        let hovered = self.startup_hover;
        let toggle_hovered = self.startup_toggle_hover;
        self.startup_scroll.paint_content(p, |p| {
            let mut index = 0usize;
            let mut y = 4.0;
            while index < rows.len() {
                paint_startup_row(
                    p,
                    &rows[index],
                    y,
                    self.layout.startup_list.w,
                    index == hovered.unwrap_or(usize::MAX),
                    index == toggle_hovered.unwrap_or(usize::MAX),
                );
                y += STARTUP_H;
                index += 1;
            }
        });
        self.startup_scroll.paint_scrollbar(p);
    }

    fn paint_footer(&mut self, _cx: &mut Ctx, p: &mut Painter) {
        let theme = p.theme().clone();
        let footer = self.layout.footer;
        p.fill_rect(RectF::new(footer.x, footer.y - 6.0, footer.w, 1.0), theme.separator.multiply_alpha(0.55));
        p.text(
            &format!("Vitals {} \u{00B7} Ctrl+Shift+Esc for quick view", self.version),
            &TextStyle::new(10.0),
            theme.text_tertiary,
            footer,
        );
        let right = if self.paused {
            "Sampling paused".to_string()
        } else {
            format!(
                "{} \u{00B7} {} processes \u{00B7} {} threads",
                components::updated_label(self.snapshot.captured_ms),
                format_count(self.snapshot.system.process_count),
                format_count(self.snapshot.system.thread_count as usize)
            )
        };
        if self.paused {
            p.fill_ellipse(PointF::new(footer.right() - 126.0, footer.center().y), 2.5, 2.5, components::ORANGE);
        }
        p.text(
            &right,
            &TextStyle::new(10.0).align(TextAlign::Trailing),
            theme.text_tertiary,
            RectF::new(footer.x, footer.y, footer.w - if self.paused { 0.0 } else { 0.0 }, footer.h),
        );
    }

    fn paint_confirm(&mut self, p: &mut Painter, confirm: &mut ConfirmState) {
        let theme = p.theme().clone();
        p.fill_rect(p.bounds(), theme.overlay_dim);
        let card = confirm.rect;
        p.shadow(card, 18.0, &theme.shadows[0].scaled_alpha(1.15));
        p.fill_round_rect(card, 18.0, theme.glass_fill_solid);
        p.hairline_round_rect(card, 18.0, theme.hairline, true);
        components::icon_tile(p, RectF::new(card.x + 24.0, card.y + 24.0, 38.0, 38.0), Icon::Warning, components::ORANGE);
        p.text(&confirm.title, &TextStyle::new(17.0).weight(Weight::Semibold), theme.text, RectF::new(card.x + 75.0, card.y + 24.0, card.w - 99.0, 24.0));
        p.text(
            &confirm.body,
            &TextStyle::body().wrap().line_height(19.0),
            theme.text_secondary,
            RectF::new(card.x + 24.0, card.y + 82.0, card.w - 48.0, 64.0),
        );
        p.text(
            "Windows will close the process immediately; unsaved work cannot be recovered.",
            &TextStyle::new(11.0),
            theme.text_tertiary,
            RectF::new(card.x + 24.0, card.y + 150.0, card.w - 48.0, 18.0),
        );
        confirm.cancel.paint(p);
        confirm.confirm.paint(p);
    }

    fn paint_notice(&mut self, p: &mut Painter) {
        let Some(notice) = &self.notice else { return };
        if now_ms() >= notice.until_ms {
            return;
        }
        let theme = p.theme().clone();
        let style = TextStyle::new(12.0).weight(Weight::Medium);
        let width = (p.measure(&notice.text, &style).w + 36.0).min(p.bounds().w - 80.0);
        let rect = RectF::new((p.bounds().w - width) / 2.0, p.bounds().h - 134.0, width, 32.0);
        p.shadow(rect, 16.0, &theme.shadows[1]);
        p.fill_round_rect(rect, 16.0, theme.glass_fill_solid);
        p.hairline_round_rect(rect, 16.0, theme.hairline, true);
        p.text(&notice.text, &style, theme.text, RectF::new(rect.x + 18.0, rect.y, rect.w - 36.0, rect.h));
    }
}

#[derive(Clone, Copy)]
struct RelativeColumns {
    name: f32,
    cpu: (f32, f32),
    memory: (f32, f32),
    disk: (f32, f32),
    status: (f32, f32),
}

fn paint_table_row(
    p: &mut Painter,
    row: &TableRow,
    y: f32,
    width: f32,
    columns: &RelativeColumns,
    hovered: bool,
    selected: bool,
    max_cpu: f32,
) {
    let theme = p.theme().clone();
    let height = row.height();
    let background = RectF::new(2.0, y + 1.0, width - 4.0, height - 2.0);
    if selected {
        p.fill_round_rect(
            background,
            10.0,
            Brush::linear(
                PointF::new(background.x, background.y),
                PointF::new(background.right(), background.y),
                &[
                    (0.0, theme.accent.multiply_alpha(if theme.is_dark() { 0.22 } else { 0.15 })),
                    (1.0, theme.accent.multiply_alpha(if theme.is_dark() { 0.10 } else { 0.07 })),
                ],
            ),
        );
        p.hairline_round_rect(background, 10.0, theme.accent.multiply_alpha(0.30), true);
        p.fill_round_rect(RectF::new(background.x + 3.0, background.y + 12.0, 2.5, height - 26.0), 1.25, theme.accent);
    } else if hovered {
        p.fill_round_rect(background, 10.0, theme.hover.multiply_alpha(0.8));
    }
    let indent = if row.child { 22.0 } else { 0.0 };
    let avatar_size = if row.child { 24.0 } else { 30.0 };
    let avatar_x = 28.0 + indent;
    if row.expandable {
        p.icon(
            if row.expanded { Icon::ChevronDown } else { Icon::ChevronRight },
            PointF::new(13.0, y + height / 2.0),
            12.0,
            if hovered { theme.text_secondary } else { theme.text_tertiary },
        );
    }
    components::app_avatar(
        p,
        RectF::new(avatar_x, y + (height - avatar_size) / 2.0, avatar_size, avatar_size),
        row.path.as_deref(),
        &row.title,
    );
    let text_x = avatar_x + avatar_size + 12.0;
    let name_width = columns.name - text_x - 12.0;
    if row.child {
        let title_width = p.text(
            &row.title,
            &TextStyle::new(12.2),
            theme.text,
            RectF::new(text_x, y, name_width, height),
        );
        p.text(
            &row.subtitle,
            &TextStyle::caption(),
            theme.text_tertiary,
            RectF::new(text_x + title_width + 8.0, y, (name_width - title_width - 8.0).max(0.0), height),
        );
    } else {
        let style = if row.expandable {
            TextStyle::new(12.8).weight(Weight::Semibold)
        } else {
            TextStyle::new(12.8).weight(Weight::Medium)
        };
        p.text(&row.title, &style, theme.text, RectF::new(text_x, y + 6.0, name_width, 19.0));
        p.text(&row.subtitle, &TextStyle::new(10.5), theme.text_tertiary, RectF::new(text_x, y + 24.0, name_width, 15.0));
    }

    let (cpu_x, cpu_w) = columns.cpu;
    let cpu_rect = RectF::new(cpu_x, y, cpu_w - 8.0, height);
    let cpu_text = if row.cpu < 0.05 { "0%".to_string() } else { format_percent(row.cpu) };
    p.text(
        &cpu_text,
        &TextStyle::new(12.0).weight(Weight::Medium).tabular().align(TextAlign::Trailing),
        if row.cpu >= 1.0 { theme.text } else { theme.text_secondary },
        cpu_rect,
    );
    if !row.child && row.cpu > 0.1 {
        let bar = RectF::new(cpu_x + 10.0, y + height - 11.0, cpu_w - 26.0, 2.5);
        p.fill_round_rect(bar, 1.25, theme.control_track.multiply_alpha(0.45));
        let filled = RectF::new(bar.x, bar.y, bar.w * (row.cpu / max_cpu).clamp(0.0, 1.0), bar.h);
        if filled.w > 0.5 {
            p.fill_round_rect(filled, 1.25, theme.accent.multiply_alpha(0.9));
        }
    }

    let (memory_x, memory_w) = columns.memory;
    p.text(
        &format_bytes(row.memory),
        &TextStyle::new(12.0).tabular().align(TextAlign::Trailing),
        theme.text_secondary,
        RectF::new(memory_x, y, memory_w - 8.0, height),
    );
    let (disk_x, disk_w) = columns.disk;
    let disk_text = if row.disk < 0.5 { "-".to_string() } else { format_rate(row.disk) };
    p.text(
        &disk_text,
        &TextStyle::new(12.0).tabular().align(TextAlign::Trailing),
        theme.text_tertiary,
        RectF::new(disk_x, y, disk_w - 8.0, height),
    );

    let (status_x, status_w) = columns.status;
    let status_rect = RectF::new(status_x + 8.0, y, status_w - 16.0, height);
    if row.child {
        let color = components::state_color(&theme, row.status);
        p.fill_ellipse(PointF::new(status_rect.x + 3.0, status_rect.center().y), 5.0, 5.0, color.multiply_alpha(0.16));
        p.fill_ellipse(PointF::new(status_rect.x + 3.0, status_rect.center().y), 2.6, 2.6, color);
        p.text(row.status.label(), &TextStyle::caption(), theme.text_secondary, RectF::new(status_rect.x + 13.0, status_rect.y, status_rect.w - 13.0, status_rect.h));
    } else if row.count > 1 && !row.system {
        p.text(
            &format!("{} processes", row.count),
            &TextStyle::caption(),
            theme.text_tertiary,
            status_rect,
        );
    } else if row.system {
        p.icon(Icon::Lock, PointF::new(status_rect.x + 6.0, status_rect.center().y), 11.0, theme.text_tertiary);
        p.text("System", &TextStyle::caption(), theme.text_tertiary, RectF::new(status_rect.x + 16.0, status_rect.y, status_rect.w - 16.0, status_rect.h));
    } else if row.elevated {
        p.icon(Icon::Shield, PointF::new(status_rect.x + 6.0, status_rect.center().y), 11.0, components::ORANGE);
        p.text("Elevated", &TextStyle::caption(), components::ORANGE, RectF::new(status_rect.x + 16.0, status_rect.y, status_rect.w - 16.0, status_rect.h));
    } else {
        let color = components::state_color(&theme, row.status);
        p.fill_ellipse(PointF::new(status_rect.x + 3.0, status_rect.center().y), 5.0, 5.0, color.multiply_alpha(0.16));
        p.fill_ellipse(PointF::new(status_rect.x + 3.0, status_rect.center().y), 2.6, 2.6, color);
        p.text(
            row.status.label(),
            &TextStyle::caption(),
            if row.status == ProcessState::Running { theme.text_secondary } else { color },
            RectF::new(status_rect.x + 13.0, status_rect.y, status_rect.w - 13.0, status_rect.h),
        );
    }
}

fn paint_startup_row(p: &mut Painter, item: &StartupRow, y: f32, width: f32, hovered: bool, toggle_hovered: bool) {
    let theme = p.theme().clone();
    let row = RectF::new(10.0, y + 3.0, width - 20.0, STARTUP_H - 9.0);
    p.fill_round_rect(
        row,
        12.0,
        if theme.is_dark() {
            Color::rgba(1.0, 1.0, 1.0, if hovered { 0.060 } else { 0.030 })
        } else {
            Color::rgba(1.0, 1.0, 1.0, if hovered { 0.95 } else { 0.68 })
        },
    );
    p.hairline_round_rect(row, 12.0, theme.hairline.multiply_alpha(if hovered { 1.0 } else { 0.65 }), true);
    let path = item.executable();
    components::app_avatar(
        p,
        RectF::new(row.x + 16.0, row.center().y - 16.0, 32.0, 32.0),
        path.as_deref(),
        &item.name,
    );
    p.text(
        &item.name,
        &TextStyle::new(12.8).weight(Weight::Medium),
        theme.text,
        RectF::new(row.x + 60.0, row.y + 7.0, row.w - 230.0, 18.0),
    );
    p.text(
        &format!(
            "{} \u{00B7} {} \u{00B7} {}",
            item.source,
            item.scope,
            compact_path(&item.command, 82)
        ),
        &TextStyle::new(10.5),
        theme.text_tertiary,
        RectF::new(row.x + 60.0, row.y + 28.0, row.w - 230.0, 16.0),
    );
    let badge_rect = RectF::new(row.right() - 104.0, row.center().y - 10.0, 38.0, 20.0);
    let badge_color = if item.enabled { theme.success } else { theme.text_tertiary };
    if toggle_hovered {
        p.fill_round_rect(badge_rect, badge_rect.h / 2.0, badge_color.multiply_alpha(0.10));
    }
    components::badge(p, badge_rect, if item.enabled { "On" } else { "Off" }, badge_color);
    if toggle_hovered {
        p.hairline_round_rect(badge_rect, badge_rect.h / 2.0, badge_color.multiply_alpha(0.55), true);
    }
    let folder = RectF::new(row.right() - 54.0, row.center().y - 15.0, 30.0, 30.0);
    p.fill_round_rect(folder, 9.0, theme.hover.multiply_alpha(if hovered { 0.95 } else { 0.55 }));
    p.icon(Icon::Folder, folder.center(), 14.0, if hovered { theme.text } else { theme.text_tertiary });
}

fn keyboard_modifiers(event: &Event) -> bool {
    match event {
        Event::KeyDown(key) => !key.mods.is_empty() && key.mods != Modifiers::SHIFT,
        _ => false,
    }
}
