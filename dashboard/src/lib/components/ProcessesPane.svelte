<script>
  import ProcessCard from "./ProcessCard.svelte";
  import SignInNotice from "./SignInNotice.svelte";
  import { apiFetch } from "../api.js";

  let {
    active,
    hasPids: hasPidsConfig,
    enabled,
    onstatus,
    hasTopProcesses,
    limitProcesses,
    allowProcessCommands = false,
    isAuthenticated = false,
  } = $props();

  let hasPids = $state(() => hasPidsConfig);

  // ── Supported signals (fetched once, shared across all cards) ─────
  const ALL_SIGNALS = [
    { value: "kill", label: "KILL" },
    { value: "term", label: "TERM" },
    { value: "int", label: "INT" },
    { value: "stop", label: "STOP" },
    { value: "cont", label: "CONT" },
  ];

  let supportedSignals = $state(/** @type {string[]} */ ([]));

  // ── Tracked processes ─────────────────────────────────────────────
  let rootGroups = $state([]);
  let workDone = $state(false);
  let trackedError = $state(false);
  let openPids = new Set();

  // ── Top processes ─────────────────────────────────────────────────
  let topProcesses = $state([]);
  let topSort = $state("cpu");
  let topLimit = $state(5);
  let topError = $state(false);

  // ── Poll controls ─────────────────────────────────────────────────
  let pollPaused = $state(false);
  let pollInterval = $state(2000); // current server-reported poll interval (ms), shared by tracked + top
  let pollIntervalInput = $state("2000");
  let pollCmdError = $state(null);

  // How often we re-check /api/process/poll/status to notice changes made
  // elsewhere (another client pausing/resuming or changing the interval).
  const STATUS_CHECK_MS = 3000;

  let processInterval = null;
  let topInterval = null;
  let statusTimer = null;

  // ── Tracked fetch ─────────────────────────────────────────────────
  async function refreshTracked() {
    if (pollPaused) return;
    try {
      const r = await apiFetch("/api/process/trees");
      if (!r.ok) throw new Error("HTTP error");
      const groups = await r.json();

      if (groups.length === 0) {
        workDone = false;
        rootGroups = [];
        return;
      }

      rootGroups = groups.map((g) => ({ ...g, pid: g.root_pid }));
      hasPids = true;
      workDone = groups.every((g) => g.work_done);
      trackedError = false;
      onstatus("● LIVE", false);
    } catch {
      trackedError = true;
      onstatus(`● OFFLINE · ${new Date().toLocaleTimeString()}`, true);
    }
  }

  // ── Top processes fetch ───────────────────────────────────────────
  async function refreshTop() {
    if (!hasTopProcesses || pollPaused) return;
    try {
      const r = await apiFetch(
        `/api/top-processes?sort=${topSort}&limit=${topLimit}`,
      );
      if (!r.ok) throw new Error("HTTP error");
      const data = await r.json();
      topProcesses = data || [];
      topError = false;
    } catch {
      topError = true;
    }
  }

  // ── Poll status ────────────────────────────────────────────────────
  /** @typedef {{interval: number, paused: boolean}} PollStatus */
  async function fetchPollStatus() {
    try {
      const r = await apiFetch("/api/process/poll/status");
      if (!r.ok) throw new Error("HTTP error");
      /** @type {PollStatus} */
      const status = await r.json();
      pollPaused = status.paused;
      pollInterval = status.interval;
      pollIntervalInput = String(status.interval);
    } catch {
      // transient failure fetching status — keep last known values,
      // the next scheduled check will retry.
    }
  }

  // ── Poll commands ─────────────────────────────────────────────────
  async function togglePoll() {
    pollCmdError = null;
    const ep = pollPaused
      ? "/api/process/poll/resume"
      : "/api/process/poll/pause";
    try {
      const r = await apiFetch(ep, { method: "POST" });
      if (!r.ok) throw new Error((await r.json()).message ?? "failed");
      await fetchPollStatus();
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
      const r = await apiFetch("/api/process/poll/interval", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ interval_ms: ms }),
      });
      if (!r.ok) throw new Error((await r.json()).message ?? "failed");
      await fetchPollStatus();
    } catch (e) {
      pollCmdError = e.message;
    }
  }

  let signals = $derived(
    supportedSignals.length > 0
      ? ALL_SIGNALS.filter((s) => supportedSignals.includes(s.value))
      : ALL_SIGNALS,
  );

  $effect(() => {
    apiFetch("/api/supported-signals")
      .then((r) => r.json())
      .then((data) => {
        supportedSignals = data;
      })
      .catch(() => {
        supportedSignals = ALL_SIGNALS.map((s) => s.value);
      });
  });

  // Fetch the server's poll status once up front, then keep re-checking it
  // periodically so we notice if another client pauses/resumes or changes
  // the interval.
  $effect(() => {
    fetchPollStatus();
    statusTimer = setInterval(fetchPollStatus, STATUS_CHECK_MS);
    return () => clearInterval(statusTimer);
  });

  // Tracked process tree refresh loop — restarts whenever pollInterval or
  // pollPaused change (e.g. after fetchPollStatus picks up a change made
  // elsewhere).
  $effect(() => {
    if (pollPaused) return;
    refreshTracked();
    processInterval = setInterval(refreshTracked, pollInterval);
    return () => clearInterval(processInterval);
  });

  // Top processes share the same poll interval as the tracked tree.
  $effect(() => {
    if (!hasTopProcesses || pollPaused) return;
    refreshTop();
    topInterval = setInterval(refreshTop, pollInterval);
    return () => clearInterval(topInterval);
  });

  // Re-fetch top when sort/limit change
  $effect(() => {
    topSort;
    topLimit;
    if (hasTopProcesses) refreshTop();
  });

  // Cap topLimit to server max
  $effect(() => {
    if (topLimit > limitProcesses) topLimit = limitProcesses;
  });

  // Whether commands are actually usable
  let canCommand = $derived(allowProcessCommands && isAuthenticated);
