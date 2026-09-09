// The Music page: a 21-row tracklist of the disc's songs on the left, an
// inspector for the selected slot on the right, one Apply. The core owns the
// disc; this page draws music_status and asks for changes.
const bridge = window.__TAURI__ ? window.__TAURI__.core : null;

// What the last read returned. Null until read, and null again if a read
// failed, so a stale list is never drawn as current.
let status = null; // music_status
// One action at a time. A second click while the first is staging would
// race it on the same record.
let busy = false;
// A failure, in the core's words, drawn at the top. Cleared by the next action.
let problem = null;
// A partial answer, not a failure.
let notice = null;
// Set while the core reports encoding or building; the page polls.
let pollTimer = null;
// The slot the inspector shows, 1..21.
let selected = 1;
// The inspector's working copy: the file chosen for the selected slot (an
// InspectView), the typed fields, and the lookup candidates. Reset when the
// selection changes.
let picked = null;
let fields = { title: "", artist: "", album: "" };
let candidates = [];
let lookupNote = "";
let searchText = "";

// ---------------------------------------------------------------------------
// Talking to the core
// ---------------------------------------------------------------------------

function log(what, value) {
  const pre = document.getElementById("out");
  if (!pre) return;
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

async function load() {
  try {
    status = await call("music_status");
  } catch (e) {
    status = null;
    problem = `Your soundtrack could not be read: ${reason(e)}`;
  }
}

// One action, then a fresh read, then a redraw. Every staging action answers
// with the new status, so the read is the answer itself.
async function act(cmd, args) {
  if (busy) return;
  busy = true;
  problem = null;
  notice = null;
  draw();
  try {
    const r = await call(cmd, args);
    if (r && typeof r === "object" && Array.isArray(r.slots)) status = r;
    else await load();
  } catch (e) {
    problem = reason(e);
  }
  busy = false;
  draw();
  schedulePoll();
}

async function refresh() {
  problem = null;
  notice = null;
  await load();
  draw();
  schedulePoll();
}

// While the core encodes or builds, ask again every second.
function schedulePoll() {
  const working = status && (status.phase === "encoding" || status.phase === "building");
  if (pollTimer) {
    clearTimeout(pollTimer);
    pollTimer = null;
  }
  if (!working) return;
  pollTimer = setTimeout(async () => {
    pollTimer = null;
    await load();
    draw();
    schedulePoll();
  }, 1000);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v === null || v === undefined) continue;
    if (k === "class") node.className = v;
    else if (k === "text") node.textContent = v;
    else if (k === "onclick") node.addEventListener("click", v);
    else if (k === "onchange") node.addEventListener("change", v);
    else if (k === "oninput") node.addEventListener("input", v);
    else if (k === "onkeydown") node.addEventListener("keydown", v);
    else if (k === "checked" || k === "disabled" || k === "selected") node[k] = !!v;
    else if (k === "value") node.value = v;
    else node.setAttribute(k, v);
  }
  for (const c of children) {
    if (c !== null && c !== undefined) node.append(c);
  }
  return node;
}

