<script>
  import { onMount, onDestroy } from "svelte";
  import { apiFetch } from "../api.js";
  import SignInNotice from "./SignInNotice.svelte";

  let {
    active,
    enabled,
    onstatus,
    allowDockerCommands = false,
    isAuthenticated = false,
  } = $props();

  // ── Container list ────────────────────────────────────────────────
  let containers = $state(/** @type {any[]} */ ([]));
  let listError = $state(false);

  // ── Poll controls ─────────────────────────────────────────────────
  let pollPaused = $state(false);
  let pollIntervalInput = $state("2000");
  let pollCmdError = $state(/** @type {string|null} */ (null));

  // ── Per-container action feedback ─────────────────────────────────
  // Map of container id → { pending: bool, error: string|null }
  let actionState = $state(
    /** @type {Record<string,{pending:boolean,error:string|null}>} */ ({}),
  );

  let pollTimer = null;

  // ── Helpers ───────────────────────────────────────────────────────
  function fmt(n, unit = "") {
    if (n == null) return "—";
    return n.toFixed(1) + unit;
  }

  function fmtBytes(b) {
    if (b == null) return "—";
    if (b >= 1_073_741_824) return (b / 1_073_741_824).toFixed(1) + " GB";
    if (b >= 1_048_576) return (b / 1_048_576).toFixed(1) + " MB";
    if (b >= 1_024) return (b / 1_024).toFixed(1) + " KB";
    return b + " B";
  }

  function statusClass(s) {
    if (!s) return "";
    const v = typeof s === "string" ? s : Object.keys(s)[0];
    if (v === "running") return "status-running";
    if (v === "paused") return "status-paused";
    if (v === "exited" || v === "dead") return "status-dead";
    if (v === "restarting") return "status-restarting";
    return "status-other";
  }

  function statusLabel(s) {
    if (!s) return "unknown";
    if (typeof s === "string") return s;
    // serde unknown variant is { unknown: "..." }
    const key = Object.keys(s)[0];
    return key === "unknown" ? `unknown(${s[key]})` : key;
  }

  function healthClass(h) {
    if (!h) return "";
    if (h === "healthy") return "health-healthy";
    if (h === "unhealthy") return "health-unhealthy";
    if (h === "starting") return "health-starting";
    return "";
  }

  function healthLabel(h) {
    if (!h || h === "none") return null;
    return h;
  }

  // ── Fetch ──────────────────────────────────────────────────────────
  async function refresh() {
    if (pollPaused) return;
    try {
      const r = await apiFetch("/api/docker-containers");
      if (!r.ok) throw new Error("HTTP error");
      containers = (await r.json()) || [];
      listError = false;
      onstatus("● LIVE", false);
    } catch {
      listError = true;
      onstatus(`● OFFLINE · ${new Date().toLocaleTimeString()}`, true);
    }
  }

  // ── Poll commands ─────────────────────────────────────────────────
  async function togglePoll() {
    pollCmdError = null;
    const ep = pollPaused
      ? "/api/docker/poll/resume"
      : "/api/docker/poll/pause";
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
      const r = await apiFetch("/api/docker/poll/interval", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ interval_ms: ms }),
      });
      if (!r.ok) throw new Error((await r.json()).message ?? "failed");
    } catch (e) {
      pollCmdError = e.message;
    }
  }

  // ── Container commands ────────────────────────────────────────────
  function getActionState(id) {
    if (!actionState[id]) actionState[id] = { pending: false, error: null };
    return actionState[id];
  }

  async function containerCmd(endpoint, id_or_name, extra = {}) {
    const s = getActionState(id_or_name);
    s.pending = true;
    s.error = null;
    actionState = { ...actionState };
    try {
      const r = await apiFetch(`/api/docker/${endpoint}`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ id_or_name, ...extra }),
      });
      if (!r.ok) {
        const body = await r.json().catch(() => ({}));
        throw new Error(body.message ?? `HTTP ${r.status}`);
      }
      await refresh();
    } catch (e) {
      s.error = e.message;
      actionState = { ...actionState };
    } finally {
      s.pending = false;
      actionState = { ...actionState };
    }
  }

  // ── Lifecycle ────────────────────────────────────────────────────
  
  $effect(() => {
    if (enabled) {
      refresh();
      pollTimer = setInterval(refresh, 2000);
      return () => clearInterval(pollTimer);
    }
  });

  let canCommand = $derived(allowDockerCommands && isAuthenticated);
</script>

