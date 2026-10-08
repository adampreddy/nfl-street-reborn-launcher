// A player ACCOUNT's name, anywhere in the launcher, opens their profile in the launcher (owner 10-08: "click on anyones
// name u can see their profile within the launcher"). Not an NFL roster player: those have no account and no profile.
//
// The profile is profile.html?player=<lowercase uuid> (profile.js reads exactly that). The link is a plain <a href>, so
// the mouse, Enter, and a controller's A (app-shell.js reaches every a[href]) all open it, and the profile's Back (or
// B / Esc, or the mouse's back button) returns to the page it was opened from (profile.js steps back in history when it
// was opened from a launcher page). A name with no account id (an older server, the CPU, a guest) stays plain text.
// Server text only through el()'s `text` / text nodes, never markup (shell.js).
import { el } from './shell.js';

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

/** A player id as the profile page takes it (a lowercase UUID), or null for anything else. */
export function playerIdOf(v) {
  const s = typeof v === 'string' ? v.trim().toLowerCase() : '';
  return UUID_RE.test(s) ? s : null;
}

/** The in-launcher profile of `id` (a valid player id; see playerIdOf). */
export function profileHref(id) {
  return `profile.html?player=${encodeURIComponent(id)}`;
}

/**
 * `kids` (a string or nodes) as a link to the profile of player `id`, or, with no usable id, inside a plain `tag`.
 * `cls` goes on either, so the name looks the same linked or not; `key` is the link's data-key (pages keep focus by it).
 * @param {unknown} id
 * @param {string|Node|Array<string|Node|null>} kids
 * @param {{ cls?: string, key?: string|null, title?: string|null, label?: string|null, tag?: string }} [opts]
 */
export function playerLink(id, kids, { cls = '', key = null, title = null, label = null, tag = 'span' } = {}) {
  const pid = playerIdOf(id);
  // A plain name is the element's text (textContent); nodes are its children.
  const text = typeof kids === 'string' ? kids : null;
  const content = text !== null ? [] : (Array.isArray(kids) ? kids : [kids]).filter((k) => k !== null && k !== undefined);
  if (!pid) return el(tag, { class: cls || null, title, text }, ...content);
  return el('a', {
    class: `player-link${cls ? ` ${cls}` : ''}`, href: profileHref(pid), 'data-key': key || `player-${pid}`, title,
    'aria-label': label, text,
  }, ...content);
}