function mmss(ms) {
  if (typeof ms !== "number") return null;
  const s = Math.round(ms / 1000);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

function slots() {
  return status && Array.isArray(status.slots) ? status.slots : [];
}

function slotAt(n) {
  return slots().find((s) => s.slot === n) || null;
}

function working() {
  return !!(status && (status.phase === "encoding" || status.phase === "building"));
}

function locked() {
  return busy || working();
}

const SOURCE_LABEL = {
  original: "Original",
  your_file: "Your song",
  pack: "Pack",
  soundtrack_mod: "Soundtrack mod",
};

function slotTitle(s) {
  if (!s || s.source === "original" || !s.title) return s ? `Original song ${s.slot}` : "";
  return s.title;
}

function firstFreeSlot() {
  const free = slots().find((s) => s.source === "original" && !s.pending);
  return free ? free.slot : 21;
}

// Selecting a slot resets the inspector's working copy to what that slot has.
function select(n) {
  selected = n;
  picked = null;
  candidates = [];
  lookupNote = "";
  searchText = "";
  const s = slotAt(n);
  const src = s && s.pending ? s.pending : s;
  fields = { title: (src && src.title) || "", artist: (src && src.artist) || "", album: (src && src.album) || "" };
  draw();
}

async function pickPath(kind) {
  try {
    return await call("pick_path", { kind });
  } catch (e) {
    problem = reason(e);
    draw();
    return null;
  }
}

// Choose a file for the selected slot: read its details, fill the fields.
async function chooseFile() {
  const path = await pickPath("audio");
  if (!path) return;
  lookupNote = "Reading the file…";
  draw();
  try {
    picked = await call("music_inspect", { path });
    fields = { title: picked.title || "", artist: picked.artist || "", album: picked.album || "" };
    candidates = picked.candidates || [];
    searchText = picked.query || "";
    lookupNote = `${picked.note} Length ${mmss(picked.duration_ms)}.`;
    if (picked.too_long) lookupNote += ` This song is too long for a slot (up to ${mmss((status && status.limit_seconds ? status.limit_seconds : 380) * 1000)}). Trim it in an audio editor and try again.`;
  } catch (e) {
    picked = null;
    lookupNote = reason(e);
  }
  draw();
}

async function runSearch() {
  const q = searchText.trim();
  if (!q) return;
  lookupNote = "Searching…";
  draw();
  try {
    candidates = await call("music_lookup", { query: q });
    lookupNote = candidates.length ? "Pick the right one, or edit the fields." : "Nothing found online. Type the details.";
  } catch (e) {
    lookupNote = `The search did not work: ${reason(e)}`;
  }
  draw();
}

function stageSelected() {
  const n = selected;
  const title = fields.title.trim();
  const artist = fields.artist.trim();
  const album = fields.album.trim();
  if (picked) {
    const path = picked.path;
    picked = null;
    candidates = [];
    lookupNote = "";
    return act("music_stage_file", { path, slot: n, title, artist, album });
  }
  return act("music_stage_details", { slot: n, title, artist, album });
}

// ---------------------------------------------------------------------------
// Building the screen
// ---------------------------------------------------------------------------

function topBar() {
  const st = status;
  const on = !!(st && st.enabled);
  const baseName = st && st.base && st.base.kind === "package" ? st.base.name : null;
  const bar = el("section", { class: "music-top" });
  const left = el("div", { class: "music-top-left" });
  left.append(
    el("span", { class: "store-kicker", text: "Custom soundtrack" }),
    el("div", { class: "music-switch" },
      el("strong", { text: on ? "On" : "Off" }),
      el("button", { type: "button", text: on ? "Turn off" : "Turn on", disabled: locked() || !(st && st.image_ok), onclick: () => act("music_set_enabled", { on: !on }) }),
    ),
    el("p", { class: "note", text: on ? (baseName ? `Based on ${baseName}. Your own songs sit on top.` : "Your songs play in the game. Turning it off puts the original soundtrack back and keeps your songs for later.") : "The game plays its original soundtrack. Turn it on to put your songs back on the disc." }),
  );
  if (st && !st.image_ok) left.append(el("p", { class: "refusal", text: st.image_note || "" }));
  bar.append(left);
  const right = el("div", { class: "music-top-right" });
  if (st && st.pending_count) {
    right.append(
      el("span", { class: "music-staged-count", text: `${st.pending_count} change${st.pending_count === 1 ? "" : "s"} staged` }),
      el("button", { type: "button", text: "Discard", disabled: locked(), onclick: () => act("music_unstage", { slot: null }) }),
      el("button", { type: "button", class: "primary", text: "Apply changes", disabled: locked() || !st.image_ok, onclick: () => act("music_apply", { confirm: true }) }),
    );
  } else {
    right.append(el("span", { class: "note", text: "Nothing staged. Changes are written to the disc when you apply." }));
  }
  bar.append(right);
  return bar;
}

function statusLine() {
  const st = status;
  if (!st) return null;
  if (working()) {
    const p = st.progress || {};
    const pct = p.total ? Math.min(100, Math.round((p.done / p.total) * 100)) : 0;
    const text = st.phase === "encoding" ? `${p.label || "Encoding"}: segment ${Math.min((p.done || 0) + 1, p.total || 40)} of ${p.total || 40}` : `${p.label || "Building your game disc"}: ${pct}%. Applying rebuilds the disc once, about half a minute.`;
    return el("p", { class: "partial music-progress", role: "status", text });
  }
  if (st.phase === "done" || st.phase === "problem") {
    const line = el("aside", { class: st.phase === "done" ? "music-done" : "refusal music-done", role: "status" });
    line.append(el("span", { text: st.note || (st.phase === "done" ? "Done." : "Something went wrong.") }), el("button", { type: "button", text: "Got it", onclick: () => act("music_dismiss") }));
    return line;
  }
  return null;
}

function trackRow(s) {
  const isSelected = s.slot === selected;
  const row = el("tr", { class: `music-track${isSelected ? " selected" : ""}${s.pending ? " staged" : ""}`, "data-slot": String(s.slot), onclick: () => select(s.slot), "aria-selected": String(isSelected), tabindex: "0", onkeydown: (ev) => { if (ev.key === "Enter" || ev.key === " ") { ev.preventDefault(); select(s.slot); } } });
  const pendingWhat = s.pending ? (s.pending.kind === "original" ? "the original song" : [s.pending.title, s.pending.artist].filter(Boolean).join(" by ") || "a song") : null;
  row.append(
    el("td", { class: "music-col-n", text: String(s.slot) }),
    el("td", { class: "music-col-title" }, el("span", { text: slotTitle(s) }), s.pending ? el("small", { class: "music-staged", text: `Staged: ${pendingWhat}` }) : null),
    el("td", { class: "music-col-artist", text: s.source === "original" ? "" : s.artist || "" }),
    el("td", { class: "music-col-len", text: mmss(s.duration_ms) || "" }),
    el("td", { class: "music-col-source" }, el("span", { class: `music-chip music-chip-${s.source}`, text: SOURCE_LABEL[s.source] || s.source || "" })),
  );
  return row;
}

function trackList() {
  const pane = el("section", { class: "music-list", "aria-label": "Songs" });
  const list = slots();
  if (!list.length) {
    pane.append(el("div", { class: "store-empty" }, el("h2", { text: "No soundtrack to show" }), el("p", { text: problem ? "The record could not be read." : "The launcher has not read the disc yet." })));
    return pane;
  }
  const table = el("table", { class: "music-table" });
  const head = el("thead", {}, el("tr", {}, el("th", { class: "music-col-n", text: "#" }), el("th", { class: "music-col-title", text: "Title" }), el("th", { class: "music-col-artist", text: "Artist" }), el("th", { class: "music-col-len", text: "Length" }), el("th", { class: "music-col-source", text: "Source" })));
  const body = el("tbody");
  for (const s of list) body.append(trackRow(s));
  table.append(head, body);
  pane.append(table);
  const actions = el("div", { class: "music-list-actions" });
  actions.append(
    el("button", { type: "button", class: "primary", text: "Add a song", disabled: locked(), onclick: async () => { select(firstFreeSlot()); await chooseFile(); } }),
    el("button", { type: "button", text: "Import a pack", disabled: locked(), onclick: packDialog }),
    el("button", { type: "button", text: "Share a pack", disabled: locked(), onclick: shareDialog }),
    el("button", { type: "button", text: "Refresh", disabled: busy, onclick: refresh }),
  );
  pane.append(actions);
  return pane;
}

function inspector() {
  const s = slotAt(selected);
  const pane = el("section", { class: "music-inspector", "aria-label": `Slot ${selected}` });
  if (!s) {
    pane.append(el("p", { class: "note", text: "Pick a song on the left." }));
    return pane;
  }
  const line = [s.source === "original" ? "The song the disc shipped with" : [s.artist, mmss(s.duration_ms)].filter(Boolean).join(" · "), SOURCE_LABEL[s.source] && s.source !== "original" ? `${SOURCE_LABEL[s.source]}${s.source_name ? ` (${s.source_name})` : ""}` : null].filter(Boolean).join(" · ");
  pane.append(
    el("span", { class: "store-kicker", text: `Slot ${s.slot}` }),
    el("h2", { text: picked ? (fields.title || "New song") : slotTitle(s) }),
    el("p", { class: "note", text: picked ? `Replacing ${slotTitle(s)} with ${picked.path.split(/[\\/]/).pop()}` : line }),
  );
  if (s.pending && !picked) {
    const what = s.pending.kind === "original" ? "the original song" : [s.pending.title, s.pending.artist].filter(Boolean).join(" by ") || "a song";
    pane.append(el("p", { class: "partial", text: `Staged: ${what}. Apply changes to put it on the disc.` }, el("button", { type: "button", text: "Undo", disabled: locked(), onclick: () => act("music_unstage", { slot: s.slot }) })));
  }
  const title = el("input", { type: "text", maxlength: "49", value: fields.title, placeholder: "Title", oninput: (ev) => { fields.title = ev.target.value; } });
  const artist = el("input", { type: "text", maxlength: "49", value: fields.artist, placeholder: "Artist", oninput: (ev) => { fields.artist = ev.target.value; } });
  const album = el("input", { type: "text", maxlength: "49", value: fields.album, placeholder: "Album", oninput: (ev) => { fields.album = ev.target.value; } });
  pane.append(
    el("label", { class: "music-field" }, el("span", { text: "Title" }), title),
    el("label", { class: "music-field" }, el("span", { text: "Artist" }), artist),
    el("label", { class: "music-field" }, el("span", { text: "Album" }), album),
  );
  const search = el("input", { type: "search", value: searchText, placeholder: "Find details online: song or artist", oninput: (ev) => { searchText = ev.target.value; }, onkeydown: (ev) => { if (ev.key === "Enter") { ev.preventDefault(); runSearch(); } } });
  pane.append(el("div", { class: "music-row" }, search, el("button", { type: "button", text: "Search", onclick: runSearch })));
  if (lookupNote) pane.append(el("p", { class: "note", text: lookupNote }));
  if (candidates.length) {
    const list = el("ul", { class: "music-candidates" });
    for (const c of candidates) {
      list.append(el("li", {}, el("button", { type: "button", class: "music-candidate", text: [c.title, c.artist, c.album].filter(Boolean).join(" · ") + (c.source ? `  (${c.source})` : ""), onclick: () => { fields = { title: c.title || "", artist: c.artist || "", album: c.album || "" }; draw(); } })));
    }
    pane.append(list);
  }
  pane.append(el("p", { class: "note", text: "The title, artist, and album show on the in-game banner when the song starts. Searching sends the song name to iTunes or MusicBrainz; nothing else leaves this computer." }));
  const actions = el("div", { class: "music-inspector-actions" });
  actions.append(el("button", { type: "button", text: picked ? "Choose a different file" : "Choose a file", disabled: locked(), onclick: chooseFile }));
  if (s.source !== "original" && !picked) {
    actions.append(el("button", { type: "button", text: "Reset to original", "aria-label": `Reset slot ${s.slot} to the original song`, disabled: locked(), onclick: () => act("music_stage_stock", { slot: s.slot }) }));
  }
  if (picked) {
    actions.append(el("button", { type: "button", text: "Cancel", onclick: () => select(s.slot) }));
  }
  const saveLabel = picked ? `Add to slot ${s.slot}` : "Save details";
  actions.append(el("button", { type: "button", class: "primary", text: saveLabel, disabled: locked() || (picked && picked.too_long), onclick: stageSelected }));
  pane.append(actions);
  return pane;
}

function draw() {
  const screen = document.getElementById("screen");
  const parts = [];
  if (problem) parts.push(el("p", { class: "refusal", role: "alert", text: problem }));
  if (notice) parts.push(el("p", { class: "partial", role: "status", text: notice }));
  const line = statusLine();
  if (line) parts.push(line);
  parts.push(topBar());
  const panes = el("div", { class: "music-panes" });
  panes.append(trackList(), inspector());
  parts.push(panes);
  parts.push(el("p", { class: "note store-footnote", text: "Songs are written into your offline game disc when you apply. The online game always keeps the original soundtrack. Soundtrack mods such as the NFL Street 1 soundtrack live on the Mods page." }));
  screen.replaceChildren(...parts);
  return parts;
}

// ---------------------------------------------------------------------------
// Pack dialogs (pack-level operations keep a dialog)
// ---------------------------------------------------------------------------

function openDialog(label) {
  document.querySelector(".mod-dialog")?.remove();
  const dialog = el("dialog", { class: "mod-dialog music-dialog", "aria-label": label });
  dialog.addEventListener("close", () => dialog.remove());
  return dialog;
}

function showDialog(dialog) {
  document.body.append(dialog);
  if (typeof dialog.showModal === "function") dialog.showModal();
}

async function packDialog() {
  const path = await pickPath("pack");
  if (!path) return;
  let preview;
  try {
    preview = await call("music_pack_preview", { path });
  } catch (e) {
    problem = reason(e);
    draw();
    return;
  }
  const dialog = openDialog("Import a pack");
  const body = el("div", { class: "mod-dialog-body" });
  const rows = el("ul", { class: "music-preview" });
  for (const song of preview.songs || []) {
    rows.append(el("li", { text: `Slot ${song.slot}: ${[song.title, song.artist].filter(Boolean).join(" by ")} ${mmss(song.duration_ms) ? `(${mmss(song.duration_ms)})` : ""} replaces ${song.replaces}` }));
  }
  body.append(
    el("span", { class: "store-kicker", text: "Soundtrack pack" }),
    el("h2", { text: preview.name || "Pack" }),
    el("p", { class: "note", text: [preview.author ? `by ${preview.author}` : null, `${(preview.songs || []).length} song(s)`, preview.format === 1 ? "older pack format" : null].filter(Boolean).join(" · ") }),
    preview.description ? el("p", { text: preview.description }) : null,
    rows,
    el("div", { class: "store-dialog-actions" },
      el("button", { type: "button", text: "Cancel", onclick: () => dialog.close() }),
      el("button", { type: "button", class: "primary", text: "Stage this pack", disabled: locked(), onclick: async () => { dialog.close(); await act("music_stage_pack", { path }); } }),
    ),
  );
  dialog.append(body);
  showDialog(dialog);
}

function shareDialog() {
  const dialog = openDialog("Share a pack");
  const body = el("div", { class: "mod-dialog-body" });
  const boxes = [];
  const list = el("div", { class: "music-share-list" });
  for (const s of slots()) {
    const box = el("input", { type: "checkbox", checked: s.source !== "original" });
    boxes.push([s.slot, box]);
    list.append(el("label", { class: "music-share-row" }, box, el("span", { text: `Slot ${s.slot}: ${slotTitle(s)}${s.source !== "original" && s.artist ? ` by ${s.artist}` : ""}` })));
  }
  const name = el("input", { type: "text", placeholder: "Pack name" });
  const author = el("input", { type: "text", placeholder: "Your name" });
  const description = el("input", { type: "text", placeholder: "A line about it (optional)" });
  body.append(
    el("span", { class: "store-kicker", text: "Share" }),
    el("h2", { text: "Save a soundtrack pack" }),
    el("p", { text: "A pack carries the songs as the game plays them, so a friend can apply it without any converting. Songs that are staged but not applied are not included yet." }),
    list,
    el("label", { class: "music-field" }, el("span", { text: "Name" }), name),
    el("label", { class: "music-field" }, el("span", { text: "Author" }), author),
    el("label", { class: "music-field" }, el("span", { text: "Description" }), description),
    el("div", { class: "store-dialog-actions" },
      el("button", { type: "button", text: "Cancel", onclick: () => dialog.close() }),
      el("button", { type: "button", class: "primary", text: "Save pack", onclick: async () => {
        const chosen = boxes.filter(([, b]) => b.checked).map(([n]) => n);
        if (!chosen.length) { notice = "Pick at least one slot to share."; draw(); return; }
        const path = await pickPath("save_pack");
        if (!path) return;
        dialog.close();
        busy = true; problem = null; notice = null; draw();
        try {
          const saved = await call("music_export", { path, slots: chosen, name: name.value.trim(), author: author.value.trim(), description: description.value.trim() });
          notice = `Saved ${saved}.`;
        } catch (e) {
          problem = reason(e);
        }
        busy = false;
        await load();
        draw();
      } }),
    ),
  );
  dialog.append(body);
  showDialog(dialog);
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
  if ((a.getAttribute("href") || "").toLowerCase() === "music.html") a.classList.add("current");
}

if (bridge) {
  (async () => {
    await load();
    select(1);
    schedulePoll();
  })();
} else {
  problem = "The launcher core is not attached. Open this page inside the launcher window.";
  draw();
}
