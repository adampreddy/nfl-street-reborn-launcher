// The Music page: 21 song slots on the disc, staged changes, one Apply.
// The core owns the disc; this page draws music_status and asks for changes.
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
    else if (k === "oninput") node.addEventListener("input", v);
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

function working() {
  return !!(status && (status.phase === "encoding" || status.phase === "building"));
}

const colors = ["#514941", "#3f4c49", "#484357", "#49505b", "#59464a", "#514f3e"];

const SOURCE_LABEL = {
  original: "Original song",
  your_file: "Your song",
  pack: "From a pack",
  soundtrack_mod: "Soundtrack mod",
};

function slotTitle(s) {
  if (s.source === "original" || !s.title) return `Original song ${s.slot}`;
  return s.title;
}

function slotCard(s, index) {
  const card = el("article", { class: "store-card music-card" });
  const open = el("button", { type: "button", class: "store-open", "aria-label": `Slot ${s.slot}: ${slotTitle(s)}`, onclick: () => detailsDialog(s) });
  const art = el("span", { class: "store-art", "aria-hidden": "true" });
  art.style.backgroundColor = colors[index % colors.length];
  art.textContent = String(s.slot);
  const line = [s.source === "original" ? null : s.artist, mmss(s.duration_ms), SOURCE_LABEL[s.source] || s.source].filter(Boolean).join(" · ");
  const body = el("span", { class: "store-card-body" }, el("strong", { text: slotTitle(s) }), el("small", { text: line || SOURCE_LABEL.original }));
  if (s.pending) {
    const what = s.pending.kind === "original" ? "the original song" : [s.pending.title, s.pending.artist].filter(Boolean).join(" by ") || "a song";
    body.append(el("small", { class: "music-staged", text: `Staged: ${what}` }));
  }
  open.append(art, body);
  const actions = el("div", { class: "store-card-actions" });
  actions.append(el("span", { class: s.pending ? "store-state active" : "store-state", text: s.pending ? "Staged" : SOURCE_LABEL[s.source] || "" }));
  const disabled = busy || working();
  actions.append(el("button", { type: "button", text: "Replace", "aria-label": `Replace slot ${s.slot}`, disabled, onclick: () => addDialog(s.slot) }));
  if (s.pending) {
    actions.append(el("button", { type: "button", text: "Undo", "aria-label": `Undo staged change for slot ${s.slot}`, disabled, onclick: () => act("music_unstage", { slot: s.slot }) }));
  } else if (s.source !== "original") {
    actions.append(el("button", { type: "button", text: "Reset", "aria-label": `Reset slot ${s.slot} to the original song`, disabled, onclick: () => act("music_stage_stock", { slot: s.slot }) }));
  }
  card.append(open, actions);
  return card;
}

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

