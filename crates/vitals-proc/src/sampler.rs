//! The sampler: one pass over the process table and the system counters, normalized into `Snapshot`.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    time::Instant,
};

use vitals_core::{file_stem, now_ms, Priority, ProcessRow, ProcessState, Snapshot, SystemStats};
use windows::{
    Win32::{
        Foundation::{FILETIME, UNICODE_STRING},
        System::{
            SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX},
            Threading::GetSystemTimes,
        },
    },
    Wdk::System::SystemInformation::{NtQuerySystemInformation, SystemProcessInformation},
};

use crate::win::{self, filetime, query_handle};

const PROTECTED: [&str; 11] = [
    "system",
    "registry",
    "memory compression",
    "secure system",
    "smss.exe",
    "csrss.exe",
    "wininit.exe",
    "winlogon.exe",
    "services.exe",
    "lsass.exe",
    "fontdrvhost.exe",
];

#[derive(Clone, Copy, Debug, Default)]
struct Times {
    kernel: u64,
    user: u64,
}

#[derive(Clone, Debug)]
struct Meta {
    creation: u64,
    path: Option<PathBuf>,
    display_name: String,
    elevated: bool,
}

struct Previous {
    system_idle: u64,
    system_total: u64,
    process: HashMap<u32, Times>,
    io: HashMap<u32, (u64, u64)>,
}

pub struct Sampler {
    previous: Option<Previous>,
    meta: HashMap<u32, Meta>,
    suspended: HashSet<u32>,
    last_at: Option<Instant>,
    cpu_history: Vec<f32>,
    memory_history: Vec<f32>,
}

impl Default for Sampler {
    fn default() -> Self {
        Self::new()
    }
}

impl Sampler {
    pub fn new() -> Self {
        Self {
            previous: None,
            meta: HashMap::new(),
            suspended: HashSet::new(),
            last_at: None,
            cpu_history: Vec::new(),
            memory_history: Vec::new(),
        }
    }

    /// Marks a task this app suspended, so the row can say "Suspended" until the process exits.
    pub fn mark_suspended(&mut self, pid: u32, suspended: bool) {
        if suspended {
            self.suspended.insert(pid);
        } else {
            self.suspended.remove(&pid);
        }
    }

    pub fn sample(&mut self) -> Snapshot {
        let now = Instant::now();
        let elapsed = self.last_at.map(|last| (now - last).as_secs_f64().max(0.05)).unwrap_or(0.0);
        self.last_at = Some(now);

        let system = system_times();
        let memory = memory_status();
        let uptime = unsafe { windows::Win32::System::SystemInformation::GetTickCount64() } / 1000;

        // One `NtQuerySystemInformation(SystemProcessInformation)` call returns the whole process table:
        // times, memory, I/O and thread counts for every process. Per-process Win32 queries cost several
        // times more CPU and produce a visible spike on every sample, which is what the old sampler did.
        let entries = system_processes().unwrap_or_default();
        let mut processes = Vec::with_capacity(entries.len());
        let mut current_times = HashMap::with_capacity(entries.len());
        let mut current_io = HashMap::with_capacity(entries.len());

        for entry in entries {
            let pid = entry.pid;
            current_times.insert(pid, Times { kernel: entry.kernel, user: entry.user });
            current_io.insert(pid, (entry.read, entry.write));

            let meta = match self.meta.get(&pid) {
                Some(meta) if meta.creation == entry.creation => meta.clone(),
                _ => {
                    // Paths, display names and elevation never change for one process instance, so this
                    // expensive lookup only runs once per new process.
                    let handle = query_handle(pid);
                    let path = handle.as_ref().and_then(win::process_path);
                    let display = path.as_deref().map(win::display_name).unwrap_or_else(|| file_stem(&entry.name));
                    let elevated = handle.as_ref().is_some_and(win::is_elevated);
                    let meta = Meta { creation: entry.creation, path, display_name: display, elevated };
                    self.meta.insert(pid, meta.clone());
                    meta
                }
            };

            let is_system = pid <= 4 || PROTECTED.contains(&entry.name.to_lowercase().as_str());
            processes.push(ProcessRow {
                pid,
                parent_pid: entry.parent_pid,
                name: entry.name,
                display_name: meta.display_name,
                path: meta.path,
                cpu: 0.0,
                memory: entry.memory,
                disk_read_bps: 0.0,
                disk_write_bps: 0.0,
                threads: entry.threads,
                handles: entry.handles,
                start_time_ms: meta.creation,
                priority: priority_from_base(entry.base_priority),
                state: if self.suspended.contains(&pid) { ProcessState::Suspended } else { ProcessState::Running },
                user: String::new(),
                elevated: meta.elevated,
                system: is_system,
            });
        }
        self.meta.retain(|pid, _| current_times.contains_key(pid));

        let mut system_cpu = 0.0;
        let system_total_delta = match &self.previous {
            Some(previous) => {
                let idle = system.idle.saturating_sub(previous.system_idle);
                let total = system.total.saturating_sub(previous.system_total);
                if total > 0 {
                    system_cpu = (total.saturating_sub(idle)) as f32 / total as f32 * 100.0;
                    total
                } else {
                    0
                }
            }
            None => 0,
        };

        if let Some(previous) = &self.previous {
            for process in &mut processes {
                if let (Some(now), Some(before)) = (current_times.get(&process.pid), previous.process.get(&process.pid)) {
                    let used = (now.kernel + now.user).saturating_sub(before.kernel + before.user);
                    if system_total_delta > 0 {
                        process.cpu = (used as f32 / system_total_delta as f32 * 100.0).clamp(0.0, 100.0);
                    }
                }
                if elapsed > 0.0
                    && let (Some(&(read, write)), Some(&(old_read, old_write))) =
                        (current_io.get(&process.pid), previous.io.get(&process.pid))
                {
                    process.disk_read_bps = read.saturating_sub(old_read) as f64 / elapsed;
                    process.disk_write_bps = write.saturating_sub(old_write) as f64 / elapsed;
                }
            }
        }
        processes.sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then_with(|| b.memory.cmp(&a.memory)));

