<script>
  import { fmtTimestamp } from "../utils/format.js";
  import SignInNotice from "./SignInNotice.svelte";
  import { apiFetch } from "../api.js";

  let {
    active,
    enabled,
    onstatus,
    allowScreenCommands = false,
    isAuthenticated = false,
  } = $props();

  /** @type {Array<{monitor_id: string|number, monitor_name: string, width: number, height: number, timestamp: string, mime: string, data: string}>} */
  let screens = $state([]);
  let selectedIndex = $state(null);
  let selectedScreen = $derived(
    selectedIndex !== null && screens[selectedIndex]
      ? screens[selectedIndex]
      : null,
  );

  // ── Poll controls ─────────────────────────────────────────────────
  let pollPaused = $state(false);
  let pollIntervalInput = $state("2000");
  let pollCmdError = $state(null);

  async function togglePoll() {
    pollCmdError = null;
    const ep = pollPaused
      ? "/api/screen/poll/resume"
      : "/api/screen/poll/pause";
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
      const r = await apiFetch("/api/screen/poll/interval", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ interval_ms: ms }),
      });
      if (!r.ok) throw new Error((await r.json()).message ?? "failed");
    } catch (e) {
      pollCmdError = e.message;
    }
  }

  async function refresh() {
    try {
      const r = await apiFetch("/api/screenshot");
      if (!r.ok) throw new Error("HTTP error");
      const data = await r.json();
      screens = data.screens;
      onstatus(`● LIVE`, false);
    } catch {
      onstatus(`● OFFLINE · ${new Date().toLocaleTimeString()}`, true);
    }
  }

  let pollTimer = null;

  // Use $effect so polling starts/stops reactively based on `enabled`
  $effect(() => {
    if (enabled) {
      refresh();
      pollTimer = setInterval(refresh, 2000);
      return () => clearInterval(pollTimer);
    }
  });

  // Whether commands are actually usable
  let canCommand = $derived(allowScreenCommands && isAuthenticated);
</script>

<main id="screens-pane">
  <div class="pane-header">
    <div class="header-title">
      <h2>Monitored Screens</h2>
    </div>
    {#if allowScreenCommands}
      {#if !isAuthenticated}
          <SignInNotice name="screen" />
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
  <div id="screens" class:single={screens.length === 1}>
    {#each screens as screen, i (screen.monitor_id ?? i)}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="screen-container"
        class:clickable={screens.length > 1}
        onclick={() => {
          if (screens.length > 1) selectedIndex = i;
        }}
      >
        <div class="screen-label-row">
          <span class="screen-label screen-name"
            >{screen.monitor_name || `Display ${i + 1}`}</span
          >
          {#if screen.width && screen.height}
            <span class="screen-meta screen-dims"
              >{screen.width}×{screen.height}</span
            >
          {/if}
          <span class="screen-meta screen-ts"
            >{fmtTimestamp(screen.timestamp)}</span
          >
        </div>
        <img
          src={`data:${screen.mime};base64,${screen.data}`}
          alt={screen.monitor_name || `Display ${i + 1}`}
        />
      </div>
    {/each}
  </div>

  {#if selectedScreen}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="lightbox" onclick={() => (selectedIndex = null)}>
      <div class="lightbox-content" onclick={(e) => e.stopPropagation()}>
        <div class="screen-label-row lightbox-header">
          <span class="screen-label screen-name"
            >{selectedScreen.monitor_name ||
              `Display ${selectedIndex + 1}`}</span
          >
          {#if selectedScreen.width && selectedScreen.height}
            <span class="screen-meta screen-dims"
              >{selectedScreen.width}×{selectedScreen.height}</span
            >
          {/if}
          <span class="screen-meta screen-ts"
            >{fmtTimestamp(selectedScreen.timestamp)}</span
          >
          <button class="close-btn" onclick={() => (selectedIndex = null)}
            >✕</button
          >
        </div>
        <img
          src={`data:${selectedScreen.mime};base64,${selectedScreen.data}`}
          alt={selectedScreen.monitor_name || `Display ${selectedIndex + 1}`}
        />
      </div>
    </div>
  {/if}
</main>

<style>
  #screens-pane {
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
  .pane-header h2 {
    font-size: 0.78rem;
    font-weight: 700;
    color: #fff;
    letter-spacing: 0.18em;
    text-transform: uppercase;
  }
  #screens {
    padding: 1.75rem 2rem 2rem;
    overflow-y: auto;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(520px, 100%), 1fr));
    gap: 1.5rem;
    flex: 1;
    align-content: start;
  }
  #screens.single {
    grid-template-columns: 1fr;
  }
  @media (max-width: 768px) {
    #screens {
      grid-template-columns: 1fr;
      padding: 1rem;
    }
    .pane-header {
      padding: 0.85rem 1rem;
    }
  }
  .screen-container {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 0.85rem;
    padding: 0.6rem;
    box-shadow: 0 10px 30px -18px rgba(0, 0, 0, 0.6);
    transition:
      transform 0.2s ease,
      border-color 0.2s ease;
  }
  .screen-container:hover {
    transform: translateY(-2px);
    border-color: var(--accent);
  }
  .screen-container.clickable {
    cursor: zoom-in;
  }
  .screen-label {
    font-size: 0.7rem;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    font-weight: 600;
    padding: 0 0.5rem;
  }
  .screen-container img {
    width: 100%;
    height: auto;
    display: block;
    border-radius: 0.6rem;
    background: #000;
    object-fit: contain;
  }
  .screen-label-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0 0.5rem;
  }
  .screen-meta {
    font-size: 0.7rem;
    color: var(--text-muted);
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas,
      monospace;
  }

  /* Lightbox */
  .lightbox {
    position: fixed;
    top: 0;
    left: 0;
    width: 100vw;
    height: 100vh;
    background: rgba(0, 0, 0, 0.85);
    backdrop-filter: blur(8px);
    z-index: 9999;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 2rem;
  }
  .lightbox-content {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: 0.85rem;
    padding: 0.6rem;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.8);
    max-width: 90vw;
    max-height: 90vh;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  .lightbox-header {
    justify-content: flex-start;
  }
  .lightbox-content img {
    max-width: 100%;
    max-height: calc(90vh - 4rem);
    object-fit: contain;
    border-radius: 0.6rem;
    background: #000;
  }
  .close-btn {
    margin-left: auto;
    background: transparent;
    border: none;
    color: var(--text-muted);
    font-size: 1.2rem;
    cursor: pointer;
    line-height: 1;
    padding: 0.2rem 0.5rem;
  }
  .close-btn:hover {
    color: #fff;
  }
</style>
