// Mods, as a screen.
//
// A feel-the-flow build, not the product interface. It exists to answer the
// questions a harness row cannot: what does "only one football" look like as a
// control, where does the button art live, and what does the page say when a
// mod is listed as on and its files are gone.
//
// Two rules, both correctness rather than style:
//
//   Every value on screen comes from a command. Nothing is a placeholder, and
//   nothing is filled in with a plausible default. What nothing measured is
//   drawn as absent.
//
//   The core owns the rules. This file never decides which packages exclude
//   each other: the exclusive groups come from the file lists in the core, and
//   turning one member on is the core turning the others off. A second copy of
//   that rule here would be a second source of truth.

const bridge = window.__TAURI__ ? window.__TAURI__.core : null;

// The emulator the Settings page names, if any. Passed with every action
// that links the texture folder, so the mods land in the user folder a
// launch of that emulator will actually read.
function runtimeHint() {
  try {
    return localStorage.getItem("h:runtime") || null;
  } catch (_) {
    return null;
  }
}

// What the last reads returned. Null until read, and null again if a read
// failed, so a stale list is never drawn as current.
let library = null; // mod_packages
let options = null; // controls_options, or { error }
let current = null; // controls_current
// The last sync the page saw, from mods_sync, mods_toggle or controls_set.
let lastSync = null;
// One action at a time. A second click while the first is composing would
// race it on the same tree.
let busy = false;
// A failure, in the core's words, drawn at the top. Cleared by the next action.
let problem = null;
// A partial answer. Not a failure: "recorded, applied at the next launch".
let notice = null;

// ---------------------------------------------------------------------------
// Talking to the core
// ---------------------------------------------------------------------------

function log(what, value) {
  const pre = document.getElementById("out");
  const body = typeof value === "string" ? value : JSON.stringify(value, null, 2);
  pre.textContent = `${what}\n${body}\n\n${pre.textContent}`.slice(0, 20000);
}

async function call(cmd, args = {}) {
  if (!bridge) throw new Error("the launcher core is not attached");
  const answer = await bridge.invoke(cmd, args);
  log(cmd, answer === undefined ? "ok" : answer);
  return answer;
}

function reason(e) {
  if (e === null || e === undefined) return "unknown failure";
  if (typeof e === "string") return e;
  if (e.message) return e.message;
  return JSON.stringify(e);
}

// Read everything the page shows. Each read settles on its own, so a failed
// pad scan does not blank the mod list and a failed mod list does not hide the
// button art. What failed is said where it would have been drawn.
async function load() {
  const [lib, opts, cur] = await Promise.allSettled([
    call("mod_packages"),
    call("controls_options", { runtime: null }),
    call("controls_current"),
  ]);
  library = lib.status === "fulfilled" ? lib.value : null;
  if (lib.status === "rejected") problem = `The mod library could not be read: ${reason(lib.reason)}`;
  options = opts.status === "fulfilled" ? opts.value : { error: reason(opts.reason) };
  current = cur.status === "fulfilled" ? cur.value : null;
  if (cur.status === "rejected") notice = `Which button art is in force could not be read: ${reason(cur.reason)}`;
}

// One action, then a fresh read, then a redraw. The result of every action is
// a sync view or carries one, and that is what "what the game loads" shows.
async function act(cmd, args) {
  if (busy) return;
  busy = true;
  problem = null;
  notice = null;
  if (window.setLauncherBusy) window.setLauncherBusy(true); else draw();
  try {
    const r = await call(cmd, args);
    if (cmd === "controls_set") {
      if (r && r.synced) lastSync = r.synced;
      else if (r) notice = r.sync_note;
    } else {
      lastSync = r;
    }
    await load();
  } catch (e) {
    problem = reason(e);
  }
  busy = false;
  window.setLauncherBusy?.(false);
  draw();
}

async function refresh() {
  problem = null;
  notice = null;
  try {
    await load();
  } catch (e) {
    problem = reason(e);
  }
  draw();
}

// ---------------------------------------------------------------------------
// Building the screen
// ---------------------------------------------------------------------------

function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v === null || v === undefined) continue;
    if (k === "class") node.className = v;
    else if (k === "text") node.textContent = v;
    else if (k === "onclick") node.addEventListener("click", v);
    else if (k === "onchange") node.addEventListener("change", v);
    else if (k === "checked" || k === "disabled") node[k] = !!v;
    else node.setAttribute(k, v);
  }
  for (const c of children) {
    if (c !== null && c !== undefined) node.append(c);
  }
  return node;
}

function mb(bytes) {
  if (typeof bytes !== "number") return null;
  return `${(bytes / 1048576).toFixed(1)} MB`;
}

function packages() {
  return library && Array.isArray(library.packages) ? library.packages : [];
}

function managed(id) {
  const list = library && Array.isArray(library.managed_by_controls) ? library.managed_by_controls : [];
  return list.includes(id);
}

