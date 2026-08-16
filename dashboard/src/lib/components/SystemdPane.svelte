<script>
  import { onMount, onDestroy } from "svelte";
  import SignInNotice from "./SignInNotice.svelte";
  import { apiFetch } from "../api.js";

  let {
    active,
    enabled,
    onstatus,
    allowSystemdCommands = false,
    isAuthenticated = false,
  } = $props();

  // ── State ─────────────────────────────────────────────────────────
  let snap = $state(null); // SystemdSnapshot
  let failedUnits = $state([]); // UnitSnapshot[]
  let selectedUnit = $state(null); // UnitSnapshot | null (detail view)
  let detailLoading = $state(false);

  // Filter / view controls
  let filterState = $state("all"); // "all" | "active" | "inactive" | "failed"
  let filterType = $state("all"); // "all" | "service" | "socket" | "timer" | "mount" | "target"
  let search = $state("");

  // ── Poll / command controls ────────────────────────────────────────
  let pollPaused = $state(false);
  let pollIntervalInput = $state("3000");
  let pollCmdError = $state(null);
  let unitCmdPending = $state(false);
  let unitCmdError = $state(null);
  let unitCmdSuccess = $state(null);

  let interval = null;

  // ── Fetch ─────────────────────────────────────────────────────────
  async function refresh() {
    if (!enabled || pollPaused) return;
    try {
      const [snapRes, failedRes] = await Promise.all([
        apiFetch("/api/systemd"),
        apiFetch("/api/failed_units"),
      ]);
      if (snapRes.ok) snap = await snapRes.json();
      if (failedRes.ok) failedUnits = await failedRes.json();
    } catch {}
  }

  async function openUnit(unitName) {
    detailLoading = true;
    selectedUnit = null;
    unitCmdError = null;
    unitCmdSuccess = null;
    try {
      const r = await apiFetch(`/api/unit/${encodeURIComponent(unitName)}`);
      if (r.ok) selectedUnit = await r.json();
    } catch {}
    detailLoading = false;
  }

  async function loadByState(state) {
    // Used when clicking the summary counters
    filterState = state;
    selectedUnit = null;
  }

  // ── Poll commands ──────────────────────────────────────────────────
  async function togglePoll() {
    pollCmdError = null;
    const ep = pollPaused
      ? "/api/systemd/poll/resume"
      : "/api/systemd/poll/pause";
    try {
      const r = await apiFetch(ep, { method: "POST" });
      if (!r.ok) throw new Error((await r.json()).message ?? "failed");
      pollPaused = !pollPaused;
    } catch (e) {
      pollCmdError = e.message;
    }
  }

  async function applyInterval() {
    pollCmdError = null;
    const ms = parseInt(pollIntervalInput, 10);
    if (!ms || ms < 100) {
      pollCmdError = "Must be ≥ 100 ms";
      return;
    }
    try {
      const r = await apiFetch("/api/systemd/poll/interval", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ interval_ms: ms }),
      });
      if (!r.ok) throw new Error((await r.json()).message ?? "failed");
    } catch (e) {
      pollCmdError = e.message;
    }
  }

  // ── Unit control ───────────────────────────────────────────────────
  const SERVICE_ACTIONS = ["start", "stop", "restart", "reload"];

  async function controlUnit(action) {
    if (!selectedUnit) return;
    unitCmdError = null;
    unitCmdSuccess = null;
    unitCmdPending = true;
    try {
      const r = await apiFetch("/api/systemd/control-unit", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ unit_name: selectedUnit.unit_name, action }),
      });
      if (!r.ok) throw new Error((await r.json()).message ?? "failed");
      unitCmdSuccess = action;
      // Refresh detail after a short delay so systemd state has propagated
      setTimeout(() => {
        openUnit(selectedUnit.unit_name);
        refresh();
      }, 800);
    } catch (e) {
      unitCmdError = e.message;
    } finally {
      unitCmdPending = false;
    }
  }

  // Whether commands are actually usable
  let canCommand = $derived(allowSystemdCommands && isAuthenticated);

  let pollTimer = null;

  // Use $effect so polling starts/stops reactively based on `enabled`
  $effect(() => {
    if (enabled) {
      refresh();
      pollTimer = setInterval(refresh, 2000);
      return () => clearInterval(pollTimer);
    }
  });

  // ── Derived ───────────────────────────────────────────────────────
  let units = $derived(snap?.units ?? []);

  let filtered = $derived(
    (() => {
      let list = units;

      if (filterState !== "all") {
        list = list.filter((u) => u.active_state === filterState);
      }
      if (filterType !== "all") {
        list = list.filter((u) => {
          const t = u.unit_type;
          // unit_type is either a string variant or {other: "..."} from serde
          if (typeof t === "string") return t === filterType;
          if (typeof t === "object" && t !== null) return false; // "other" variants
          return false;
        });
      }
      if (search.trim()) {
        const q = search.trim().toLowerCase();
        list = list.filter(
          (u) =>
            u.unit_name.toLowerCase().includes(q) ||
            u.description.toLowerCase().includes(q),
        );
      }
      return list;
    })(),
  );

  // ── Helpers ───────────────────────────────────────────────────────
  function stateColor(state) {
    switch (state) {
      case "active":
        return "var(--success)";
      case "failed":
        return "var(--error)";
      case "activating":
      case "reloading":
        return "var(--warning)";
      default:
        return "var(--text-muted)";
    }
  }

  function stateDot(state) {
    switch (state) {
      case "active":
        return "dot-active";
      case "failed":
        return "dot-failed";
      case "activating":
      case "reloading":
        return "dot-warn";
      default:
        return "dot-inactive";
    }
  }

  function unitTypeLabel(t) {
    if (typeof t === "string") return t;
    if (typeof t === "object" && t !== null) {
      const key = Object.keys(t)[0];
      return t[key] || "other";
    }
    return "?";
  }

  function formatBytes(b) {
    if (b == null) return "—";
    if (b < 1024) return `${b} B`;
    if (b < 1024 ** 2) return `${(b / 1024).toFixed(1)} KB`;
    if (b < 1024 ** 3) return `${(b / 1024 ** 2).toFixed(1)} MB`;
    return `${(b / 1024 ** 3).toFixed(2)} GB`;
  }

  function formatCpuNs(ns) {
    if (ns == null) return "—";
    const ms = ns / 1_000_000;
    if (ms < 1000) return `${ms.toFixed(0)} ms`;
    return `${(ms / 1000).toFixed(2)} s`;
  }

  function formatSince(iso) {
    if (!iso) return "—";
    try {
      const d = new Date(iso);
      return d.toLocaleString();
    } catch {
      return iso;
    }
  }