        let memory_used = memory.ullTotalPhys.saturating_sub(memory.ullAvailPhys);
        let memory_percent =
            if memory.ullTotalPhys == 0 { 0.0 } else { memory_used as f32 / memory.ullTotalPhys as f32 * 100.0 };
        push_history(&mut self.cpu_history, system_cpu);
        push_history(&mut self.memory_history, memory_percent);

        let stats = SystemStats {
            cpu_percent: system_cpu,
            memory_used,
            memory_total: memory.ullTotalPhys,
            process_count: processes.len(),
            thread_count: processes.iter().map(|p| p.threads).sum(),
            uptime_secs: uptime,
            cpu_history: self.cpu_history.clone(),
            memory_history: self.memory_history.clone(),
        };

        self.previous = Some(Previous {
            system_idle: system.idle,
            system_total: system.total,
            process: current_times,
            io: current_io,
        });

        Snapshot { captured_ms: now_ms(), processes, system: stats }
    }
}

fn push_history(values: &mut Vec<f32>, value: f32) {
    values.push(value);
    if values.len() > 60 {
        values.remove(0);
    }
}

/// One process row as returned by the single-process-table syscall.
struct NtProcess {
    pid: u32,
    parent_pid: u32,
    name: String,
    creation: u64,
    kernel: u64,
    user: u64,
    memory: u64,
    read: u64,
    write: u64,
    threads: u32,
    handles: u32,
    base_priority: i32,
}

/// The documented layout of `SYSTEM_PROCESS_INFORMATION` (x64). Thread entries follow the header inside
/// `NextEntryOffset`, so the fields below are all that is needed.
#[repr(C)]
#[derive(Clone, Copy)]
struct RawProcessInformation {
    next_entry_offset: u32,
    number_of_threads: u32,
    working_set_private_size: i64,
    hard_fault_count: u32,
    number_of_threads_high_watermark: u32,
    cycle_time: i64,
    create_time: i64,
    user_time: i64,
    kernel_time: i64,
    image_name: UNICODE_STRING,
    base_priority: i32,
    unique_process_id: usize,
    inherited_from_unique_process_id: usize,
    handle_count: u32,
    session_id: u32,
    unique_process_key: usize,
    peak_virtual_size: usize,
    virtual_size: usize,
    page_fault_count: u32,
    peak_working_set_size: usize,
    working_set_size: usize,
    quota_peak_paged_pool_usage: usize,
    quota_paged_pool_usage: usize,
    quota_peak_non_paged_pool_usage: usize,
    quota_non_paged_pool_usage: usize,
    pagefile_usage: usize,
    peak_pagefile_usage: usize,
    private_page_count: usize,
    read_operation_count: i64,
    write_operation_count: i64,
    other_operation_count: i64,
    read_transfer_count: i64,
    write_transfer_count: i64,
    other_transfer_count: i64,
}

