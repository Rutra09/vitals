//! The resident application: one sampler thread, the tray and global-shortcut integration, three windows and the
//! translation of `PanelIntent` into Win32 actions.

use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use vitals_core::{PointI, Settings, Snapshot};
use vitals_panel::intent::command_executable;
use vitals_panel::quick::WINDOW_SIZE as QUICK_SIZE;
use vitals_panel::{DashboardView, PanelIntent, QuickView, SettingsView, StartupRow, Tab};
use vitals_proc::{Sampler, end_task, resume, set_priority, set_startup_enabled, startup_items, suspend};
use vitals_sys::{
    HookConfig, InputHook, Tray, TrayCommand, cursor_pos,
    instance::{ACTIVATE_EVENT, QUIT_EVENT, Signal},
    paths, scale_at,
    settings_store, shell, work_area_at,
};
use vitals_ui::{App, AppProxy, RectI, SizeF, WindowId, WindowSpec};
use windows::{
    Win32::{
        Foundation::HWND,
        UI::{
            Shell::SetCurrentProcessExplicitAppUserModelID,
            WindowsAndMessaging::{IsIconic, SW_RESTORE, ShowWindow},
        },
    },
    core::w,
};

const DASHBOARD_SIZE: SizeF = SizeF::new(1180.0, 820.0);
const DASHBOARD_MIN: SizeF = SizeF::new(960.0, 620.0);
const SETTINGS_SIZE: SizeF = SizeF::new(760.0, 680.0);
const QUICK_MARGIN: f32 = 16.0;

type Shared = Rc<RefCell<State>>;

#[derive(Clone, Copy)]
struct SampleReady;
#[derive(Clone, Copy)]
struct ActivateRequest;
#[derive(Clone, Copy)]
struct QuitRequest;
#[derive(Clone, Copy)]
struct QuickViewRequest;

#[derive(Default)]
struct Windows {
    dashboard: Option<WindowId>,
    quick: Option<WindowId>,
    settings: Option<WindowId>,
}

struct State {
    settings: Settings,
    snapshot: Snapshot,
    startup: Vec<StartupRow>,
    paused: bool,
    quick_shown: bool,
    quick_generation: u64,
    windows: Windows,
    tray: Option<Tray>,
    hook: Option<InputHook>,
    samples: Option<SamplerWorker>,
    version: String,
}

/// Latest snapshot hand-off. The worker replaces the slot instead of queueing, so a busy UI can never fall behind.
struct SamplerShared {
    stop: AtomicBool,
    paused: AtomicBool,
    interval_ms: AtomicU64,
    refresh: AtomicBool,
    pending: AtomicBool,
    slot: Mutex<Option<Snapshot>>,
    suspension: Mutex<Vec<(u32, bool)>>,
}

struct SamplerWorker {
    shared: Arc<SamplerShared>,
    thread: Option<thread::JoinHandle<()>>,
}

impl SamplerWorker {
    fn start(proxy: AppProxy, settings: &Settings) -> Self {
        let shared = Arc::new(SamplerShared {
            stop: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            interval_ms: AtomicU64::new(settings.sampling_ms.clamp(250, 10_000)),
            refresh: AtomicBool::new(false),
            pending: AtomicBool::new(false),
            slot: Mutex::new(None),
            suspension: Mutex::new(Vec::new()),
        });
        let thread_shared = shared.clone();
        let handle = thread::Builder::new()
            .name("vitals-sampler".into())
            .spawn(move || sample_loop(thread_shared, proxy))
            .map_err(|error| log::error!("could not start sampler thread: {error}"))
            .ok();
        Self { shared, thread: handle }
    }

    fn take(&self) -> Option<Snapshot> {
        if !self.shared.pending.swap(false, Ordering::AcqRel) {
            return None;
        }
        self.shared.slot.lock().ok().and_then(|mut slot| slot.take())
    }

    fn set_paused(&self, paused: bool) {
        self.shared.paused.store(paused, Ordering::Relaxed);
    }

