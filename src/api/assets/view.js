const statusEl = document.getElementById("status");
const screensDiv = document.getElementById("screens");
const rootSection = document.getElementById("root-section");
const childSection = document.getElementById("children-section");
const childrenList = document.getElementById("children-list");
const childCount = document.getElementById("children-count");
const workBanner = document.getElementById("work-banner");

// ── Helpers ────────────────────────────────────────────────────────

function statePill(state) {
  const cls =
    state === "running"
      ? "state-running"
      : state === "sleeping"
        ? "state-sleeping"
        : state === "gone"
          ? "state-gone"
          : "state-other";
  return `<span class="state-pill ${cls}">${state}</span>`;
}

function metaItem(label, value) {
  return `<div class="proc-meta-item">
    <span class="label">${label}</span>
    <span class="value">${value}</span>
  </div>`;
}

function fmtBytes(n) {
  if (n == null) return "—";
  if (n >= 1073741824) return (n / 1073741824).toFixed(1) + " GB";
  if (n >= 1048576) return (n / 1048576).toFixed(1) + " MB";
  if (n >= 1024) return (n / 1024).toFixed(1) + " KB";
  return n + " B";
}

function fmtTimestamp(ts) {
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

function buildCard(proc, isRoot = false) {
  // ── Linux-only extras ──────────────────────────────────────────────
  let linuxExtras = "";

  if (proc.cmdline && proc.cmdline.length > 0) {
    const cmd = proc.cmdline.join(" ");
    linuxExtras += `<div class="proc-cmdline" title="${cmd}">${cmd}</div>`;
  }

  const hasIO = proc.io_stats != null;
  const hasCwd = proc.cwd != null;
  const hasFds = proc.open_fds != null;

  if (hasCwd || hasFds || hasIO) {
    linuxExtras += `<div class="proc-meta proc-meta-linux">`;
    if (hasCwd)
      linuxExtras += metaItem(
        "CWD",
        `<span title="${proc.cwd}">${proc.cwd}</span>`,
      );
    if (hasFds) linuxExtras += metaItem("FDs", proc.open_fds);
    if (hasIO) {
      linuxExtras += metaItem("READ", fmtBytes(proc.io_stats.read_bytes));
      linuxExtras += metaItem("WRITE", fmtBytes(proc.io_stats.write_bytes));
    }
    linuxExtras += `</div>`;
  }

  if (proc.open_files && proc.open_files.length > 0) {
    const fdTypeColor = {
      file: "#a78bfa",
      socket: "#34d399",
      pipe: "#fbbf24",
      other: "#a1a1aa",
    };
    const rows = proc.open_files
      .map((f) => {
        const color = fdTypeColor[f.fd_type] || "#a1a1aa";
        return `<div class="fd-row">
          <span class="fd-num">${f.fd}</span>
          <span class="fd-type" style="color:${color}">${f.fd_type}</span>
          <span class="fd-target" title="${f.target}">${f.target}</span>
        </div>`;
      })
      .join("");
    linuxExtras += `
      <div class="fd-section">
        <div class="fd-section-header">
          <span>Open File Descriptors</span>
          <span class="count-badge">${proc.open_files.length}</span>
        </div>
        <div class="fd-list">${rows}</div>
      </div>`;
  }

  return `
    <div class="proc-card ${isRoot ? "root-card" : ""}">
      <div class="proc-header">
        <div class="proc-name" title="${proc.name} (PID ${proc.pid})">
          ${isRoot ? "⬢ " : ""}${proc.name}
        </div>
        ${statePill(proc.state)}
      </div>
      <div class="proc-meta">
        ${metaItem("PID", proc.pid)}
        ${metaItem("CPU", proc.cpu_usage.toFixed(1) + "%")}
        ${metaItem("MEM", proc.memory_human)}
      </div>
      ${linuxExtras}
    </div>`;
}

// ── Screenshot refresh ─────────────────────────────────────────────

function refreshScreenshots() {
  const start = Date.now();
  fetch("/screenshot")
    .then((r) => {
      if (!r.ok) throw new Error("HTTP error");
      return r.json();
    })
    .then((data) => {
      data.screens.forEach((screen, i) => {
        const containerId = `screen-${screen.monitor_id ?? i}`;
        let container = document.getElementById(containerId);

        if (!container) {
          container = document.createElement("div");
          container.id = containerId;
          container.className = "screen-container";
          container.innerHTML = `
            <div class="screen-label-row">
              <span class="screen-label screen-name"></span>
              <span class="screen-meta screen-dims"></span>
              <span class="screen-meta screen-ts"></span>
            </div>
            <img alt="" />`;
          screensDiv.appendChild(container);
        }

        // Update label fields
        const nameEl = container.querySelector(".screen-name");
        const dimsEl = container.querySelector(".screen-dims");
        const tsEl = container.querySelector(".screen-ts");

        if (nameEl)
          nameEl.textContent = screen.monitor_name || `Display ${i + 1}`;
        if (dimsEl && screen.width && screen.height)
          dimsEl.textContent = `${screen.width}×${screen.height}`;
        if (tsEl) tsEl.textContent = fmtTimestamp(screen.timestamp);

        // Only swap src if the image actually changed
        const img = container.querySelector("img");
        img.alt = screen.monitor_name || `Display ${i + 1}`;
        const newSrc = `data:${screen.mime};base64,${screen.data}`;
        if (img.src !== newSrc) img.src = newSrc;
      });

      // Remove stale containers
      screensDiv.querySelectorAll(".screen-container").forEach((el) => {
        const idx = [
          ...screensDiv.querySelectorAll(".screen-container"),
        ].indexOf(el);
        if (idx >= data.screens.length) el.remove();
      });

      const elapsed = Date.now() - start;
      statusEl.textContent = `● LIVE · ${data.screens.length} SCREEN${data.screens.length > 1 ? "S" : ""} · ${elapsed}MS`;
    })
    .catch(() => {
      statusEl.innerHTML = `<span style="color:var(--error)">● OFFLINE · ${new Date().toLocaleTimeString()}</span>`;
    });
}

// ── Process refresh ────────────────────────────────────────────────

function refreshProcess() {
  fetch("/process")
    .then((r) => {
      if (!r.ok) throw new Error("HTTP error");
      return r.json();
    })
    .then((data) => {
      workBanner.classList.toggle("visible", data.work_done);

      if (data.root) {
        rootSection.innerHTML = buildCard(data.root, true);
      } else {
        rootSection.innerHTML = `<div class="muted">Root process exited</div>`;
      }

      if (data.child_count > 0) {
        childSection.style.display = "flex";
        childSection.style.flexDirection = "column";
        childSection.style.gap = "0.75rem";
        childCount.textContent = data.child_count;
        childrenList.innerHTML = data.children
          .map((c) => buildCard(c))
          .join("");
      } else {
        childSection.style.display = "none";
      }
    })
    .catch(() => {
      rootSection.innerHTML = `<div class="muted">Monitor disabled</div>`;
      childSection.style.display = "none";
    });
}

// ── Shutdown ───────────────────────────────────────────────────────

document.getElementById("shutdown-btn").addEventListener("click", () => {
  if (!confirm("Shut down the server?")) return;
  const btn = document.getElementById("shutdown-btn");
  btn.disabled = true;
  btn.textContent = "Shutting down…";
  fetch("/shutdown", { method: "POST" })
    .then(() => {
      statusEl.innerHTML = `<span style="color:var(--error)">● OFFLINE · Server shut down</span>`;
    })
    .catch(() => {
      // Server likely closed the connection immediately — that's expected
      statusEl.innerHTML = `<span style="color:var(--error)">● OFFLINE · Server shut down</span>`;
    });
});

// ── Boot ───────────────────────────────────────────────────────────

refreshScreenshots();
refreshProcess();
setInterval(refreshScreenshots, 2000);
setInterval(refreshProcess, 2000);
