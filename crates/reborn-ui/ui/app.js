// The launcher shell.
//
// This file draws and navigates. It decides nothing. Every fact on screen comes
// from a Tauri command backed by reborn-core, where the rules are tested. If a
// value is not available, the page says so rather than showing a placeholder,
// because an interface that shows a controller which is not connected is worse
// than one that shows nothing.

// The bridge to Rust.
//
// Guarded rather than assumed. The first version of this file read
// window.__TAURI__.core.invoke at module scope, and because Tauri 2 does not
// expose that global unless withGlobalTauri is set, the line threw before
// anything drew. The result was a window with a title bar and nothing else:
// no rail, no content, and no error anywhere a user could see it.
//
// So a missing bridge is now a visible, explained state.
const bridge = window.__TAURI__ && window.__TAURI__.core;

async function invoke(cmd, args) {
  if (!bridge) {
    throw new Error(
      "the bridge to the launcher core is not available, so no real data can be shown"
    );
  }
  return bridge.invoke(cmd, args);
}

// ---------------------------------------------------------------------------
// Destinations, in the order the information architecture fixes them.
// Settings is separated by a divider because it is a utility, not a content
// destination.
// ---------------------------------------------------------------------------

const SECTIONS = [
  { id: "home", label: "Home", icon: "◆" },
  { id: "play", label: "Play", icon: "▶" },
  { id: "online", label: "Online", icon: "●" },
  { id: "mods", label: "Mods", icon: "■" },
  { id: "music", label: "Music", icon: "♪" },
  { id: "stats", label: "Stats", icon: "≡" },
  { divider: true },
  { id: "settings", label: "Settings", icon: "⚙" },
];

let current = "home";

// The runtime is not known until the player points at one, and the launcher
// does not pretend otherwise. Device names are only authoritative when they
// come from the runtime itself.
let runtimePath = localStorage.getItem("runtimePath") || null;

// ---------------------------------------------------------------------------
// Rail
// ---------------------------------------------------------------------------

function buildRail() {
  const rail = document.getElementById("rail");
  rail.replaceChildren();

  for (const s of SECTIONS) {
    if (s.divider) {
      const d = document.createElement("div");
      d.className = "rail-divider";
      rail.appendChild(d);
      continue;
    }
    const b = document.createElement("button");
    b.className = "rail-item";
    b.dataset.section = s.id;
    b.innerHTML =
      `<span class="rail-icon">${s.icon}</span>` +
      `<span class="rail-label">${s.label}</span>`;
    b.addEventListener("click", () => go(s.id));
    rail.appendChild(b);
  }
  markCurrent();
}

function markCurrent() {
  for (const b of document.querySelectorAll(".rail-item")) {
    if (b.dataset.section === current) {
      b.setAttribute("aria-current", "page");
    } else {
      b.removeAttribute("aria-current");
    }
  }
}

function go(section) {
  current = section;
  markCurrent();
  const meta = SECTIONS.find((s) => s.id === section);
  document.getElementById("page-title").textContent = meta ? meta.label : "";
  render();
}

/** Shoulder buttons cycle sections, which is the one-press jump the rail exists
 * to support. Wraps, because a controller user should never hit a dead end. */
function cycleSection(delta) {
  const ids = SECTIONS.filter((s) => !s.divider).map((s) => s.id);
  const i = ids.indexOf(current);
  go(ids[(i + delta + ids.length) % ids.length]);
}

// ---------------------------------------------------------------------------
// Rendering helpers
// ---------------------------------------------------------------------------

function el(tag, cls, text) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text !== undefined) e.textContent = text;
  return e;
}

function card(title) {
  const c = el("div", "card");
  if (title) c.appendChild(el("h3", null, title));
  return c;
}

function row(label, value, pill) {
  const r = el("div", "row");
  r.appendChild(el("div", "label", label));
  if (pill) {
    r.appendChild(pill);
  } else {
    r.appendChild(el("div", "value", value));
  }
  return r;
}

function pill(text, kind) {
  return el("span", `pill ${kind}`, text);
}