function field(labelText, input) {
  const label = el("label", { class: "music-field" }, el("span", { text: labelText }), input);
  return label;
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

// Add or replace: pick a file, read its details, fix them, stage.
function addDialog(preferredSlot) {
  const dialog = openDialog("Add a song");
  const body = el("div", { class: "mod-dialog-body" });
  const firstFree = slots().find((s) => s.source === "original" && !s.pending);
  let slot = preferredSlot || (firstFree ? firstFree.slot : 21);
  let picked = null; // the InspectView
  const status_line = el("p", { class: "note", text: "Pick an MP3, M4A, FLAC, OGG, WAV, or AIFF file. Songs up to about 6:20 fit a slot." });
  const chosen = el("p", { class: "note music-file", text: "" });
  const title = el("input", { type: "text", maxlength: "49", placeholder: "Title" });
  const artist = el("input", { type: "text", maxlength: "49", placeholder: "Artist" });
  const album = el("input", { type: "text", maxlength: "49", placeholder: "Album" });
  const from = el("p", { class: "note", text: "" });
  const candidates = el("ul", { class: "music-candidates" });
  const search = el("input", { type: "search", placeholder: "Not right? Search by song or artist" });
  const searchButton = el("button", { type: "button", text: "Search", onclick: () => runSearch() });
  const slotSelect = el("select", { "aria-label": "Slot" });
  for (const s of slots()) {
    slotSelect.append(el("option", { value: String(s.slot), text: `Slot ${s.slot}: ${slotTitle(s)}`, selected: s.slot === slot }));
  }
  slotSelect.addEventListener("change", () => { slot = Number(slotSelect.value); addButton.textContent = `Add to slot ${slot}`; });
  const addButton = el("button", { type: "button", class: "primary", text: `Add to slot ${slot}`, disabled: true, onclick: async () => {
    if (!picked) return;
    dialog.close();
    await act("music_stage_file", { path: picked.path, slot, title: title.value.trim(), artist: artist.value.trim(), album: album.value.trim() });
  } });
  const fillFrom = (c) => { title.value = c.title || ""; artist.value = c.artist || ""; album.value = c.album || ""; };
  const showCandidates = (list) => {
    candidates.replaceChildren(...list.map((c) => el("li", {}, el("button", { type: "button", class: "music-candidate", text: [c.title, c.artist, c.album].filter(Boolean).join(" · ") + (c.source ? `  (${c.source})` : ""), onclick: () => fillFrom(c) }))));
  };
  const runSearch = async () => {
    const q = search.value.trim();
    if (!q) return;
    from.textContent = "Searching…";
    try {
      const list = await call("music_lookup", { query: q });
      from.textContent = list.length ? "Pick the right one, or edit the fields." : "Nothing found online. Type the details.";
      showCandidates(list);
    } catch (e) {
      from.textContent = `The search did not work: ${reason(e)}`;
    }
  };
  search.addEventListener("keydown", (ev) => { if (ev.key === "Enter") { ev.preventDefault(); runSearch(); } });
  const choose = el("button", { type: "button", text: "Choose a file", onclick: async () => {
    const path = await pickPath("audio");
    if (!path) return;
    chosen.textContent = path;
    from.textContent = "Reading the file…";
    addButton.disabled = true;
    try {
      picked = await call("music_inspect", { path });
      fillFrom(picked);
      const len = mmss(picked.duration_ms);
      from.textContent = `${picked.note} Length ${len}.` + (picked.too_long ? ` This song is too long for a slot (up to ${mmss((status?.limit_seconds || 380) * 1000)}). Trim it in an audio editor and try again.` : "");
      showCandidates(picked.candidates || []);
      if (!search.value) search.value = picked.query || "";
      addButton.disabled = !!picked.too_long;
    } catch (e) {
      picked = null;
      from.textContent = reason(e);
    }
  } });
  body.append(
    el("span", { class: "store-kicker", text: "Add a song" }),
    el("h2", { text: preferredSlot ? `Replace slot ${preferredSlot}` : "Add a song" }),
    status_line,
    el("div", { class: "music-row" }, choose, chosen),
    field("Slot", slotSelect),
    field("Title", title),
    field("Artist", artist),
    field("Album", album),
    from,
    el("div", { class: "music-row" }, search, searchButton),
    candidates,
    el("p", { class: "note", text: "Searching sends the song name to iTunes or MusicBrainz to find its details. Nothing else leaves this computer." }),
    el("div", { class: "store-dialog-actions" }, el("button", { type: "button", text: "Cancel", onclick: () => dialog.close() }), addButton),
  );
  dialog.append(body);
  showDialog(dialog);
}

// Banner text only.
function detailsDialog(s) {
  const dialog = openDialog(`Slot ${s.slot}`);
  const body = el("div", { class: "mod-dialog-body" });
  const src = s.pending || s;
  const title = el("input", { type: "text", maxlength: "49", value: src.title || "", placeholder: "Title" });
  const artist = el("input", { type: "text", maxlength: "49", value: src.artist || "", placeholder: "Artist" });
  const album = el("input", { type: "text", maxlength: "49", value: src.album || "", placeholder: "Album" });
  const line = [SOURCE_LABEL[s.source] || s.source, s.source_name, mmss(s.duration_ms)].filter(Boolean).join(" · ");
  body.append(
    el("span", { class: "store-kicker", text: `Slot ${s.slot}` }),
    el("h2", { text: slotTitle(s) }),
    el("p", { class: "note", text: line }),
    el("p", { text: "The title, artist, and album show on the in-game banner when the song starts. Up to 49 characters each." }),
    field("Title", title),
    field("Artist", artist),
    field("Album", album),
    el("div", { class: "store-dialog-actions" },
      el("button", { type: "button", text: "Close", onclick: () => dialog.close() }),
      el("button", { type: "button", text: "Replace song", onclick: () => { dialog.close(); addDialog(s.slot); } }),
      el("button", { type: "button", class: "primary", text: "Save details", disabled: busy || working(), onclick: async () => { dialog.close(); await act("music_stage_details", { slot: s.slot, title: title.value.trim(), artist: artist.value.trim(), album: album.value.trim() }); } }),
    ),
  );
  dialog.append(body);
  showDialog(dialog);
}

// Import a pack: preview what each slot becomes, then stage.
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
      el("button", { type: "button", class: "primary", text: "Stage this pack", disabled: busy || working(), onclick: async () => { dialog.close(); await act("music_stage_pack", { path }); } }),
    ),
  );
  dialog.append(body);
  showDialog(dialog);
}

