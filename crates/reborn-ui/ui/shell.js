// Shared by every shell page: talking to the core, building elements, the
// settings the harness and the shell keep in common, and marking the current
// page in the nav.
//
// The same two rules as every other page here. Every value on screen comes
// from a command, and the core owns the rules; this file only draws.

export const bridge = window.__TAURI__ ? window.__TAURI__.core : null;

// The keys the harness uses, kept identical on purpose: a path set on the
// Settings page is the path the Advanced page sends, and the other way round.
const FALLBACKS = {};

export function get(key) {
  try {
    const v = localStorage.getItem("h:" + key);
    if (v) return v;
  } catch (_) {
    /* storage can be unavailable; absent is the honest answer */
  }
  return FALLBACKS[key] || null;
}

export function set(key, value) {
  try {
    if (value) localStorage.setItem("h:" + key, value);
    else localStorage.removeItem("h:" + key);
  } catch (_) {
    /* see get */
  }
}

// A path as a person pasted it: Explorer's Copy as path wraps it in quotes.
export function cleanPath(text) {
  let s = (text || "").trim();
  if (s.length >= 2 && s[0] === '"' && s[s.length - 1] === '"') s = s.slice(1, -1).trim();
  if (s.length >= 2 && s[0] === "'" && s[s.length - 1] === "'") s = s.slice(1, -1).trim();
  return s;
}

export function log(what, value) {
  const pre = document.getElementById("out");
  if (!pre) return;
  const body = typeof value === "string" ? value : JSON.stringify(value, null, 2);
  pre.textContent = `${what}\n${body}\n\n${pre.textContent}`.slice(0, 20000);
}

export async function call(cmd, args = {}) {
  if (!bridge) throw new Error("the launcher core is not attached. Open this page inside the launcher window.");
  const answer = await bridge.invoke(cmd, args);
  if(['devices','controller_slots','controls_current','controls_options','launch_check'].includes(cmd)){
    try{sessionStorage.setItem('reborn:controller-cache:'+cmd,JSON.stringify({at:Date.now(),value:answer}));}catch{}
  }
  log(cmd, answer === undefined ? "ok" : answer);
  return answer;
}

export function reason(e) {
  if (e === null || e === undefined) return "unknown failure";
  if (typeof e === "string") return e;
  if (e.message) return e.message;
  return JSON.stringify(e);
}

export function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v === null || v === undefined) continue;
    if (k === "class") node.className = v;
    else if (k === "text") node.textContent = v;
    else if (k === "onclick") node.addEventListener("click", v);
    else if (k === "onchange") node.addEventListener("change", v);
    else if (k === "oninput") node.addEventListener("input", v);
    else if (k === "checked" || k === "disabled" || k === "value" || k === "selected") node[k] = v;
    else node.setAttribute(k, v);
  }
  for (const c of children) {
    if (c !== null && c !== undefined) node.append(c);
  }
  return node;
}

// A label / value row for a facts table. A value nothing measured is drawn as
// absent, in so many words, never as a plausible default.
export function fact(name, value, cls) {
  const absent = value === null || value === undefined || value === "";
  const cell = absent
    ? el("td", { class: "absent", text: "not known" })
    : el("td", { class: cls || null, text: String(value) });
  return el("tr", {}, el("th", { text: name }), cell);
}

export function facts(rows) {
  const table = el("table", { class: "facts" });
  for (const r of rows) table.append(r);
  return table;
}

export function refusal(text) {
  return el("p", { class: "refusal", role: "alert", text });
}

export function partial(text) {
  return el("p", { class: "partial", text });
}

// Mark the nav link for this page, by file name.
export function markCurrent() {
  const here = (location.pathname.split("/").pop() || "index.html").toLowerCase();
  if (typeof document.querySelectorAll !== "function") return;
  for (const a of document.querySelectorAll(".shell-nav a")) {
    const target = (a.getAttribute("href") || "").toLowerCase();
    if (target === here) a.classList.add("current");
  }
}

// The fold that holds raw answers, with its Clear button wired.
export function wireCoreFold() {
  const clear = document.getElementById("clear");
  if (clear) {
    clear.addEventListener("click", () => {
      const pre = document.getElementById("out");
      if (pre) pre.textContent = "";
    });
  }
}

// Minutes as a person would say them.
export function minutes(n) {
  if (typeof n !== "number") return null;
  if (n < 1) return "under a minute";
  if (n < 60) return `${Math.round(n)} min`;
  const h = Math.floor(n / 60);
  const m = Math.round(n % 60);
  return m ? `${h} h ${m} min` : `${h} h`;
}

// Disk preferences are authoritative across windows and the Play shortcut.
// Wait before a page builds its first command arguments from browser storage.
if (bridge) {
  try {
    const saved = await bridge.invoke("settings_paths");
    for (const key of ["runtime", "game", "userDir", "legacyRoot"]) {
      if (saved && Object.prototype.hasOwnProperty.call(saved, key)) set(key, saved[key]);
    }
  } catch (_) { /* Pages surface their own read failures and keep existing values. */ }
}
