// One anchored menu (spec §3.4, §5.2): a glass list with a check on the current option.
// A modal <dialog> on <body>, so it sits in the top layer: never clipped by a panel, never anchored to
// #main while #main animates in, and the shell's pad navigation treats it as its own scope (the D-pad
// moves inside it, B closes it). Opened by a click; nothing here runs on page load.
import { el } from "./shell.js";

let openNow = null;

export function openMenu(anchor, { label, options, current = null, onPick }) {
  openNow?.close();
  let menu = null;
  const items = options.map((o) => el("button", {
    type: "button", class: "menu-item", role: "menuitemradio", "aria-checked": String(o.value === current), "data-value": o.value,
    ...(o.value === current ? { "data-primary": "true" } : {}),
    onclick: () => { menu.close(); onPick(o.value); },
  }, el("span", { text: o.label }), el("span", { class: "menu-check", "aria-hidden": "true", text: "✓" })));
  menu = el("dialog", { class: "menu", "aria-label": label }, el("div", { class: "menu-list", role: "menu", "aria-label": label }, ...items));
  menu.addEventListener("keydown", (e) => {
    const i = items.indexOf(document.activeElement);
    const step = e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0;
    if (step) { e.preventDefault(); items[(i + step + items.length) % items.length].focus(); }
    else if (e.key === "Home" || e.key === "End") { e.preventDefault(); items[e.key === "Home" ? 0 : items.length - 1].focus(); }
  });
  // A click on the backdrop, outside the list, closes it.
  menu.addEventListener("click", (e) => {
    if (e.target !== menu) return;
    const r = menu.getBoundingClientRect();
    if (e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom) menu.close();
  });
  menu.addEventListener("close", () => {
    menu.remove();
    if (openNow === menu) openNow = null;
    // `close` is queued, so it can arrive after another menu has opened, even one on this same
    // anchor (a redraw reopening it by id, ruling P2-18). That menu owns the anchor now: leave it be.
    if (openNow && openNow !== menu) return;
    anchor.setAttribute("aria-expanded", "false");
    // The page may have redrawn while the menu was open: go back to the button that replaced this one.
    const back = anchor.isConnected ? anchor : anchor.id ? document.getElementById(anchor.id) : null;
    back?.setAttribute("aria-expanded", "false");
    back?.focus({ preventScroll: true });
  }, { once: true });
  anchor.setAttribute("aria-expanded", "true");
  document.body.append(menu);
  menu.showModal();
  place(anchor, menu);
  (items.find((n) => n.dataset.value === String(current)) || items[0])?.focus();
  openNow = menu;
  return menu;
}

// Above the button, right-aligned 8px inside it (the mockup's strip sits at the bottom of Play);
// below it when there is no room above; always inside the window.
function place(anchor, menu) {
  const a = anchor.getBoundingClientRect(), m = menu.getBoundingClientRect(), gap = 8;
  const above = a.top - m.height - gap >= gap;
  const top = above ? a.top - m.height - gap : Math.min(innerHeight - m.height - gap, a.bottom + gap);
  const left = Math.max(gap, Math.min(innerWidth - m.width - gap, a.right - m.width - gap));
  menu.style.setProperty("--menu-x", `${Math.round(left)}px`);
  menu.style.setProperty("--menu-y", `${Math.round(top)}px`);
  menu.dataset.side = above ? "above" : "below";
}
