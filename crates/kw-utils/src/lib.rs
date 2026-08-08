pub mod conv;

#[must_use]
pub fn format_time(secs: u64) -> String {
    let days = secs / 86_400;
    let hours = (secs % 86_400) / 3_600;
    let mins = (secs % 3_600) / 60;
    let secs = secs % 60;
    let mut buf = vec![];
    if days != 0 {
        buf.push(format!("{days}d"));
    }
    if hours != 0 {
        buf.push(format!("{hours}h"));
    }
    if mins != 0 {
        buf.push(format!("{mins}m"));
    }
    if secs != 0 {
        buf.push(format!("{secs}s"));
    }
    if buf.is_empty() {
        "0s".to_string()
    } else {
        buf.join(" ")
    }
}

#[must_use]
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1_024;
    const MB: u64 = KB * 1_024;
    const GB: u64 = MB * 1_024;
    const TB: u64 = GB * 1_024;

    fn format_unit(bytes: u64, unit: u64, suffix: &str) -> String {
        let whole = bytes.checked_div(unit).unwrap_or(0);
        let remainder = bytes.checked_rem(unit).unwrap_or(0);
        let tenths = remainder.saturating_mul(10).checked_div(unit).unwrap_or(0);
        format!("{whole}.{tenths} {suffix}")
    }

    match bytes {
        b if b >= TB => format_unit(b, TB, "TB"),
        b if b >= GB => format_unit(b, GB, "GB"),
        b if b >= MB => format_unit(b, MB, "MB"),
        b if b >= KB => format_unit(b, KB, "KB"),
        b => format!("{b} B"),
    }
}