/** A banner for a partial answer. Not an error: "we did not look everywhere" is
 * not a failure, but it must never be silent. */
function notice(html) {
  const n = el("div", "notice");
  n.innerHTML = html;
  return n;
}

// ---------------------------------------------------------------------------
// Pages
// ---------------------------------------------------------------------------

async function render() {
  const content = document.getElementById("content");
  content.replaceChildren();

  switch (current) {
    case "home":
      return renderHome(content);
    case "play":
      return renderPlay(content);
    case "settings":
      return renderSettings(content);
    default:
      return renderPlaceholder(content);
  }
}

async function renderHome(content) {
  content.appendChild(el("h2", "page-heading", "Home"));
  content.appendChild(
    el("p", "page-sub", "Where everything lives, and whether it is safe."),
  );

  let p;
  try {
    p = await invoke("paths");
  } catch (e) {
    content.appendChild(notice(`<strong>Could not read paths.</strong> ${e}`));
    return;
  }

  const c = card("Locations");
  c.appendChild(row("Install root", p.install_root));
  c.appendChild(row("Data root", p.data_root));
  c.appendChild(
    row(
      "Saves survive uninstall",
      null,
      p.saves_survive_uninstall
        ? pill("yes", "ok")
        : pill("NO, memory cards are at risk", "bad"),
    ),
  );
  content.appendChild(c);

  // The property above is the whole reason the layout is what it is: the 2.1
  // uninstaller deletes its install root, and 2.1 keeps memory cards inside it.
  if (!p.saves_survive_uninstall) {
    content.appendChild(
      notice(
        "<strong>The data root is inside the install root.</strong> " +
          "Uninstalling would delete memory cards. This is the exact bug the " +
          "new layout exists to prevent.",
      ),
    );
  }

  const made = p.directories.filter((d) => d.exists).length;
  const d = card(`Data directories (${made} of ${p.directories.length} created)`);
  for (const dir of p.directories) {
    const r = el("div", "row");
    r.appendChild(el("div", "label", dir.relative));
    r.appendChild(
      dir.class === "user"
        ? pill("user data", "ok")
        : dir.class === "derived"
          ? pill("derived", "data")
          : pill("managed", "warn"),
    );
    r.appendChild(el("div", "muted", dir.exists ? "present" : "not created yet"));
    d.appendChild(r);
  }
  content.appendChild(d);
}

async function renderPlay(content) {
  content.appendChild(el("h2", "page-heading", "Play"));
  content.appendChild(
    el("p", "page-sub", "Controllers are read from the runtime, never guessed."),
  );

  let report;
  try {
    report = await invoke("devices", { runtime: runtimePath });
  } catch (e) {
    content.appendChild(notice(`<strong>Could not read devices.</strong> ${e}`));
    return;
  }

  // The distinction that matters. A list from the built in scan sees XInput
  // only, which on a real machine misses an entire backend, so the page says
  // which answer it is showing.
  if (report.source !== "runtime") {
    content.appendChild(
      notice(
        "<strong>This list is incomplete.</strong> It comes from the built in " +
          "scan, which only sees XInput. Point Settings at a Reborn Dolphin to " +
          "get the authoritative list, which is the only one that includes " +
          "PlayStation pads.",
      ),
    );
  }

  const c = card("Controllers");
  if (report.pads.length === 0) {
    c.appendChild(el("p", "muted", "No controller detected."));
    c.appendChild(
      el(
        "p",
        "muted",
        "If one is plugged in it may have gone to sleep. Press its home " +
          "button to wake it, then reopen this page.",
      ),
    );
  } else {
    for (const d of report.pads) {
      const r = el("div", "row");
      r.appendChild(el("div", "label", `Port ${d.port}`));
      r.appendChild(el("div", "value", d.device_string));
      r.appendChild(pill(d.family, "data"));
      c.appendChild(r);
    }
  }
  content.appendChild(c);

  if (report.other.length > 0) {
    const o = card("Also detected, not used");
    for (const d of report.other) {
      const r = el("div", "row");
      r.appendChild(el("div", "value", d.device_string));
      r.appendChild(el("div", "muted", d.set_aside || ""));
      o.appendChild(r);
    }
    content.appendChild(o);
  }

  if (!report.complete) {
    for (const u of report.unsearched) {
      content.appendChild(notice(`<strong>Not searched.</strong> ${u}`));
    }
  }

  updatePadIndicator(report);
}

