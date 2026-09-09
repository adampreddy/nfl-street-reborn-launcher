// The Music page: the old editor's flow, one column, top to bottom.
//
//   Your disc right now        one sentence, and the switch
//   1. Choose a song file      a button; nothing else until a file is chosen
//   2. Check the details       title, artist, album, filled from the file or online
//   3. Choose where it goes    the 21 slots as a list, the first free one picked
//   Ready to apply             the queue of staged changes, one Apply button
//   Songs on your disc         what plays today, with Change and Reset per song
//
// The core owns the disc; this page draws music_status and asks for changes.
const bridge = window.__TAURI__ ? window.__TAURI__.core : null;

let status = null; // music_status; null until read or after a failed read
let busy = false; // one action at a time
let problem = null; // a failure, in the core's words, drawn at the top
let notice = null; // a partial answer, not a failure
let pollTimer = null; // set while the core encodes or builds

// The song being added: the InspectView for the chosen file, the fields as
// typed, the lookup candidates, and the slot picked in step 3. `editing` is a
// slot whose details are being changed without a new file.
let picked = null;
let editing = null;
let fields = { title: "", artist: "", album: "" };
let candidates = [];
let lookupNote = "";
let searchText = "";
let targetSlot = null;

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
  original: "original song",
  your_file: "your song",
  pack: "from a pack",
  soundtrack_mod: "soundtrack mod",
};

function slotTitle(s) {
  if (!s || s.source === "original" || !s.title) return s ? `Original song ${s.slot}` : "";
  return s.title;
}

function slotLine(s) {
  if (!s) return "";
  if (s.source === "original") return `Original song ${s.slot}`;
  return [s.title || "Untitled", s.artist].filter(Boolean).join(" by ") + (mmss(s.duration_ms) ? ` (${mmss(s.duration_ms)})` : "");
}

function firstFreeSlot() {
  const free = slots().find((s) => s.source === "original" && !s.pending);
  return free ? free.slot : 21;
}

function pendingWhat(p) {
  return p.kind === "original" ? "the original song" : [p.title, p.artist].filter(Boolean).join(" by ") || "a song";
}

