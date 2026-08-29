export function formatBytes(bytes) {
  if (bytes == null) return "—";
  const KB = 1024,
    MB = KB * 1024,
    GB = MB * 1024,
    TB = GB * 1024;
  if (bytes >= TB) return (bytes / TB).toFixed(1) + " TB";
  if (bytes >= GB) return (bytes / GB).toFixed(1) + " GB";
  if (bytes >= MB) return (bytes / MB).toFixed(1) + " MB";
  if (bytes >= KB) return (bytes / KB).toFixed(1) + " KB";
  return bytes + " B";
}

export function formatTime(secs) {
  const days = Math.floor(secs / 86400);
  const hours = Math.floor((secs % 86400) / 3600);
  const mins = Math.floor((secs % 3600) / 60);
  const s = secs % 60;
  const parts = [];
  if (days) parts.push(`${days}d`);
  if (hours) parts.push(`${hours}h`);
  if (mins) parts.push(`${mins}m`);
  if (s) parts.push(`${s}s`);
  return parts.length ? parts.join(" ") : "0s";
}

export function fmtTimestamp(ts) {
  if (!ts) return "—";
  try {
    return new Date(ts).toLocaleTimeString(undefined, {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
  } catch {
    return ts;
  }
}

// ── History helpers ──────────────────────────────────────────────────────

/** Full local date + time, for history rows that can span multiple days. */
export function fmtDateTime(ts) {
  if (!ts) return "—";
  try {
    return new Date(ts).toLocaleString(undefined, {
      year: "numeric",
      month: "short",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
  } catch {
    return ts;
  }
}

/**
 * RFC3339 timestamp -> value usable in <input type="datetime-local">,
 * expressed in the browser's local time.
 */
export function isoToLocalInput(iso) {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  const pad = (n) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(
    d.getHours(),
  )}:${pad(d.getMinutes())}`;
}

/**
 * <input type="datetime-local"> value -> RFC3339 (UTC) string,
 * or null if empty/invalid. Safe to compare lexically against the
 * server's stored event timestamps.
 */
export function localInputToIso(value) {
  if (!value) return null;
  const d = new Date(value);
  return Number.isNaN(d.getTime()) ? null : d.toISOString();
}