async function renderSettings(content) {
  content.appendChild(el("h2", "page-heading", "Settings"));
  content.appendChild(el("p", "page-sub", "Runtime and session history."));

  const c = card("Reborn Dolphin");
  c.appendChild(
    row("Runtime", runtimePath || "not set, device names will be incomplete"),
  );

  const input = document.createElement("input");
  input.className = "value";
  input.style.cssText =
    "flex:1;background:var(--surface-3);border:1px solid var(--border-default);" +
    "border-radius:8px;height:var(--control-height);padding:0 var(--space-3);" +
    "color:var(--text-primary);";
  input.placeholder = "Full path to Dolphin.exe";
  input.value = runtimePath || "";

  const save = el("button", "btn primary", "Use this runtime");
  save.addEventListener("click", () => {
    runtimePath = input.value.trim() || null;
    if (runtimePath) {
      localStorage.setItem("runtimePath", runtimePath);
    } else {
      localStorage.removeItem("runtimePath");
    }
    updateRuntimeLabel();
    go("play");
  });

  const r = el("div", "row");
  r.appendChild(el("div", "label", "Path"));
  r.appendChild(input);
  r.appendChild(save);
  c.appendChild(r);
  content.appendChild(c);

  const s = card("Recent sessions");
  try {
    const list = await invoke("sessions");
    if (list.length === 0) {
      s.appendChild(el("p", "muted", "No sessions recorded yet."));
    } else {
      for (const x of list.slice(0, 10)) {
        const rr = el("div", "row");
        rr.appendChild(el("div", "label", x.id));
        rr.appendChild(el("div", "muted", `${x.minutes.toFixed(1)} min`));
        rr.appendChild(
          x.needs_recovery ? pill(x.ended, "bad") : pill(x.ended, "ok"),
        );
        s.appendChild(rr);
      }
    }
  } catch (e) {
    s.appendChild(el("p", "muted", String(e)));
  }
  content.appendChild(s);
}

function renderPlaceholder(content) {
  const meta = SECTIONS.find((s) => s.id === current);
  content.appendChild(el("h2", "page-heading", meta.label));
  // Named as not built rather than dressed up as empty. An empty state implies
  // the feature works and has nothing in it, which is a different claim.
  content.appendChild(
    notice(
      `<strong>${meta.label} is not built yet.</strong> The shell routes to it ` +
        "so navigation can be tested, but there is nothing behind it.",
    ),
  );
}

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

function updateRuntimeLabel() {
  const t = document.getElementById("runtime-state");
  t.textContent = runtimePath ? "runtime set" : "no runtime selected";
}

function updatePadIndicator(report) {
  const dot = document.getElementById("pad-dot");
  const label = document.getElementById("pad-state");
  if (report && report.pads.length > 0) {
    dot.className = "dot live";
    label.textContent =
      report.pads.length === 1 ? "1 controller" : `${report.pads.length} controllers`;
  } else {
    dot.className = "dot off";
    label.textContent = "no controller";
  }
}

// ---------------------------------------------------------------------------
// Input
// ---------------------------------------------------------------------------

document.addEventListener("keydown", (e) => {
  // Keyboard equivalents of the shoulder buttons, so the shell is navigable
  // before gamepad polling exists.
  if (e.key === "[") cycleSection(-1);
  if (e.key === "]") cycleSection(1);
});

buildRail();
if (!bridge) {
  const c = document.getElementById("content");
  c.appendChild(
    notice(
      "<strong>Running without the launcher core.</strong> The shell is drawn but every value would be invented, so none are shown."
    )
  );
}
updateRuntimeLabel();
go("home");
