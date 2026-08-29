<script>
  import { auth } from "../api.js";
  import { formatTime } from "../utils/format.js";

  let {
    info = null,
    alarms = null,
    activeTab,
    status,
    statusError,
    showScreens,
    showSystem,
    showProcesses,
    showSystemd,
    showDocker,
    showHistory = true,
    showLoginButton,
    shutdownDisabled,
    shutdownLabel,
    onactivatetab,
    onshutdown,
    onloginbutton,
  } = $props();

  // ── Alarms dropdown ──────────────────────────────────────────────
  let alarmsOpen = $state(false);
  let alarmBtnEl = $state(null);
  let alarmPanelEl = $state(null);

  function secsSince(since) {
    if (!since) return null;
    const ms =
      since.secs_since_epoch * 1000 + Math.floor(since.nanos_since_epoch / 1e6);
    return Math.max(0, Math.floor((Date.now() - ms) / 1000));
  }

  let activeAlarms = $derived(
    (() => {
      if (!alarms) return [];
      const out = [];
      if (alarms.cpu?.active)
        out.push({ label: "CPU", since: alarms.cpu.since });
      if (alarms.memory?.active)
        out.push({ label: "Memory", since: alarms.memory.since });
      if (alarms.battery_low?.active)
        out.push({ label: "Battery", since: alarms.battery_low.since });
      for (const [mount, status] of alarms.disks ?? []) {
        if (status.active)
          out.push({ label: `Disk ${mount}`, since: status.since });
      }
      return out;
    })(),
  );

  function toggleAlarms() {
    if (activeAlarms.length === 0) return; // nothing to show when clear
    alarmsOpen = !alarmsOpen;
  }

  function closeAlarms() {
    alarmsOpen = false;
  }

  function handleWindowClick(e) {
    if (!alarmsOpen) return;
    if (alarmBtnEl?.contains(e.target) || alarmPanelEl?.contains(e.target))
      return;
    closeAlarms();
  }

  function handleWindowKeydown(e) {
    if (e.key === "Escape" && alarmsOpen) {
      closeAlarms();
      alarmBtnEl?.focus();
    }
  }

  // Tab indicator
  let tabnavEl = $state(null);
  let tabEls = $state({});
  let indicatorStyle = $state("width:0;transform:translateX(0)");

  export function moveIndicator(name) {
    const btn = tabEls[name];
    if (!btn || !tabnavEl) return;
    const navRect = tabnavEl.getBoundingClientRect();
    const r = btn.getBoundingClientRect();
    indicatorStyle = `width:${r.width}px;transform:translateX(${r.left - navRect.left - 4}px)`;
  }

  export function getTabEl(name) {
    return tabEls[name];
  }

  let canLogout = $derived(
    Boolean(
      $auth.token &&
      (info?.auth_enabled ||
        info?.allow_process_commands ||
        info?.allow_screen_commands ||
        info?.allow_system_resources_commands ||
        info?.allow_systemd_commands ||
        info?.allow_docker_commands),
    ),
  );
</script>

<svelte:window onclick={handleWindowClick} onkeydown={handleWindowKeydown} />

