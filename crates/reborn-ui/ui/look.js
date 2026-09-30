// The launcher's look: asks the core, sets the page, remembers for the next first paint.
import { bridge } from "./shell.js";

export function applyLook(view) {
  if (!view || typeof view !== "object") return;
  const glass = view.glassApplied === true, slow = view.slowAnimations !== false;
  const d = document.documentElement.dataset;
  d.glass = glass ? "on" : "off";
  d.motion = slow ? "slow" : "quick";
  try { localStorage.setItem("reborn:look", JSON.stringify({ glass, slow })); } catch (_) {}
}

// One read per page: the shell's own call and Settings' both get the SAME corrected view.
// A glass preview that was never saved must not outlive its page, so the first read puts the window back to the SAVED state.
let pending = null;
async function readLook() {
  let v = await bridge.invoke("launcher_look");
  if (v && v.glassSupported && v.glassApplied !== v.windowGlass) {
    try { const r = await bridge.invoke("window_glass_apply", { enabled: v.windowGlass }); if (r && typeof r === "object") v = r; } catch (_) {}
  }
  applyLook(v); return v;
}
// { view, error }: error is the raw failure (for a page that must say why), view is null when it failed.
export function refreshLookResult() {
  if (!bridge) return Promise.resolve({ view: null, error: null });
  if (!pending) {
    pending = readLook().then(view => ({ view, error: null }), error => { pending = null; return { view: null, error }; });
  }
  return pending;
}
export async function refreshLook() { return (await refreshLookResult()).view; }

// One marker that glides to the current nav item (spec §5.2, §6.1).
export function mountNavMarker() {
  const nav = document.querySelector(".shell-nav");
  if (!nav || nav.querySelector(".nav-marker")) return;
  const marker = document.createElement("span");
  marker.className = "nav-marker still";
  marker.setAttribute("aria-hidden", "true");
  nav.append(marker);
  const place = () => {
    const a = nav.querySelector("a.current:not(.brand)");
    if (!a) { marker.style.opacity = "0"; return; }
    marker.style.opacity = "1";
    marker.style.transform = `translateY(${a.offsetTop + (a.offsetHeight - 18) / 2}px)`;
  };
  // Start where the previous page's marker was, then glide to this page's item.
  let from = null;
  try { from = sessionStorage.getItem("reborn:nav-marker"); } catch (_) {}
  if (from) marker.style.transform = from;
  else place(); // first load: appear at the target, no glide from the logo
  requestAnimationFrame(() => {
    if (from) { marker.classList.remove("still"); place(); }
    else requestAnimationFrame(() => marker.classList.remove("still"));
    try { sessionStorage.setItem("reborn:nav-marker", marker.style.transform); } catch (_) {}
  });
  addEventListener("resize", () => { marker.classList.add("still"); place(); requestAnimationFrame(() => marker.classList.remove("still")); });
}