// One radio row. `checked` is the fact from the core; `pick` is what to do.
function choice(name, what, why, checked, pick) {
  const input = el("input", { type: "radio", name, value: what, checked, disabled: busy, onchange: pick });
  return el(
    "label",
    { class: "choice" + (checked ? " selected" : "") },
    input,
    el("span", { class: "what", text: what }),
    why ? el("span", { class: "why", text: why }) : null,
  );
}

function controlsSection() {
  const sec = el("section", { class: "block" }, el("h2", { text: "Button textures" }));

  if (current && current.profile) {
    const from = current.source === "settings" ? "settings" : "first run";
    sec.append(el("p", { text: `Current: ${current.label}, from ${from}.` }));
  } else {
    sec.append(
      el("p", {
        class: "absent",
        text: "Using the original GameCube button textures.",
      }),
    );
  }
  sec.append(
    el("p", {
      class: "note",
      text:
        "Choose the artwork that matches your controller. Changes take effect when you next start the game.",
    }),
  );

  if (!options || options.error) {
    sec.append(
      el("p", {
        class: "partial",
        text: `The profile list could not be read: ${options ? options.error : "not loaded"}`,
      }),
    );
    return sec;
  }
  const list = Array.isArray(options) ? options : [];
  for (const o of list) {
    const isCurrent = !!(current && current.profile === o.slug);
    const what = o.label;
    const why = `${o.label} button artwork for the game.`;
    const option=choice("controls", what, why, isCurrent, () => act("controls_set", { profile: o.slug, runtime: runtimeHint() }));
    if(o.suggested)option.append(el("span",{class:"recommendation-badge",text:"Recommended"}));
    sec.append(option);
  }
  return sec;
}

// The pick-one groups. Returns the sections and every id they covered, so the
// "other mods" list can leave those out.
function groupSections() {
  const sections = [];
  const covered = new Set();
  const groups = library && Array.isArray(library.exclusive) ? library.exclusive : [];

  for (const g of groups) {
    const ids = Array.isArray(g.members) ? g.members : [];
    ids.forEach((id) => covered.add(id));
    // The controls packs are their own exclusive pair and they are drawn as
    // the button art setting above. Offering them here too would let a mod
    // toggle fight the setting.
    const shown = ids.filter((id) => !managed(id));
    if (shown.length === 0) continue;

    const entries = shown.map((id) => packages().find((p) => p.id === id)).filter(Boolean);
    const missing = shown.filter((id) => !packages().some((p) => p.id === id));
    const active = entries.find((p) => p.active) || null;

    const sec = el("section", { class: "block" }, el("h2", { text: "Choose a variant" }));
    sec.append(
      el("p", {
        class: "note",
        text: "These mods change the same part of the game. Select one to use.",
      }),
    );
    sec.append(
      choice("group-" + sections.length, "None", "The game's own texture.", !active, () => {
        if (active) act("mods_toggle", { id: active.id, version: active.version, on: false, runtime: runtimeHint() });
      }),
    );
    for (const p of entries) {
      const size = mb(p.bytes);
      const why = [p.version, `${p.files} ${p.files === 1 ? "file" : "files"}`, size].filter(Boolean).join(", ");
      sec.append(
        choice("group-" + sections.length, p.name || p.id, why, !!p.active, () => {
          if (!p.active) act("mods_toggle", { id: p.id, version: p.version, on: true, runtime: runtimeHint() });
        }),
      );
    }
    for (const id of missing) {
      sec.append(el("p", { class: "partial", text: `${id} is in this group but could not be read.` }));
    }
    const grid = el("div", { class: "mod-options" });
    // Move only choices; warnings and the explanation remain full width.
    if (typeof sec.querySelectorAll === "function") {
      for (const choice of sec.querySelectorAll(":scope > .choice")) grid.append(choice);
      sec.append(grid);
    }
    sections.push(sec);
  }
  return { sections, covered };
}

function otherSection(covered) {
  const rest = packages().filter((p) => !covered.has(p.id) && !managed(p.id));
  if (rest.length === 0) return null;
  const sec = el("section", { class: "block" }, el("h2", { text: "Other mods" }));
  sec.append(el("p", { class: "note", text: "Enable the texture packs you want to use." }));
  for (const p of rest) {
    const input = el("input", {
      type: "checkbox",
      checked: !!p.active,
      disabled: busy,
      onchange: () => act("mods_toggle", { id: p.id, version: p.version, on: !p.active, runtime: runtimeHint() }),
    });
    const why = [p.version, `${p.files} ${p.files === 1 ? "file" : "files"}`, mb(p.bytes)].filter(Boolean).join(", ");
    sec.append(
      el(
        "label",
        { class: "choice" + (p.active ? " selected" : "") },
        input,
        el("span", { class: "what", text: p.name || p.id }),
        el("span", { class: "why", text: why }),
      ),
    );
  }
  return sec;
}