function clearFlow() {
  picked = null;
  editing = null;
  fields = { title: "", artist: "", album: "" };
  candidates = [];
  lookupNote = "";
  searchText = "";
  targetSlot = null;
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

// Step 1: choose a file. Reads its details and moves on to steps 2 and 3.
async function chooseFile() {
  const path = await pickPath("audio");
  if (!path) return;
  editing = null;
  picked = { path, reading: true };
  lookupNote = "Reading the file…";
  draw();
  try {
    picked = await call("music_inspect", { path });
    fields = { title: picked.title || "", artist: picked.artist || "", album: picked.album || "" };
    candidates = picked.candidates || [];
    searchText = picked.query || "";
    lookupNote = `${picked.note} Length ${mmss(picked.duration_ms)}.`;
    if (picked.too_long) lookupNote += ` This song is too long for a slot (up to ${mmss((status && status.limit_seconds ? status.limit_seconds : 380) * 1000)}). Trim it in an audio editor and choose it again.`;
    if (targetSlot === null) targetSlot = firstFreeSlot();
  } catch (e) {
    picked = null;
    problem = reason(e);
  }
  draw();
}

// Change a song's details without a new file: steps 2 and 3 with the slot fixed.
function editDetails(n) {
  const s = slotAt(n);
  const src = s && s.pending ? s.pending : s;
  picked = null;
  editing = n;
  targetSlot = n;
  fields = { title: (src && src.title) || "", artist: (src && src.artist) || "", album: (src && src.album) || "" };
  candidates = [];
  lookupNote = "";
  searchText = "";
  draw();
  const anchor = document.getElementById("step-details");
  if (anchor && typeof anchor.scrollIntoView === "function") anchor.scrollIntoView({ behavior: "smooth", block: "start" });
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

async function addToQueue() {
  const n = targetSlot || firstFreeSlot();
  const title = fields.title.trim();
  const artist = fields.artist.trim();
  const album = fields.album.trim();
  if (picked && picked.path && !picked.reading) {
    const path = picked.path;
    clearFlow();
    await act("music_stage_file", { path, slot: n, title, artist, album });
  } else if (editing !== null) {
    clearFlow();
    await act("music_stage_details", { slot: n, title, artist, album });
  }
}

// ---------------------------------------------------------------------------
// Sections
// ---------------------------------------------------------------------------

function discNow() {
  const st = status;
  const sec = el("section", { class: "mz-now" });
  if (!st) {
    sec.append(el("h2", { text: "Your disc" }), el("p", { class: "note", text: "The launcher has not read the disc yet." }));
    return sec;
  }
  const custom = slots().filter((s) => s.source !== "original").length;
  const baseName = st.base && st.base.kind === "package" ? st.base.name : null;
  let sentence;
  if (!st.image_ok) sentence = st.image_note || "There is no game image yet.";
  else if (!st.enabled) sentence = "Your disc plays the original soundtrack. Your own songs are kept for when you turn the custom soundtrack back on.";
  else if (custom === 0) sentence = "Your disc plays the original soundtrack.";
  else sentence = `Your disc plays ${custom} custom song${custom === 1 ? "" : "s"}${baseName ? ` on top of the ${baseName}` : ""}.`;
  sec.append(el("span", { class: "store-kicker", text: "Your disc right now" }), el("h2", { text: sentence }));
  if (st.image_ok) {
    sec.append(el("div", { class: "mz-row" },
      el("button", { type: "button", text: st.enabled ? "Turn the custom soundtrack off" : "Turn the custom soundtrack on", disabled: locked(), onclick: () => act("music_set_enabled", { on: !st.enabled }) }),
      el("button", { type: "button", text: "Import a pack", disabled: locked(), onclick: packDialog }),
      el("button", { type: "button", text: "Share a pack", disabled: locked(), onclick: shareDialog }),
      el("button", { type: "button", text: "Refresh", disabled: busy, onclick: refresh }),
    ));
  }
  return sec;
}

function statusLine() {
  const st = status;
  if (!st) return null;
  if (working()) {
    const p = st.progress || {};
    const pct = p.total ? Math.min(100, Math.round((p.done / p.total) * 100)) : 0;
    const text = st.phase === "encoding" ? `${p.label || "Encoding"}: segment ${Math.min((p.done || 0) + 1, p.total || 40)} of ${p.total || 40}` : `${p.label || "Building your game disc"}: ${pct}%. This rebuilds the disc once, about half a minute.`;
    return el("p", { class: "partial mz-progress", role: "status", text });
  }
  if (st.phase === "done" || st.phase === "problem") {
    const line = el("aside", { class: st.phase === "done" ? "mz-done" : "refusal mz-done", role: "status" });
    line.append(el("span", { text: st.note || (st.phase === "done" ? "Done." : "Something went wrong.") }), el("button", { type: "button", text: "Got it", onclick: () => act("music_dismiss") }));
    return line;
  }
  return null;
}

function stepHeader(n, title, done) {
  return el("div", { class: "mz-step-head" }, el("span", { class: `mz-step-n${done ? " done" : ""}`, text: String(n) }), el("h3", { text: title }));
}

function stepChoose() {
  const sec = el("section", { class: "mz-step", id: "step-choose" });
  const hasFile = !!(picked && picked.path);
  sec.append(stepHeader(1, editing !== null ? `Changing the details of slot ${editing}` : "Choose a song file", hasFile || editing !== null));
  if (editing !== null) {
    sec.append(el("p", { class: "note", text: `No new file. Fix the title, artist, or album below, then add it to the queue.` }), el("div", { class: "mz-row" }, el("button", { type: "button", text: "Cancel", onclick: () => { clearFlow(); draw(); } })));
    return sec;
  }
  if (!hasFile) {
    sec.append(
      el("p", { class: "note", text: "MP3, M4A, FLAC, OGG, WAV, or AIFF. Songs up to about 6:20 fit a slot." }),
      el("div", { class: "mz-row" }, el("button", { type: "button", class: "primary mz-big", text: "Choose a file", disabled: locked() || !(status && status.image_ok), onclick: chooseFile })),
    );
  } else {
    sec.append(
      el("p", { class: "mz-file", text: picked.path.split(/[\\/]/).pop() + (picked.reading ? " (reading…)" : mmss(picked.duration_ms) ? ` · ${mmss(picked.duration_ms)}` : "") }),
      el("div", { class: "mz-row" }, el("button", { type: "button", text: "Choose a different file", disabled: locked(), onclick: chooseFile }), el("button", { type: "button", text: "Cancel", onclick: () => { clearFlow(); draw(); } })),
    );
    if (picked.too_long) sec.append(el("p", { class: "refusal", text: lookupNote }));
  }
  return sec;
}

function stepDetails() {
  const active = (picked && picked.path && !picked.reading) || editing !== null;
  const sec = el("section", { class: `mz-step${active ? "" : " mz-off"}`, id: "step-details", "aria-disabled": String(!active) });
  sec.append(stepHeader(2, "Check the details", false));
  if (!active) {
    sec.append(el("p", { class: "note", text: "The title, artist, and album show on the in-game banner. They fill in from the file, or from an online lookup by its name." }));
    return sec;
  }
  if (picked && lookupNote && !picked.too_long) sec.append(el("p", { class: "note", text: lookupNote }));
  const title = el("input", { type: "text", maxlength: "49", value: fields.title, placeholder: "Title", oninput: (ev) => { fields.title = ev.target.value; } });
  const artist = el("input", { type: "text", maxlength: "49", value: fields.artist, placeholder: "Artist", oninput: (ev) => { fields.artist = ev.target.value; } });
  const album = el("input", { type: "text", maxlength: "49", value: fields.album, placeholder: "Album", oninput: (ev) => { fields.album = ev.target.value; } });
  sec.append(el("div", { class: "mz-fields" },
    el("label", { class: "mz-field" }, el("span", { text: "Title" }), title),
    el("label", { class: "mz-field" }, el("span", { text: "Artist" }), artist),
    el("label", { class: "mz-field" }, el("span", { text: "Album" }), album),
  ));
  const search = el("input", { type: "search", value: searchText, placeholder: "Not right? Search by song or artist", oninput: (ev) => { searchText = ev.target.value; }, onkeydown: (ev) => { if (ev.key === "Enter") { ev.preventDefault(); runSearch(); } } });
  sec.append(el("div", { class: "mz-row" }, search, el("button", { type: "button", text: "Search", onclick: runSearch })));
  if (editing !== null && lookupNote) sec.append(el("p", { class: "note", text: lookupNote }));
  if (candidates.length) {
    const list = el("ul", { class: "mz-candidates" });
    for (const c of candidates) {
      list.append(el("li", {}, el("button", { type: "button", class: "mz-candidate", text: [c.title, c.artist, c.album].filter(Boolean).join(" · ") + (c.source ? `  (${c.source})` : ""), onclick: () => { fields = { title: c.title || "", artist: c.artist || "", album: c.album || "" }; draw(); } })));
    }
    sec.append(list);
  }
  sec.append(el("p", { class: "note mz-privacy", text: "Searching sends the song name to iTunes or MusicBrainz. Nothing else leaves this computer." }));
  return sec;
}

function stepSlot() {
  const active = (picked && picked.path && !picked.reading) || editing !== null;
  const sec = el("section", { class: `mz-step${active ? "" : " mz-off"}`, id: "step-slot", "aria-disabled": String(!active) });
  sec.append(stepHeader(3, editing !== null ? `Slot ${editing}` : "Choose where it goes", false));
  if (!active) {
    sec.append(el("p", { class: "note", text: "The disc has 21 song slots. The new song replaces the one in the slot you pick; the first untouched slot is picked for you." }));
    return sec;
  }
  if (editing !== null) {
    sec.append(el("p", { class: "note", text: `The details apply to slot ${editing}.` }));
  } else {
    const list = el("div", { class: "mz-slots", role: "radiogroup", "aria-label": "Slot" });
    for (const s of slots()) {
      const id = `slot-${s.slot}`;
      const radio = el("input", { type: "radio", name: "slot", id, value: String(s.slot), checked: s.slot === targetSlot, onchange: () => { targetSlot = s.slot; draw(); } });
      const what = s.pending ? `staged: ${pendingWhat(s.pending)}` : slotLine(s);
      list.append(el("label", { class: `mz-slot${s.slot === targetSlot ? " selected" : ""}`, for: id }, radio, el("span", { class: "mz-slot-n", text: String(s.slot) }), el("span", { class: "mz-slot-what", text: what }), s.source !== "original" || s.pending ? el("span", { class: "mz-slot-tag", text: s.pending ? "staged" : SOURCE_LABEL[s.source] || "" }) : null));
    }
    sec.append(list);
  }
  const n = targetSlot || firstFreeSlot();
  const current = slotAt(n);
  const label = editing !== null ? `Add the new details to the queue` : `Add to the queue: slot ${n}${current && current.source !== "original" ? ` (replaces ${slotTitle(current)})` : ""}`;
  sec.append(el("div", { class: "mz-row mz-end" }, el("button", { type: "button", class: "primary mz-big", text: label, disabled: locked() || (picked && (picked.too_long || picked.reading)), onclick: addToQueue })));
  return sec;
}

function queue() {
  const st = status;
  const sec = el("section", { class: "mz-queue", id: "queue" });
  const staged = slots().filter((s) => s.pending);
  sec.append(el("span", { class: "store-kicker", text: "Ready to apply" }));
  if (!staged.length) {
    sec.append(el("h2", { text: "Nothing queued" }), el("p", { class: "note", text: "Add a song above, or use Change or Reset on a song below. Changes are written to the disc when you apply." }));
    return sec;
  }
  sec.append(el("h2", { text: `${staged.length} change${staged.length === 1 ? "" : "s"} ready` }));
  const list = el("ul", { class: "mz-queue-list" });
  for (const s of staged) {
    list.append(el("li", {}, el("span", { text: `Slot ${s.slot}: ${pendingWhat(s.pending)}` }), el("button", { type: "button", text: "Remove", "aria-label": `Remove the change for slot ${s.slot}`, disabled: locked(), onclick: () => act("music_unstage", { slot: s.slot }) })));
  }
  sec.append(list);
  sec.append(el("p", { class: "note", text: "Applying rebuilds your game disc once, about half a minute. Close the game first." }));
  sec.append(el("div", { class: "mz-row mz-end" },
    el("button", { type: "button", text: "Discard all", disabled: locked(), onclick: () => act("music_unstage", { slot: null }) }),
    el("button", { type: "button", class: "primary mz-big", text: "Apply to the game", disabled: locked() || !(st && st.image_ok), onclick: () => act("music_apply", { confirm: true }) }),
  ));
  return sec;
}

function discList() {
  const sec = el("section", { class: "mz-disc", id: "disc" });
  sec.append(el("span", { class: "store-kicker", text: "Songs on your disc" }), el("h2", { text: "What plays today" }));
  const list = slots();
  if (!list.length) {
    sec.append(el("p", { class: "note", text: "The launcher has not read the disc yet." }));
    return sec;
  }
  const table = el("table", { class: "mz-table" });
  const body = el("tbody");
  for (const s of list) {
    const row = el("tr", { class: `mz-track${s.pending ? " staged" : ""}`, "data-slot": String(s.slot) });
    row.append(
      el("td", { class: "mz-col-n", text: String(s.slot) }),
      el("td", { class: "mz-col-what" }, el("span", { text: slotLine(s) }), s.pending ? el("small", { class: "mz-staged", text: `Staged: ${pendingWhat(s.pending)}` }) : null),
      el("td", { class: "mz-col-tag", text: s.source === "original" ? "" : SOURCE_LABEL[s.source] || "" }),
      el("td", { class: "mz-col-actions" },
        el("button", { type: "button", text: "Change", "aria-label": `Change the details of slot ${s.slot}`, disabled: locked(), onclick: () => editDetails(s.slot) }),
        s.source !== "original" && !s.pending ? el("button", { type: "button", text: "Reset", "aria-label": `Reset slot ${s.slot} to the original song`, disabled: locked(), onclick: () => act("music_stage_stock", { slot: s.slot }) }) : null,
        s.pending ? el("button", { type: "button", text: "Undo", "aria-label": `Undo the change for slot ${s.slot}`, disabled: locked(), onclick: () => act("music_unstage", { slot: s.slot }) }) : null,
      ),
    );
    body.append(row);
  }
  table.append(body);
  sec.append(table);
  return sec;
}

function draw() {
  const screen = document.getElementById("screen");
  const parts = [];
  if (problem) parts.push(el("p", { class: "refusal", role: "alert", text: problem }));
  if (notice) parts.push(el("p", { class: "partial", role: "status", text: notice }));
  const line = statusLine();
  if (line) parts.push(line);
  parts.push(discNow());
  const add = el("section", { class: "mz-add" });
  add.append(el("span", { class: "store-kicker", text: "Add a song" }), stepChoose(), stepDetails(), stepSlot());
  parts.push(add);
  parts.push(queue());
  parts.push(discList());
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
  const rows = el("ul", { class: "mz-preview" });
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
      el("button", { type: "button", class: "primary", text: "Add these songs to the queue", disabled: locked(), onclick: async () => { dialog.close(); await act("music_stage_pack", { path }); } }),
    ),
  );
  dialog.append(body);
  showDialog(dialog);
}

