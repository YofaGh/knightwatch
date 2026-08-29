<script>
  import { onMount } from "svelte";
  import "./global.css";
  import ScreensPane from "./lib/components/ScreensPane.svelte";
  import SystemResourcesPane from "./lib/components/SystemResourcesPane.svelte";
  import ProcessesPane from "./lib/components/ProcessesPane.svelte";
  import SystemdPane from "./lib/components/SystemdPane.svelte";
  import LoginPage from "./lib/components/LoginPage.svelte";
  import DockerPane from "./lib/components/DockerPane.svelte";
  import HistoryPane from "./lib/components/HistoryPane.svelte";
  import TopBar from "./lib/components/TopBar.svelte";
  import { auth, apiFetch } from "./lib/api.js";

  // ── State ──────────────────────────────────────────────────────────
  let activeTab = $state("screens");
  let info = $state(null);
  let shutdownDisabled = $state(false);
  let shutdownLabel = $state("Shutdown");

  // ── Alarms ────────────────────────────────────────────────────────
  let alarms = $state(null);

  // ── Per-pane statuses ──────────────────────────────────────────────
  let paneStatuses = $state({
    screens: null,
    system: null,
    processes: null,
    systemd: null,
    docker: null,
    history: null,
    app: null, // for shutdown / app-level messages
  });

  function setPaneStatus(pane, label, error = false) {
    paneStatuses = { ...paneStatuses, [pane]: { label, error } };
  }

  // Derived: app-level overrides first, then active tab, then first error, then first non-null
  let status = $derived(
    (() => {
      if (paneStatuses.app) return paneStatuses.app;
      const active = paneStatuses[activeTab];
      if (active) return active;
      const errEntry = Object.values(paneStatuses).find((s) => s?.error);
      if (errEntry) return errEntry;
      return (
        Object.values(paneStatuses).find((s) => s !== null) ?? {
          label: "Loading…",
          error: false,
        }
      );
    })(),
  );

  let statusLabel = $derived(status.label);
  let statusError = $derived(status.error);

  // ── Auth ───────────────────────────────────────────────────────────
  let needsLogin = $derived(info !== null && info.auth_enabled && !$auth.token);
  let showLoginButton = $derived(
    info !== null &&
      !info.auth_enabled &&
      (info.allow_process_commands ||
        info.allow_screen_commands ||
        info.allow_system_resources_commands ||
        info.allow_docker_commands ||
        info.allow_systemd_commands) &&
      !$auth.token,
  );

  function onLogin() {
    loadInfo();
  }

  let showLoginPage = $state(false);

  function handleLoginButton() {
    showLoginPage = true;
  }

  function handleLoginPageLogin() {
    showLoginPage = false;
    loadInfo();
  }

  function handleLoginPageBack() {
    showLoginPage = false;
  }

  // ── Tab visibility ─────────────────────────────────────────────────
  let showScreens = $derived(!info || !info.blind);
  let showSystem = $derived(!info || info.system_resources !== false);
  let showDocker = $derived(!info || info.docker !== false);
  let showProcesses = $derived(
    !info ||
      (info.pid && info.pid.length > 0) ||
      info.top_processes === true ||
      info.allow_process_commands === true,
  );
  let showSystemd = $derived(!info || info.systemd !== false);
  let historyAuthed = $derived(!info?.auth_enabled || !!$auth.token);

  const TAB_NAMES = [
    "screens",
    "system",
    "processes",
    "systemd",
    "docker",
    "history",
  ];

  // ── TopBar ref (for indicator control) ────────────────────────────
  let topbar = $state(null);

  // ── Tab activation ─────────────────────────────────────────────────
  function activateTab(name, { focus = false } = {}) {
    activeTab = name;
    try {
      history.replaceState(null, "", "#" + name);
    } catch {}
    requestAnimationFrame(() => {
      topbar?.moveIndicator(name);
      if (focus) topbar?.getTabEl(name)?.focus();
    });
  }

  // ── info load ───────────────────────────────────────────────────
  async function loadInfo() {
    try {
      const r = await apiFetch("/api/info");
      if (!r.ok) throw new Error("info fetch failed");
      info = await r.json();
    } catch (e) {
      if (e?.message === "Unauthorized") return;
      info = {
        auth_enabled: false,
        shutdown_enabled: false,
        blind: false,
        pid: [],
        top_processes: false,
        limit_processes: 5,
        telegram_bot: false,
        system_resources: false,
        systemd: false,
        docker: false,
        allow_process_commands: false,
        allow_screen_commands: false,
        allow_system_resources_commands: false,
        allow_systemd_commands: false,
        allow_docker_commands: false,
      };
    }

    if (info.blind && activeTab === "screens") {
      activateTab("system");
    }

    requestAnimationFrame(() => {
      const visibleTabs = TAB_NAMES.filter((t) => {
        if (t === "screens" && info.blind) return false;
        if (t === "system" && !info.system_resources) return false;
        if (t === "systemd" && !info.systemd) return false;
        if (t === "docker" && !info.docker) return false;
        return true;
      });
      if (!visibleTabs.includes(activeTab)) {
        activateTab(visibleTabs[0] ?? "processes");
      } else {
        topbar?.moveIndicator(activeTab);
      }
    });
  }

  async function loadAlarms() {
    try {
      const r = await apiFetch("/api/alarms");
      if (!r.ok) throw new Error("alarms fetch failed");
      alarms = await r.json();
    } catch (e) {
      if (e?.message === "Unauthorized") return;
    }
  }

  // ── Shutdown ──────────────────────────────────────────────────────
  async function handleShutdown() {
    if (!confirm("Shut down the server?")) return;
    shutdownDisabled = true;
    shutdownLabel = "Shutting down…";
    try {
      await apiFetch("/api/shutdown", { method: "POST" });
    } catch {}
    setPaneStatus("app", "● OFFLINE · Server shut down", true);
  }

  // ── Init ──────────────────────────────────────────────────────────
  onMount(() => {
    const fromHash = (location.hash || "").replace("#", "");
    const initial = TAB_NAMES.includes(fromHash) ? fromHash : "screens";
    requestAnimationFrame(() => activateTab(initial));
    let alarmsInterval = null;
    loadInfo().then(() => {
      if (info?.system_resources) {
        loadAlarms();
        alarmsInterval = setInterval(loadAlarms, 10000);
      }
    });
    const onResize = () => topbar?.moveIndicator(activeTab);
    window.addEventListener("resize", onResize);
    return () => {
      window.removeEventListener("resize", onResize);
      if (alarmsInterval) clearInterval(alarmsInterval);
    };
  });
