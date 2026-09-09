// A test harness for the launcher core.
//
// Not a product interface. The real one is being designed separately, so this
// exists to do one job: give every feature a control, and show the raw answer.
//
// Data driven on purpose. A feature is one entry in FEATURES, which is a line
// of data rather than a layout decision. That keeps design off the critical
// path while the core is finished, and means a command can be exercised the
// moment it exists.
//
// Two rules are enforced here because they are correctness, not style:
//   a control that writes, deletes or launches is red
//   a destructive command is never sent without confirm: true

const bridge = window.__TAURI__ && window.__TAURI__.core;

// ---------------------------------------------------------------------------
// Shared settings. Kept in this window only: the real interface will own
// persistence, and a second store here would be a second source of truth.
// ---------------------------------------------------------------------------

const SETTINGS = [
  { key: "runtime", name: "Dolphin", placeholder: "Full path to Dolphin.exe" },
  { key: "game", name: "Game image", placeholder: "Full path to the .iso or .rvz" },
  { key: "userDir", name: "User folder", placeholder: "Blank to auto detect" },
  {
    key: "legacyRoot",
    name: "2.1 install",
    placeholder: "Folder holding User and MODS",
    fallback: "",
  },
];

// "id@version, id@version" into the shape the mods commands take.
//
// Split on the LAST @ so a version containing one still parses, and drop empty
// entries so a trailing comma is not sent as a package with no name.
function parseRefs(text) {
  return (text || "")
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean)
    .map((s) => {
      const at = s.lastIndexOf("@");
      if (at < 1) return { id: s, version: "" };
      return { id: s.slice(0, at), version: s.slice(at + 1) };
    });
}

function get(key) {
  const v = localStorage.getItem("h:" + key);
  if (v) return v;
  const s = SETTINGS.find((x) => x.key === key);
  return (s && s.fallback) || null;
}

// A path as a person pasted it, cleaned.
//
// Windows Explorer's Copy as path wraps the path in quotes. Pasted into a box
// they are stored with it, and the result matches nothing while looking
// perfect: the refusal prints the path back with the quotes, and they read as
// ordinary punctuation. This cost a session's worth of wrong guesses.
//
// The core strips them too, on the way in. Doing it here as well means the
// STORED value is clean, so it is clean the next time it is shown.
function cleanPath(value) {
  let out = (value || "").trim();
  for (const q of ['"', "'"]) {
    if (out.length >= 2 && out.startsWith(q) && out.endsWith(q)) {
      out = out.slice(1, -1).trim();
      break;
    }
  }
  return out;
}

function put(key, value) {
  value = cleanPath(value);
  if (value) localStorage.setItem("h:" + key, value);
  else localStorage.removeItem("h:" + key);
}

let presetValue = "balanced";
let fullscreenValue = true;

// ---------------------------------------------------------------------------
// The 31 commands.
//
// `inputs` renders a text box per argument, remembered per row.
// `confirm` renders a checkbox and refuses to run unless it is ticked, which is
// how a destructive command is prevented from firing on a single click.
// `write` colours the button.
// ---------------------------------------------------------------------------