// Share: pick slots, name it, save a .nsrmusic.
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
    field("Name", name),
    field("Author", author),
    field("Description", description),
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

function headerBlock() {
  const st = status;
  const block = el("section", { class: "store-featured music-head" });
  const on = !!(st && st.enabled);
  const baseName = st && st.base && st.base.kind === "package" ? st.base.name : null;
  block.append(
    el("span", { class: "store-kicker", text: "Custom soundtrack" }),
    el("h2", { text: on ? (baseName ? `On, based on ${baseName}` : "On") : "Off" }),
    el("p", { text: on ? "Your songs play in the game. Turn it off to go back to the original soundtrack; your songs are kept for when you turn it on again." : "The game plays its original soundtrack. Turn it on to put your songs back on the disc." }),
  );
  const row = el("div", { class: "music-row" });
  row.append(el("button", { type: "button", class: on ? "" : "primary", text: on ? "Turn off" : "Turn on", disabled: busy || working() || !(st && st.image_ok), onclick: () => act("music_set_enabled", { on: !on }) }));
  if (st && !st.image_ok) row.append(el("span", { class: "note", text: st.image_note || "" }));
  block.append(row);
  return block;
}

function progressBlock() {
  const st = status;
  if (!st) return null;
  if (working()) {
    const p = st.progress || {};
    const pct = p.total ? Math.min(100, Math.round((p.done / p.total) * 100)) : 0;
    const text = st.phase === "encoding" ? `${p.label || "Encoding"}: segment ${Math.min((p.done || 0) + 1, p.total || 40)} of ${p.total || 40}` : `${p.label || "Building your game disc"}: ${pct}%`;
    return el("p", { class: "partial music-progress", role: "status", text });
  }
  if (st.phase === "done" || st.phase === "problem") {
    const line = el("aside", { class: st.phase === "done" ? "music-done" : "refusal", role: "status" });
    line.append(el("span", { text: st.note || (st.phase === "done" ? "Done." : "Something went wrong.") }), el("button", { type: "button", text: "Got it", onclick: () => act("music_dismiss") }));
    return line;
  }
  return null;
}

function applyBar() {
  const st = status;
  if (!st || !st.pending_count) return null;
  const bar = el("div", { class: "music-apply", role: "status" });
  bar.append(
    el("span", { text: `${st.pending_count} change${st.pending_count === 1 ? "" : "s"} staged. Applying rebuilds your game disc once, about half a minute.` }),
    el("button", { type: "button", text: "Discard", disabled: busy || working(), onclick: () => act("music_unstage", { slot: null }) }),
    el("button", { type: "button", class: "primary", text: "Apply changes", disabled: busy || working() || !st.image_ok, onclick: () => act("music_apply", { confirm: true }) }),
  );
  return bar;
}

function draw() {
  const screen = document.getElementById("screen");
  const parts = [];
  if (problem) parts.push(el("p", { class: "refusal", role: "alert", text: problem }));
  if (notice) parts.push(el("p", { class: "partial", role: "status", text: notice }));
  const progress = progressBlock();
  if (progress) parts.push(progress);
  parts.push(headerBlock());
  const bar = applyBar();
  if (bar) parts.push(bar);
  const toolbar = el("div", { class: "store-toolbar music-toolbar" });
  const disabled = busy || working();
  toolbar.append(
    el("button", { type: "button", class: "primary", text: "Add a song", disabled, onclick: () => addDialog(null) }),
    el("button", { type: "button", text: "Import a pack", disabled, onclick: packDialog }),
    el("button", { type: "button", text: "Share a pack", disabled, onclick: shareDialog }),
    el("button", { type: "button", text: "Refresh", disabled: busy, onclick: refresh }),
  );
  parts.push(toolbar);
  const grid = el("section", { class: "store-grid music-grid", "aria-label": "Songs" });
  const list = slots();
  if (list.length) grid.append(...list.map((s, i) => slotCard(s, i)));
  else grid.append(el("div", { class: "store-empty" }, el("h2", { text: "No soundtrack to show" }), el("p", { text: problem ? "The record could not be read." : "The launcher has not read the disc yet." })));
  parts.push(grid);
  parts.push(el("p", { class: "note store-footnote", text: "Songs are written into your offline game disc when you apply. The online game always keeps the original soundtrack. Soundtrack mods such as the NFL Street 1 soundtrack live on the Mods page." }));
  screen.replaceChildren(...parts);
  return parts;
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
  refresh();
} else {
  problem = "The launcher core is not attached. Open this page inside the launcher window.";
  draw();
}