    fn set_interval(&self, milliseconds: u64) {
        self.shared.interval_ms.store(milliseconds.clamp(250, 10_000), Ordering::Relaxed);
        self.shared.refresh.store(true, Ordering::Relaxed);
    }

    fn refresh(&self) {
        self.shared.refresh.store(true, Ordering::Relaxed);
    }

    fn mark_suspended(&self, pid: u32, suspended: bool) {
        if let Ok(mut queue) = self.shared.suspension.lock() {
            queue.push((pid, suspended));
        }
    }
}

impl Drop for SamplerWorker {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn sample_loop(shared: Arc<SamplerShared>, proxy: AppProxy) {
    let mut sampler = Sampler::new();
    // Warm up the counters before the window first opens so CPU is a real delta, not a row of zeros.
    let _ = sampler.sample();
    thread::sleep(Duration::from_millis(180));

    let mut next = Instant::now();
    loop {
        if shared.stop.load(Ordering::Acquire) {
            break;
        }
        if let Ok(mut queue) = shared.suspension.lock() {
            for (pid, suspended) in queue.drain(..) {
                sampler.mark_suspended(pid, suspended);
            }
        }
        if shared.paused.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(80));
            next = Instant::now();
            continue;
        }
        let now = Instant::now();
        let forced = shared.refresh.swap(false, Ordering::Relaxed);
        if now >= next || forced {
            let snapshot = sampler.sample();
            if let Ok(mut slot) = shared.slot.lock() {
                *slot = Some(snapshot);
            }
            shared.pending.store(true, Ordering::Release);
            if !proxy.post(SampleReady) {
                break;
            }
            let interval = shared.interval_ms.load(Ordering::Relaxed).clamp(250, 10_000);
            next = Instant::now() + Duration::from_millis(interval);
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn bootstrap_snapshot() -> Snapshot {
    let mut sampler = Sampler::new();
    let _ = sampler.sample();
    thread::sleep(Duration::from_millis(130));
    sampler.sample()
}

fn startup_rows() -> Vec<StartupRow> {
    startup_items()
        .into_iter()
        .map(|item| StartupRow {
            id: item.id(),
            name: item.name,
            command: item.command,
            source: item.source.label().into(),
            scope: item.scope.label().into(),
            enabled: item.enabled,
        })
        .collect()
}

pub fn run(background: bool) -> Result<()> {
    match vitals_sys::instance::acquire() {
        vitals_sys::instance::InstanceRole::Secondary => {
            let requested = vitals_sys::instance::request_activate().unwrap_or(false);
            if !requested {
                vitals_sys::instance::activate_existing().context("could not reach the running copy")?;
            }
            Ok(())
        }
        vitals_sys::instance::InstanceRole::Primary(_guard) => {
            let settings = settings_store::load();
            let snapshot = bootstrap_snapshot();
            let startup = startup_rows();
            let state: Shared = Rc::new(RefCell::new(State {
                settings,
                snapshot,
                startup,
                paused: false,
                quick_shown: false,
                quick_generation: 0,
                windows: Windows::default(),
                tray: None,
                hook: None,
                samples: None,
                version: env!("CARGO_PKG_VERSION").into(),
            }));
            let setup_state = state.clone();
            let result = vitals_ui::run(move |app| setup(app, setup_state, background));
            // Stop the sampler before the tray and hook go away, and before App drops its handler closures.
            if let Ok(mut state) = state.try_borrow_mut() {
                let _ = state.samples.take();
            }
            result
        }
    }
}

fn setup(app: &App, state: Shared, background: bool) -> Result<()> {
    unsafe {
        let _ = SetCurrentProcessExplicitAppUserModelID(w!("Vitals.Vitals"));
    }
    let settings = state.borrow().settings.clone();
    app.set_appearance(settings.theme, false);
    register_handlers(app, state.clone());

    watch_signals(app)?;

    let start_hidden = background || settings.start_minimized;
    ensure_dashboard(app, &state, start_hidden).context("creating the dashboard")?;
    ensure_quick(app, &state).context("creating the quick view")?;

    let paused = state.borrow().paused;
    let (sender, receiver) = mpsc::channel::<TrayCommand>();
    match Tray::install(sender, paused) {
        Ok(tray) => {
            state.borrow_mut().tray = Some(tray);
            let proxy = app.proxy();
            let _ = thread::Builder::new().name("vitals-tray".into()).spawn(move || {
                for command in receiver {
                    if !proxy.post(command) {
                        break;
                    }
                }
            });
        }
        Err(error) => log::error!("tray icon unavailable: {error:#}"),
    }

    match InputHook::start(HookConfig::from(&settings)) {
        Ok((hook, receiver)) => {
            state.borrow_mut().hook = Some(hook);
            let proxy = app.proxy();
            let _ = thread::Builder::new().name("vitals-hotkey-events".into()).spawn(move || {
                for _ in receiver {
                    if !proxy.post(QuickViewRequest) {
                        break;
                    }
                }
            });
        }
        Err(error) => log::error!("global shortcut unavailable: {error:#}"),
    }

    let worker = SamplerWorker::start(app.proxy(), &settings);
    state.borrow_mut().samples = Some(worker);
    log::info!("dashboard ready (background={background})");
    Ok(())
}

fn watch_signals(app: &App) -> Result<()> {
    let activate = Signal::create(ACTIVATE_EVENT).context("creating the activate signal")?;
    let quit = Signal::create(QUIT_EVENT).context("creating the quit signal")?;
    let proxy = app.proxy();
    thread::Builder::new()
        .name("vitals-activate".into())
        .spawn(move || {
            // The event is auto-reset and every double-click on the launcher, shortcut or tray "Open" signals it.
            // Waiting only once made every activation after the first one disappear silently.
            loop {
                activate.wait();
                if !proxy.post(ActivateRequest) {
                    break;
                }
            }
        })
        .context("starting the activate listener")?;
    let proxy = app.proxy();
    thread::Builder::new()
        .name("vitals-quit".into())
        .spawn(move || {
            quit.wait();
            let _ = proxy.post(QuitRequest);
        })
        .context("starting the quit listener")?;
    Ok(())
}

fn register_handlers(app: &App, state: Shared) {
    let handler_state = state.clone();
    app.on_event::<PanelIntent>(move |app, intent| handle_intent(app, &handler_state, intent));
    let handler_state = state.clone();
    app.on_event::<TrayCommand>(move |app, command| handle_tray(app, &handler_state, command));
    let handler_state = state.clone();
    app.on_event::<QuickViewRequest>(move |app, _| toggle_quick(app, &handler_state));
    let handler_state = state.clone();
    app.on_event::<SampleReady>(move |app, _| handle_sample(app, &handler_state));
    let handler_state = state.clone();
    app.on_event::<ActivateRequest>(move |app, _| {
        if let Err(error) = show_dashboard(app, &handler_state) {
            log::error!("could not show the dashboard: {error:#}");
        }
    });
    let handler_state = state;
    app.on_event::<QuitRequest>(move |app, _| quit(app, &handler_state));
}

fn handle_intent(app: &App, state: &Shared, intent: PanelIntent) {
    match intent {
        PanelIntent::ShowDashboard => report(app, state, show_dashboard(app, state)),
        PanelIntent::ShowTab(tab) => {
            if tab == Tab::Startup {
                refresh_startup(app, state);
            }
            match ensure_dashboard(app, state, false) {
                Ok(id) => {
                    app.with_view::<DashboardView, _>(id, |view, cx| {
                        view.set_tab(tab);
                        cx.show_window(true);
                    });
                    restore_if_minimized(app, id);
                }
                Err(error) => log::error!("could not show tab {}: {error:#}", tab.label()),
            }
        }
        PanelIntent::ShowProcess { pid } => match show_dashboard(app, state) {
            Ok(id) => {
                app.with_view::<DashboardView, _>(id, |view, cx| {
                    view.set_tab(Tab::Processes);
                    view.focus_process(pid);
                    cx.request_paint();
                });
            }
            Err(error) => log::error!("could not focus PID {pid}: {error:#}"),
        },
        PanelIntent::HideDashboard => hide_dashboard(app, state),
        PanelIntent::ShowSettings => report(app, state, show_settings(app, state)),
        PanelIntent::HideSettings => hide_settings(app, state),
        PanelIntent::HideQuickView => hide_quick(app, state),
        PanelIntent::ShowQuickView => report(app, state, show_quick(app, state)),
        PanelIntent::Quit => quit(app, state),
        PanelIntent::Refresh => refresh(app, state),
        PanelIntent::RefreshStartup => refresh_startup(app, state),
        PanelIntent::SetStartupEnabled { id, enabled } => {
            let name = state
                .borrow()
                .startup
                .iter()
                .find(|row| row.id == id)
                .map(|row| row.name.clone())
                .unwrap_or_else(|| "Startup entry".into());
            match set_startup_enabled(&id, enabled) {
                Ok(()) => {
                    refresh_startup(app, state);
                    notify(
                        app,
                        state,
                        &format!(
                            "{name} will {}.",
                            if enabled { "start with Windows" } else { "no longer start with Windows" }
                        ),
                    );
                }
                Err(error) => notify(app, state, &format!("{error:#}")),
            }
        }
        PanelIntent::OpenTaskManager => {
            if let Err(error) = std::process::Command::new("taskmgr.exe").spawn() {
                notify(app, state, &format!("Could not open Task Manager: {error}"));
            }
        }
        PanelIntent::OpenStartup { command } => {
            let path = if Path::new(&command).exists() {
                Some(PathBuf::from(&command))
            } else {
                command_executable(&command)
            };
            match path {
                Some(path) => {
                    if let Err(error) = shell::reveal(&path) {
                        notify(app, state, &format!("Could not open {}: {error}", path.display()));
                    }
                }
                None => notify(app, state, "That startup command has no executable file to show."),
            }
        }
        PanelIntent::SetPaused(paused) => set_paused(app, state, paused),
        PanelIntent::EndTask { pids, name } => {
            let outcome = apply_to_pids(&pids, end_task);
            finish_action(app, state, "Ended", &name, outcome);
            refresh(app, state);
        }
        PanelIntent::Suspend { pids, name } => {
            let outcome = apply_to_pids(&pids, suspend);
            if outcome.is_ok()
                && let Some(worker) = state.borrow().samples.as_ref()
            {
                for &pid in &pids {
                    worker.mark_suspended(pid, true);
                }
            }
            finish_action(app, state, "Suspended", &name, outcome);
            refresh(app, state);
        }
        PanelIntent::Resume { pids, name } => {
            let outcome = apply_to_pids(&pids, resume);
            if outcome.is_ok()
                && let Some(worker) = state.borrow().samples.as_ref()
            {
                for &pid in &pids {
                    worker.mark_suspended(pid, false);
                }
            }
            finish_action(app, state, "Resumed", &name, outcome);
            refresh(app, state);
        }
        PanelIntent::SetPriority { pids, priority, name } => {
            let outcome = apply_to_pids(&pids, |pid| set_priority(pid, priority));
            finish_action(app, state, "Set priority on", &name, outcome);
            refresh(app, state);
        }
        PanelIntent::OpenLocation(path) => {
            if let Err(error) = shell::reveal(&path) {
                notify(app, state, &format!("Could not open {}: {error}", path.display()));
            }
        }
        PanelIntent::CopyText(text) => match shell::copy_text(&text) {
            Ok(()) => notify(app, state, "Copied to clipboard."),
            Err(error) => notify(app, state, &format!("Could not copy: {error:#}")),
        },
        PanelIntent::OpenDataFolder => match paths::settings_dir() {
            Ok(path) => {
                if let Err(error) = shell::open_path(&path) {
                    notify(app, state, &format!("Could not open {}: {error}", path.display()));
                }
            }
            Err(error) => notify(app, state, &format!("Could not find the data folder: {error:#}")),
        },
        PanelIntent::OpenUrl(url) => {
            if let Err(error) = shell::open_url(&url) {
                notify(app, state, &format!("Could not open the link: {error:#}"));
            }
        }
        PanelIntent::SettingsChanged(settings) => apply_settings(app, state, *settings),
    }
}

fn handle_tray(app: &App, state: &Shared, command: TrayCommand) {
    match command {
        TrayCommand::Open => report(app, state, show_dashboard(app, state)),
        TrayCommand::QuickView => toggle_quick(app, state),
        TrayCommand::Pause(paused) => set_paused(app, state, paused),
        TrayCommand::Settings => report(app, state, show_settings(app, state)),
        TrayCommand::Quit => quit(app, state),
        TrayCommand::Recreate => {
            if let Some(tray) = state.borrow_mut().tray.as_mut()
                && let Err(error) = tray.recreate()
            {
                log::warn!("could not restore the tray icon: {error:#}");
            }
        }
    }
}

fn finish_action(
    app: &App,
    state: &Shared,
    verb: &str,
    name: &str,
    outcome: std::result::Result<(), ActionError>,
) {
    match outcome {
        Ok(()) => notify(app, state, &format!("{verb} {name}.")),
        Err(error) => notify(app, state, &error.0),
    }
}

struct ActionError(String);

fn apply_to_pids(pids: &[u32], action: impl Fn(u32) -> Result<()>) -> std::result::Result<(), ActionError> {
    let mut errors = Vec::new();
    for &pid in pids {
        if let Err(error) = action(pid) {
            errors.push(format!("PID {pid}: {error}"));
        }
    }
    match errors.into_iter().next() {
        None => Ok(()),
        Some(first) => Err(ActionError(if pids.len() == 1 {
            format!("That action failed: {first}")
        } else {
            format!("Some processes could not be changed: {first}")
        })),
    }
}

fn notify(app: &App, state: &Shared, text: &str) {
    log::info!("{text}");
    let dashboard = state.borrow().windows.dashboard;
    if let Some(id) = dashboard {
        app.with_view::<DashboardView, _>(id, |view, cx| view.show_notice(cx, text));
    }
}

fn report(app: &App, state: &Shared, result: Result<WindowId>) {
    if let Err(error) = result {
        log::error!("window error: {error:#}");
        notify(app, state, &format!("Could not open the window: {error:#}"));
    }
}

fn ensure_dashboard(app: &App, state: &Shared, start_hidden: bool) -> Result<WindowId> {
    if let Some(id) = state.borrow().windows.dashboard {
        return Ok(id);
    }
    let (snapshot, settings, paused, startup) = {
        let state = state.borrow();
        (state.snapshot.clone(), state.settings.clone(), state.paused, state.startup.clone())
    };
    let mut spec = WindowSpec::normal("Vitals", DASHBOARD_SIZE)
        .unified_title_bar()
        .mica(true)
        .min_size(DASHBOARD_MIN)
        .centered_in(work_area_at(cursor_pos()));
    if start_hidden {
        spec = spec.hidden();
    }
    let mut view = DashboardView::new(snapshot, settings, paused);
    view.set_startup(startup);
    let id = app.open(spec, view).context("opening the dashboard window")?;
    state.borrow_mut().windows.dashboard = Some(id);
    if start_hidden {
        app.with_view::<DashboardView, _>(id, |_, cx| cx.render_hidden());
    }
    Ok(id)
}

fn ensure_quick(app: &App, state: &Shared) -> Result<WindowId> {
    if let Some(id) = state.borrow().windows.quick {
        return Ok(id);
    }
    let (snapshot, paused) = {
        let state = state.borrow();
        (state.snapshot.clone(), state.paused)
    };
    let rect = quick_rect();
    let mut spec = WindowSpec::popup(PointI::new(rect.x, rect.y), QUICK_SIZE)
        .activating()
        .hidden();
    spec.title = "Vitals Quick View".into();
    let id = app.open(spec, QuickView::new(snapshot, paused)).context("opening the quick view")?;
    state.borrow_mut().windows.quick = Some(id);
    app.with_view::<QuickView, _>(id, |_, cx| cx.render_hidden());
    Ok(id)
}

fn ensure_settings(app: &App, state: &Shared) -> Result<WindowId> {
    if let Some(id) = state.borrow().windows.settings {
        return Ok(id);
    }
    let (settings, version) = {
        let state = state.borrow();
        (state.settings.clone(), state.version.clone())
    };
    let spec = WindowSpec::normal("Vitals Settings", SETTINGS_SIZE)
        .unified_title_bar()
        .mica(true)
        .fixed_size()
        .centered_in(work_area_at(cursor_pos()));
    let id = app
        .open(spec, SettingsView::new(settings, version))
        .context("opening the settings window")?;
    state.borrow_mut().windows.settings = Some(id);
    Ok(id)
}

fn restore_if_minimized(app: &App, id: WindowId) {
    if let Some(raw) = app.hwnd(id) {
        let hwnd = HWND(raw as _);
        unsafe {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
        }
    }
}

fn show_dashboard(app: &App, state: &Shared) -> Result<WindowId> {
    let id = ensure_dashboard(app, state, false)?;
    let snapshot = state.borrow().snapshot.clone();
    app.with_view::<DashboardView, _>(id, |view, cx| {
        view.set_snapshot(snapshot);
        cx.show_window(true);
    });
    restore_if_minimized(app, id);
    Ok(id)
}

fn hide_dashboard(app: &App, state: &Shared) {
    let id = state.borrow().windows.dashboard;
    if let Some(id) = id {
        app.with_view::<DashboardView, _>(id, |_, cx| cx.hide_window());
    }
}

fn show_quick(app: &App, state: &Shared) -> Result<WindowId> {
    let id = ensure_quick(app, state)?;
    {
        let mut state = state.borrow_mut();
        state.quick_generation = state.quick_generation.wrapping_add(1);
        state.quick_shown = true;
    }
    let snapshot = state.borrow().snapshot.clone();
    let paused = state.borrow().paused;
    let rect = quick_rect();
    app.with_view::<QuickView, _>(id, |view, cx| {
        view.set_snapshot(snapshot);
        view.set_paused(paused);
        cx.set_window_rect_px(rect);
        cx.show_window(true);
    });
    Ok(id)
}

fn hide_quick(app: &App, state: &Shared) {
    if !state.borrow().quick_shown {
        return;
    }
    let generation = {
        let mut state = state.borrow_mut();
        state.quick_shown = false;
        state.quick_generation = state.quick_generation.wrapping_add(1);
        state.quick_generation
    };
    let id = state.borrow().windows.quick;
    let Some(id) = id else { return };
    app.with_view::<QuickView, _>(id, |view, cx| {
        view.set_shown(false);
        cx.animate();
    });
    let delayed = state.clone();
    app.set_timer(Duration::from_millis(150), move |app| {
        let should_hide = {
            let state = delayed.borrow();
            !state.quick_shown && state.quick_generation == generation
        };
        if should_hide
            && let Some(id) = delayed.borrow().windows.quick
        {
            app.with_view::<QuickView, _>(id, |_, cx| cx.hide_window());
        }
    });
}

fn toggle_quick(app: &App, state: &Shared) {
    let shown = state.borrow().quick_shown;
    if shown {
        hide_quick(app, state);
    } else {
        report(app, state, show_quick(app, state));
    }
}

fn show_settings(app: &App, state: &Shared) -> Result<WindowId> {
    let id = ensure_settings(app, state)?;
    let settings = state.borrow().settings.clone();
    app.with_view::<SettingsView, _>(id, |view, cx| {
        view.set_settings(settings);
        cx.show_window(true);
    });
    restore_if_minimized(app, id);
    Ok(id)
}

fn hide_settings(app: &App, state: &Shared) {
    let id = state.borrow().windows.settings;
    if let Some(id) = id {
        app.with_view::<SettingsView, _>(id, |_, cx| cx.hide_window());
    }
}

fn handle_sample(app: &App, state: &Shared) {
    let snapshot = state.borrow().samples.as_ref().and_then(SamplerWorker::take);
    let Some(snapshot) = snapshot else { return };
    state.borrow_mut().snapshot = snapshot.clone();
    if let Some(id) = state.borrow().windows.dashboard {
        app.with_view::<DashboardView, _>(id, |view, cx| {
            view.set_snapshot(snapshot.clone());
            cx.request_paint();
        });
    }
    if let Some(id) = state.borrow().windows.quick {
        app.with_view::<QuickView, _>(id, |view, cx| {
            view.set_snapshot(snapshot);
            // The quick view has no self-animating counters; without this it only refreshed while the pointer moved.
            cx.request_paint();
        });
    }
}

fn refresh(_app: &App, state: &Shared) {
    if let Some(worker) = state.borrow().samples.as_ref() {
        worker.refresh();
    }
}

fn refresh_startup(app: &App, state: &Shared) {
    let rows = startup_rows();
    state.borrow_mut().startup = rows.clone();
    if let Some(id) = state.borrow().windows.dashboard {
        app.with_view::<DashboardView, _>(id, |view, cx| {
            view.set_startup(rows);
            cx.request_paint();
        });
    }
}

fn set_paused(app: &App, state: &Shared, paused: bool) {
    {
        let mut state = state.borrow_mut();
        state.paused = paused;
        if let Some(worker) = state.samples.as_ref() {
            worker.set_paused(paused);
        }
        if let Some(tray) = state.tray.as_mut()
            && let Err(error) = tray.set_paused(paused)
        {
            log::warn!("could not update the tray tooltip: {error:#}");
        }
    }
    let dashboard = state.borrow().windows.dashboard;
    if let Some(id) = dashboard {
        app.with_view::<DashboardView, _>(id, |view, cx| {
            view.set_paused(paused);
            cx.request_paint();
        });
    }
    let quick = state.borrow().windows.quick;
    if let Some(id) = quick {
        app.with_view::<QuickView, _>(id, |view, cx| {
            view.set_paused(paused);
            cx.request_paint();
        });
    }
}

fn apply_settings(app: &App, state: &Shared, mut settings: Settings) {
    let old = state.borrow().settings.clone();
    if settings.launch_at_login != old.launch_at_login
        && let Err(error) = vitals_sys::install::set_launch_at_login(settings.launch_at_login)
    {
        notify(app, state, &format!("Could not change launch at login: {error:#}"));
        settings.launch_at_login = old.launch_at_login;
    }
    if let Err(error) = settings_store::save(&settings) {
        notify(app, state, &format!("Could not save settings: {error:#}"));
    }
    {
        let mut state = state.borrow_mut();
        state.settings = settings.clone();
        if old.sampling_ms != settings.sampling_ms
            && let Some(worker) = state.samples.as_ref()
        {
            worker.set_interval(settings.sampling_ms);
        }
        if let Some(hook) = state.hook.as_ref() {
            hook.set_config(HookConfig::from(&settings));
        }
    }
    if old.theme != settings.theme {
        app.set_appearance(settings.theme, false);
    }
    if let Some(id) = state.borrow().windows.dashboard {
        app.with_view::<DashboardView, _>(id, |view, cx| {
            view.set_settings(settings.clone());
            cx.request_paint();
        });
    }
    if let Some(id) = state.borrow().windows.settings {
        app.with_view::<SettingsView, _>(id, |view, cx| {
            view.set_settings(settings);
            cx.request_paint();
        });
    }
}

fn quit(app: &App, state: &Shared) {
    state.borrow_mut().samples.take();
    app.quit();
}

fn quick_rect() -> RectI {
    let point = cursor_pos();
    let area = work_area_at(point);
    let scale = scale_at(point).max(1.0);
    let width = (QUICK_SIZE.w * scale).round() as i32;
    let height = (QUICK_SIZE.h * scale).round() as i32;
    let margin = (QUICK_MARGIN * scale).round() as i32;
    let x = (area.right() - width - margin).max(area.x + margin);
    let y = (area.y + margin).min((area.bottom() - height - margin).max(area.y + margin));
    RectI::new(x, y, width, height)
}