</script>

<div class="pane-header">
  <div class="header-title">
    <h2>Systemd</h2>
  </div>
  <div class="header-right">
    {#if snap}
      <div class="header-stats">
        <button
          class="stat-chip"
          class:chip-active={filterState === "active"}
          onclick={() =>
            loadByState(filterState === "active" ? "all" : "active")}
        >
          <span class="dot dot-active"></span>
          {snap.active_count} active
        </button>
        <button
          class="stat-chip"
          class:chip-inactive={filterState === "inactive"}
          onclick={() =>
            loadByState(filterState === "inactive" ? "all" : "inactive")}
        >
          <span class="dot dot-inactive"></span>
          {snap.inactive_count} inactive
        </button>
        {#if snap.failed_count > 0}
          <button
            class="stat-chip chip-failed-always"
            class:chip-failed={filterState === "failed"}
            onclick={() =>
              loadByState(filterState === "failed" ? "all" : "failed")}
          >
            <span class="dot dot-failed"></span>
            {snap.failed_count} failed
          </button>
        {/if}
      </div>
    {/if}
    {#if allowSystemdCommands}
      {#if !isAuthenticated}
        <SignInNotice name="systemd" />
      {:else}
        <div class="poll-controls">
          <button
            class="poll-btn"
            class:paused={pollPaused}
            onclick={togglePoll}
            >{pollPaused ? "▶ Resume Poll" : "⏸ Pause Poll"}</button
          >
          <div class="interval-row">
            <input
              type="number"
              class="control-input"
              style="width:5rem"
              bind:value={pollIntervalInput}
              min="100"
              placeholder="ms"
            />
            <button class="poll-btn apply" onclick={applyInterval}
              >Set ms</button
            >
          </div>
          {#if pollCmdError}
            <span class="poll-error">{pollCmdError}</span>
          {/if}
        </div>
      {/if}
    {/if}
  </div>
</div>

<div id="systemd-panel">
  {#if !enabled}
    <div class="empty-state">Systemd monitor is disabled.</div>
  {:else if !snap}
    <div class="empty-state">Loading…</div>
  {:else}
    <div id="systemd-inner">
      <!-- ── Left: unit list ───────────────────────────────────────── -->
      <div class="unit-list-col">
        <!-- Failed banner -->
        {#if failedUnits.length > 0 && filterState !== "failed"}
          <div class="failed-banner">
            <span class="dot dot-failed"></span>
            {failedUnits.length} failed unit{failedUnits.length > 1 ? "s" : ""}:
            {failedUnits.map((u) => u.unit_name).join(", ")}
          </div>
        {/if}

        <!-- Controls -->
        <div class="controls-row">
          <input
            class="search-input"
            type="search"
            placeholder="Search units…"
            bind:value={search}
          />
          <select class="control-select" bind:value={filterState}>
            <option value="all">All states</option>
            <option value="active">Active</option>
            <option value="inactive">Inactive</option>
            <option value="failed">Failed</option>
            <option value="activating">Activating</option>
            <option value="reloading">Reloading</option>
            <option value="deactivating">Deactivating</option>
          </select>
          <select class="control-select" bind:value={filterType}>
            <option value="all">All types</option>
            <option value="service">Service</option>
            <option value="socket">Socket</option>
            <option value="timer">Timer</option>
            <option value="mount">Mount</option>
            <option value="target">Target</option>
          </select>
        </div>

        <!-- Unit rows -->
        <div class="unit-rows">
          {#if filtered.length === 0}
            <div class="muted">No units match the current filter.</div>
          {:else}
            {#each filtered as u (u.unit_name)}
              <button
                class="unit-row"
                class:unit-row-selected={selectedUnit?.unit_name ===
                  u.unit_name}
                onclick={() => openUnit(u.unit_name)}
              >
                <span class="dot {stateDot(u.active_state)}"></span>
                <span class="unit-name" title={u.unit_name}>{u.unit_name}</span>
                <span class="unit-sub-state">{u.sub_state}</span>
                <span class="unit-type-pill">{unitTypeLabel(u.unit_type)}</span>
              </button>
            {/each}
          {/if}
        </div>

        <div class="list-footer">
          {filtered.length} / {units.length} units
        </div>
      </div>

      <!-- ── Right: unit detail ────────────────────────────────────── -->
      <div class="unit-detail-col">
        {#if detailLoading}
          <div class="muted">Loading…</div>
        {:else if selectedUnit}
          {@const u = selectedUnit}
          <div class="detail-header">
            <span class="dot {stateDot(u.active_state)}" style="flex-shrink:0"
            ></span>
            <span class="detail-unit-name">{u.unit_name}</span>
          </div>
          <p class="detail-description">{u.description || "—"}</p>

          {#if canCommand}
            <div class="unit-actions">
              {#each SERVICE_ACTIONS as action}
                <button
                  class="action-btn action-{action}"
                  disabled={unitCmdPending}
                  onclick={() => controlUnit(action)}
                >
                  {action}
                </button>
              {/each}
              {#if unitCmdPending}
                <span class="cmd-feedback cmd-pending">⏳ working…</span>
              {:else if unitCmdSuccess}
                <span class="cmd-feedback cmd-ok">✔ {unitCmdSuccess}</span>
              {:else if unitCmdError}
                <span class="cmd-feedback cmd-err">{unitCmdError}</span>
              {/if}
            </div>
          {/if}

          <div class="detail-grid">
            <div class="sys-kv">
              <span class="sk">Active State</span>
              <span class="sv" style="color:{stateColor(u.active_state)}"
                >{u.active_state}</span
              >
            </div>
            <div class="sys-kv">
              <span class="sk">Sub State</span>
              <span class="sv">{u.sub_state}</span>
            </div>
            <div class="sys-kv">
              <span class="sk">Load State</span>
              <span class="sv">{u.load_state}</span>
            </div>
            <div class="sys-kv">
              <span class="sk">Type</span>
              <span class="sv">{unitTypeLabel(u.unit_type)}</span>
            </div>
            {#if u.main_pid != null}
              <div class="sys-kv">
                <span class="sk">Main PID</span>
                <span class="sv">{u.main_pid}</span>
              </div>
            {/if}
            {#if u.restart_count != null}
              <div class="sys-kv">
                <span class="sk">Restarts</span>
                <span
                  class="sv"
                  style:color={u.restart_count > 0
                    ? "var(--warning)"
                    : undefined}
                >
                  {u.restart_count}
                </span>
              </div>
            {/if}
            {#if u.memory_bytes != null}
              <div class="sys-kv">
                <span class="sk">Memory</span>
                <span class="sv">{formatBytes(u.memory_bytes)}</span>
              </div>
            {/if}
            {#if u.cpu_usage_ns != null}
              <div class="sys-kv">
                <span class="sk">CPU Time</span>
                <span class="sv">{formatCpuNs(u.cpu_usage_ns)}</span>
              </div>
            {/if}
            {#if u.since}
              <div class="sys-kv" style="grid-column:1/-1">
                <span class="sk">Since</span>
                <span class="sv">{formatSince(u.since)}</span>
              </div>
            {/if}
            {#if u.fragment_path}
              <div class="sys-kv" style="grid-column:1/-1">
                <span class="sk">Unit File</span>
                <span class="sv sv-mono">{u.fragment_path}</span>
              </div>
            {/if}
          </div>
        {:else}
          <div class="muted detail-placeholder">
            Select a unit to see details.
          </div>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  /* ── Header ─────────────────────────────────────────────────────── */
  .pane-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 1rem 2rem;
    border-bottom: 1px solid var(--border-soft);
    flex-shrink: 0;
    gap: 1rem;
    flex-wrap: wrap;
  }
  .pane-header h2 {
    font-size: 0.78rem;
    font-weight: 700;
    color: #fff;
    letter-spacing: 0.18em;
    text-transform: uppercase;
  }
  .header-title {
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }
  .header-right {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    flex-wrap: wrap;
  }
  .header-stats {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .stat-chip {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 0.2rem 0.6rem;
    font-size: 0.68rem;
    font-weight: 600;
    color: var(--text-muted);
    cursor: pointer;
    font-family: inherit;
    transition:
      border-color 0.15s,
      color 0.15s,
      background 0.15s;
  }
  .stat-chip:hover {
    border-color: var(--accent);
    color: var(--text-base);
  }
  .stat-chip.chip-active {
    border-color: var(--success);
    color: var(--success);
    background: rgba(16, 185, 129, 0.08);
  }
  .stat-chip.chip-inactive {
    border-color: var(--border);
    color: var(--text-base);
    background: var(--bg-elev);
  }
  .stat-chip.chip-failed-always {
    color: var(--error);
  }
  .stat-chip.chip-failed {
    border-color: var(--error);
    background: rgba(239, 68, 68, 0.1);
  }

  /* ── Panel layout ────────────────────────────────────────────────── */
  #systemd-panel {
    flex: 1;
    overflow: hidden;
    padding: 1.5rem 2rem 2rem;
    display: flex;
    flex-direction: column;
  }
  #systemd-inner {
    flex: 1;
    display: grid;
    grid-template-columns: minmax(0, 1.4fr) minmax(0, 1fr);
    gap: 1.25rem;
    overflow: hidden;
    min-height: 0;
  }

  /* ── Unit list column ────────────────────────────────────────────── */
  .unit-list-col {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    background: var(--bg-panel);
    border: 1px solid var(--border-soft);
    border-radius: 0.85rem;
    padding: 1rem;
    overflow: hidden;
    min-height: 0;
  }

  .failed-banner {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    background: rgba(239, 68, 68, 0.08);
    border: 1px solid rgba(239, 68, 68, 0.25);
    border-radius: 0.5rem;
    padding: 0.5rem 0.75rem;
    font-size: 0.72rem;
    color: var(--error);
    font-weight: 600;
    flex-shrink: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .controls-row {
    display: flex;
    gap: 0.5rem;
    flex-shrink: 0;
    flex-wrap: wrap;
  }
  .search-input {
    flex: 1;
    min-width: 120px;
    background: var(--bg-card);
    color: var(--text-base);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 0.25rem 0.6rem;
    font-size: 0.72rem;
    font-family: inherit;
    outline: none;
  }
  .search-input:focus {
    border-color: var(--accent);
  }
  .control-select {
    background: var(--bg-card);
    color: var(--text-base);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 0.2rem 0.4rem;
    font-size: 0.7rem;
    font-family: inherit;
    outline: none;
    cursor: pointer;
  }
  .control-select:focus {
    border-color: var(--accent);
  }

  .unit-rows {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    min-height: 0;
  }
  .unit-row {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    padding: 0.45rem 0.65rem;
    border-radius: 0.5rem;
    border: 1px solid transparent;
    background: transparent;
    cursor: pointer;
    font-family: inherit;
    text-align: left;
    transition:
      background 0.12s,
      border-color 0.12s;
    min-width: 0;
  }
  .unit-row:hover {
    background: var(--bg-elev);
    border-color: var(--border);
  }
  .unit-row-selected {
    background: var(--accent-glow);
    border-color: var(--accent) !important;
  }
  .unit-name {
    flex: 1;
    font-size: 0.75rem;
    font-family:
      ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    color: #d4d4d8;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .unit-sub-state {
    font-size: 0.62rem;
    color: var(--text-muted);
    white-space: nowrap;
    flex-shrink: 0;
  }
  .unit-type-pill {
    font-size: 0.58rem;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-muted);
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0.05rem 0.35rem;
    flex-shrink: 0;
  }

  .list-footer {
    font-size: 0.62rem;
    color: var(--text-muted);
    text-align: right;
    flex-shrink: 0;
    padding-top: 0.25rem;
    border-top: 1px solid var(--border-soft);
  }

  /* ── Detail column ───────────────────────────────────────────────── */
  .unit-detail-col {
    background: var(--bg-panel);
    border: 1px solid var(--border-soft);
    border-radius: 0.85rem;
    padding: 1.1rem;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
    min-height: 0;
  }

  .detail-header {
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }
  .detail-unit-name {
    font-size: 0.82rem;
    font-family:
      ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    color: var(--text-base);
    font-weight: 600;
    word-break: break-all;
  }
  .detail-description {
    font-size: 0.75rem;
    color: var(--text-muted);
    line-height: 1.4;
  }
  .detail-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    column-gap: 1.1rem;
    row-gap: 0.75rem;
    align-items: start;
  }
  .detail-placeholder {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  /* ── Shared with SystemResourcesPane style ───────────────────────── */
  .sys-kv {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    min-width: 0;
  }
  .sys-kv .sk {
    font-size: 0.6rem;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.07em;
    font-weight: 600;
    white-space: nowrap;
  }
  .sys-kv .sv {
    font-size: 0.85rem;
    color: #e4e4e7;
    font-family:
      ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sv-mono {
    font-size: 0.72rem;
    white-space: normal;
    word-break: break-all;
  }

  /* ── Status dots ─────────────────────────────────────────────────── */
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    display: inline-block;
    flex-shrink: 0;
  }
  .dot-active {
    background: var(--success);
    box-shadow: 0 0 4px var(--success);
  }
  .dot-failed {
    background: var(--error);
    box-shadow: 0 0 4px var(--error);
  }
  .dot-warn {
    background: var(--warning);
    box-shadow: 0 0 4px var(--warning);
  }
  .dot-inactive {
    background: var(--border);
  }

  /* ── Empty / loading states ──────────────────────────────────────── */
  .empty-state {
    color: var(--text-muted);
    font-size: 0.85rem;
    text-align: center;
    padding: 4rem 0;
    font-style: italic;
  }
  .muted {
    color: var(--text-muted);
    font-size: 0.85rem;
    text-align: center;
    padding: 2rem 0;
    font-style: italic;
  }

  /* ── Poll controls (mirrors ProcessesPane) ───────────────────────── */
  .poll-controls {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .interval-row {
    display: flex;
    align-items: center;
    gap: 0.25rem;
  }
  .poll-btn {
    background: var(--bg-card);
    color: var(--text-muted);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 0.2rem 0.65rem;
    font-size: 0.7rem;
    font-family: inherit;
    cursor: pointer;
    transition:
      background 0.12s,
      border-color 0.12s,
      color 0.12s;
  }
  .poll-btn:hover {
    border-color: var(--accent);
    color: var(--text-base);
  }
  .poll-btn.paused {
    color: #34d399;
    border-color: rgba(52, 211, 153, 0.4);
  }
  .poll-btn.apply {
    color: var(--accent);
    border-color: rgba(99, 102, 241, 0.4);
  }
  .poll-error {
    font-size: 0.68rem;
    color: #f87171;
    font-family: ui-monospace, monospace;
  }
  .control-input {
    background: var(--bg-card);
    color: var(--text-base);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 0.2rem 0.4rem;
    font-size: 0.7rem;
    outline: none;
    font-family: inherit;
  }
  .control-input:focus {
    border-color: var(--accent);
  }

  /* ── Unit action buttons ─────────────────────────────────────────── */
  .unit-actions {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    flex-wrap: wrap;
    padding: 0.6rem 0;
    border-top: 1px solid var(--border-soft);
    border-bottom: 1px solid var(--border-soft);
  }
  .action-btn {
    font-family: inherit;
    font-size: 0.68rem;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    border-radius: 5px;
    padding: 0.22rem 0.65rem;
    cursor: pointer;
    border: 1px solid var(--border);
    background: var(--bg-card);
    color: var(--text-muted);
    transition:
      background 0.12s,
      border-color 0.12s,
      color 0.12s;
  }
  .action-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .action-btn:not(:disabled):hover {
    color: var(--text-base);
    border-color: var(--accent);
  }
  .action-start:not(:disabled):hover {
    color: var(--success);
    border-color: var(--success);
    background: rgba(16, 185, 129, 0.08);
  }
  .action-stop:not(:disabled):hover {
    color: var(--error);
    border-color: var(--error);
    background: rgba(239, 68, 68, 0.08);
  }
  .action-restart:not(:disabled):hover {
    color: var(--warning);
    border-color: var(--warning);
    background: rgba(245, 158, 11, 0.08);
  }
  .action-reload:not(:disabled):hover {
    color: var(--accent);
    border-color: var(--accent);
    background: rgba(99, 102, 241, 0.08);
  }
  .cmd-feedback {
    font-size: 0.68rem;
    font-family: ui-monospace, monospace;
    margin-left: 0.25rem;
  }
  .cmd-pending {
    color: var(--text-muted);
  }
  .cmd-ok {
    color: var(--success);
  }
  .cmd-err {
    color: #f87171;
  }
</style>