const FEATURES = [
  {
    group: "State",
    items: [
      { cmd: "paths", desc: "Roots, and whether saves survive uninstall" },
      {
        cmd: "home_state",
        desc: "Sessions, minutes, controllers, mods applied",
        args: () => ({ runtime: get("runtime"), legacyRoot: get("legacyRoot") }),
      },
      {
        cmd: "user_dir",
        desc: "Which Dolphin user folder a launch would use, and why",
        args: () => ({ chosen: get("userDir"), runtime: get("runtime") }),
      },
      { cmd: "components", desc: "Component identities, so nothing guesses a name" },
      { cmd: "sessions", desc: "Recorded sessions and how each ended" },
    ],
  },
  {
    group: "Controllers",
    items: [
      {
        cmd: "devices",
        desc: "Pads, named the way the emulator names them",
        args: () => ({ runtime: get("runtime") }),
      },
      {
        cmd: "gamepad_contract",
        desc: "Launcher navigation bindings for a pad family",
        inputs: [{ key: "family", placeholder: "xbox, playstation, blank for default" }],
      },
    ],
  },
  {
    group: "Launch",
    launchOptions: true,
    items: [
      {
        cmd: "prepare_launch",
        desc: "Write config, start nothing",
        write: true,
        needs: ["runtime", "game"],
        args: () => ({
          runtime: get("runtime"),
          game: get("game"),
          preset: presetValue,
          fullscreen: fullscreenValue,
          userDir: get("userDir"),
        }),
      },
      {
        cmd: "launch_check",
        desc: "What a launch would use. Finds, never starts",
        args: () => ({ runtime: get("runtime"), game: get("game"), userDir: get("userDir") }),
      },
      {
        cmd: "launch",
        desc: "Start the game and wait for it to exit",
        write: true,
        slow: true,
        needs: ["runtime", "game"],
        args: () => ({
          runtime: get("runtime"),
          game: get("game"),
          preset: presetValue,
          fullscreen: fullscreenValue,
          userDir: get("userDir"),
          modsDisabled: false,
          leagueMatch: false,
        }),
      },
    ],
  },
  {
    group: "Recovery and updates",
    items: [
      {
        cmd: "open_runtime",
        desc: "Open the emulator itself, no game, against the folder a launch uses",
        write: true,
        args: () => ({ runtime: get("runtime"), userDir: get("userDir") }),
      },
      {
        cmd: "diagnostic_bundle",
        desc: "Copy settings, config and the session list into one folder under recovery",
        write: true,
        args: () => ({ runtime: get("runtime") }),
      },
      {
        cmd: "update_check",
        desc: "Ask the release page whether a newer launcher exists",
        inputs: [{ key: "source", placeholder: "manifest URL, blank for the release page" }],
      },
      {
        cmd: "update_download",
        desc: "Download and verify the newer installer. Installs nothing",
        write: true,
        inputs: [{ key: "source", placeholder: "manifest URL, blank for the release page" }],
      },
      {
        cmd: "update_apply",
        desc: "Start the downloaded installer and close the launcher",
        write: true,
        confirm: true,
        inputs: [{ key: "path", placeholder: "installer path from update_download" }],
      },
    ],
  },
  {
    group: "Mods",
    items: [
      {
        cmd: "mods",
        desc: "Mods on disk and whether each is applied",
        args: () => ({ legacyRoot: get("legacyRoot") }),
      },
      { cmd: "mod_packages", desc: "Packages the launcher knows about" },
      {
        cmd: "jsgme_library",
        desc: "Read the real JSGME mods on disk",
        inputs: [
          { key: "modsRoot", placeholder: "MODS folder", derive: () => get("legacyRoot") + "\\MODS" },
          { key: "games", placeholder: "game ids, comma separated", list: true },
        ],
      },
      {
        cmd: "mods_plan",
        desc: "What applying would do. Writes nothing",
        inputs: [
          { key: "game", placeholder: "game id" },
          {
            key: "packages",
            placeholder: "id@version, id@version",
            refs: true,
          },
        ],
      },
      {
        cmd: "mods_apply",
        desc: "Compose the tree by hardlink",
        write: true,
        confirm: true,
        inputs: [
          { key: "game", placeholder: "game id" },
          {
            key: "packages",
            placeholder: "id@version, id@version",
            refs: true,
          },
        ],
      },
      {
        cmd: "mods_unapply",
        desc: "Rebuild without overlays. Nothing to lose, by design",
        write: true,
        confirm: true,
        inputs: [{ key: "game", placeholder: "game id" }],
      },
      {
        cmd: "mods_sync",
        desc: "Make the game load what activation.json says. Idle when it already does",
        write: true,
        args: () => ({ runtime: get("runtime") }),
      },
      {
        cmd: "mods_toggle",
        desc: "One package on or off, others in its group off, then sync",
        write: true,
        args: () => ({ runtime: get("runtime") }),
        inputs: [
          { key: "id", placeholder: "package id" },
          { key: "version", placeholder: "version" },
          { key: "on", placeholder: "on: true or false", bool: true },
        ],
      },
      {
        cmd: "mods_verify",
        desc: "Does the composed tree match the plan",
        inputs: [
          { key: "game", placeholder: "game id" },
          {
            key: "packages",
            placeholder: "id@version, id@version",
            refs: true,
          },
        ],
      },
    ],
  },
  {
    group: "Migration",
    items: [
      {
        cmd: "migration_plan",
        desc: "What moving a 2.1 install would do. Copies nothing",
        args: () => ({ legacyRoot: get("legacyRoot") }),
      },
      {
        cmd: "migration_apply",
        desc: "Copy it. Saves first, textures last",
        write: true,
        confirm: true,
        needs: ["legacyRoot"],
        args: () => ({ legacyRoot: get("legacyRoot"), savesOnly: true }),
      },
    ],
  },
  {
    group: "Library",
    items: [
      {
        cmd: "library_verify",
        desc: "Hash a tree against a manifest",
        inputs: [
          { key: "manifestPath", placeholder: "path to a manifest json" },
          { key: "root", placeholder: "tree to check, blank for default" },
        ],
      },
      {
        cmd: "library_repair",
        desc: "Replace damaged files. Spares user content",
        write: true,
        confirm: true,
        inputs: [{ key: "component", placeholder: "component id" }],
      },
    ],
  },
  {
    group: "Content",
    items: [
      {
        cmd: "content_installed",
        desc: "What version of a component is installed",
        inputs: [{ key: "component", placeholder: "component id" }],
      },
      {
        cmd: "content_update",
        desc: "Stage, verify, promote",
        write: true,
        confirm: true,
        inputs: [{ key: "component", placeholder: "component id" }],
      },
    ],
  },
  {
    group: "Stats",
    items: [
      { cmd: "stats_queue", desc: "Captures waiting to be submitted" },
      {
        cmd: "stats_ingest",
        desc: "Read every capture folder into the queue",
        write: true,
        confirm: true,
        inputs: [
          { key: "userDir", placeholder: "Dolphin user folder", derive: () => get("userDir") || get("legacyRoot") + "\\User" },
          { key: "sessionId", placeholder: "session id, optional" },
        ],
      },
      {
        cmd: "settings_set_path",
        desc: "Record a path on disk, where the shortcuts read it. Empty clears it",
        write: true,
        inputs: [
          { key: "which", placeholder: "runtime|game|user_dir" },
          { key: "value", placeholder: "path, or empty to clear" },
        ],
      },
      {
        cmd: "stats_submit",
        // No confirm box: the command takes no confirm, and sending is what
        // the queue is for. The server answers a repeat as a duplicate, so a
        // send that happens twice costs nothing.
        desc: "Send everything queued. Results the server will not take are kept",
        write: true,
      },
    ],
  },
  {
    group: "Music",
    items: [
      { cmd: "music_sources", desc: "Imported tracks the launcher can see" },
      {
        cmd: "music_build_pack",
        desc: "Build a pack. The binary table writer is a known gap",
        write: true,
        confirm: true,
      },
    ],
  },
  {
    group: "First run",
    items: [
      {
        cmd: "onboarding_progress",
        desc: "Which first run stages are satisfied",
        args: () => ({ legacyRoot: get("legacyRoot") }),
      },
      {
        cmd: "onboarding_reach",
        desc: "Open a stage's screen. Stage slug, eg mod_selection",
        inputs: [{ key: "stage", placeholder: "stage slug" }],
        args: () => ({ legacyRoot: get("legacyRoot") }),
        write: true,
      },
      {
        cmd: "onboarding_back",
        desc: "Where Back goes from a stage. Changes nothing",
        inputs: [{ key: "stage", placeholder: "stage slug" }],
        args: () => ({
          stage: get("backFrom"),
          legacyRoot: get("legacyRoot"),
        }),
      },
      {
        cmd: "onboarding_acknowledge",
        desc: "Answer the welcome screen",
        args: () => ({ legacyRoot: get("legacyRoot") }),
        write: true,
      },
      {
        cmd: "onboarding_decline_migration",
        desc: "Leave the 2.1 install alone. Writes only a journal line",
        args: () => ({ legacyRoot: get("legacyRoot") }),
        write: true,
      },
      {
        cmd: "onboarding_accept_migration",
        desc: "Choose to bring 2.1 over. Records the decision, copies nothing",
        args: () => ({ legacyRoot: get("legacyRoot") }),
        write: true,
      },
      {
        cmd: "onboarding_confirm_game",
        desc: "Confirm the image. Its size is weighed here, not passed in",
        inputs: [{ key: "image", placeholder: "path to the iso" }],
        args: () => ({ legacyRoot: get("legacyRoot") }),
        write: true,
      },
      {
        cmd: "onboarding_choose_graphics",
        desc: "Record a preset. Nothing measures this PC, so no recommendation",
        inputs: [{ key: "preset", placeholder: "performance|balanced|quality" }],
        args: () => ({ legacyRoot: get("legacyRoot") }),
        write: true,
      },
      {
        cmd: "onboarding_choose_controller",
        desc: "Accept the pad on a port. Its device string comes from the emulator",
        inputs: [
          { key: "expectedDevice", placeholder: "device string from the controller scan" },
          { key: "port", placeholder: "port, 0 based", number: true },
          { key: "runtime", placeholder: "path to Dolphin exe, for every backend" },
        ],
        args: () => ({ legacyRoot: get("legacyRoot") }),
        write: true,
      },
      {
        cmd: "onboarding_decline_controller",
        desc: "No pad, and why. The reason is shown later, so it is required",
        inputs: [{ key: "because", placeholder: "why there is no pad" }],
        args: () => ({ legacyRoot: get("legacyRoot") }),
        write: true,
      },
      {
        cmd: "onboarding_choose_updates",
        desc: "First run: may the launcher update on its own",
        write: true,
        args: () => ({ legacyRoot: get("legacyRoot") }),
        inputs: [{ key: "automatic", placeholder: "true or false", bool: true }],
      },
      { cmd: "update_policy", desc: "Whether updates are automatic, and where that was decided" },
      { cmd: "texture_policy", desc: "Whether the launcher manages the emulator's texture folder" },
      {
        cmd: "texture_policy_set",
        desc: "Off leaves your own texture folder completely alone. For texture authors",
        write: true,
        inputs: [{ key: "managed", placeholder: "true or false", bool: true }],
      },
      {
        cmd: "update_policy_set",
        desc: "Change it later, without re-walking first run",
        write: true,
        inputs: [{ key: "automatic", placeholder: "true or false", bool: true }],
      },
      {
        cmd: "controls_options",
        desc: "The three button art profiles, with the one your pad suggests",
        inputs: [{ key: "runtime", placeholder: "path to Dolphin exe, for the suggestion" }],
      },
      { cmd: "controls_current", desc: "The button art in force, and where it came from" },
      {
        cmd: "controls_set",
        desc: "Change the button art at any time. A setting, not a first run answer",
        args: () => ({ runtime: get("runtime") }),
        inputs: [{ key: "profile", placeholder: "xbox|playstation|gamecube" }],
        write: true,
      },
      {
        cmd: "onboarding_choose_controls",
        desc: "Record the button art. xbox, playstation, or gamecube",
        inputs: [{ key: "profile", placeholder: "xbox|playstation|gamecube" }],
        args: () => ({ legacyRoot: get("legacyRoot") }),
        write: true,
      },
      {
        cmd: "onboarding_run_task",
        desc: "Do a preparation step for real, then record it. write_config|check_everything",
        inputs: [
          { key: "task", placeholder: "write_config|check_everything" },
          { key: "runtime", placeholder: "path to Dolphin exe" },
          { key: "game", placeholder: "path to the game image" },
        ],
        args: () => ({ legacyRoot: get("legacyRoot") }),
        write: true,
      },
      {
        cmd: "onboarding_record_task",
        desc: "Mark a preparation task done. Refused if an earlier stage is unanswered",
        inputs: [{ key: "task", placeholder: "task slug" }],
        args: () => ({ legacyRoot: get("legacyRoot") }),
        write: true,
      },
      {
        cmd: "onboarding_prepared",
        desc: "Preparation finished. Refused while a task is outstanding",
        args: () => ({ legacyRoot: get("legacyRoot") }),
        write: true,
      },
      {
        cmd: "onboarding_finish",
        desc: "Latch first run complete. Once only",
        args: () => ({ legacyRoot: get("legacyRoot") }),
        write: true,
        confirm: true,
      },
      {
        cmd: "onboarding_start_over",
        desc: "Discard first run and offer it again. The only way past the latch",
        args: () => ({ legacyRoot: get("legacyRoot") }),
        write: true,
        confirm: true,
      },
    ],
  },
  {
    group: "System",
    items: [
      { cmd: "removable_drives", desc: "Drives the Xbox wizard may write to" },

      {
        cmd: "credential_stored",
        desc: "Whether a secret is stored. Never the secret",
        inputs: [{ key: "account", placeholder: "account name" }],
      },
      {
        cmd: "ini_value",
        desc: "Read one key out of an ini",
        inputs: [
          { key: "path", placeholder: "path to the ini" },
          { key: "section", placeholder: "section" },
          { key: "key", placeholder: "key" },
        ],
      },
    ],
  },
];

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

