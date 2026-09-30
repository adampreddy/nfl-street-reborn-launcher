// The one save bar (spec §7). It lives in its own lane at the bottom of the page, so it never covers content.
import { el } from "./shell.js";

export function mountSaveBar(lane, { onSave, onDiscard }) {
  lane.classList.add("save-lane");
  const title = el("div", { class: "t" }), detail = el("div", { class: "d" });
  const discard = el("button", { type: "button", class: "btn", text: "Discard" });
  const save = el("button", { type: "button", class: "btn primary", text: "Save changes" });
  let busy = false;
  const run = (fn) => async () => { if (busy) return; busy = true; discard.disabled = save.disabled = true;
    try { await fn(); } finally { busy = false; discard.disabled = save.disabled = false; } };
  discard.addEventListener("click", run(onDiscard));
  save.addEventListener("click", run(onSave));
  lane.replaceChildren(el("div", { class: "save-bar", role: "region", "aria-label": "Unsaved changes" },
    el("span", { class: "dot", "aria-hidden": "true" }), el("div", {}, title, detail), discard, save));
  return {
    update(changes) {
      const n = changes.length;
      title.textContent = `${n} unsaved change${n === 1 ? "" : "s"}`;
      detail.textContent = changes.at(-1) || "";
      lane.classList.toggle("open", n > 0);
    },
  };
}