// Listed as on in activation.json, with no installed package behind it. Drawn
// rather than dropped: a mod that silently vanishes from the list is
// indistinguishable from one that was never turned on.
function staleSection() {
  const listed = library && Array.isArray(library.activation) ? library.activation : [];
  const stale = listed.filter((a) => !packages().some((p) => p.id === a.id && p.version === a.version));
  if (stale.length === 0) return null;
  const sec = el("section", { class: "block" }, el("h2", { text: "Missing mod files" }));
  sec.append(
    el("p", {
      class: "note",
      text:
        "The launcher leaves these on. Reinstalling the package brings it back " +
        "without another click; turning it off here is the other way out.",
    }),
  );
  for (const a of stale) {
    sec.append(
      el(
        "div",
        { class: "mod" },
        el("span", { class: "what", text: `${a.id}@${a.version}` }),
        el("button", {
          type: "button",
          text: "Turn off",
          disabled: busy,
          onclick: () => act("mods_toggle", { id: a.id, version: a.version, on: false, runtime: runtimeHint() }),
        }),
      ),
    );
  }
  return sec;
}

function unreadableSection() {
  const list = library && Array.isArray(library.unreadable) ? library.unreadable : [];
  if (list.length === 0) return null;
  const sec = el("section", { class: "block" }, el("h2", { text: "Unavailable mods" }));
  for (const why of list) sec.append(el("p", { class: "partial", text: String(why) }));
  return sec;
}

function fact(name, value) {
  const cell = value === null || value === undefined
    ? el("td", { class: "absent", text: "not recorded" })
    : el("td", { text: String(value) });
  return el("tr", {}, el("th", { text: name }), cell);
}

function appliedSection() {
  const sec = el("section", { class: "block" }, el("h2", { text: "Applied mods" }));
  if (lastSync) {
    sec.append(el("p", { class: lastSync.synced ? "" : "refusal", text: lastSync.note || "" }));
    if (lastSync.synced) {
      const table = el("table", { class: "facts" });
      table.append(fact("Tree", lastSync.tree === "rebuilt" ? `rebuilt, ${lastSync.entries} entries` : "already in place"));
      table.append(fact("Linked at", lastSync.link_at));
      if (lastSync.moved_aside) {
        table.append(fact("Folder that was there", `${lastSync.moved_aside} (nothing was deleted; rename it back to undo)`));
      }
      const missing = Array.isArray(lastSync.missing) ? lastSync.missing : [];
      if (missing.length > 0) {
        table.append(fact("Listed, not installed", missing.map((m) => m.id).join(", ")));
      }
      sec.append(table);
    }
  } else {
    sec.append(
      el("p", {
        class: "absent",
        text:
          "Not checked yet. Sync makes the game load the list above. " +
          "A launch does this by itself; the button is for seeing it now.",
      }),
    );
  }
  sec.append(el("button", { type: "button", class: "primary", text: busy ? "Working" : "Sync now", disabled: busy, onclick: () => act("mods_sync", { runtime: runtimeHint() }) }));
  return sec;
}

// Draw everything from module state. Never talks to the core, so a harness can
// feed it any state and see what it does with it.
function draw() {
  const screen = document.getElementById("screen");
  const parts = [];

  if (problem) parts.push(el("p", { class: "refusal", text: problem }));
  if (notice) parts.push(el("p", { class: "partial", text: notice }));



  if (!library) {
    parts.push(
      el("p", {
        class: problem ? "absent" : "note",
        text: problem ? "The mod list is not shown because the library could not be read." : "Reading the library.",
      }),
    );
  } else {
    const { sections, covered } = groupSections();
    parts.push(...sections);
    parts.push(otherSection(covered));
    parts.push(staleSection());
    parts.push(unreadableSection());
    if (packages().length === 0 && sections.length === 0) {
      parts.push(
        el("p", {
          class: "absent",
          text: `No packages are installed under ${library.packages_root || "the packages folder"}. Resume game setup to import your existing mods.`,
        }),
      );
    }
  }

  parts.push(el("a", { href: "onboarding.html", class: "button", text: "Import from an existing installation" }));
  parts.push(el("details", { class: "block" }, el("summary", { text: "Button prompts" }), controlsSection()));
  parts.push(el("details", { class: "block" }, el("summary", { text: "Mod synchronization details" }), appliedSection()));
  screen.replaceChildren(...parts.filter(Boolean));
  return parts.filter(Boolean);
}

// ---------------------------------------------------------------------------
// Boot
// ---------------------------------------------------------------------------

const clearButton = document.getElementById("clear");
if (clearButton) {
  clearButton.addEventListener("click", () => {
    document.getElementById("out").textContent = "";
  });
}

// A harness fakes document without querySelectorAll; the nav is not its concern.
for (const a of typeof document.querySelectorAll === "function" ? document.querySelectorAll(".shell-nav a") : []) {
  if ((a.getAttribute("href") || "").toLowerCase() === "mods.html") a.classList.add("current");
}

if (bridge) {
  refresh();
} else {
  problem = "The launcher core is not attached. Open this page inside the launcher window.";
  draw();
}