<header id="topbar">
  <div class="topbar-brand">
    <span class="brand-dot"></span>
    <h1>Knight Watch</h1>
    <span id="status" class:error={statusError} class:ok={!statusError}
      >{status}</span
    >
  </div>

  <div id="tabnav" role="tablist" aria-label="Sections" bind:this={tabnavEl}>
    {#if showScreens}
      <button
        class="tab"
        role="tab"
        aria-selected={activeTab === "screens"}
        onclick={() => onactivatetab("screens")}
        bind:this={tabEls["screens"]}
      >
        <span class="tab-icon" aria-hidden="true">▦</span>
        <span class="tab-label">Screenshots</span>
      </button>
    {/if}

    {#if showSystem}
      <button
        class="tab"
        role="tab"
        aria-selected={activeTab === "system"}
        onclick={() => onactivatetab("system")}
        bind:this={tabEls["system"]}
      >
        <span class="tab-icon" aria-hidden="true">◉</span>
        <span class="tab-label">System</span>
      </button>
    {/if}

    {#if showProcesses}
      <button
        class="tab"
        role="tab"
        aria-selected={activeTab === "processes"}
        onclick={() => onactivatetab("processes")}
        bind:this={tabEls["processes"]}
      >
        <span class="tab-icon" aria-hidden="true">≡</span>
        <span class="tab-label">Processes</span>
      </button>
    {/if}

    {#if showSystemd}
      <button
        class="tab"
        role="tab"
        aria-selected={activeTab === "systemd"}
        onclick={() => onactivatetab("systemd")}
        bind:this={tabEls["systemd"]}
      >
        <span class="tab-icon" aria-hidden="true">≡</span>
        <span class="tab-label">Systemd</span>
      </button>
    {/if}

    {#if showDocker}
      <button
        class="tab"
        role="tab"
        aria-selected={activeTab === "docker"}
        onclick={() => onactivatetab("docker")}
        bind:this={tabEls["docker"]}
      >
        <span class="tab-icon" aria-hidden="true">◉</span>
        <span class="tab-label">Docker</span>
      </button>
    {/if}

    {#if showHistory}
      <button
        class="tab"
        role="tab"
        aria-selected={activeTab === "history"}
        onclick={() => onactivatetab("history")}
        bind:this={tabEls["history"]}
      >
        <span class="tab-icon" aria-hidden="true">▤</span>
        <span class="tab-label">History</span>
      </button>
    {/if}

    <span class="tab-indicator" aria-hidden="true" style={indicatorStyle}
    ></span>
  </div>

  <div class="topbar-actions">
    {#if alarms}
      <div class="alarm-wrap">
        <button
          class="alarm-indicator"
          class:alarm-active={activeAlarms.length > 0}
          class:alarm-clickable={activeAlarms.length > 0}
          aria-expanded={alarmsOpen}
          aria-haspopup="true"
          disabled={activeAlarms.length === 0}
          onclick={toggleAlarms}
          bind:this={alarmBtnEl}
        >
          <span class="alarm-dot"></span>
          {activeAlarms.length > 0
            ? `${activeAlarms.length} Alarm${activeAlarms.length > 1 ? "s" : ""}`
            : "Alarms Clear"}
        </button>

        {#if alarmsOpen}
          <div class="alarm-panel" role="menu" bind:this={alarmPanelEl}>
            <div class="alarm-panel-title">Active alarms</div>
            <ul>
              {#each activeAlarms as a (a.label)}
                <li>
                  <span class="alarm-panel-label">{a.label}</span>
                  <span class="alarm-panel-time">
                    {#if a.since !== null}
                      {formatTime(secsSince(a.since))}
                    {:else}
                      active
                    {/if}
                  </span>
                </li>
              {/each}
            </ul>
          </div>
        {/if}
      </div>
    {/if}
    {#if info}
      <span
        class="telegram-indicator"
        class:tg-on={info.telegram_bot}
        class:tg-off={!info.telegram_bot}
        title={info.telegram_bot
          ? "Telegram bot is running"
          : "Telegram bot is not running"}
      >
        TG Bot
      </span>
    {/if}

    {#if info?.shutdown_enabled}
      <button
        id="shutdown-btn"
        title="Shut down the server"
        disabled={shutdownDisabled}
        onclick={onshutdown}
      >
        <span class="sd-dot"></span>
        {shutdownLabel}
      </button>
    {/if}

    {#if showLoginButton}
      <button
        id="login-btn"
        title="Sign in to perform actions"
        onclick={onloginbutton}
      >
        <span class="login-icon" aria-hidden="true">⏻</span>
        Sign in
      </button>
    {/if}

    {#if canLogout}
      <button id="logout-btn" title="Sign out" onclick={() => auth.logout()}>
        <span class="logout-icon" aria-hidden="true">⏻</span>
        Sign out
      </button>
    {/if}
  </div>
</header>

<style>
  /* ── Top bar ───────────────────────────────────────────── */
  #topbar {
    height: var(--topbar-h);
    flex-shrink: 0;
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    gap: 1rem;
    padding: 0 1.25rem;
    background: rgba(18, 18, 20, 0.85);
    backdrop-filter: saturate(160%) blur(10px);
    -webkit-backdrop-filter: saturate(160%) blur(10px);
    border-bottom: 1px solid var(--border);
    position: relative;
    z-index: 50;
  }

  .topbar-brand {
    display: flex;
    align-items: center;
    gap: 0.65rem;
    min-width: 0;
  }
  .brand-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--accent);
    box-shadow: 0 0 12px var(--accent);
    flex-shrink: 0;
  }
  .topbar-brand h1 {
    font-size: 1rem;
    font-weight: 700;
    color: #fff;
    letter-spacing: 0.02em;
    white-space: nowrap;
  }
  #status {
    color: var(--text-muted);
    font-size: 0.7rem;
    font-family:
      ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    padding-left: 0.65rem;
    border-left: 1px solid var(--border);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  #status.error {
    color: var(--error);
  }
  #status.ok {
    color: var(--success);
  }

  .topbar-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 0.65rem;
  }

  .telegram-indicator {
    display: inline-flex;
    align-items: center;
    gap: 0.375rem;
    font-size: 0.68rem;
    font-weight: 700;
    font-family:
      ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    padding: 0.35rem 0.6rem;
    border-radius: 0.5rem;
    border: 1px solid var(--border);
    background: var(--bg-card);
  }
  .telegram-indicator::before {
    content: "✈";
    font-style: normal;
  }
  .telegram-indicator.tg-on {
    color: #34d399;
    border-color: rgba(16, 185, 129, 0.35);
  }
  .telegram-indicator.tg-off {
    color: var(--text-muted);
  }
  .alarm-wrap {
    position: relative;
  }

  .alarm-indicator {
    display: inline-flex;
    align-items: center;
    gap: 0.375rem;
    font-size: 0.68rem;
    font-weight: 700;
    font-family:
      ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    padding: 0.35rem 0.6rem;
    border-radius: 0.5rem;
    border: 1px solid var(--border);
    background: var(--bg-card);
    color: var(--text-muted);
    cursor: default;
  }
  .alarm-indicator.alarm-clickable {
    cursor: pointer;
  }
  .alarm-indicator:disabled {
    opacity: 1; /* still fully legible when clear, just not interactive */
  }

  .alarm-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--text-muted);
    flex-shrink: 0;
  }
  .alarm-indicator.alarm-active {
    color: var(--error);
    border-color: rgba(239, 68, 68, 0.35);
  }
  .alarm-indicator.alarm-active .alarm-dot {
    background: var(--error);
    box-shadow: 0 0 8px var(--error);
    animation: alarmPulse 1.4s ease-in-out infinite;
  }
  @keyframes alarmPulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.35;
    }
  }

  .alarm-panel {
    position: absolute;
    top: calc(100% + 8px);
    right: 0;
    min-width: 220px;
    background: rgba(24, 24, 27, 0.98);
    border: 1px solid var(--border);
    border-radius: 0.6rem;
    box-shadow: 0 12px 28px rgba(0, 0, 0, 0.45);
    padding: 0.6rem;
    z-index: 60;
  }
  .alarm-panel-title {
    font-size: 0.65rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-muted);
    padding: 0 0.3rem 0.4rem;
    border-bottom: 1px solid var(--border);
    margin-bottom: 0.35rem;
  }
  .alarm-panel ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }
  .alarm-panel li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.35rem 0.3rem;
    border-radius: 0.35rem;
    font-size: 0.75rem;
  }
  .alarm-panel li:hover {
    background: var(--bg-card);
  }
  .alarm-panel-label {
    color: var(--text-base);
    font-weight: 600;
  }
  .alarm-panel-time {
    color: var(--error);
    font-family:
      ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.7rem;
  }

  #shutdown-btn {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    background: transparent;
    border: 1px solid var(--error);
    color: var(--error);
    font-size: 0.7rem;
    font-weight: 700;
    font-family:
      ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    padding: 0.45rem 0.8rem;
    border-radius: 0.5rem;
    cursor: pointer;
    transition:
      background 0.15s ease,
      color 0.15s ease,
      transform 0.1s ease;
  }
  #shutdown-btn :global(.sd-dot) {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--error);
    box-shadow: 0 0 8px var(--error);
  }
  #shutdown-btn:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.12);
  }
  #shutdown-btn:active:not(:disabled) {
    transform: translateY(1px);
  }
  #shutdown-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  /* ── Tab nav ──────────────────────────────────────────── */
  #tabnav {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 999px;
    justify-self: center;
  }
  :global(.tab) {
    position: relative;
    z-index: 2;
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
    background: transparent;
    border: none;
    color: var(--text-muted);
    font-family: inherit;
    font-size: 0.78rem;
    font-weight: 600;
    letter-spacing: 0.02em;
    padding: 0.5rem 1rem;
    border-radius: 999px;
    cursor: pointer;
    transition: color 0.2s ease;
    white-space: nowrap;
  }
  :global(.tab .tab-icon) {
    font-size: 0.85rem;
    opacity: 0.85;
  }
  :global(.tab:hover) {
    color: var(--text-base);
  }
  :global(.tab[aria-selected="true"]) {
    color: #fff;
  }
  .tab-indicator {
    position: absolute;
    z-index: 1;
    top: 4px;
    bottom: 4px;
    left: 0;
    width: 0;
    border-radius: 999px;
    background: linear-gradient(135deg, var(--accent), var(--accent-2));
    box-shadow: 0 4px 14px rgba(59, 130, 246, 0.35);
    transition:
      transform 0.28s cubic-bezier(0.5, 0.05, 0.2, 1),
      width 0.28s cubic-bezier(0.5, 0.05, 0.2, 1);
  }

  /* ── Login button ─────────────────────────────────────── */
  #login-btn {
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
    letter-spacing: 0.06em;
    text-transform: uppercase;
    padding: 0.45rem 0.8rem;
    border-radius: 0.5rem;
    cursor: pointer;
    transition:
      background 0.15s ease,
      color 0.15s ease,
      transform 0.1s ease;
  }
  #login-btn:hover {
    background: rgba(59, 130, 246, 0.12);
    color: #fff;
  }
  #login-btn:active {
    transform: translateY(1px);
  }
  .login-icon {
    font-size: 0.8rem;
    opacity: 0.75;
  }

  /* ── Logout button ────────────────────────────────────── */
  #logout-btn {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text-muted);
    font-size: 0.7rem;
    font-weight: 700;
    font-family:
      ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    padding: 0.45rem 0.8rem;
    border-radius: 0.5rem;
    cursor: pointer;
    transition:
      background 0.15s ease,
      color 0.15s ease,
      border-color 0.15s ease,
      transform 0.1s ease;
  }
  #logout-btn:hover {
    background: var(--bg-card);
    color: var(--text-base);
    border-color: #3f3f46;
  }
  #logout-btn:active {
    transform: translateY(1px);
  }
  .logout-icon {
    font-size: 0.8rem;
    opacity: 0.75;
  }

  /* ── Responsive ───────────────────────────────────────── */
  @media (max-width: 720px) {
    #topbar {
      grid-template-columns: 1fr auto;
      grid-template-rows: auto auto;
      height: auto;
      padding: 0.6rem 0.85rem;
      gap: 0.5rem;
    }
    #tabnav {
      grid-column: 1 / -1;
      justify-self: stretch;
      overflow-x: auto;
    }
    :global(.tab-label) {
      display: none;
    }
    :global(.tab) {
      padding: 0.5rem 0.75rem;
    }
    #status {
      display: none;
    }
  }
</style>