</script>

{#if needsLogin}
  <LoginPage {onLogin} />
{:else if showLoginPage}
  <LoginPage onLogin={handleLoginPageLogin} onBack={handleLoginPageBack} />
{:else}
  <TopBar
    bind:this={topbar}
    {info}
    {alarms}
    {activeTab}
    status={statusLabel}
    {statusError}
    {showScreens}
    {showSystem}
    {showProcesses}
    {showSystemd}
    {showDocker}
    {showLoginButton}
    {shutdownDisabled}
    {shutdownLabel}
    onactivatetab={activateTab}
    onshutdown={handleShutdown}
    onloginbutton={handleLoginButton}
  />

  <div id="panes">
    {#if info}
      {#if showScreens}
        <section
          class="pane"
          class:active={activeTab === "screens"}
          role="tabpanel"
        >
          <ScreensPane
            active={activeTab === "screens"}
            enabled={!info.blind}
            allowScreenCommands={info.allow_screen_commands ?? false}
            isAuthenticated={!!$auth.token}
            onstatus={(label, error) => setPaneStatus("screens", label, error)}
          />
        </section>
      {/if}

      {#if showSystem}
        <section
          class="pane"
          class:active={activeTab === "system"}
          role="tabpanel"
        >
          <SystemResourcesPane
            active={activeTab === "system"}
            enabled={info.system_resources}
            allowSystemResourcesCommands={info.allow_system_resources_commands ??
              false}
            isAuthenticated={!!$auth.token}
            onstatus={(label, error) => setPaneStatus("system", label, error)}
          />
        </section>
      {/if}

      {#if showProcesses}
        <section
          class="pane"
          class:active={activeTab === "processes"}
          role="tabpanel"
        >
          <ProcessesPane
            active={activeTab === "processes"}
            hasPids={info.pid && info.pid.length > 0}
            hasTopProcesses={info.top_processes}
            limitProcesses={info.limit_processes ?? 50}
            allowProcessCommands={info.allow_process_commands ?? false}
            isAuthenticated={!!$auth.token}
            onstatus={(label, error) =>
              setPaneStatus("processes", label, error)}
          />
        </section>
      {/if}

      {#if showSystemd}
        <section
          class="pane"
          class:active={activeTab === "systemd"}
          role="tabpanel"
        >
          <SystemdPane
            active={activeTab === "systemd"}
            enabled={info.systemd}
            allowSystemdCommands={info.allow_systemd_commands ?? false}
            isAuthenticated={!!$auth.token}
            onstatus={(label, error) => setPaneStatus("systemd", label, error)}
          />
        </section>
      {/if}

      {#if showDocker}
        <section
          class="pane"
          class:active={activeTab === "docker"}
          role="tabpanel"
        >
          <DockerPane
            active={activeTab === "docker"}
            enabled={info.docker}
            allowDockerCommands={info.allow_docker_commands ?? false}
            isAuthenticated={!!$auth.token}
            onstatus={(label, error) => setPaneStatus("docker", label, error)}
          />
        </section>
      {/if}

      <section
        class="pane"
        class:active={activeTab === "history"}
        role="tabpanel"
      >
        <HistoryPane
          active={activeTab === "history"}
          isAuthenticated={historyAuthed}
          onstatus={(label, error) => setPaneStatus("history", label, error)}
        />
      </section>
    {/if}
  </div>
{/if}

<style>
  /* ── Pane container ───────────────────────────────────── */
  #panes {
    flex: 1;
    position: relative;
    overflow: hidden;
  }
  .pane {
    position: absolute;
    inset: 0;
    display: none;
    flex-direction: column;
    overflow: hidden;
    animation: paneIn 0.25s ease both;
  }
  .pane.active {
    display: flex;
  }
  @keyframes paneIn {
    from {
      opacity: 0;
      transform: translateY(4px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
</style>
