// The name a player is shown by (nicknames, backend migration 026): the nickname they chose, else their Discord name.
//
// The server keeps `display_name` = the Discord name everywhere (so launcher 3.0.33 and older keep working) and adds
// `nickname` (null when unset). Both are read here and nowhere else, so every page applies one rule. The server's own
// `name` (nickname ?? display_name) is deliberately not trusted: a few older objects already carry a `name` that means
// something else, and the two raw fields give the same answer.

const str = (v) => (typeof v === 'string' ? v.trim() : '');

/** What to call this player: their nickname when set, else their Discord name, else ''. */
export function shownName(p) {
  if (!p || typeof p !== 'object') return '';
  return str(p.nickname) || str(p.display_name);
}

/** The Discord name to show beside a nickname (a subtitle or tooltip), or null when there is nothing to add. */
export function discordName(p) {
  if (!p || typeof p !== 'object') return null;
  const n = str(p.nickname), d = str(p.display_name);
  return n && d && n.toLowerCase() !== d.toLowerCase() ? d : null;
}

/** "Nickname (@discord)" for an accessible label or a tooltip, or just the name. */
export function nameWithDiscord(p, fallback = 'Player') {
  const n = shownName(p) || fallback, d = discordName(p);
  return d ? `${n} (@${d})` : n;
}

/** The rule the Settings field shows under it, word for word what the server enforces. */
export const NICKNAME_HINT = "2-20 characters: letters, numbers, spaces and _ - . ' . Leave blank to use your Discord name. You can change it once a day.";
