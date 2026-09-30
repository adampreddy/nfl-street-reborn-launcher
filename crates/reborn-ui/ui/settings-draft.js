// Pending Settings changes (spec §7, §8.2): every change waits here until Save changes or Discard.
// Pure: no DOM, no commands. Each entry brings its own save (and, for a live preview, its own revert).
export const HELD = "held";

export function createDraft() {
  const entries = new Map();
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
    // choice the player must make (HELD); that one and every later one stay pending.
    async saveAll() {
      let saved = 0;
      for (const [key, e] of [...entries]) {
        let outcome;
        try { outcome = await e.save(e.value); }
        catch (err) { return { saved, stopped: { key, label: e.label, reason: message(err) } }; }
        if (outcome === HELD) return { saved, stopped: { key, label: e.label, reason: HELD } };
        entries.delete(key);
        saved++;
      }
      return { saved, stopped: null };
    },
    // Put every previewed change back, newest first. Nothing is saved. Returns what could not be put back.
    async discardAll() {
      const failed = [];
      for (const [, e] of [...entries].reverse()) {
        if (!e.revert) continue;
        try { await e.revert(); } catch (err) { failed.push(`${e.label}: ${message(err)}`); }
      }
      entries.clear();
      return failed;
    },
  };
}

function message(e) {
  if (e === null || e === undefined) return "unknown failure";
  if (typeof e === "string") return e;
  return e.message || JSON.stringify(e);
}
