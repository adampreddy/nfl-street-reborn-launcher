// The one save bar (spec §7). It lives in its own lane at the bottom of the page, so it never covers content.
// A page opts in with <body class="… has-save-lane"> and an empty <div id="save-lane"></div> after #main.
// A save or discard that throws is said in the bar (its reason, not thrown past it); the bar stays open.
import { el, reason } from "./shell.js";

export function mountSaveBar(lane, { onSave, onDiscard, returnFocus, noun = "unsaved change", saveText = "Save changes" }) {
  lane.classList.add("save-lane");
  lane.inert = true; // closed = not focusable, not clickable
  const title = el("div", { class: "t" }), detail = el("div", { class: "d" });
  const error = el("p", { class: "err", role: "alert" });
  error.hidden = true;
  const discard = el("button", { type: "button", class: "btn", text: "Discard" });
  const save = el("button", { type: "button", class: "btn primary", text: saveText });
  let busy = false, listed = "[]";
  // The line shows two lines at most; the full reason is always in its tooltip.
  const fail = (message) => {
    error.textContent = message || ""; error.hidden = !message;
    if (message) error.title = message; else error.removeAttribute("title");
  };
  const run = (fn) => async () => {
    if (busy) return;
    busy = true; discard.disabled = save.disabled = true; fail("");
    try { await fn(); } catch (e) { fail(reason(e)); }
    finally { busy = false; discard.disabled = save.disabled = false; }
  };
  discard.addEventListener("click", run(onDiscard));
  save.addEventListener("click", run(onSave));
  lane.replaceChildren(el("div", { class: "save-bar", role: "region", "aria-label": "Unsaved changes" },
    el("span", { class: "dot", "aria-hidden": "true" }), el("div", { class: "save-text" }, title, detail, error), discard, save));
  return {
    update(changes) {
      const n = changes.length;
      title.textContent = `${n} ${noun}${n === 1 ? "" : "s"}`;
      detail.textContent = changes.at(-1) || "";
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
  };
}
