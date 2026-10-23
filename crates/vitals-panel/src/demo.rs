//! Deterministic sample data for offscreen previews, `--selftest` and visual tests. It never reflects the machine.

use std::path::PathBuf;

use vitals_core::{now_ms, Priority, ProcessRow, ProcessState, Settings, Snapshot, SystemStats};

use crate::intent::StartupRow;

pub fn demo_settings() -> Settings {
    Settings::default()
}

pub fn demo_startup() -> Vec<StartupRow> {
    [
        ("Microsoft Edge", r#""C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe" --no-startup-window"#, "Registry Run key", "This account"),
        ("OneDrive", r#""C:\Program Files\Microsoft OneDrive\OneDrive.exe" /background"#, "Registry Run key", "This account"),
        ("Steam", r#""C:\Program Files (x86)\Steam\steam.exe" -silent"#, "Registry Run key", "All users"),
        ("Spotify", r#""C:\Users\demo\AppData\Roaming\Spotify\Spotify.exe" --autostart"#, "Startup folder", "This account"),
        ("PowerToys", r#""C:\Program Files\PowerToys\PowerToys.exe""#, "Registry Run key", "All users"),
        ("Windows Security", r#""C:\Program Files\Windows Defender\MSASCuiL.exe""#, "Registry Run key", "All users"),
        ("Docker Desktop", r#""C:\Program Files\Docker\Docker\Docker Desktop.exe""#, "Startup folder", "This account"),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (name, command, source, scope))| StartupRow {
        id: format!("demo:{index}"),
        name: name.into(),
        command: command.into(),
        source: source.into(),
        scope: scope.into(),
        enabled: true,
    })
    .collect()
}

struct Spec {
    name: &'static str,
    display: &'static str,
    path: &'static str,
    cpu: f32,
    memory: u64,
    disk: f64,
    pid: u32,
    system: bool,
    elevated: bool,
    state: ProcessState,
    priority: Priority,
}

const SPECS: &[Spec] = &[
    Spec { name: "chrome.exe", display: "Google Chrome", path: r"C:\Program Files\Google\Chrome\Application\chrome.exe", cpu: 8.4, memory: 2_184_000_000, disk: 340_000.0, pid: 12844, system: false, elevated: false, state: ProcessState::Running, priority: Priority::Normal },
    Spec { name: "Code.exe", display: "Visual Studio Code", path: r"C:\Users\demo\AppData\Local\Programs\Microsoft VS Code\Code.exe", cpu: 5.2, memory: 812_000_000, disk: 1_240_000.0, pid: 9640, system: false, elevated: false, state: ProcessState::Running, priority: Priority::Normal },
    Spec { name: "msedge.exe", display: "Microsoft Edge", path: r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe", cpu: 3.8, memory: 1_340_000_000, disk: 92_000.0, pid: 8812, system: false, elevated: false, state: ProcessState::Running, priority: Priority::Normal },
    Spec { name: "Discord.exe", display: "Discord", path: r"C:\Users\demo\AppData\Local\Discord\app-1.0.9180\Discord.exe", cpu: 1.9, memory: 486_000_000, disk: 44_000.0, pid: 14108, system: false, elevated: false, state: ProcessState::Running, priority: Priority::Normal },
    Spec { name: "Spotify.exe", display: "Spotify", path: r"C:\Users\demo\AppData\Roaming\Spotify\Spotify.exe", cpu: 1.1, memory: 312_000_000, disk: 18_000.0, pid: 11020, system: false, elevated: false, state: ProcessState::Running, priority: Priority::Normal },
    Spec { name: "explorer.exe", display: "Windows Explorer", path: r"C:\Windows\explorer.exe", cpu: 0.9, memory: 214_000_000, disk: 12_000.0, pid: 5820, system: false, elevated: false, state: ProcessState::Running, priority: Priority::Normal },
    Spec { name: "Steam.exe", display: "Steam", path: r"C:\Program Files (x86)\Steam\steam.exe", cpu: 0.7, memory: 268_000_000, disk: 88_000.0, pid: 7324, system: false, elevated: false, state: ProcessState::Running, priority: Priority::Normal },
    Spec { name: "WindowsTerminal.exe", display: "Windows Terminal", path: r"C:\Program Files\WindowsApps\Microsoft.WindowsTerminal_1.22.10352.0_x64__8wekyb3d8bbwe\WindowsTerminal.exe", cpu: 0.5, memory: 96_000_000, disk: 2_000.0, pid: 15812, system: false, elevated: false, state: ProcessState::Running, priority: Priority::Normal },
    Spec { name: "SearchHost.exe", display: "Search", path: r"C:\Windows\SystemApps\MicrosoftWindows.Client.CBS_cw5n1h2txyewy\SearchHost.exe", cpu: 0.3, memory: 128_000_000, disk: 0.0, pid: 6204, system: true, elevated: false, state: ProcessState::Running, priority: Priority::Normal },
    Spec { name: "dwm.exe", display: "Desktop Window Manager", path: r"C:\Windows\System32\dwm.exe", cpu: 2.6, memory: 412_000_000, disk: 0.0, pid: 1420, system: true, elevated: false, state: ProcessState::Running, priority: Priority::High },
    Spec { name: "svchost.exe", display: "Service Host: Network Service", path: r"C:\Windows\System32\svchost.exe", cpu: 0.4, memory: 74_000_000, disk: 6_000.0, pid: 2912, system: true, elevated: false, state: ProcessState::Running, priority: Priority::Normal },
    Spec { name: "svchost.exe", display: "Service Host: Local System", path: r"C:\Windows\System32\svchost.exe", cpu: 0.2, memory: 64_000_000, disk: 4_000.0, pid: 3128, system: true, elevated: false, state: ProcessState::Running, priority: Priority::Normal },
    Spec { name: "RuntimeBroker.exe", display: "Runtime Broker", path: r"C:\Windows\System32\RuntimeBroker.exe", cpu: 0.1, memory: 38_000_000, disk: 0.0, pid: 4816, system: true, elevated: false, state: ProcessState::Running, priority: Priority::Normal },
    Spec { name: "SecurityHealthService.exe", display: "Windows Security Service", path: r"C:\Windows\System32\SecurityHealthService.exe", cpu: 0.1, memory: 26_000_000, disk: 0.0, pid: 3420, system: true, elevated: true, state: ProcessState::Running, priority: Priority::Normal },
    Spec { name: "OneDrive.exe", display: "Microsoft OneDrive", path: r"C:\Program Files\Microsoft OneDrive\OneDrive.exe", cpu: 0.2, memory: 142_000_000, disk: 32_000.0, pid: 7108, system: false, elevated: false, state: ProcessState::Running, priority: Priority::Normal },
    Spec { name: "slack.exe", display: "Slack", path: r"C:\Users\demo\AppData\Local\slack\slack.exe", cpu: 0.6, memory: 196_000_000, disk: 8_000.0, pid: 12740, system: false, elevated: false, state: ProcessState::Suspended, priority: Priority::BelowNormal },
    Spec { name: "Notion.exe", display: "Notion", path: r"C:\Users\demo\AppData\Local\Programs\Notion\Notion.exe", cpu: 0.0, memory: 184_000_000, disk: 0.0, pid: 13204, system: false, elevated: false, state: ProcessState::Suspended, priority: Priority::Normal },
    Spec { name: "Dropbox.exe", display: "Dropbox", path: r"C:\Program Files (x86)\Dropbox\Client\Dropbox.exe", cpu: 0.8, memory: 156_000_000, disk: 410_000.0, pid: 6996, system: false, elevated: false, state: ProcessState::Running, priority: Priority::Normal },
    Spec { name: "audiodg.exe", display: "Windows Audio Device Graph", path: r"C:\Windows\System32\audiodg.exe", cpu: 0.3, memory: 42_000_000, disk: 0.0, pid: 5088, system: true, elevated: false, state: ProcessState::Running, priority: Priority::Normal },
    Spec { name: "lsass.exe", display: "Local Security Authority", path: r"C:\Windows\System32\lsass.exe", cpu: 0.1, memory: 22_000_000, disk: 0.0, pid: 916, system: true, elevated: true, state: ProcessState::Running, priority: Priority::High },
    Spec { name: "Taskmgr.exe", display: "Task Manager", path: r"C:\Windows\System32\Taskmgr.exe", cpu: 0.0, memory: 12_000_000, disk: 0.0, pid: 16680, system: false, elevated: false, state: ProcessState::NotResponding, priority: Priority::Normal },
    Spec { name: "ApplicationFrameHost.exe", display: "Application Frame Host", path: r"C:\Windows\System32\ApplicationFrameHost.exe", cpu: 0.1, memory: 64_000_000, disk: 0.0, pid: 8020, system: true, elevated: false, state: ProcessState::Running, priority: Priority::Normal },
    Spec { name: "PhoneExperienceHost.exe", display: "Phone Link", path: r"C:\Program Files\WindowsApps\Microsoft.YourPhone_1.24092.101.0_x64__8wekyb3d8bbwe\PhoneExperienceHost.exe", cpu: 0.4, memory: 128_000_000, disk: 0.0, pid: 9012, system: false, elevated: false, state: ProcessState::Running, priority: Priority::Normal },
    Spec { name: "dllhost.exe", display: "COM Surrogate", path: r"C:\Windows\System32\dllhost.exe", cpu: 0.0, memory: 18_000_000, disk: 0.0, pid: 11244, system: true, elevated: false, state: ProcessState::Running, priority: Priority::Normal },
];

pub fn demo_snapshot() -> Snapshot {
    let processes: Vec<ProcessRow> = SPECS
        .iter()
        .map(|spec| ProcessRow {
            pid: spec.pid,
            parent_pid: 4,
            name: spec.name.into(),
            display_name: spec.display.into(),
            path: Some(PathBuf::from(spec.path)),
            cpu: spec.cpu,
            memory: spec.memory,
            disk_read_bps: spec.disk * 0.35,
            disk_write_bps: spec.disk * 0.65,
            threads: 8 + spec.pid % 31,
            handles: 120 + spec.pid % 940,
            start_time_ms: 133_000_000_000_000_000 + spec.pid as u64 * 90_000_000,
            priority: spec.priority,
            state: spec.state,
            user: if spec.system { "SYSTEM".into() } else { "demo".into() },
            elevated: spec.elevated,
            system: spec.system,
        })
        .collect();
    let cpu_history = (0..60)
        .map(|index| {
            let t = index as f32 / 59.0;
            12.0 + 9.0 * (t * std::f32::consts::TAU * 1.7).sin() + 5.0 * (t * std::f32::consts::TAU * 4.3).cos() + if index > 43 { 8.0 } else { 0.0 }
        })
        .collect();
    let memory_history = (0..60)
        .map(|index| 34.0 + (index as f32 / 59.0) * 6.0 + (index as f32 * 0.7).sin() * 1.5)
        .collect();
    let process_count = processes.len();
    let thread_count = processes.iter().map(|p| p.threads).sum();
    Snapshot {
        captured_ms: now_ms(),
        processes,
        system: SystemStats {
            cpu_percent: 27.0,
            memory_used: 12_400_000_000,
            memory_total: 32_000_000_000,
            process_count,
            thread_count,
            uptime_secs: 3 * 86_400 + 4 * 3600 + 12 * 60,
            cpu_history,
            memory_history,
        },
    }
}
