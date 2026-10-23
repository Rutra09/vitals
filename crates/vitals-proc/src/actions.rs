//! Process control. Every action takes a PID, never a name; the UI confirms destructive ones first.

use anyhow::{Context, Result};
use vitals_core::Priority;
use windows::Win32::{
    Foundation::HANDLE,
    System::Threading::{
        ABOVE_NORMAL_PRIORITY_CLASS, BELOW_NORMAL_PRIORITY_CLASS, HIGH_PRIORITY_CLASS, IDLE_PRIORITY_CLASS,
        NORMAL_PRIORITY_CLASS, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_INFORMATION,
        PROCESS_TERMINATE, REALTIME_PRIORITY_CLASS, SetPriorityClass, TerminateProcess,
    },
};

use crate::win::open_process;

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtSuspendProcess(process: HANDLE) -> i32;
    fn NtResumeProcess(process: HANDLE) -> i32;
}

pub fn end_task(pid: u32) -> Result<()> {
    let process = open_process(pid, PROCESS_TERMINATE)
        .with_context(|| format!("cannot open PID {pid}; it may run elevated or have already exited"))?;
    unsafe { TerminateProcess(process.0, 1) }.with_context(|| format!("cannot end PID {pid}"))
}

pub fn suspend(pid: u32) -> Result<()> {
    let process = open_process(pid, PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SET_INFORMATION)
        .with_context(|| format!("cannot open PID {pid}"))?;
    let status = unsafe { NtSuspendProcess(process.0) };
    if status >= 0 { Ok(()) } else { anyhow::bail!("NtSuspendProcess failed with 0x{status:08X}") }
}

pub fn resume(pid: u32) -> Result<()> {
    let process = open_process(pid, PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SET_INFORMATION)
        .with_context(|| format!("cannot open PID {pid}"))?;
    let status = unsafe { NtResumeProcess(process.0) };
    if status >= 0 { Ok(()) } else { anyhow::bail!("NtResumeProcess failed with 0x{status:08X}") }
}

pub fn set_priority(pid: u32, priority: Priority) -> Result<()> {
    let process = open_process(pid, PROCESS_SET_INFORMATION).with_context(|| format!("cannot open PID {pid}"))?;
    let class = match priority {
        Priority::Idle => IDLE_PRIORITY_CLASS,
        Priority::BelowNormal => BELOW_NORMAL_PRIORITY_CLASS,
        Priority::Normal => NORMAL_PRIORITY_CLASS,
        Priority::AboveNormal => ABOVE_NORMAL_PRIORITY_CLASS,
        Priority::High => HIGH_PRIORITY_CLASS,
        Priority::Realtime => REALTIME_PRIORITY_CLASS,
        Priority::Unknown => return Ok(()),
    };
    unsafe { SetPriorityClass(process.0, class) }.with_context(|| format!("cannot set priority of PID {pid}"))
}
