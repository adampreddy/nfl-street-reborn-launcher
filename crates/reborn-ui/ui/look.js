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

let restoredOnce = false;
export async function refreshLook() {
  if (!bridge) return null;
  try {
    let v = await bridge.invoke("launcher_look");
    // A glass preview that was never saved must not outlive its page: put the window back to the SAVED state (once per page load).
    if (v && v.glassSupported && v.glassApplied !== v.windowGlass && !restoredOnce) {
      restoredOnce = true;
      try { const r = await bridge.invoke("window_glass_apply", { enabled: v.windowGlass }); if (r && typeof r === "object") v = r; } catch (_) {}
    }
    applyLook(v); return v;
  } catch (_) { return null; }
}

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
