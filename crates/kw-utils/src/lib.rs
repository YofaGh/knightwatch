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

pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1_024;
    const MB: u64 = KB * 1_024;
    const GB: u64 = MB * 1_024;
    const TB: u64 = GB * 1_024;
    match bytes {
        b if b >= TB => format!("{:.1} TB", b as f64 / TB as f64),
        b if b >= GB => format!("{:.1} GB", b as f64 / GB as f64),
        b if b >= MB => format!("{:.1} MB", b as f64 / MB as f64),
        b if b >= KB => format!("{:.1} KB", b as f64 / KB as f64),
        b => format!("{b} B"),
    }
}