// Shared by every Publisher page: talking to the core, building elements,
// and marking the current page in the nav.
//
// Two rules, same as the launcher's shell: every value on screen comes from
// a command, and every rule lives in the core (or in `ops.rs`, which is
// this app's own front door into it); this file only draws and dispatches.

export const bridge = window.__TAURI__ ? window.__TAURI__.core : null;

/// Invoke a command. Always resolves; never throws into the caller.
///
/// A page that awaited `bridge.invoke` directly would need its own
/// try/catch around every single call to survive a command that rejects,
/// and the one call a page author forgot would be the one that breaks that
/// page. This does it once, here: `{ok: true, value}` on success,
/// `{ok: false, error}` when the bridge is missing, the command rejected,
/// or invoking it threw for any other reason.
export async function call(name, args = {}) {
  if (!bridge) {
    return {
      ok: false,
      error: "The Publisher core is not attached. Open this page inside the Publisher window.",
    };
  }
  try {
    const value = await bridge.invoke(name, args);
    return { ok: true, value };
  } catch (e) {
    return { ok: false, error: reason(e) };
  }
}

/// A thrown value, or a rejected promise's reason, as one sentence.
export function reason(e) {
  if (e === null || e === undefined) return "unknown failure";
  if (typeof e === "string") return e;
  if (e.message) return e.message;
  try {
    return JSON.stringify(e);
  } catch (_) {
    return String(e);
  }
}

/// Build one element. `attrs.class` sets className, `attrs.text` sets
/// textContent, `on*` keys add listeners, `checked`/`disabled`/`value`/
/// `selected`/`hidden` are set as IDL properties (never as attributes, so
/// visibility toggles with `el.hidden` the way the rest of this app does),
/// everything else becomes an attribute. Children may be nested arrays.
export function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v === null || v === undefined) continue;
    if (k === "class") node.className = v;
    else if (k === "text") node.textContent = v;
    else if (k.startsWith("on") && typeof v === "function") node.addEventListener(k.slice(2), v);
    else if (k === "checked" || k === "disabled" || k === "value" || k === "selected" || k === "hidden") {
      node[k] = v;
    } else node.setAttribute(k, v);
  }
  for (const child of children.flat(Infinity)) {
    if (child === null || child === undefined || child === false) continue;
    node.append(child);
  }
  return node;
}

/// A label / value row for a facts table. A value nothing measured is drawn
/// as absent, in words, never as a plausible-looking default.
export function fact(name, value, cls) {
  const absent = value === null || value === undefined || value === "";
  const cell = absent
    ? el("td", { class: "absent", text: "not known" })
    : el("td", { class: cls || null, text: String(value) });
  return el("tr", {}, el("th", { text: name }), cell);
}

export function facts(rows) {
  return el("table", { class: "facts" }, ...rows);
}

export function refusal(text) {
  return el("p", { class: "refusal", role: "alert", text });
}

export function partial(text) {
  return el("p", { class: "partial", text });
}

/// Mark the nav link for the current page, by file name.
export function markCurrent() {
  const here = (location.pathname.split("/").pop() || "index.html").toLowerCase();
  for (const a of document.querySelectorAll(".pub-nav a")) {
    const target = (a.getAttribute("href") || "").toLowerCase();
    if (target === here) a.classList.add("current");
  }
}

/// A byte count the way a person reads it: whole numbers under 1 KB, two
/// decimals down to one, then one, then none, as the unit grows. Not meant
/// to be exact past the point anyone would act on it.
export function fmtBytes(n) {
  if (typeof n !== "number" || !Number.isFinite(n) || n < 0) return "unknown";
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = n / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = value < 10 ? 2 : value < 100 ? 1 : 0;
  return `${value.toFixed(digits)} ${units[unit]}`;
}
