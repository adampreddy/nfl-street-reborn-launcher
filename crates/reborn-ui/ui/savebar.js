// The one save bar (spec §7). It lives in its own lane at the bottom of the page, so it never covers content.
// A save or discard that throws is said in the bar (its reason, not thrown past it); the bar stays open.
//
// Seams a page must honour (app-shell.js relies on them):
// - <body class="… has-save-lane">: theme.css gives the shell grid its extra 'save' row only then.
// - The lane is an empty <div id="save-lane"></div> right after #main, and its id must be exactly
//   "save-lane": app-shell looks it up by id for pad navigation (Down from the page reaches the bar),
//   the leave guard (a[href] clicks, B twice and LB/RB while it is .open) and window.pointAtSaveBar.
// - A page whose content is a column of rows (Settings) opts into row-first vertical pad moves with
//   data-nav="rows" on its content container; without it, Up/Down pick the nearest control by position.
// - bar.lock(saveHeld, discardHeld = saveHeld) keeps the buttons disabled until lock(false); while both are,
//   .save-bar itself is focusable (tabindex -1) and app-shell falls back to it.
// - The detail line is one line (ellipsis); its full text is in its tooltip.
// - keepsOnLeave: true is for changes held outside the page (Music: the core stages songs and keeps them
//   across navigation). It marks the lane data-keeps, and app-shell's leave guards (link clicks, B from the
//   sidebar, LB/RB, update-notice's View updates) then let the player leave; nothing is lost by leaving.
//   Pad moves into the bar and toSaveBar focus are unchanged. A page-local draft (Settings) must not set it.
import { el, reason } from "./shell.js";

export function mountSaveBar(lane, { onSave, onDiscard, returnFocus, noun = "unsaved change", saveText = "Save changes", keepsOnLeave = false }) {
  lane.classList.add("save-lane");
  if (keepsOnLeave) lane.dataset.keeps = "";
  lane.inert = true; // closed = not focusable, not clickable
  const title = el("div", { class: "t" }), detail = el("div", { class: "d" });
  const error = el("p", { class: "err", role: "alert" });
  error.hidden = true;
  const discard = el("button", { type: "button", class: "btn", text: "Discard" });
  const save = el("button", { type: "button", class: "btn primary", text: saveText });
  let busy = false, listed = "[]";
  // A page may hold the buttons while its own work runs (Music: while the core encodes or builds).
  let held = { save: false, discard: false };
  const bar = el("div", { class: "save-bar", role: "region", "aria-label": "Unsaved changes" });
  const sync = () => {
    save.disabled = busy || held.save; discard.disabled = busy || held.discard;
    // With both buttons disabled nothing in the bar can take focus, so the bar itself does (a refused
    // leave, or a pad moving down into it, must never focus nothing).
    if (save.disabled && discard.disabled) bar.tabIndex = -1; else bar.removeAttribute("tabindex");
  };
  // The line shows two lines at most; the full reason is always in its tooltip.
  const fail = (message) => {
    error.textContent = message || ""; error.hidden = !message;
    if (message) error.title = message; else error.removeAttribute("title");
  };
  const run = (fn) => async () => {
    if (busy) return;
    busy = true; sync(); fail("");
    try { await fn(); } catch (e) { fail(reason(e)); }
    finally { busy = false; sync(); }
  };
  discard.addEventListener("click", run(onDiscard));
  save.addEventListener("click", run(onSave));
  bar.append(el("span", { class: "dot", "aria-hidden": "true" }),
    el("div", { class: "save-text", "aria-live": "polite" }, title, detail, error), discard, save);
  lane.replaceChildren(bar);
  return {
    update(changes) {
      const n = changes.length;
      title.textContent = `${n} ${noun}${n === 1 ? "" : "s"}`;
      detail.textContent = changes.at(-1) || "";
      if (detail.textContent) detail.title = detail.textContent; else detail.removeAttribute("title");
      const hadFocus = n === 0 && lane.contains(document.activeElement);
      lane.classList.toggle("open", n > 0);
      lane.inert = n === 0;
      // A reason belongs to the list it failed on: a different list (or none) clears it.
      const key = JSON.stringify(changes);
      if (n === 0 || key !== listed) fail("");
      listed = key;
      if (hadFocus) (returnFocus?.() || document.getElementById("main"))?.focus({ preventScroll: true });
    },
    fail,
    // Keep Save (and Discard, unless told otherwise) disabled until lock(false).
    lock(saveHeld, discardHeld = saveHeld) { held = { save: !!saveHeld, discard: !!discardHeld }; sync(); },
  };
}
