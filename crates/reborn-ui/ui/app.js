// A test harness for the launcher core.
//
// Not a product interface. The real one is being designed separately, so this
// exists to do one job: give every feature a control the moment it exists, and
// show the raw answer.
//
// It is data driven on purpose. Adding a feature is one entry in FEATURES
// below, which is a line of data rather than a layout decision. That is what
// keeps design out of the critical path while the core is being finished.

const bridge = window.__TAURI__ && window.__TAURI__.core;

// ---------------------------------------------------------------------------
// Settings the harness needs to call anything. Kept in this window only: the
// real interface will own persistence, and duplicating it here would create a
// second source of truth to disagree with.
// ---------------------------------------------------------------------------

const SETTINGS = [
  {
    key: "runtime",
    name: "Dolphin",
    placeholder: "Full path to Dolphin.exe",
  },
  {
    key: "game",
    name: "Game image",
    placeholder: "Full path to the .iso or .rvz",
  },
  {
    key: "userDir",
    name: "User folder",
    placeholder: "Leave blank to auto detect",
  },
  {
    key: "legacyRoot",
    name: "2.1 install",
    placeholder: "Folder holding User and MODS",
    fallback: "C:\\Users\\Adam\\AppData\\Roaming\\NFL Street Reborn",
  },
];

function get(key) {
  const v = localStorage.getItem("h:" + key);
  if (v) return v;
  const s = SETTINGS.find((x) => x.key === key);
  return (s && s.fallback) || null;
}

function put(key, value) {
  if (value) localStorage.setItem("h:" + key, value);
  else localStorage.removeItem("h:" + key);
}

// ---------------------------------------------------------------------------
// The feature list.
//
// Each entry: a group, a label, the command name, its arguments, and whether
// it writes anything. `write: true` colours the button red, which is the only
// visual rule this harness has: a control that changes something on disk must
// not look like one that reads.
//
// `args` is a function returning the argument object, so it reads the current
// settings at click time rather than at load time.
// ---------------------------------------------------------------------------

const FEATURES = [
  {
    group: "Read",
    items: [
      {
        name: "paths",
        cmd: "paths",
        desc: "Install root, data root, whether saves survive uninstall",
        args: () => ({}),
      },
      {
        name: "home_state",
        cmd: "home_state",
        desc: "Sessions, minutes, controllers, mods applied",
        args: () => ({ runtime: get("runtime"), legacyRoot: get("legacyRoot") }),
      },
      {
        name: "devices",
        cmd: "devices",
        desc: "Controllers, as the emulator names them",
        args: () => ({ runtime: get("runtime") }),
      },
      {
        name: "sessions",
        cmd: "sessions",
        desc: "Recorded play sessions and how each ended",
        args: () => ({}),
      },
      {
        name: "mods",
        cmd: "mods",
        desc: "Mods on disk and whether each is applied",
        args: () => ({ legacyRoot: get("legacyRoot") }),
      },
    ],
  },
  {
    group: "Launch",
    items: [
      {
        name: "prepare_launch",
        cmd: "prepare_launch",
        desc: "Write config, start nothing. Says which user folder it used",
        write: true,
        needs: ["runtime", "game"],
        args: () => ({
          runtime: get("runtime"),
          game: get("game"),
          preset: preset(),
          fullscreen: fullscreen(),
          userDir: get("userDir"),
        }),
      },
      {
        name: "launch",
        cmd: "launch",
        desc: "Start the game and wait for it to exit",
        write: true,
        needs: ["runtime", "game"],
        slow: true,
        args: () => ({
          runtime: get("runtime"),
          game: get("game"),
          preset: preset(),
          fullscreen: fullscreen(),
          userDir: get("userDir"),
        }),
      },
    ],
    controls: true,
  },
];

// Launch options, shared by both launch rows.
let presetValue = "balanced";
let fullscreenValue = true;

function preset() {
  return presetValue;
}

