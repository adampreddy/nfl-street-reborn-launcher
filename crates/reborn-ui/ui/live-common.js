// Shared by the two live pages: "Live now" on the Online page (live-now.js) and the native match
// view (live.js). One copy of each rule, so the two pages can never drift apart:
//   - the backend origin and the two image prefixes the CSP `img-src` allows (a Rust test holds
//     LIVE_ORIGIN to identity::DEFAULT_ORIGIN and to tauri.conf.json);
//   - the strict `/live-assets/...png` path rule;
//   - which seat has which side;
//   - polling: one request at a time, exponential backoff with jitter on errors, a full minute
//     after a rate limit (429), and the normal interval again after the first success.

// The backend origin the CSP's img-src is pinned to (identity::DEFAULT_ORIGIN on the Rust side).
// Deliberately NOT read from a setting: a player-configured `backend_origin` cannot change what
// the static CSP allows, so image URLs are always built against this default. A self-hosted
// backend's images simply fail to load and draw their badges instead. "Watch on web" also opens
// this origin's page: the production site only.
export const LIVE_ORIGIN = 'https://api.nflstreetreborn.com';
export const AVATAR_PREFIX = 'https://cdn.discordapp.com/avatars/';
export const ASSET_RE = /^\/live-assets\/[A-Za-z0-9._/-]+\.png$/;

/** A `/live-assets/...png` path as an absolute URL under the default origin, or null. */
export function assetUrl(path) {
  return typeof path === 'string' && ASSET_RE.test(path) && !path.includes('..') ? LIVE_ORIGIN + path : null;
}

/** A seat's Discord avatar URL, only when it is already exactly that CDN's prefix. */
export function avatarSrc(seat) {
  const u = seat && typeof seat.avatar_url === 'string' ? seat.avatar_url : '';
  return u.startsWith(AVATAR_PREFIX) ? u : null;
}

/** Which side each seat has: `[sideOfSeat0, sideOfSeat1]`, each 0|1|null. */
export function sideOfSeats(sides) {
  const out = [null, null];
  (Array.isArray(sides) ? sides : []).forEach((s, side) => { if (s && (s.seat === 0 || s.seat === 1) && side < 2) out[s.seat] = side; });
  return out;
}

/** Which seat has each side: `[seatOfSide0, seatOfSide1]`; seat order `[0, 1]` when the sides are not known. */
export function seatsBySide(sides) {
  const [s0, s1] = sideOfSeats(sides);
  if (s0 === null || s1 === null || s0 === s1) return [0, 1];
  return s0 === 0 ? [0, 1] : [1, 0];
}

// ---- polling ----

export const BACKOFF_MIN_MS = 5000;
export const BACKOFF_MAX_MS = 60000;
export const RATE_LIMIT_MS = 60000;

/** Whether a rejected command was the backend's 429 (the Rust side's `kind: "rate_limited"`). */
export function isRateLimited(e) {
  return Boolean(e && typeof e === 'object' && e.kind === 'rate_limited');
}

/** The wait after the `failures`-th error in a row: 5 s doubling to 60 s, plus up to 25 % jitter
    (capped at 60 s), so a room of launchers that failed together does not retry together. A rate
    limit waits at least 60 s. */
export function backoffDelay(failures, rateLimited, rand = Math.random) {
  const base = Math.min(BACKOFF_MAX_MS, BACKOFF_MIN_MS * 2 ** Math.max(0, failures - 1));
  const delay = Math.min(BACKOFF_MAX_MS, base + base * 0.25 * rand());
  return rateLimited ? Math.max(RATE_LIMIT_MS, delay) : delay;
}

/** Polls `task` every `intervalMs` while started. `task` resolves on success and rejects on an
    error (after drawing it); only one runs at a time. `start()` polls at once -- unless an error's
    backoff is still running, which it waits out, so showing the page again never skips a rate
    limit -- and does nothing if already started. `stop()` cancels the next poll, and a poll still
    in flight when `stop()` is called schedules nothing. `task` may call `stop()` itself (a match
    that has ended). */
export function makePoller(task, intervalMs) {
  let timer = null, inflight = false, running = false, failures = 0, notBefore = 0;
  async function fire() {
    timer = null;
    if (inflight || !running) return;
    inflight = true;
    let delay = intervalMs;
    try {
      await task();
      failures = 0;
      notBefore = 0;
    } catch (e) {
      failures++;
      delay = backoffDelay(failures, isRateLimited(e));
      notBefore = Date.now() + delay;
    } finally {
      inflight = false;
    }
    if (running && !timer) timer = setTimeout(fire, delay);
  }
  return {
    start() {
      if (running) return;
      running = true;
      const wait = notBefore - Date.now();
      if (wait > 0) { if (!timer) timer = setTimeout(fire, wait); } else fire();
    },
    stop() { running = false; if (timer) { clearTimeout(timer); timer = null; } },
    get running() { return running; },
    get failures() { return failures; },
  };
}
