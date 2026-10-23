//! Human formatting for the dashboard. Decimal units, tabular figures in the UI.

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else if value < 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.0} {}", UNITS[unit])
    }
}

pub fn format_rate(bytes_per_second: f64) -> String {
    if bytes_per_second < 0.5 {
        return "0 B/s".into();
    }
    format!("{}/s", format_bytes(bytes_per_second.round() as u64))
}

pub fn format_percent(percent: f32) -> String {
    if percent < 0.05 {
        "0%".into()
    } else if percent < 10.0 {
        format!("{percent:.1}%")
    } else {
        format!("{percent:.0}%")
    }
}

pub fn format_percent_exact(percent: f32) -> String {
    format!("{percent:.1}%")
}

pub fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = seconds % 86_400 / 3600;
    let minutes = seconds % 3600 / 60;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

pub fn format_count(count: usize) -> String {
    if count >= 10_000 {
        format!("{:.1}k", count as f64 / 1000.0)
    } else {
        count.to_string()
    }
}

/// `C:\Program Files\App\app.exe` -> `Program Files\App\app.exe` when it is too wide for the card.
pub fn compact_path(path: &str, max_chars: usize) -> String {
    if path.chars().count() <= max_chars {
        return path.to_string();
    }
    let tail: String = path.chars().rev().take(max_chars.saturating_sub(2)).collect::<String>().chars().rev().collect();
    format!("…{tail}")
}

pub fn format_start_time(ms: u64) -> String {
    if ms == 0 {
        return "—".into();
    }
    // Windows FILETIME epoch offset in Unix milliseconds.
    let unix_ms = ms as i64 - 11_644_473_600_000;
    let total_seconds = unix_ms / 1000;
    let seconds_of_day = total_seconds.rem_euclid(86_400);
    let hours = seconds_of_day / 3600;
    let minutes = seconds_of_day % 3600 / 60;
    format!("{hours:02}:{minutes:02}")
}