function fullscreen() {
  return fullscreenValue;
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

const out = () => document.getElementById("out");

function log(label, value) {
  const stamp = new Date().toLocaleTimeString();
  const body =
    typeof value === "string" ? value : JSON.stringify(value, null, 2);
  out().textContent = `[${stamp}] ${label}\n${body}\n\n` + out().textContent;
}

function logError(label, e) {
  const stamp = new Date().toLocaleTimeString();
  out().textContent = `[${stamp}] ${label} FAILED\n${e}\n\n` + out().textContent;
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

function el(tag, cls, text) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text !== undefined) e.textContent = text;
  return e;
}

function renderSettings() {
  const host = document.getElementById("settings-rows");
  host.replaceChildren();
  for (const s of SETTINGS) {
    const row = el("div", "settings-row");
    row.appendChild(el("span", "name", s.name));
    const input = document.createElement("input");
    input.type = "text";
    input.placeholder = s.placeholder;
    input.value = get(s.key) || "";
    input.spellcheck = false;
    input.addEventListener("change", () => {
      put(s.key, input.value.trim());
      renderFeatures();
    });
    row.appendChild(input);
    host.appendChild(row);
  }
}

function launchControls() {
  const row = el("div", "row");
  row.appendChild(el("span", "name", "Launch options"));

  const sel = document.createElement("select");
  for (const p of ["performance", "balanced", "quality", "ultra"]) {
    const o = document.createElement("option");
    o.value = p;
    o.textContent = p;
    if (p === presetValue) o.selected = true;
    sel.appendChild(o);
  }
  sel.addEventListener("change", () => {
    presetValue = sel.value;
  });
  row.appendChild(sel);

  const label = el("label", "inline");
  const cb = document.createElement("input");
  cb.type = "checkbox";
  cb.checked = fullscreenValue;
  cb.addEventListener("change", () => {
    fullscreenValue = cb.checked;
  });
  label.appendChild(cb);
  label.appendChild(el("span", null, "fullscreen"));
  row.appendChild(label);

  row.appendChild(el("span", "desc", ""));
  return row;
}

function renderFeatures() {
  const host = document.getElementById("features");
  host.replaceChildren();

  for (const g of FEATURES) {
    host.appendChild(el("h2", null, g.group));
    if (g.controls) host.appendChild(launchControls());

    for (const f of g.items) {
      const row = el("div", "row");
      row.appendChild(el("span", "name", f.name));

      // Say what is missing instead of failing on click.
      const missing = (f.needs || []).filter((k) => !get(k));
      if (missing.length) {
        row.appendChild(
          el("span", "desc missing", "needs " + missing.join(" and ")),
        );
      } else {
        row.appendChild(el("span", "desc", f.desc));
      }

      const btn = el("button", f.write ? "write" : "", "Run");
      btn.disabled = missing.length > 0 || !bridge;
      btn.addEventListener("click", () => run(f, btn));
      row.appendChild(btn);

      host.appendChild(row);
    }
  }
}

async function run(f, btn) {
  const label = btn.textContent;
  btn.disabled = true;
  btn.textContent = f.slow ? "Running" : "...";
  if (f.slow) log(f.cmd, "started, waiting for the game to exit");
  try {
    const r = await bridge.invoke(f.cmd, f.args());
    log(f.cmd, r === null || r === undefined ? "ok, no value" : r);
  } catch (e) {
    logError(f.cmd, e);
  } finally {
    btn.disabled = false;
    btn.textContent = label;
  }
}

// ---------------------------------------------------------------------------
// Boot
// ---------------------------------------------------------------------------

document.getElementById("clear").addEventListener("click", () => {
  out().textContent = "";
});

const pill = document.getElementById("bridge");
if (bridge) {
  pill.textContent = "core attached";
  pill.className = "pill ok";
} else {
  // A missing bridge is stated, not left as a set of buttons that silently do
  // nothing.
  pill.textContent = "core NOT attached";
  pill.className = "pill bad";
  log("startup", "The launcher core is not attached, so nothing can be called.");
}

renderSettings();
renderFeatures();