<aside id="docker-pane">
  <!-- ── Header ────────────────────────────────────────────────────── -->
  <div class="pane-header">
    <div class="header-title">
      <h2>Docker</h2>
    </div>

    <div class="header-right">
      {#if allowDockerCommands}
        {#if !isAuthenticated}
          <SignInNotice name="docker" />
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

  <!-- ── Content ───────────────────────────────────────────────────── -->
  <div id="docker-content">
    {#if listError}
      <div class="muted">Failed to load containers</div>
    {:else if containers.length === 0}
      <div class="muted">No containers found</div>
    {:else}
      <div class="container-grid">
        {#each containers as c (c.id)}
          {@const as = actionState[c.id] ?? { pending: false, error: null }}
          {@const sl = statusLabel(c.status)}
          {@const sc = statusClass(c.status)}
          {@const hl = healthLabel(c.health)}
          {@const hc = healthClass(c.health)}
          {@const running = sl === "running"}
          {@const paused = sl === "paused"}
          {@const stopped =
            sl === "exited" || sl === "dead" || sl === "created"}

          <div class="container-card" class:pending={as.pending}>
            <!-- Card header -->
            <div class="card-header">
              <div class="card-name-row">
                <span class="container-name" title={c.name}>{c.name}</span>
                <span class="short-id">{c.short_id}</span>
              </div>
              <div class="card-badges">
                <span class="status-badge {sc}">{sl}</span>
                {#if hl}
                  <span class="health-badge {hc}">{hl}</span>
                {/if}
              </div>
            </div>

            <!-- Image -->
            <div class="card-image" title={c.image}>{c.image}</div>

            <!-- Stats -->
            {#if c.stats}
              {@const st = c.stats}
              <div class="stats-grid">
                <div class="stat">
                  <span class="stat-label">CPU</span>
                  <span class="stat-value">{fmt(st.cpu_percent, "%")}</span>
                </div>
                <div class="stat">
                  <span class="stat-label">MEM</span>
                  <span class="stat-value">
                    {fmtBytes(st.memory_bytes)}
                    {#if st.memory_percent != null}
                      <span class="stat-sub"
                        >{fmt(st.memory_percent * 100, "%")}</span
                      >
                    {/if}
                  </span>
                </div>
                <div class="stat">
                  <span class="stat-label">NET ↓/↑</span>
                  <span class="stat-value"
                    >{fmtBytes(st.net_rx_bytes)} / {fmtBytes(
                      st.net_tx_bytes,
                    )}</span
                  >
                </div>
                <div class="stat">
                  <span class="stat-label">DISK R/W</span>
                  <span class="stat-value"
                    >{fmtBytes(st.block_read_bytes)} / {fmtBytes(
                      st.block_write_bytes,
                    )}</span
                  >
                </div>
                <div class="stat">
                  <span class="stat-label">PIDs</span>
                  <span class="stat-value">{st.pid_count}</span>
                </div>
              </div>
            {:else}
              <div class="no-stats">No stats available</div>
            {/if}

            <!-- Actions -->
            {#if canCommand}
              <div class="card-actions">
                {#if stopped}
                  <button
                    class="act-btn act-start"
                    disabled={as.pending}
                    onclick={() => containerCmd("start-container", c.id)}
                    >▶ Start</button
                  >
                {/if}
                {#if running}
                  <button
                    class="act-btn act-stop"
                    disabled={as.pending}
                    onclick={() => containerCmd("stop-container", c.id)}
                    >■ Stop</button
                  >
                  <button
                    class="act-btn act-restart"
                    disabled={as.pending}
                    onclick={() => containerCmd("restart-container", c.id)}
                    >↺ Restart</button
                  >
                  <button
                    class="act-btn act-pause"
                    disabled={as.pending}
                    onclick={() => containerCmd("pause-container", c.id)}
                    >⏸ Pause</button
                  >
                  <button
                    class="act-btn act-kill"
                    disabled={as.pending}
                    onclick={() => containerCmd("kill-container", c.id)}
                    >✕ Kill</button
                  >
                {/if}
                {#if paused}
                  <button
                    class="act-btn act-start"
                    disabled={as.pending}
                    onclick={() => containerCmd("unpause-container", c.id)}
                    >▶ Unpause</button
                  >
                {/if}
              </div>
              {#if as.error}
                <div class="action-error">{as.error}</div>
              {/if}
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </div>
</aside>

<style>
  #docker-pane {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  /* ── Header ─────────────────────────────────────────────── */
  .pane-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 1rem 2rem;
    border-bottom: 1px solid var(--border-soft);
    flex-shrink: 0;
    flex-wrap: wrap;
    gap: 0.75rem;
  }
  .header-title {
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }
  .pane-header h2 {
    font-size: 0.78rem;
    font-weight: 700;
    color: #fff;
    letter-spacing: 0.18em;
    text-transform: uppercase;
  }
  .header-right {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    flex-wrap: wrap;
  }

  /* ── Content ────────────────────────────────────────────── */
  #docker-content {
    overflow-y: auto;
    flex: 1;
    padding: 1.5rem 2rem 2rem;
  }
  .container-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(360px, 100%), 1fr));
    gap: 1rem;
    align-items: start;
  }

  /* ── Container card ─────────────────────────────────────── */
  .container-card {
    background: var(--bg-panel);
    border: 1px solid var(--border-soft);
    border-radius: 0.85rem;
    padding: 0.9rem 1rem 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
    transition: opacity 0.15s ease;
  }
  .container-card.pending {
    opacity: 0.65;
    pointer-events: none;
  }

  .card-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 0.5rem;
  }
  .card-name-row {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    min-width: 0;
  }
  .container-name {
    font-size: 0.82rem;
    font-weight: 700;
    color: #fff;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .short-id {
    font-size: 0.65rem;
    color: var(--text-muted);
    font-family:
      ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    letter-spacing: 0.04em;
  }
  .card-badges {
    display: flex;
    gap: 0.35rem;
    flex-shrink: 0;
    flex-wrap: wrap;
    justify-content: flex-end;
  }

  .card-image {
    font-size: 0.68rem;
    color: var(--text-muted);
    font-family:
      ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* ── Status / health badges ─────────────────────────────── */
  .status-badge,
  .health-badge {
    font-size: 0.62rem;
    font-weight: 700;
    font-family:
      ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    padding: 0.15rem 0.5rem;
    border-radius: 4px;
    border: 1px solid transparent;
  }
  .status-running {
    color: #34d399;
    background: rgba(16, 185, 129, 0.1);
    border-color: rgba(16, 185, 129, 0.3);
  }
  .status-paused {
    color: #fbbf24;
    background: rgba(245, 158, 11, 0.1);
    border-color: rgba(245, 158, 11, 0.3);
  }
  .status-dead {
    color: #f87171;
    background: rgba(239, 68, 68, 0.1);
    border-color: rgba(239, 68, 68, 0.25);
  }
  .status-restarting {
    color: #60a5fa;
    background: rgba(59, 130, 246, 0.1);
    border-color: rgba(59, 130, 246, 0.3);
  }
  .status-other {
    color: var(--text-muted);
    background: var(--bg-card);
    border-color: var(--border);
  }
  .health-healthy {
    color: #34d399;
    background: rgba(16, 185, 129, 0.08);
    border-color: rgba(16, 185, 129, 0.25);
  }
  .health-unhealthy {
    color: #f87171;
    background: rgba(239, 68, 68, 0.08);
    border-color: rgba(239, 68, 68, 0.2);
  }
  .health-starting {
    color: #fbbf24;
    background: rgba(245, 158, 11, 0.08);
    border-color: rgba(245, 158, 11, 0.25);
  }

  /* ── Stats ──────────────────────────────────────────────── */
  .stats-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.4rem 0.75rem;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 0.5rem;
    padding: 0.6rem 0.75rem;
  }
  .stat {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
  }
  .stat-label {
    font-size: 0.58rem;
    font-weight: 700;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.1em;
  }
  .stat-value {
    font-size: 0.75rem;
    font-family:
      ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    color: var(--text-base);
    display: flex;
    align-items: baseline;
    gap: 0.3rem;
  }
  .stat-sub {
    font-size: 0.65rem;
    color: var(--text-muted);
  }
  .no-stats {
    font-size: 0.72rem;
    color: var(--text-muted);
    font-style: italic;
    text-align: center;
    padding: 0.4rem 0;
  }

  /* ── Actions ────────────────────────────────────────────── */
  .card-actions {
    display: flex;
    gap: 0.4rem;
    flex-wrap: wrap;
  }
  .act-btn {
    font-size: 0.65rem;
    font-weight: 700;
    font-family: inherit;
    letter-spacing: 0.04em;
    padding: 0.25rem 0.6rem;
    border-radius: 5px;
    border: 1px solid var(--border);
    background: var(--bg-card);
    color: var(--text-muted);
    cursor: pointer;
    transition:
      background 0.12s,
      color 0.12s,
      border-color 0.12s;
  }
  .act-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .act-start {
    color: #34d399;
    border-color: rgba(16, 185, 129, 0.35);
  }
  .act-start:hover:not(:disabled) {
    background: rgba(16, 185, 129, 0.1);
  }
  .act-stop {
    color: #f87171;
    border-color: rgba(239, 68, 68, 0.35);
  }
  .act-stop:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.1);
  }
  .act-kill {
    color: #f87171;
    border-color: rgba(239, 68, 68, 0.35);
  }
  .act-kill:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.15);
  }
  .act-restart {
    color: #60a5fa;
    border-color: rgba(59, 130, 246, 0.35);
  }
  .act-restart:hover:not(:disabled) {
    background: rgba(59, 130, 246, 0.1);
  }
  .act-pause {
    color: #fbbf24;
    border-color: rgba(245, 158, 11, 0.35);
  }
  .act-pause:hover:not(:disabled) {
    background: rgba(245, 158, 11, 0.1);
  }
  .action-error {
    font-size: 0.67rem;
    color: #f87171;
    font-family: ui-monospace, monospace;
  }

  /* ── Shared controls ─────────────────────────────────────── */
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
  .muted {
    color: var(--text-muted);
    font-size: 0.85rem;
    text-align: center;
    padding: 2rem 0;
    font-style: italic;
  }
</style>
