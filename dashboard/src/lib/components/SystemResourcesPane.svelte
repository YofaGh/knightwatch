<script>
  import { formatBytes, formatTime } from "../utils/format.js";
  import SignInNotice from "./SignInNotice.svelte";
  import { apiFetch } from "../api.js";

  let {
    active,
    enabled,
    onstatus,
    allowSystemResourcesCommands = false,
    isAuthenticated = false,
  } = $props();

  let snap = $state(null);

  async function refresh() {
    try {
      const r = await apiFetch("/api/system");
      if (!r.ok) throw new Error("HTTP error");
      snap = await r.json();
      onstatus("● LIVE", false);
    } catch {
      onstatus(`● OFFLINE · ${new Date().toLocaleTimeString()}`, true);
    }
  }

  let pollTimer = null;
  let statusTimer = null;

  // ── Lifecycle ────────────────────────────────────────────────────

  $effect(() => {
    if (!enabled) return;
    const pollAll = () => {
      fetchPollStatus();
      fetchThresholds();
      fetchMask();
    };
    pollAll();
    statusTimer = setInterval(pollAll, STATUS_CHECK_MS);
    return () => clearInterval(statusTimer);
  });

  // Drive the actual container-list refresh loop off the current known
  // interval/paused state. Re-runs (restarting the timer) whenever
  // pollInterval or pollPaused change, e.g. after fetchPollStatus picks up
  // a change made elsewhere.
  $effect(() => {
    if (!enabled || pollPaused) return;
    refresh();
    pollTimer = setInterval(refresh, pollInterval);
    return () => clearInterval(pollTimer);
  });

  // Whether commands are actually usable
  let canCommand = $derived(allowSystemResourcesCommands && isAuthenticated);

  // ── Poll controls ─────────────────────────────────────────────────
  let pollPaused = $state(false);
  let pollInterval = $state(1000); // current server-reported poll interval (ms)
  let pollIntervalInput = $state("1000");
  let pollCmdError = $state(null);

  // How often we re-check /api/docker/poll/status to notice changes made
  // elsewhere (another client pausing/resuming or changing the interval).
  const STATUS_CHECK_MS = 5000;

  async function togglePoll() {
    pollCmdError = null;
    const ep = pollPaused
      ? "/api/resources/poll/resume"
      : "/api/resources/poll/pause";
    try {
      const r = await apiFetch(ep, { method: "POST" });
      if (!r.ok) throw new Error((await r.json()).message ?? "failed");
      await fetchPollStatus();
    } catch (e) {
      pollCmdError = e.message;
    }
  }

  // ── Poll status ────────────────────────────────────────────────────
  async function fetchPollStatus() {
    try {
      const r = await apiFetch("/api/resources/poll/status");
      if (!r.ok) throw new Error("HTTP error");
      const status = await r.json();
      pollPaused = status.paused;
      pollInterval = status.interval;
      pollIntervalInput = String(status.interval);
    } catch {
      // transient failure fetching status — keep last known values,
      // the next scheduled check will retry.
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
      const r = await apiFetch("/api/resources/poll/interval", {
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

  // ── Thresholds ────────────────────────────────────────────────────
  let thresholds = $state({
    cpu_warn: 75,
    memory_warn: 75,
    disk_warn: 80,
    battery_low: 20,
  });
  let thresholdError = $state(null);
  let thresholdSuccess = $state(false);
  let showThresholds = $state(false);

  // ── Thresholds ────────────
  async function fetchThresholds() {
    try {
      const r = await apiFetch("/api/resources/thresholds");
      if (!r.ok) throw new Error("HTTP error");
      thresholds = await r.json();
    } catch {
      // transient failure — keep last known values, next scheduled check retries
    }
  }

  // ── Refresh mask ───────────
  async function fetchMask() {
    try {
      const r = await apiFetch("/api/resources/refresh-mask");
      if (!r.ok) throw new Error("HTTP error");
      mask = await r.json();
    } catch {
      // transient failure — keep last known values, next scheduled check retries
    }
  }

  async function applyThresholds() {
    thresholdError = null;
    thresholdSuccess = false;
    const { cpu_warn, memory_warn, disk_warn, battery_low } = thresholds;
    for (const v of [cpu_warn, memory_warn, disk_warn, battery_low]) {
      if (v < 0 || v > 100) {
        thresholdError = "All values must be 0–100";
        return;
      }
    }
    try {
      const r = await apiFetch("/api/resources/thresholds", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ cpu_warn, memory_warn, disk_warn, battery_low }),
      });
      if (!r.ok) throw new Error((await r.json()).message ?? "failed");
      thresholdSuccess = true;
      setTimeout(() => (thresholdSuccess = false), 2000);
    } catch (e) {
      thresholdError = e.message;
    }
  }

  // ── Refresh mask ──────────────────────────────────────────────────
  let mask = $state({
    cpu: true,
    memory: true,
    disks: true,
    networks: true,
    temperatures: true,
    gpus: true,
  });
  let maskError = $state(null);
  let maskSuccess = $state(false);
  let showMask = $state(false);

  async function applyMask() {
    maskError = null;
    maskSuccess = false;
    try {
      const r = await apiFetch("/api/resources/refresh-mask", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(mask),
      });
      if (!r.ok) throw new Error((await r.json()).message ?? "failed");
      maskSuccess = true;
      setTimeout(() => (maskSuccess = false), 2000);
    } catch (e) {
      maskError = e.message;
    }
  }

  // ── Derived helpers ───────────────────────────────────────────────
  const MAX_CORE_H = 32;

  function coreHeight(pct) {
    return Math.max(2, (pct / 100) * MAX_CORE_H);
  }
  function coreColor(pct) {
    if (pct >= 90) return "var(--error)";
    if (pct >= 75) return "var(--warning)";
    return "var(--accent)";
  }
  function barClass(pct) {
    if (pct >= 90) return "crit";
    if (pct >= 75) return "warn";
    return "";
  }
  function healthClass(h) {
    if (h === "healthy") return "health-healthy";
    if (h === "warning") return "health-warning";
    return "health-critical";
  }
  function activeNets(nets) {
    return (nets || []).filter(
      (n) => n.rx_total_bytes > 0 || n.tx_total_bytes > 0,
    );
  }