</script>

<aside id="process-pane">
  <div class="pane-header">
    <div class="header-title">
      <h2>Processes</h2>
      <button
        class="poll-btn refresh-btn"
        onclick={refreshTracked}
        title="Refresh processes"
      >
        ↻ Refresh
      </button>
    </div>
    {#if allowProcessCommands}
      {#if !isAuthenticated}
          <SignInNotice name="process" />
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

  <div id="process-content">
    {#if workDone}
      <div id="work-banner" class="visible">
        ✔ Work complete — all children exited
      </div>
    {/if}

    <div class="process-columns">
      <!-- Tracked column -->
      {#if hasPids && (hasPidsConfig || rootGroups.length > 0)}
        <div class="process-column">
          <div class="section-header">Tracked</div>

          {#if trackedError}
            <div class="muted">Monitor disabled</div>
          {:else if rootGroups.length === 0}
            <div class="muted">No process tracker running.</div>
          {:else}
            {#each rootGroups as group (group.pid)}
              <div class="process-group">
                {#if group.root}
                  <ProcessCard
                    proc={group.root}
                    isRoot={true}
                    isBeingTracked={true}
                    {allowProcessCommands}
                    {isAuthenticated}
                    {signals}
                    onRefresh={refreshTracked}
                  />
                {:else}
                  <div class="muted">Root process {group.pid} exited</div>
                {/if}

                {#if group.child_count > 0}
                  <details
                    class="children-group"
                    style="margin-top:0.5rem;margin-left:0.75rem"
                    bind:open={openPids[group.pid]}
                  >
                    <summary
                      class="section-header"
                      style="margin-top:0;cursor:pointer;user-select:none"
                    >
                      Children <span class="count-badge"
                        >{group.child_count}</span
                      >
                      <span
                        class="muted"
                        style="margin-left:auto;font-size:0.7rem;font-weight:normal"
                        >(click to toggle)</span
                      >
                    </summary>
                    <div
                      style="border-left:2px solid var(--border);padding-left:0.75rem;margin-top:0.5rem;display:flex;flex-direction:column;gap:0.5rem"
                    >
                      {#each group.children as child (child.pid)}
                        <ProcessCard
                          proc={child}
                          isBeingTracked={false}
                          {allowProcessCommands}
                          {isAuthenticated}
                          {signals}
                          onRefresh={refreshTracked}
                        />
                      {/each}
                    </div>
                  </details>
                {/if}
              </div>
            {/each}
          {/if}
        </div>
      {/if}

      <!-- Top processes column -->
      {#if hasTopProcesses}
        <div class="process-column">
          <div class="section-header">
            Top Processes
            <div class="top-controls">
              <select class="control-input" bind:value={topSort}>
                <option value="cpu">CPU</option>
                <option value="memory">MEMORY</option>
                <option value="disk">DISK</option>
              </select>
              <input
                type="number"
                class="control-input"
                style="width:3rem"
                bind:value={topLimit}
                min="1"
                max={limitProcesses}
              />
            </div>
          </div>

          {#if topError}
            <div class="muted">Failed to load top processes</div>
          {:else if topProcesses.length === 0}
            <div class="muted">No processes found</div>
          {:else}
            <div id="top-processes-list">
              {#each topProcesses as proc (proc.pid)}
                <ProcessCard
                  {proc}
                  isBeingTracked={false}
                  {allowProcessCommands}
                  {isAuthenticated}
                  {signals}
                  onRefresh={refreshTop}
                />
              {/each}
            </div>
          {/if}
        </div>
      {/if}
    </div>
  </div>
</aside>

<style>
  #process-pane {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
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
  .refresh-btn {
    color: var(--accent);
    border-color: rgba(99, 102, 241, 0.4);
  }
  .refresh-btn:hover {
    background: rgba(99, 102, 241, 0.1);
  }
  .pane-header h2 {
    font-size: 0.78rem;
    font-weight: 700;
    color: #fff;
    letter-spacing: 0.18em;
    text-transform: uppercase;
  }
  #process-content {
    overflow-y: auto;
    flex: 1;
    padding: 1.5rem 2rem 2rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }
  .process-columns {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(420px, 100%), 1fr));
    gap: 1.5rem;
    align-items: start;
  }
  .process-column {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    background: var(--bg-panel);
    border: 1px solid var(--border-soft);
    border-radius: 0.85rem;
    padding: 1rem 1rem 1.1rem;
  }
  .process-group {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  #work-banner {
    display: none;
    background: rgba(16, 185, 129, 0.1);
    border: 1px solid rgba(16, 185, 129, 0.25);
    border-radius: 0.6rem;
    color: var(--success);
    font-size: 0.8rem;
    font-weight: 600;
    padding: 0.75rem;
    text-align: center;
  }
  #work-banner.visible {
    display: block;
  }
  .section-header {
    font-size: 0.7rem;
    font-weight: 700;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.12em;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
  }
  .count-badge {
    background: var(--border);
    color: var(--text-base);
    border-radius: 6px;
    padding: 0.125rem 0.5rem;
    font-size: 0.7rem;
  }
  #top-processes-list {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  .top-controls {
    display: flex;
    gap: 0.5rem;
    align-items: center;
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
  .muted {
    color: var(--text-muted);
    font-size: 0.85rem;
    text-align: center;
    padding: 2rem 0;
    font-style: italic;
  }
  .children-group > summary {
    list-style: none;
  }
  .children-group > summary::-webkit-details-marker {
    display: none;
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
</style>