const STATUS_INFO_LENGTH_MISMATCH: i32 = 0xC000_0004u32 as i32;

fn system_processes() -> Option<Vec<NtProcess>> {
    let mut size = 1u32 << 20;
    let mut buffer = Vec::<u64>::new();
    let mut ok = false;
    for _ in 0..4 {
        buffer.resize((size as usize).div_ceil(8), 0);
        let mut needed = 0u32;
        // SAFETY: the buffer is at least `size` bytes and lives until the table has been parsed below.
        let status = unsafe {
            NtQuerySystemInformation(SystemProcessInformation, buffer.as_mut_ptr().cast(), size, &mut needed)
        };
        if status.0 >= 0 {
            ok = true;
            break;
        }
        if status.0 == STATUS_INFO_LENGTH_MISMATCH {
            size = needed.max(size.saturating_mul(2));
            continue;
        }
        log::warn!("NtQuerySystemInformation(SystemProcessInformation) failed: 0x{:08x}", status.0);
        return None;
    }
    if !ok {
        log::warn!("NtQuerySystemInformation(SystemProcessInformation) did not fit in the buffer");
        return None;
    }

    let bytes = buffer.len() * 8;
    let base = buffer.as_ptr().cast::<u8>();
    let mut offset = 0usize;
    let mut result = Vec::with_capacity(256);
    while offset + size_of::<RawProcessInformation>() <= bytes {
        // SAFETY: `offset` stays inside the buffer and the syscall filled it with this exact layout.
        let raw = unsafe { &*base.add(offset).cast::<RawProcessInformation>() };
        let pid = raw.unique_process_id as u32;
        if pid != 0 {
            let name = if raw.image_name.Buffer.is_null() || raw.image_name.Length == 0 {
                String::new()
            } else {
                let length = raw.image_name.Length as usize / 2;
                // SAFETY: `ImageName.Buffer` points into the buffer returned by the syscall.
                let text = unsafe { std::slice::from_raw_parts(raw.image_name.Buffer.0, length) };
                win::from_wide(text)
            };
            result.push(NtProcess {
                pid,
                parent_pid: raw.inherited_from_unique_process_id as u32,
                name,
                creation: raw.create_time as u64,
                kernel: raw.kernel_time.max(0) as u64,
                user: raw.user_time.max(0) as u64,
                memory: raw.private_page_count as u64,
                read: raw.read_transfer_count.max(0) as u64,
                write: raw.write_transfer_count.max(0) as u64,
                threads: raw.number_of_threads,
                handles: raw.handle_count,
                base_priority: raw.base_priority,
            });
        }
        if raw.next_entry_offset == 0 {
            break;
        }
        offset += raw.next_entry_offset as usize;
    }
    Some(result)
}

fn priority_from_base(base: i32) -> Priority {
    match base {
        4 => Priority::Idle,
        6 => Priority::BelowNormal,
        8 => Priority::Normal,
        10 => Priority::AboveNormal,
        13 => Priority::High,
        24 => Priority::Realtime,
        _ => Priority::Unknown,
    }
}

struct SystemTimes {
    idle: u64,
    total: u64,
}

fn system_times() -> SystemTimes {
    let mut idle = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    if unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) }.is_err() {
        return SystemTimes { idle: 0, total: 0 };
    }
    let kernel = filetime(kernel);
    let user = filetime(user);
    // Kernel time includes idle time.
    SystemTimes { idle: filetime(idle), total: kernel + user }
}

fn memory_status() -> MEMORYSTATUSEX {
    let mut status = MEMORYSTATUSEX { dwLength: size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    let _ = unsafe { GlobalMemoryStatusEx(&mut status) };
    status
}