function shareDialog() {
  const dialog = openDialog("Share a pack");
  const body = el("div", { class: "mod-dialog-body" });
  const boxes = [];
  const list = el("div", { class: "mz-share-list" });
  for (const s of slots()) {
    const box = el("input", { type: "checkbox", checked: s.source !== "original" });
    boxes.push([s.slot, box]);
    list.append(el("label", { class: "mz-share-row" }, box, el("span", { text: `Slot ${s.slot}: ${slotLine(s)}` })));
  }
  const name = el("input", { type: "text", placeholder: "Pack name" });
  const author = el("input", { type: "text", placeholder: "Your name" });
  const description = el("input", { type: "text", placeholder: "A line about it (optional)" });
  body.append(
    el("span", { class: "store-kicker", text: "Share" }),
    el("h2", { text: "Save a soundtrack pack" }),
    el("p", { text: "A pack carries the songs as the game plays them, so a friend can apply it without any converting. Songs still in the queue are not included until you apply." }),
    list,
    el("label", { class: "mz-field" }, el("span", { text: "Name" }), name),
    el("label", { class: "mz-field" }, el("span", { text: "Author" }), author),
    el("label", { class: "mz-field" }, el("span", { text: "Description" }), description),
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

for (const a of typeof document.querySelectorAll === "function" ? document.querySelectorAll(".shell-nav a") : []) {
  if ((a.getAttribute("href") || "").toLowerCase() === "music.html") a.classList.add("current");
}

if (bridge) {
  refresh();
} else {
  problem = "The launcher core is not attached. Open this page inside the launcher window.";
  draw();
}
