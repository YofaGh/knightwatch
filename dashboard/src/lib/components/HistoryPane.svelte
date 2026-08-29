<script>
  import { apiFetch } from "../api.js";
  import {
    fmtDateTime,
    isoToLocalInput,
    localInputToIso,
  } from "../utils/format.js";

  let {
    active = false,
    isAuthenticated = false,
    onstatus = () => {},
  } = $props();

  // ── Static metadata ──────────────────────────────────────────────────
  const SOURCES = [
    { value: "ProcessTracker", label: "Process Tracker", color: "#60a5fa" },
    { value: "SystemResources", label: "System Resources", color: "#34d399" },
    { value: "Systemd", label: "Systemd", color: "#a78bfa" },
    { value: "DockerTracker", label: "Docker", color: "#22d3ee" },
  ];
  const SOURCE_COLOR = Object.fromEntries(
    SOURCES.map((s) => [s.value, s.color]),
  );
  const SOURCE_LABEL = Object.fromEntries(
    SOURCES.map((s) => [s.value, s.label]),
  );

  // Events worth calling out visually — failures, kills, thresholds crossed.
  const NOTABLE_EVENTS = new Set([
    "process.process_killed",
    "process.root_exited",
    "docker.container_oom_killed",
    "docker.container_action_result",
    "docker.container_status_changed",
    "docker.container_health_changed",
    "systemd.unit_failed",
    "resources.cpu_threshold_exceeded",
    "resources.memory_threshold_exceeded",
    "resources.disk_threshold_exceeded",
    "resources.battery_low",
  ]);

  const QUICK_RANGES = [
    { label: "1h", ms: 60 * 60 * 1000 },
    { label: "24h", ms: 24 * 60 * 60 * 1000 },
    { label: "7d", ms: 7 * 24 * 60 * 60 * 1000 },
    { label: "30d", ms: 30 * 24 * 60 * 60 * 1000 },
  ];

  // ── Mode ──────────────────────────────────────────────────────────────
  let mode = $state("server"); // "server" | "upload"

  // ── Shared filters ────────────────────────────────────────────────────
  let filterSource = $state("");
  let sinceLocal = $state("");
  let untilLocal = $state("");
  let search = $state("");
  let pageSize = $state(100);

  let sinceIso = $derived(localInputToIso(sinceLocal));
  let untilIso = $derived(localInputToIso(untilLocal));

  function setQuickRange(ms) {
    const now = new Date();
    sinceLocal = isoToLocalInput(new Date(now.getTime() - ms).toISOString());
    untilLocal = "";
    if (mode === "server") loadFirstPage();
  }

  function clearRange() {
    sinceLocal = "";
    untilLocal = "";
    if (mode === "server") loadFirstPage();
  }

  function applyFilters() {
    if (mode === "server") loadFirstPage();
  }

  function matches(ev) {
    if (filterSource && ev.source !== filterSource) return false;
    if (sinceIso && ev.timestamp < sinceIso) return false;
    if (untilIso && ev.timestamp > untilIso) return false;
    if (search) {
      const q = search.toLowerCase();
      const hay =
        ev.event.toLowerCase() +
        " " +
        JSON.stringify(ev.data ?? {}).toLowerCase();
      if (!hay.includes(q)) return false;
    }
    return true;
  }

  // ── Server-backed history (infinite scroll) ──────────────────────────
  let events = $state([]);
  let loading = $state(false);
  let hasMore = $state(true);
  let loadErr = $state(null);
  let didInitialLoad = $state(false);

  function buildParams(extra = {}) {
    const params = new URLSearchParams();
    if (filterSource) params.set("source", filterSource);
    if (sinceIso) params.set("since", sinceIso);
    for (const [k, v] of Object.entries(extra)) {
      if (v !== null && v !== undefined && v !== "") params.set(k, v);
    }
    return params;
  }

  // Multiple events (e.g. a restart's `initial_snapshot` + `battery_state_changed`)
  // can share the exact same timestamp, so cursor pagination can't just drop the
  // first row of the next page — it has to dedupe by full signature, or a tie at
  // a page boundary re-fetches the same group forever.
  function eventKey(ev) {
    return `${ev.timestamp}|${ev.source}|${ev.event}|${JSON.stringify(ev.data)}`;
  }

  let loadedKeys = new Set();
  const HARD_CAP = 5000; // circuit breaker, just in case

  async function loadFirstPage() {
    if (!isAuthenticated) return;
    loading = true;
    loadErr = null;
    hasMore = true;
    try {
      const params = buildParams({ until: untilIso, limit: pageSize });
      const res = await apiFetch(`/api/history?${params}`);
      if (!res.ok) throw new Error(`request failed (${res.status})`);
      const data = await res.json();
      // The server returns newest-first only when no `until` is given.
      // A custom "To" filter makes it return the matching window oldest-first.
      events = untilIso ? [...data].reverse() : data;
      loadedKeys = new Set(events.map(eventKey));
      hasMore = data.length >= pageSize;
      onstatus(
        `${data.length} event${data.length === 1 ? "" : "s"} loaded`,
        false,
      );
    } catch (e) {
      loadErr = e?.message ?? String(e);
      events = [];
      loadedKeys = new Set();
      hasMore = false;
      onstatus("Failed to load history", true);
    } finally {
      loading = false;
    }
  }

  async function loadMore() {
    if (!isAuthenticated || loading || !hasMore || events.length === 0) return;
    if (events.length >= HARD_CAP) {
      hasMore = false;
      return;
    }
    loading = true;
    try {
      const cursor = events[events.length - 1].timestamp;
      // Fetch extra beyond pageSize as headroom for timestamp ties sitting
      // right at the cursor boundary.
      const params = buildParams({ until: cursor, limit: pageSize + 50 });
      const res = await apiFetch(`/api/history?${params}`);
      if (!res.ok) throw new Error(`request failed (${res.status})`);
      const data = await res.json();
      const page = [...data].reverse(); // explicit `until` -> ascending from server
      const fresh = page.filter((ev) => !loadedKeys.has(eventKey(ev)));
      if (fresh.length === 0) {
        // Nothing new past this cursor: either truly done, or every event at
        // the boundary is a tie we've already loaded. Either way, stop.
        hasMore = false;
      } else {
        const take = fresh.slice(0, pageSize);
        take.forEach((ev) => loadedKeys.add(eventKey(ev)));
        events = [...events, ...take];
        hasMore = fresh.length >= pageSize;
      }
    } catch (e) {
      loadErr = e?.message ?? String(e);
      hasMore = false;
    } finally {
      loading = false;
    }
  }

  // Infinite scroll sentinel
  let sentinelEl = $state(null);
  let observer;
  $effect(() => {
    if (mode !== "server" || !sentinelEl) return;
    observer?.disconnect();
    observer = new IntersectionObserver(
      (entries) => {
        if (entries[0]?.isIntersecting) loadMore();
      },
      { rootMargin: "200px" },
    );
    observer.observe(sentinelEl);
    return () => observer?.disconnect();
  });

  $effect(() => {
    if (active && mode === "server" && isAuthenticated && !didInitialLoad) {
      didInitialLoad = true;
      loadFirstPage();
    }
  });

  // ── Uploaded history ──────────────────────────────────────────────────
  let uploadedEvents = $state([]);
  let uploadFileName = $state("");
  let uploadErr = $state(null);

  async function handleFileChange(e) {
    const file = e.currentTarget.files?.[0];
    e.currentTarget.value = ""; // allow re-selecting the same file later
    if (!file) return;
    uploadErr = null;
    try {
      const text = await file.text();
      let parsed;
      try {
        parsed = JSON.parse(text);
        if (!Array.isArray(parsed)) throw new Error("not an array");
      } catch {
        // Fall back to the raw JSONL log format (one event per line),
        // e.g. a knightwatch-events-YYYY-MM-DD.log pulled off the server.
        parsed = text
          .split("\n")
          .map((l) => l.trim())
          .filter(Boolean)
          .map((l) => JSON.parse(l));
      }
      const valid = parsed.filter(
        (ev) =>
          ev &&
          typeof ev.event === "string" &&
          typeof ev.timestamp === "string",
      );
      valid.sort((a, b) => b.timestamp.localeCompare(a.timestamp));
      uploadedEvents = valid;
      uploadFileName = file.name;
      onstatus(
        `${valid.length} event${valid.length === 1 ? "" : "s"} from ${file.name}`,
        false,
      );
    } catch (err) {
      uploadErr = `Couldn't parse file: ${err?.message ?? err}`;
      uploadedEvents = [];
      onstatus("Failed to parse uploaded file", true);
    }
  }

  function clearUpload() {
    uploadedEvents = [];
    uploadFileName = "";
    uploadErr = null;
  }

  // ── Display list ──────────────────────────────────────────────────────
  let displayed = $derived(
    mode === "server"
      ? events.filter((ev) => !search || matches(ev))
      : uploadedEvents.filter(matches),
  );

  // ── Row expand/collapse ─────────────────────────────────────────────
  let expandedRows = $state(new Set());
  function toggleRow(i) {
    const next = new Set(expandedRows);
    if (next.has(i)) next.delete(i);
    else next.add(i);
    expandedRows = next;
  }

  // ── Export ────────────────────────────────────────────────────────────
  function triggerDownload(content, filename, mimeType) {
    const blob = new Blob([content], { type: mimeType });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = filename;
    a.click();
    URL.revokeObjectURL(url);
  }

  function rangeSuffix() {
    const s = sinceLocal || "start";
    const u = untilLocal || "now";
    return `${s}_to_${u}`.replace(/:/g, "-");
  }

  function toCsv(list) {
    const esc = (v) => {
      const s = String(v ?? "");
      return /[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
    };
    const header = "event,timestamp,source,data";
    const rows = list.map((ev) =>
      [ev.event, ev.timestamp, ev.source, JSON.stringify(ev.data)]
        .map(esc)
        .join(","),
    );
    return [header, ...rows].join("\n");
  }

  async function downloadServer(format) {
    loading = true;
    try {
      // Ignores the on-screen page size — exports everything matching the filters.
      const params = buildParams({ until: untilIso });
      const res = await apiFetch(`/api/history?${params}`);
      if (!res.ok) throw new Error(`request failed (${res.status})`);
      const data = await res.json();
      const filtered = data.filter(matches);
      const name = `knightwatch-history-${rangeSuffix()}.${format}`;
      if (format === "csv") triggerDownload(toCsv(filtered), name, "text/csv");
      else
        triggerDownload(
          JSON.stringify(filtered, null, 2),
          name,
          "application/json",
        );
    } catch {
      onstatus("Download failed", true);
    } finally {
      loading = false;
    }
  }

  function downloadUpload(format) {
    const name = `knightwatch-history-filtered-${rangeSuffix()}.${format}`;
    if (format === "csv") triggerDownload(toCsv(displayed), name, "text/csv");
    else
      triggerDownload(
        JSON.stringify(displayed, null, 2),
        name,
        "application/json",
      );
  }
</script>

<div class="history-pane">
  {#if !isAuthenticated}
    <div class="signin-notice">
      <p>Sign in to browse server-side event history.</p>
      <p class="hint">
        You can still load a previously downloaded export below once signed in,
        or view one you already have on disk after authenticating.
      </p>
    </div>
  {:else}
    <div class="controls">
      <div class="mode-toggle" role="tablist" aria-label="History source">
        <button
          class="mode-btn"
          role="tab"
          aria-selected={mode === "server"}
          onclick={() => (mode = "server")}
        >
          Server
        </button>
        <button
          class="mode-btn"
          role="tab"
          aria-selected={mode === "upload"}
          onclick={() => (mode = "upload")}
        >
          Uploaded file
        </button>
      </div>

      <div class="filters">
        <select bind:value={filterSource} onchange={applyFilters}>
          <option value="">All sources</option>
          {#each SOURCES as s (s.value)}
            <option value={s.value}>{s.label}</option>
          {/each}
        </select>

        <div class="quick-ranges">
          {#each QUICK_RANGES as r (r.label)}
            <button class="chip" onclick={() => setQuickRange(r.ms)}
              >{r.label}</button
            >
          {/each}
          <button class="chip" onclick={clearRange}>All time</button>
        </div>

        <label class="dt-field">
          <span>From</span>
          <input
            type="datetime-local"
            bind:value={sinceLocal}
            onchange={applyFilters}
          />
        </label>
        <label class="dt-field">
          <span>To</span>
          <input
            type="datetime-local"
            bind:value={untilLocal}
            onchange={applyFilters}
          />
        </label>

        <input
          class="search-input"
          type="search"
          placeholder="Search event name or payload…"
          bind:value={search}
        />

        {#if mode === "server"}
          <label class="dt-field">
            <span>Page size</span>
            <input
              type="number"
              min="10"
              max="1000"
              step="10"
              bind:value={pageSize}
              onchange={applyFilters}
            />
          </label>
        {/if}
      </div>

      <div class="actions">
        {#if mode === "server"}
          <button
            class="btn"
            onclick={() => downloadServer("json")}
            disabled={loading}
          >
            Download JSON
          </button>
          <button
            class="btn"
            onclick={() => downloadServer("csv")}
            disabled={loading}
          >
            Download CSV
          </button>
          <span class="hint"
            >exports all matching events, ignores page size</span
          >
        {:else}
          <label class="btn file-btn">
            {uploadFileName ? "Replace file" : "Upload file"}
            <input
              type="file"
              accept=".json,.log,.txt"
              onchange={handleFileChange}
              hidden
            />
          </label>
          {#if uploadFileName}
            <span class="upload-name">{uploadFileName}</span>
            <button class="btn" onclick={() => downloadUpload("json")}>
              Download filtered JSON
            </button>
            <button class="btn" onclick={() => downloadUpload("csv")}>
              Download filtered CSV
            </button>
            <button class="btn btn-ghost" onclick={clearUpload}>Clear</button>
          {/if}
        {/if}
      </div>
    </div>

    {#if mode === "upload" && uploadErr}
      <div class="error-banner">{uploadErr}</div>
    {/if}
    {#if mode === "server" && loadErr}
      <div class="error-banner">{loadErr}</div>
    {/if}

    <div class="event-list">
      {#if displayed.length === 0 && !loading}
        <div class="empty-state">
          {#if mode === "upload" && !uploadFileName}
            Upload a history export (.json) or a raw log file (.log) to browse
            it here.
          {:else}
            No events match the current filters.
          {/if}
        </div>
      {/if}

      {#each displayed as ev, i (mode + "-" + i)}
        <div class="event-row" class:notable={NOTABLE_EVENTS.has(ev.event)}>
          <button class="event-summary" onclick={() => toggleRow(i)}>
            <span
              class="source-badge"
              style={`--badge-color:${SOURCE_COLOR[ev.source] ?? "#71717a"}`}
            >
              {SOURCE_LABEL[ev.source] ?? ev.source}
            </span>
            <span class="event-name">{ev.event}</span>
            <span class="event-time">{fmtDateTime(ev.timestamp)}</span>
            <span class="expand-icon" aria-hidden="true">
              {expandedRows.has(i) ? "▾" : "▸"}
            </span>
          </button>
          {#if expandedRows.has(i)}
            <pre class="event-data">{JSON.stringify(ev.data, null, 2)}</pre>
          {/if}
        </div>
      {/each}

      {#if mode === "server"}
        <div class="sentinel" bind:this={sentinelEl}></div>
        {#if loading}
          <div class="loading-row">Loading…</div>
        {:else if !hasMore && events.length > 0}
          <div class="loading-row">End of history</div>
        {/if}
      {/if}
    </div>
  {/if}
</div>

<style>
  .history-pane {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    gap: 0.75rem;
    padding: 1rem 1.25rem;
  }

  .signin-notice {
    margin: auto;
    text-align: center;
    color: var(--text-muted);
    max-width: 32rem;
  }
  .signin-notice p {
    margin: 0.35rem 0;
  }

  /* ── Controls ─────────────────────────────────────────── */
  .controls {
    display: flex;
    flex-direction: column;
    gap: 0.65rem;
    flex-shrink: 0;
  }

  .mode-toggle {
    display: inline-flex;
    align-self: flex-start;
    gap: 4px;
    padding: 4px;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 999px;
  }
  .mode-btn {
    background: transparent;
    border: none;
    color: var(--text-muted);
    font-family: inherit;
    font-size: 0.75rem;
    font-weight: 600;
    padding: 0.4rem 0.85rem;
    border-radius: 999px;
    cursor: pointer;
    transition:
      background 0.15s ease,
      color 0.15s ease;
  }
  .mode-btn[aria-selected="true"] {
    background: linear-gradient(135deg, var(--accent), var(--accent-2));
    color: #fff;
  }

  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: end;
    gap: 0.6rem;
  }
  .filters select,
  .filters input[type="datetime-local"],
  .filters input[type="number"],
  .search-input {
    background: var(--bg-card);
    border: 1px solid var(--border);
    color: var(--text-base);
    border-radius: 0.5rem;
    padding: 0.4rem 0.6rem;
    font-size: 0.78rem;
    font-family: inherit;
    color-scheme: dark;
  }
  .search-input {
    flex: 1;
    min-width: 12rem;
  }
  .dt-field {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    font-size: 0.65rem;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .dt-field input[type="number"] {
    width: 5rem;
  }

  .quick-ranges {
    display: inline-flex;
    gap: 0.35rem;
  }
  .chip {
    background: var(--bg-card);
    border: 1px solid var(--border);
    color: var(--text-muted);
    font-size: 0.7rem;
    font-weight: 600;
    padding: 0.4rem 0.65rem;
    border-radius: 999px;
    cursor: pointer;
    transition:
      color 0.15s ease,
      border-color 0.15s ease;
  }
  .chip:hover {
    color: var(--text-base);
    border-color: #3f3f46;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .btn {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    background: transparent;
    border: 1px solid var(--accent);
    color: var(--accent);
    font-size: 0.7rem;
    font-weight: 700;
    font-family:
      ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    padding: 0.45rem 0.8rem;
    border-radius: 0.5rem;
    cursor: pointer;
    transition: background 0.15s ease;
  }
  .btn:hover:not(:disabled) {
    background: rgba(59, 130, 246, 0.12);
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .btn-ghost {
    border-color: var(--border);
    color: var(--text-muted);
  }
  .file-btn {
    position: relative;
  }
  .upload-name {
    font-size: 0.75rem;
    color: var(--text-muted);
  }
  .hint {
    font-size: 0.68rem;
    color: var(--text-muted);
    opacity: 0.8;
  }

  .error-banner {
    background: rgba(239, 68, 68, 0.12);
    border: 1px solid rgba(239, 68, 68, 0.35);
    color: var(--error);
    font-size: 0.78rem;
    padding: 0.5rem 0.75rem;
    border-radius: 0.5rem;
    flex-shrink: 0;
  }

  /* ── Event list ───────────────────────────────────────── */
  .event-list {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    padding-right: 0.25rem;
  }

  .empty-state {
    margin: 2rem auto;
    color: var(--text-muted);
    font-size: 0.85rem;
    text-align: center;
    max-width: 28rem;
  }

  .event-row {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 0.6rem;
    overflow: hidden;
    flex-shrink: 0; /* Add this line */
  }
  .event-row.notable {
    border-left: 3px solid var(--error);
  }

  .event-summary {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 0.65rem;
    background: transparent;
    border: none;
    padding: 0.55rem 0.75rem;
    cursor: pointer;
    text-align: left;
    color: var(--text-base);
    font-family: inherit;
  }

  .source-badge {
    flex-shrink: 0;
    font-size: 0.62rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 0.2rem 0.5rem;
    border-radius: 0.4rem;
    color: var(--badge-color);
    background: color-mix(in srgb, var(--badge-color) 16%, transparent);
    border: 1px solid color-mix(in srgb, var(--badge-color) 40%, transparent);
    white-space: nowrap;
  }

  .event-name {
    flex: 1;
    font-size: 0.8rem;
    font-family:
      ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .event-time {
    flex-shrink: 0;
    font-size: 0.72rem;
    color: var(--text-muted);
    font-family:
      ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  }

  .expand-icon {
    flex-shrink: 0;
    color: var(--text-muted);
    font-size: 0.7rem;
  }

  .event-data {
    margin: 0;
    padding: 0.75rem;
    border-top: 1px solid var(--border);
    background: rgba(0, 0, 0, 0.25);
    font-size: 0.72rem;
    line-height: 1.5;
    overflow-x: auto;
    color: var(--text-base);
  }

  .sentinel {
    height: 1px;
  }
  .loading-row {
    text-align: center;
    color: var(--text-muted);
    font-size: 0.75rem;
    padding: 0.75rem 0;
  }

  @media (max-width: 720px) {
    .history-pane {
      padding: 0.75rem;
    }
    .event-name {
      max-width: 40vw;
    }
  }
</style>