const out = () => document.getElementById("out");

function log(label, value) {
  const stamp = new Date().toLocaleTimeString();
  const body = typeof value === "string" ? value : JSON.stringify(value, null, 2);
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

function launchOptionsRow() {
  const row = el("div", "row");
  row.appendChild(el("span", "name", "options"));
  const sel = document.createElement("select");
  for (const p of ["performance", "balanced", "quality"]) {
    const o = document.createElement("option");
    o.value = p;
    o.textContent = p;
    if (p === presetValue) o.selected = true;
    sel.appendChild(o);
  }
  sel.addEventListener("change", () => (presetValue = sel.value));
  row.appendChild(sel);

  const lab = el("label", "inline");
  const cb = document.createElement("input");
  cb.type = "checkbox";
  cb.checked = fullscreenValue;
  cb.addEventListener("change", () => (fullscreenValue = cb.checked));
  lab.appendChild(cb);
  lab.appendChild(el("span", null, "fullscreen"));
  row.appendChild(lab);
  row.appendChild(el("span", "desc", ""));
  return row;
}

function renderFeatures() {
  const host = document.getElementById("features");
  host.replaceChildren();

  for (const g of FEATURES) {
    host.appendChild(el("h2", null, g.group));
    if (g.launchOptions) host.appendChild(launchOptionsRow());

    for (const f of g.items) {
      const row = el("div", "row");
      row.appendChild(el("span", "name", f.cmd));

      const missing = (f.needs || []).filter((k) => !get(k));
      if (missing.length) {
        row.appendChild(el("span", "desc missing", "needs " + missing.join(" and ")));
      } else if (!f.inputs) {
        row.appendChild(el("span", "desc", f.desc));
      }

      // Per-row argument boxes, remembered so a path is typed once.
      const fields = {};
      for (const inp of f.inputs || []) {
        const box = document.createElement("input");
        box.type = "text";
        box.placeholder = inp.placeholder;
        const saved = localStorage.getItem("a:" + f.cmd + ":" + inp.key);
        box.value = saved || (inp.derive ? inp.derive() : "");
        box.spellcheck = false;
        box.addEventListener("change", () => {
          // Cleaned in the box as well as in storage, so what is shown is what
          // is sent.
          box.value = cleanPath(box.value);
          localStorage.setItem("a:" + f.cmd + ":" + inp.key, box.value);
        });
        fields[inp.key] = {
          box,
          list: inp.list,
          refs: inp.refs,
          number: inp.number,
        };
        row.appendChild(box);
      }

      // Confirm, for anything destructive. Unticked means it will not fire.
      let confirmBox = null;
      if (f.confirm) {
        const lab = el("label", "inline");
        confirmBox = document.createElement("input");
        confirmBox.type = "checkbox";
        lab.appendChild(confirmBox);
        lab.appendChild(el("span", null, "confirm"));
        row.appendChild(lab);
      }

      const btn = el("button", f.write ? "write" : "", "Run");
      btn.disabled = missing.length > 0 || !bridge;
      btn.addEventListener("click", () => run(f, btn, fields, confirmBox));
      row.appendChild(btn);

      host.appendChild(row);
    }
  }
}

function buildArgs(f, fields, confirmBox) {
  const args = f.args ? f.args() : {};
  for (const [key, { box, list, refs, number, bool }] of Object.entries(fields)) {
    const raw = box.value.trim();
    if (bool) {
      // Anything but an explicit yes is false. A bool that defaulted to
      // true on an empty box would turn a mod on by leaving a field blank.
      args[key] = ["true", "1", "on", "yes"].includes(raw.toLowerCase());
    } else if (refs) {
      args[key] = parseRefs(raw);
    } else if (list) {
      args[key] = raw ? raw.split(",").map((s) => s.trim()).filter(Boolean) : [];
    } else if (number) {
      // An empty box is 0 rather than null: the commands that take a number
      // take a port, and port 1 is the sensible default to try.
      args[key] = raw ? Number(raw) : 0;
    } else {
      args[key] = raw || null;
    }
  }
  if (f.confirm) args.confirm = !!(confirmBox && confirmBox.checked);
  return args;
}

async function run(f, btn, fields, confirmBox) {
  const args = buildArgs(f, fields, confirmBox);

  // A destructive command is never sent unconfirmed. Said here rather than
  // relying on the core to refuse, so the reason is visible where the click is.
  if (f.confirm && !args.confirm) {
    log(f.cmd, "not sent: tick confirm first. This command changes files.");
    return;
  }

  const label = btn.textContent;
  btn.disabled = true;
  btn.textContent = f.slow ? "Running" : "...";
  if (f.slow) log(f.cmd, "started, waiting for the game to exit");

  try {
    const r = await bridge.invoke(f.cmd, args);
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
  const n = FEATURES.reduce((a, g) => a + g.items.length, 0);
  pill.textContent = `core attached, ${n} commands`;
  pill.className = "pill ok";
} else {
  pill.textContent = "core NOT attached";
  pill.className = "pill bad";
  log("startup", "The launcher core is not attached, so nothing can be called.");
}

renderSettings();
renderFeatures();