</script>

<div class="pane-header">
  <div class="header-title">
    <h2>System</h2>
  </div>
  {#if allowSystemResourcesCommands}
    {#if !isAuthenticated}
      <SignInNotice name="system resources" />
    {:else}
      <div class="cmd-controls">
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
        </div>
        <button
          class="poll-btn"
          onclick={() => (showThresholds = !showThresholds)}
          >⚠ Thresholds</button
        >
        <button class="poll-btn" onclick={() => (showMask = !showMask)}
          >⚙ Refresh Mask</button
        >
        {#if pollCmdError}
          <span class="poll-error">{pollCmdError}</span>
        {/if}
      </div>
    {/if}
  {/if}
</div>

{#if canCommand && showThresholds}
  <div class="sub-panel">
    <div class="sub-panel-title">Alert Thresholds (%)</div>
    <div class="sub-panel-row">
      {#each [["CPU warn", "cpu_warn"], ["Memory warn", "memory_warn"], ["Disk warn", "disk_warn"], ["Battery low", "battery_low"]] as [label, key]}
        <label class="sub-label">
          {label}
          <input
            type="number"
            class="control-input"
            style="width:4rem"
            bind:value={thresholds[key]}
            min="0"
            max="100"
          />
        </label>
      {/each}
      <button class="poll-btn apply" onclick={applyThresholds}>Apply</button>
      {#if thresholdSuccess}<span class="poll-ok">✓ Saved</span>{/if}
      {#if thresholdError}<span class="poll-error">{thresholdError}</span>{/if}
    </div>
  </div>
{/if}

{#if canCommand && showMask}
  <div class="sub-panel">
    <div class="sub-panel-title">Refresh Mask</div>
    <div class="sub-panel-row">
      {#each Object.keys(mask) as key}
        <label class="mask-label">
          <input type="checkbox" bind:checked={mask[key]} />
          {key}
        </label>
      {/each}
      <button class="poll-btn apply" onclick={applyMask}>Apply</button>
      {#if maskSuccess}<span class="poll-ok">✓ Saved</span>{/if}
      {#if maskError}<span class="poll-error">{maskError}</span>{/if}
    </div>
  </div>
{/if}

<div id="system-panel">
  {#if snap}
    <div id="system-panel-inner">
      <!-- Host -->
      <div class="sys-section">
        <div class="sys-section-title">Host</div>
        <div class="sys-grid">
          {#each [["Hostname", snap.host.hostname], ["OS", snap.host.os_name], ["Kernel", snap.host.kernel_version], ["Arch", snap.host.cpu_arch], ["Uptime", formatTime(snap.host.uptime_secs)], ["Procs", snap.host.process_count], ["Health", snap.health, healthClass(snap.health)]] as [label, value, cls]}
            <div class="sys-kv">
              <span class="sk">{label}</span>
              <span
                class="sv"
                class:health-healthy={cls === "health-healthy"}
                class:health-warning={cls === "health-warning"}
                class:health-critical={cls === "health-critical"}
                >{value ?? "—"}</span
              >
            </div>
          {/each}
        </div>
      </div>

      <!-- CPU -->
      <div class="sys-section">
        <div class="sys-section-title">CPU</div>
        <div class="sys-grid">
          <div class="sys-kv">
            <span class="sk">Brand</span><span class="sv">{snap.cpu.brand}</span
            >
          </div>
          <div class="sys-kv">
            <span class="sk">Cores</span><span class="sv"
              >{snap.cpu.physical_core_count ?? snap.cpu.cores.length}</span
            >
          </div>
          <div class="sys-kv">
            <span class="sk">Freq</span><span class="sv"
              >{snap.cpu.frequency_mhz} MHz</span
            >
          </div>
          <div class="sys-kv">
            <span class="sk">Usage</span><span class="sv"
              >{snap.cpu.usage_percent.toFixed(1)}%</span
            >
          </div>
          {#if snap.cpu.load_avg}
            <div class="sys-kv">
              <span class="sk">Load 1m</span><span class="sv"
                >{snap.cpu.load_avg.one.toFixed(2)}</span
              >
            </div>
            <div class="sys-kv">
              <span class="sk">Load 5m</span><span class="sv"
                >{snap.cpu.load_avg.five.toFixed(2)}</span
              >
            </div>
          {/if}
        </div>
        <div class="sys-cores-row">
          {#each snap.cpu.cores as core}
            <div
              class="sys-core-bar"
              title="{core.name}: {core.usage_percent.toFixed(1)}%"
              style="height:{coreHeight(
                core.usage_percent,
              )}px;background:{coreColor(core.usage_percent)}"
            ></div>
          {/each}
        </div>
      </div>

      <!-- Memory -->
      <div class="sys-section">
        <div class="sys-section-title">Memory</div>
        <div class="sys-grid">
          <div class="sys-bar-wrap" style="grid-column:1/-1">
            <div class="sys-bar-row">
              <span class="sys-bar-label">RAM</span>
              <div class="sys-bar-track">
                <div
                  class="sys-bar-fill {barClass(snap.memory.used_percent)}"
                  style="width:{Math.min(100, snap.memory.used_percent).toFixed(
                    1,
                  )}%"
                ></div>
              </div>
              <span class="sys-bar-val"
                >{snap.memory.used_percent.toFixed(1)}%</span
              >
            </div>
            {#if snap.memory.swap_used_percent != null}
              <div class="sys-bar-row">
                <span class="sys-bar-label">SWAP</span>
                <div class="sys-bar-track">
                  <div
                    class="sys-bar-fill {barClass(
                      snap.memory.swap_used_percent,
                    )}"
                    style="width:{Math.min(
                      100,
                      snap.memory.swap_used_percent,
                    ).toFixed(1)}%"
                  ></div>
                </div>
                <span class="sys-bar-val"
                  >{snap.memory.swap_used_percent.toFixed(1)}%</span
                >
              </div>
            {/if}
          </div>
          <div class="sys-kv">
            <span class="sk">Total</span><span class="sv"
              >{formatBytes(snap.memory.total_bytes)}</span
            >
          </div>
          <div class="sys-kv">
            <span class="sk">Used</span><span class="sv"
              >{formatBytes(snap.memory.used_bytes)}</span
            >
          </div>
          <div class="sys-kv">
            <span class="sk">Free</span><span class="sv"
              >{formatBytes(snap.memory.free_bytes)}</span
            >
          </div>
          <div class="sys-kv">
            <span class="sk">Avail</span><span class="sv"
              >{formatBytes(snap.memory.available_bytes)}</span
            >
          </div>
          {#if snap.memory.swap_total_bytes > 0}
            <div class="sys-kv">
              <span class="sk">Swap Total</span><span class="sv"
                >{formatBytes(snap.memory.swap_total_bytes)}</span
              >
            </div>
          {/if}
          {#if snap.memory.swap_used_bytes > 0}
            <div class="sys-kv">
              <span class="sk">Swap Used</span><span class="sv"
                >{formatBytes(snap.memory.swap_used_bytes)}</span
              >
            </div>
          {/if}
        </div>
      </div>

      <!-- Disks -->
      <div class="sys-section">
        <div class="sys-section-title">Disks</div>
        {#each snap.disks as d}
          <div class="sys-item">
            <span class="sys-item-name" title={d.name}>{d.mount_point}</span>
            <span class="sys-item-sub"
              >{d.file_system} · {d.kind}{d.is_removable
                ? " · removable"
                : ""}</span
            >
            <div class="sys-bar-track">
              <div
                class="sys-bar-fill {d.used_percent >= 95
                  ? 'crit'
                  : d.used_percent >= 80
                    ? 'warn'
                    : ''}"
                style="width:{Math.min(100, d.used_percent).toFixed(1)}%"
              ></div>
            </div>
            <div style="display:flex;gap:0.75rem;flex-wrap:wrap">
              <div class="sys-kv">
                <span class="sk">Used</span><span class="sv"
                  >{formatBytes(d.used_bytes)}</span
                >
              </div>
              <div class="sys-kv">
                <span class="sk">Free</span><span class="sv"
                  >{formatBytes(d.available_bytes)}</span
                >
              </div>
              <div class="sys-kv">
                <span class="sk">Total</span><span class="sv"
                  >{formatBytes(d.total_bytes)}</span
                >
              </div>
            </div>
          </div>
        {/each}
      </div>

      <!-- Network -->
      <div class="sys-section">
        <div class="sys-section-title">Network</div>
        {#if activeNets(snap.networks).length === 0}
          <span class="sys-item-sub">No active interfaces</span>
        {:else}
          {#each activeNets(snap.networks) as n}
            <div class="sys-item">
              <span class="sys-item-name">{n.interface}</span>
              <div class="sys-net-io">
                <div class="sys-net-badge">
                  <span class="dir">↓</span><span class="bw"
                    >{formatBytes(n.rx_bytes_per_sec)}/s</span
                  >
                </div>
                <div class="sys-net-badge">
                  <span class="dir">↑</span><span class="bw"
                    >{formatBytes(n.tx_bytes_per_sec)}/s</span
                  >
                </div>
              </div>
              <div style="display:flex;gap:0.75rem;flex-wrap:wrap">
                <div class="sys-kv">
                  <span class="sk">RX Total</span><span class="sv"
                    >{formatBytes(n.rx_total_bytes)}</span
                  >
                </div>
                <div class="sys-kv">
                  <span class="sk">TX Total</span><span class="sv"
                    >{formatBytes(n.tx_total_bytes)}</span
                  >
                </div>
              </div>
            </div>
          {/each}
        {/if}
      </div>

      <!-- GPU (conditional) -->
      {#if snap.gpus && snap.gpus.length > 0}
        <div class="sys-section">
          <div class="sys-section-title">GPU</div>
          {#each snap.gpus as g}
            <div class="sys-item">
              <span class="sys-item-name">{g.name}</span>
              {#if g.usage_percent != null}
                <div class="sys-bar-track">
                  <div
                    class="sys-bar-fill {barClass(g.usage_percent)}"
                    style="width:{Math.min(100, g.usage_percent).toFixed(1)}%"
                  ></div>
                </div>
              {/if}
              <div style="display:flex;gap:0.75rem;flex-wrap:wrap">
                {#if g.usage_percent != null}
                  <div class="sys-kv">
                    <span class="sk">Core</span><span class="sv"
                      >{g.usage_percent.toFixed(1)}%</span
                    >
                  </div>
                {/if}
                {#if g.vram_used_bytes != null}
                  <div class="sys-kv">
                    <span class="sk">VRAM</span><span class="sv"
                      >{formatBytes(g.vram_used_bytes)} / {formatBytes(
                        g.vram_total_bytes,
                      ) ?? "?"}</span
                    >
                  </div>
                {/if}
                {#if g.temperature_celsius != null}
                  <div class="sys-kv">
                    <span class="sk">Temp</span><span class="sv"
                      >{g.temperature_celsius.toFixed(0)}°C</span
                    >
                  </div>
                {/if}
                {#if g.power_draw_watts != null}
                  <div class="sys-kv">
                    <span class="sk">Power</span><span class="sv"
                      >{g.power_draw_watts.toFixed(0)}W</span
                    >
                  </div>
                {/if}
                {#if g.fan_speed_percent && g.fan_speed_percent.length > 0}
                  <div class="sys-kv">
                    <span class="sk"
                      >{g.fan_speed_percent.length > 1 ? "Fans" : "Fan"}</span
                    >
                    <span class="sv"
                      >{g.fan_speed_percent
                        .map((f) => f.toFixed(0) + "%")
                        .join(", ")}</span
                    >
                  </div>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      {/if}

      <!-- Battery (conditional) -->
      {#if snap.battery}
        <div class="sys-section">
          <div class="sys-section-title">Battery</div>
          <div class="sys-grid">
            <div class="sys-bar-wrap" style="grid-column:1/-1">
              <div class="sys-bar-row">
                <span class="sys-bar-label">Charge</span>
                <div class="sys-bar-track">
                  <div
                    class="sys-bar-fill {barClass(snap.battery.charge_percent)}"
                    style="width:{Math.min(
                      100,
                      snap.battery.charge_percent,
                    ).toFixed(1)}%"
                  ></div>
                </div>
                <span class="sys-bar-val"
                  >{snap.battery.charge_percent.toFixed(1)}%</span
                >
              </div>
            </div>
            <div class="sys-kv">
              <span class="sk">State</span><span class="sv"
                >{snap.battery.state}</span
              >
            </div>
            {#if snap.battery.time_to_empty_secs != null}
              <div class="sys-kv">
                <span class="sk">Empty in</span><span class="sv"
                  >{formatTime(snap.battery.time_to_empty_secs)}</span
                >
              </div>
            {/if}
            {#if snap.battery.time_to_full_secs != null}
              <div class="sys-kv">
                <span class="sk">Full in</span><span class="sv"
                  >{formatTime(snap.battery.time_to_full_secs)}</span
                >
              </div>
            {/if}
            {#if snap.battery.power_draw_watts != null}
              <div class="sys-kv">
                <span class="sk">Draw</span><span class="sv"
                  >{snap.battery.power_draw_watts.toFixed(1)}W</span
                >
              </div>
            {/if}
            {#if snap.battery.health_percent != null}
              <div class="sys-kv">
                <span class="sk">Health</span><span class="sv"
                  >{snap.battery.health_percent.toFixed(0)}%</span
                >
              </div>
            {/if}
            {#if snap.battery.cycle_count != null}
              <div class="sys-kv">
                <span class="sk">Cycles</span><span class="sv"
                  >{snap.battery.cycle_count}</span
                >
              </div>
            {/if}
          </div>
        </div>
      {/if}

      <!-- Thermals (conditional) -->
      {#if snap.temperatures && snap.temperatures.filter((t) => t.temperature_celsius != null).length > 0}
        <div class="sys-section">
          <div class="sys-section-title">Thermals</div>
          <div class="sys-thermal-chips">
            {#each snap.temperatures.filter((t) => t.temperature_celsius != null) as t}
              {@const isCrit =
                t.temperature_critical_celsius != null &&
                t.temperature_celsius >= t.temperature_critical_celsius}
              {@const isWarn = !isCrit && t.temperature_celsius >= 80}
              <div class="sys-thermal-chip">
                <span class="sys-thermal-label" title={t.label}>{t.label}</span>
                <span
                  class="sys-thermal-val"
                  class:crit={isCrit}
                  class:warn={isWarn}>{t.temperature_celsius.toFixed(0)}°C</span
                >
                {#if t.temperature_critical_celsius != null}
                  <span class="sys-thermal-label" style="min-width:0"
                    >/ {t.temperature_critical_celsius.toFixed(0)}°C</span
                  >
                {/if}
              </div>
            {/each}
          </div>
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
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
  .cmd-controls {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
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
  .poll-ok {
    font-size: 0.68rem;
    color: #34d399;
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
  .sub-panel {
    padding: 0.6rem 2rem;
    border-bottom: 1px solid var(--border-soft);
    background: var(--bg-panel);
    flex-shrink: 0;
  }
  .sub-panel-title {
    font-size: 0.6rem;
    font-weight: 800;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.12em;
    margin-bottom: 0.4rem;
  }
  .sub-panel-row {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    flex-wrap: wrap;
  }
  .sub-label {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    font-size: 0.68rem;
    color: var(--text-muted);
    font-family: inherit;
    cursor: default;
  }
  .mask-label {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    font-size: 0.68rem;
    color: var(--text-muted);
    font-family: inherit;
    cursor: pointer;
    text-transform: capitalize;
  }
  .pane-header h2 {
    font-size: 0.78rem;
    font-weight: 700;
    color: #fff;
    letter-spacing: 0.18em;
    text-transform: uppercase;
  }
  #system-panel {
    flex: 1;
    overflow: auto;
    padding: 1.5rem 2rem 2rem;
  }
  #system-panel-inner {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(320px, 100%), 1fr));
    gap: 1.25rem;
    align-items: start;
  }
  .sys-section {
    background: var(--bg-panel);
    border: 1px solid var(--border-soft);
    border-radius: 0.85rem;
    padding: 1rem 1.1rem 1.1rem;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
    min-width: 0;
  }
  .sys-section-title {
    font-size: 0.62rem;
    font-weight: 800;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.14em;
    margin-bottom: 0.15rem;
  }
  .sys-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    column-gap: 1.1rem;
    row-gap: 0.5rem;
    align-items: start;
  }
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
  .sys-kv .sv.health-healthy {
    color: var(--success);
  }
  .sys-kv .sv.health-warning {
    color: var(--warning);
  }
  .sys-kv .sv.health-critical {
    color: var(--error);
  }

  .sys-bar-wrap {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }
  .sys-bar-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .sys-bar-label {
    font-size: 0.6rem;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    font-weight: 600;
    white-space: nowrap;
    min-width: 2.5rem;
  }
  .sys-bar-track {
    flex: 1;
    height: 6px;
    background: rgba(255, 255, 255, 0.07);
    border-radius: 10px;
    overflow: hidden;
    min-width: 60px;
  }
  .sys-bar-fill {
    height: 100%;
    border-radius: 10px;
    transition: width 0.5s ease;
    background: linear-gradient(90deg, var(--accent), var(--accent-2));
  }
  .sys-bar-fill.warn {
    background: var(--warning);
  }
  .sys-bar-fill.crit {
    background: var(--error);
  }
  .sys-bar-val {
    font-size: 0.72rem;
    font-family:
      ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    color: #e4e4e7;
    min-width: 2.8rem;
    text-align: right;
    white-space: nowrap;
  }
  .sys-cores-row {
    display: flex;
    flex-direction: row;
    gap: 3px;
    flex-wrap: wrap;
    align-items: flex-end;
    min-height: 32px;
  }
  .sys-core-bar {
    width: 6px;
    border-radius: 2px 2px 0 0;
    background: var(--accent);
    min-height: 2px;
    transition: height 0.4s ease;
  }
  .sys-item {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }
  .sys-item + .sys-item {
    border-top: 1px solid var(--border);
    padding-top: 0.5rem;
    margin-top: 0.3rem;
  }
  .sys-item-name {
    font-size: 0.75rem;
    font-family:
      ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    color: #d4d4d8;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sys-item-sub {
    font-size: 0.62rem;
    color: var(--text-muted);
    white-space: nowrap;
  }
  .sys-thermal-chips {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    max-height: 200px;
    overflow-y: auto;
  }
  .sys-thermal-chip {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.7rem;
    font-family:
      ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  }
  .sys-thermal-label {
    color: var(--text-muted);
    min-width: 120px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sys-thermal-val {
    color: #e4e4e7;
    font-weight: 700;
  }
  .sys-thermal-val.crit {
    color: var(--error);
  }
  .sys-thermal-val.warn {
    color: var(--warning);
  }
  .sys-net-io {
    display: flex;
    gap: 0.75rem;
  }
  .sys-net-badge {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    font-size: 0.7rem;
    font-family:
      ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  }
  .sys-net-badge .dir {
    font-size: 0.6rem;
    color: var(--text-muted);
    font-weight: 700;
    text-transform: uppercase;
  }
  .sys-net-badge .bw {
    color: #e4e4e7;
  }
</style>
