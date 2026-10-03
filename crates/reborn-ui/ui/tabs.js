// Segmented tabs with one sliding highlight (spec §3.4, §5.2). Buttons with aria-pressed, as before, so
// the pad and the checks find them by name. A page that redraws gets a new element each time; the
// highlight starts where this group's last one was and glides to the selected option.
// The highlight is a fixed-width box moved with transform only (translateX + scaleX); width and left never animate.
import { el } from "./shell.js";

const BASE = 100; // px: the highlight's real width; scaleX stretches it to the tab's width
const last = new Map();

export function tabs(group, names, current, onPick, { disabled = false, label } = {}) {
  const ind = el("span", { class: "tab-ind still", "aria-hidden": "true" });
  const bar = el("div", { class: "tabs", role: "group", "aria-label": label || group }, ind,
    ...names.map((name) => el("button", { type: "button", class: "tab", text: name, "aria-pressed": String(name === current), disabled,
      onclick: () => { if (name !== current) onPick(name); } })));
  // Arrow keys walk focus between the tabs (Enter/Space picks). The pad's D-pad does the same by position.
  bar.addEventListener("keydown", (e) => {
    const all = [...bar.querySelectorAll(".tab:not(:disabled)")];
    const at = all.indexOf(document.activeElement);
    if (at < 0) return;
    const to = e.key === "ArrowRight" ? at + 1 : e.key === "ArrowLeft" ? at - 1 : e.key === "Home" ? 0 : e.key === "End" ? all.length - 1 : null;
    if (to === null) return;
    e.preventDefault();
    all[Math.max(0, Math.min(all.length - 1, to))]?.focus();
  });
  if (typeof requestAnimationFrame === "function") requestAnimationFrame(() => place(group, bar, ind));
  return bar;
}

const at = (p) => `translateX(${p.x}px) scaleX(${p.w / BASE})`;

function place(group, bar, ind) {
  const on = bar.querySelector('[aria-pressed="true"]');
  if (!bar.isConnected || !on) return;
  const to = { x: on.offsetLeft, w: on.offsetWidth };
  // The highlight sits exactly on the selected tab: same top and height (static; only transform animates).
  ind.style.top = `${on.offsetTop}px`; ind.style.height = `${on.offsetHeight}px`;
  // A strip wider than its box scrolls inside itself: keep the selected tab in view (the highlight scrolls with it).
  if (bar.scrollWidth > bar.clientWidth) {
    const right = on.offsetLeft + on.offsetWidth;
    if (on.offsetLeft < bar.scrollLeft) bar.scrollLeft = on.offsetLeft - 4;
    else if (right > bar.scrollLeft + bar.clientWidth) bar.scrollLeft = right - bar.clientWidth + 4;
  }
  const from = last.get(group) || to;
  ind.style.transform = at(from);
  last.set(group, to);
  requestAnimationFrame(() => {
    ind.classList.remove("still");
    ind.style.transform = at(to);
  });
  // A web font that arrives late, or the window crossing the 900px rule (tab padding 14 -> 10px), changes every tab's
  // width after this measure: re-measure and snap (no glide) to the new box.
  let snapped = to;
  const snap = () => {
    if (!bar.isConnected) { observer?.disconnect(); return; }
    const now = { x: on.offsetLeft, w: on.offsetWidth };
    if (now.x === snapped.x && now.w === snapped.w) return;
    snapped = now;
    ind.classList.add("still");
    ind.style.transform = at(now);
    last.set(group, now);
    requestAnimationFrame(() => requestAnimationFrame(() => ind.classList.remove("still")));
  };
  const observer = typeof ResizeObserver === "function" ? new ResizeObserver(snap) : null;
  observer?.observe(bar);
  observer?.observe(on);
  document.fonts?.ready.then(snap);
}
