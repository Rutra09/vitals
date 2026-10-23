//! The process and system model both the sampler and the views speak.

use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Priority {
    Idle,
    BelowNormal,
    #[default]
    Normal,
    AboveNormal,
    High,
    Realtime,
    Unknown,
}

impl Priority {
    pub const ALL: [Priority; 6] =
        [Priority::Idle, Priority::BelowNormal, Priority::Normal, Priority::AboveNormal, Priority::High, Priority::Realtime];

    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "Low",
            Self::BelowNormal => "Below normal",
            Self::Normal => "Normal",
            Self::AboveNormal => "Above normal",
            Self::High => "High",
            Self::Realtime => "Realtime",
            Self::Unknown => "Unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProcessState {
    #[default]
    Running,
    Suspended,
    NotResponding,
}

impl ProcessState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Running => "Running",
            Self::Suspended => "Suspended",
            Self::NotResponding => "Not responding",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ProcessRow {
    pub pid: u32,
    pub parent_pid: u32,
    /// Executable file name, e.g. `chrome.exe`.
    pub name: String,
    /// Friendly FileDescription, e.g. `Google Chrome`.
    pub display_name: String,
    pub path: Option<PathBuf>,
    /// Percent of the whole machine, 0..100.
    pub cpu: f32,
    pub memory: u64,
    pub disk_read_bps: f64,
    pub disk_write_bps: f64,
    pub threads: u32,
    pub handles: u32,
    pub start_time_ms: u64,
    pub priority: Priority,
    pub state: ProcessState,
    pub user: String,
    pub elevated: bool,
    pub system: bool,
}

impl ProcessRow {
    pub fn group_key(&self) -> String {
        match &self.path {
            Some(path) => format!("path:{}", path.to_string_lossy().to_lowercase()),
            None => format!("name:{}", self.name.to_lowercase()),
        }
    }

    pub fn group_name(&self) -> String {
        if self.display_name.trim().is_empty() { file_stem(&self.name) } else { self.display_name.clone() }
    }

    pub fn matches(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let query = query.to_lowercase();
        self.display_name.to_lowercase().contains(&query)
            || self.name.to_lowercase().contains(&query)
            || self.pid.to_string().contains(&query)
            || self.path.as_ref().is_some_and(|p| p.to_string_lossy().to_lowercase().contains(&query))
    }

    pub fn disk_bps(&self) -> f64 {
        self.disk_read_bps + self.disk_write_bps
    }

    pub fn endable(&self) -> bool {
        self.pid > 4 && !self.system
    }
}

#[derive(Clone, Debug, Default)]
pub struct SystemStats {
    /// Percent of the whole machine, 0..100.
    pub cpu_percent: f32,
    pub memory_used: u64,
    pub memory_total: u64,
    pub process_count: usize,
    pub thread_count: u32,
    pub uptime_secs: u64,
    /// Up to 60 samples, oldest first.
    pub cpu_history: Vec<f32>,
    pub memory_history: Vec<f32>,
}

impl SystemStats {
    pub fn memory_percent(&self) -> f32 {
        if self.memory_total == 0 { 0.0 } else { self.memory_used as f32 / self.memory_total as f32 * 100.0 }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub captured_ms: i64,
    pub processes: Vec<ProcessRow>,
    pub system: SystemStats,
}

impl Snapshot {
    pub fn find_pid(&self, pid: u32) -> Option<usize> {
        self.processes.iter().position(|p| p.pid == pid)
    }

    pub fn process(&self, pid: u32) -> Option<&ProcessRow> {
        self.processes.iter().find(|p| p.pid == pid)
    }

    pub fn total_memory(&self) -> u64 {
        self.processes.iter().map(|p| p.memory).sum()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortKey {
    #[default]
    Cpu,
    Memory,
    Name,
    Disk,
    Pid,
}

impl SortKey {
    pub fn label(self) -> &'static str {
        match self {
            Self::Cpu => "CPU",
            Self::Memory => "Memory",
            Self::Name => "Name",
            Self::Disk => "Disk",
            Self::Pid => "PID",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ProcessGroup {
    pub key: String,
    pub name: String,
    pub path: Option<PathBuf>,
    /// Indices into `Snapshot::processes`, strongest first.
    pub indices: Vec<usize>,
    pub cpu: f32,
    pub memory: u64,
    pub disk_bps: f64,
}

impl ProcessGroup {
    pub fn count(&self) -> usize {
        self.indices.len()
    }

    pub fn endable(&self, processes: &[ProcessRow]) -> bool {
        self.indices.iter().any(|&index| processes[index].endable())
    }
}

/// Groups rows by executable, aggregates CPU/memory/disk and sorts groups by `sort`.
pub fn group_processes(processes: &[ProcessRow], sort: SortKey, descending: bool) -> Vec<ProcessGroup> {
    let mut groups: Vec<ProcessGroup> = Vec::new();
    for (index, process) in processes.iter().enumerate() {
        let key = process.group_key();
        let position = match groups.iter().position(|group| group.key == key) {
            Some(position) => position,
            None => {
                groups.push(ProcessGroup {
                    key: key.clone(),
                    name: process.group_name(),
                    path: process.path.clone(),
                    indices: Vec::new(),
                    cpu: 0.0,
                    memory: 0,
                    disk_bps: 0.0,
                });
                groups.len() - 1
            }
        };
        let group = &mut groups[position];
        group.indices.push(index);
        group.cpu += process.cpu;
        group.memory += process.memory;
        group.disk_bps += process.disk_bps();
    }
    for group in &mut groups {
        group.indices.sort_by(|&a, &b| {
            let (a, b) = (&processes[a], &processes[b]);
            b.cpu.total_cmp(&a.cpu).then_with(|| b.memory.cmp(&a.memory))
        });
        group.cpu = group.cpu.min(100.0);
    }
    groups.sort_by(|a, b| {
        let order = match sort {
            SortKey::Cpu => b.cpu.total_cmp(&a.cpu),
            SortKey::Memory => b.memory.cmp(&a.memory),
            SortKey::Disk => b.disk_bps.total_cmp(&a.disk_bps),
            SortKey::Pid => a.indices.first().cmp(&b.indices.first()),
            SortKey::Name => {
                if descending {
                    b.name.to_lowercase().cmp(&a.name.to_lowercase())
                } else {
                    a.name.to_lowercase().cmp(&b.name.to_lowercase())
                }
            }
        };
        order.then_with(|| a.name.cmp(&b.name))
    });
    groups
}

pub fn file_stem(name: &str) -> String {
    Path::new(name)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| name.to_string())
}

pub fn initials(name: &str) -> String {
    let mut chars = name
        .split(|c: char| c.is_whitespace() || matches!(c, '-' | '_' | '.' | '(' | ')'))
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.chars().next())
        .take(2);
    let first = chars.next().unwrap_or('?');
    let second = chars.next();
    match second {
        Some(second) => format!("{first}{second}").to_uppercase(),
        None => first.to_uppercase().to_string(),
    }
}
