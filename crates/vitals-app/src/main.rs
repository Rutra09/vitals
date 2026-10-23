#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod image_png;
mod logging;
mod preview;
mod selftest;

use std::{
    path::PathBuf,
    process::ExitCode,
};

use anyhow::Result;
use vitals_sys::{install, instance};

const HELP: &str = "\
Vitals - a calm, fast process manager for Windows 11

USAGE:
    vitals [OPTIONS]

OPTIONS:
    --background        Start in the notification area without opening the dashboard
    --preview [DIR]     Render deterministic PNG previews (default: previews)
    --selftest          Run internal checks and render smoke tests
    --install           Install for the current user and start in the background
    --uninstall         Remove Vitals; add --purge to delete settings and logs
    --quit              Ask the running copy to exit
    --version           Print the version
    -h, --help          Show this help

The dashboard opens with Ctrl+Shift+Esc while Vitals is running. Windows gets its
Task Manager shortcut back as soon as Vitals exits.
";

enum Command {
    Run { background: bool },
    Preview(PathBuf),
    Selftest,
    Install,
    Uninstall { purge: bool },
    Quit,
    Help,
    Version,
}

fn parse() -> Result<Command, String> {
    let mut args = std::env::args().skip(1);
    let mut command = Command::Run { background: false };
    let mut background = false;
    let mut preview = false;
    let mut preview_dir: Option<PathBuf> = None;
    let mut purge = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--background" => background = true,
            "--preview" => {
                preview = true;
                if let Some(next) = args.next() {
                    if next.starts_with('-') {
                        return Err(format!("expected a folder after --preview, got {next}"));
                    }
                    preview_dir = Some(PathBuf::from(next));
                }
            }
            "--selftest" => command = Command::Selftest,
            "--install" => command = Command::Install,
            "--uninstall" => command = Command::Uninstall { purge: false },
            "--purge" => purge = true,
            "--quit" => command = Command::Quit,
            "--version" | "-V" => command = Command::Version,
            "--help" | "-h" => command = Command::Help,
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    if preview {
        command = Command::Preview(preview_dir.unwrap_or_else(|| PathBuf::from("previews")));
    }
    if background {
        match command {
            Command::Run { .. } => command = Command::Run { background: true },
            Command::Help | Command::Version => {}
            _ => return Err("--background only applies when starting Vitals".into()),
        }
    }
    if purge {
        match &mut command {
            Command::Uninstall { purge: enabled } => *enabled = true,
            _ => return Err("--purge must be used with --uninstall".into()),
        }
    }
    Ok(command)
}

fn show_message(text: &str, error: bool) {
    #[cfg(not(debug_assertions))]
    {
        use windows::{
            Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MessageBoxW},
            core::{HSTRING, w},
        };
        let icon = if error { MB_ICONERROR } else { MB_ICONINFORMATION };
        unsafe {
            MessageBoxW(None, &HSTRING::from(text), w!("Vitals"), MB_OK | icon);
        }
    }
    #[cfg(debug_assertions)]
    {
        if error {
            eprintln!("{text}");
        } else {
            println!("{text}");
        }
    }
}

fn start_background() {
    if let Some(exe) = install::installed_exe() {
        let _ = std::process::Command::new(exe).arg("--background").spawn();
    }
}

fn main() -> ExitCode {
    logging::init();
    let command = match parse() {
        Ok(command) => command,
        Err(error) => {
            show_message(&format!("{error}\n\n{HELP}"), true);
            return ExitCode::from(2);
        }
    };

    match command {
        Command::Help => {
            show_message(HELP, false);
            ExitCode::SUCCESS
        }
        Command::Version => {
            show_message(&format!("Vitals {}", env!("CARGO_PKG_VERSION")), false);
            ExitCode::SUCCESS
        }
        Command::Preview(dir) => match preview::run(&dir) {
            Ok(()) => {
                show_message(&format!("Previews written to {}", dir.display()), false);
                ExitCode::SUCCESS
            }
            Err(error) => {
                log::error!("preview failed: {error:#}");
                show_message(&format!("Could not render previews:\n\n{error:#}"), true);
                ExitCode::FAILURE
            }
        },
        Command::Selftest => match selftest::run() {
            Ok(()) => {
                show_message("Vitals selftest passed.", false);
                ExitCode::SUCCESS
            }
            Err(error) => {
                log::error!("selftest failed: {error:#}");
                show_message(&format!("Vitals selftest failed:\n\n{error:#}"), true);
                ExitCode::FAILURE
            }
        },
        Command::Install => match install::install() {
            Ok(path) => {
                log::info!("installed to {}", path.display());
                start_background();
                show_message(&format!("Vitals is installed and running in the notification area.\n\n{}", path.display()), false);
                ExitCode::SUCCESS
            }
            Err(error) => {
                log::error!("install failed: {error:#}");
                show_message(&format!("Vitals could not be installed:\n\n{error:#}"), true);
                ExitCode::FAILURE
            }
        },
        Command::Uninstall { purge } => {
            let _ = instance::request_quit();
            std::thread::sleep(std::time::Duration::from_millis(650));
            match install::uninstall(purge) {
                Ok(()) => {
                    show_message("Vitals has been removed.", false);
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    log::error!("uninstall failed: {error:#}");
                    show_message(&format!("Vitals could not be removed:\n\n{error:#}"), true);
                    ExitCode::FAILURE
                }
            }
        }
        Command::Quit => match instance::request_quit() {
            Ok(true) => {
                show_message("Vitals is closing.", false);
                ExitCode::SUCCESS
            }
            Ok(false) => {
                show_message("Vitals is not running.", false);
                ExitCode::SUCCESS
            }
            Err(error) => {
                log::error!("quit request failed: {error:#}");
                show_message(&format!("Could not ask Vitals to quit:\n\n{error:#}"), true);
                ExitCode::FAILURE
            }
        },
        Command::Run { background } => match app::run(background) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                log::error!("Vitals stopped with an error: {error:#}");
                show_message(&format!("Vitals could not start:\n\n{error:#}"), true);
                ExitCode::FAILURE
            }
        },
    }
}
