// Pending Settings changes (spec §7, §8.2): every change waits here until Save changes or Discard.
// Pure: no DOM, no commands. Each entry brings its own save (and, for a live preview, its own revert).
export const HELD = "held";

export function createDraft() {
  const entries = new Map();
  // One save or discard at a time. The same one pressed again joins the run in flight; the other one
  // waits for it and then runs on what is left, so no change is ever saved or reverted twice.
  let tail = null;
  const inflight = {};
  const once = (kind, fn) => {
    if (inflight[kind]) return inflight[kind];
    // Nothing in flight: start now, as a plain call would. Otherwise start when the other one ends.
    const started = tail ? tail.catch(() => {}).then(fn) : fn();
    const run = started.finally(() => {
      if (inflight[kind] === run) inflight[kind] = null;
      if (tail === run) tail = null;
    });
    inflight[kind] = run;
    tail = run;
    return run;
  };
  return {
    // Record a change; `same` means it now equals what is saved, which removes it. Re-inserted, so the
    // newest change is listed last (the bar names the last change).
    put(key, { label, value, same = false, save, revert = null }) {
      entries.delete(key);
      if (!same) entries.set(key, { label, value, save, revert });
    },
    drop(key) { entries.delete(key); },
    has(key) { return entries.has(key); },
    value(key, fallback) { return entries.has(key) ? entries.get(key).value : fallback; },
    changes() { return [...entries.values()].map((e) => e.label); },
    // In the order the changes were made. Stops at the first that fails (its message) or is held for a
    // choice the player must make (HELD); that one and every later one stay pending. A change made again
    // while its save was in flight is newer than what was saved, so it stays pending too.
    saveAll() {
      return once("save", async () => {
        let saved = 0;
        for (const [key, e] of [...entries]) {
          if (entries.get(key) !== e) continue; // replaced or dropped since the run began
          let outcome;
          try { outcome = await e.save(e.value); }
          catch (err) { return { saved, stopped: { key, label: e.label, reason: message(err) } }; }
          if (outcome === HELD) return { saved, stopped: { key, label: e.label, reason: HELD } };
          if (entries.get(key) === e) entries.delete(key);
          saved++;
        }
        return { saved, stopped: null };
      });
    },
    // Put every previewed change back, newest first. Nothing is saved. A revert that fails stops the
    // discard and throws its reason: that change, and every older one not yet put back, stay pending, so
    // nothing still previewed is ever shown as discarded (spec §7).
    discardAll() {
      return once("discard", async () => {
        for (const [key, e] of [...entries].reverse()) {
          if (entries.get(key) !== e) continue;
          if (e.revert) {
            try { await e.revert(); } catch (err) { throw new Error(`${e.label}: ${message(err)}`); }
          }
          if (entries.get(key) === e) entries.delete(key);
        }
      });
    },
  };
}

// A reason a person can read: never "{}" or an empty string.
function message(e) {
  const plain = "Something went wrong. Try again.";
  if (e === null || e === undefined) return plain;
  if (typeof e === "string") return e.trim() || plain;
  if (typeof e.message === "string" && e.message.trim()) return e.message;
  let s = "";
  try { s = JSON.stringify(e); } catch { s = ""; }
  return s && s !== "{}" && s !== "[]" && s !== '""' ? s : plain;
}
