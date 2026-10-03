// Vendored from Reborn Backend src/web/live-render.js @ca3fdf5, do not edit: run Tools/sync-site-render.mjs.
// The launcher game page (game.js) draws with it exactly as the site does.
// Pure render helpers for the live game page (/live/<match>). Plain data in, DOM nodes out -- built ONLY with
// doc.createElement, textContent, className, setAttribute and appendChild. Never markup strings: every name and every
// summary.text is a player's untrusted copy. `doc` is a parameter so tests can hand in a minimal fake document.

import { liveCard, finalCard, playRows, fillSeats, sideColours, teamFor, bigPortrait, FIXED, palette, bandStops, seriesLine, seriesOutcome, NEUTRAL, teamColour, situationLine } from "./live-card-core.js";
import { seatTeams, seatColours, scoreBySeat, stripStatus, lineScore, seriesPill, newestOf, gameIsLive, sideOfSeat, driveList, scoringDrives, DRIVE_END_WORDS, spotWords, fieldFromBody, boxScore, teamStats } from "./site-model.js";

// The core's team colour, for live-index.js (it imports this module only, so its import stays one versioned line).
export { teamColour, fieldFromBody };

const AVATAR_PREFIX = "https://cdn.discordapp.com/avatars/";
// Team logos and portraits: only our own same-origin catalog paths (src/liveAssets.js), under the catalog version
// (final review I4: /live-assets/r/<release>.<hash>/...), nothing else.
const LOGO_RE = /^\/live-assets\/r\/[A-Za-z0-9.-]+\/teams\/\d+-(128|256)\.png$/;
const PORTRAIT_RE = /^\/live-assets\/r\/[A-Za-z0-9.-]+\/portraits\/\d+-(96|192)\.png$/;
const DASH = "-";   // hyphens only (site restyle spec §11), even in the legacy renderers
const BIG_YARDS = 20;

function el(doc, tag, className, text) {
  const n = doc.createElement(tag);
  if (className) n.className = className;
  if (text !== undefined && text !== null) n.textContent = String(text);
  return n;
}

const num = (v) => (Number.isFinite(v) ? v : 0);
export const seqOf = (p) => (Number.isInteger(p?.seq) ? p.seq : p?.body?.seq);

const bit = (v) => v === 0 || v === 1;

/**
 * Which CAPTURE team each play SIDE is: `{ team: [captureIndexOfSide0, captureIndexOfSide1], key }`.
 *
 * A finished game carries two orders. `game.teams` (names, scores), `participants[].team_index` and the stored box
 * lines' `team_index` are CAPTURE order: the fork writes teams[] off the Game Stats screen and every team_index is
 * `side_to_team[side]` into it. The plays, drives, `teams_view` and the header are play SIDE order (the entity side,
 * the controller port's side). The two differ whenever the fork's `side_to_team` is not the identity -- its
 * measured default is side s -> teams[1 - s]. `key` says how the map was settled:
 *   * "players" -- the capture's own players, each carrying `side` AND `team_index`: the fork's own side_to_team,
 *                  the same map it applied to the seats' team_index. Exact.
 *   * "names"   -- each side's team name (the capture players' `team` by side, else `teams_view[side].name`) found
 *                  among the capture's team names.
 *   * "order"   -- nothing links them (an old capture with no side on its players, a mirror match with no players):
 *                  capture order is taken as side order, which is what every builder did before.
 */
export function sideTeams(game, view = null) {
  const pairs = [null, null];
  let clash = false;
  for (const p of Array.isArray(game?.players) ? game.players : []) {
    if (!p || !bit(p.side) || !bit(p.team_index)) continue;
    if (pairs[p.side] === null) pairs[p.side] = p.team_index;
    else if (pairs[p.side] !== p.team_index) clash = true;
  }
  const settle = (t) => {
    if (t[0] === null && t[1] === null) return null;
    const out = [t[0] ?? (t[1] === null ? null : 1 - t[1]), t[1] ?? 1 - t[0]];
    return out[0] !== out[1] ? out : null;
  };
  const byPlayers = clash ? null : settle(pairs);
  if (byPlayers) return { team: byPlayers, key: "players" };

  const capture = [0, 1].map((i) => (typeof game?.teams?.[i]?.name === "string" ? game.teams[i].name : null));
  if (capture[0] !== null && capture[1] !== null && capture[0] !== capture[1]) {
    const named = [null, null];
    for (const p of Array.isArray(game?.players) ? game.players : [])
      if (p && bit(p.side) && typeof p.team === "string" && p.team && named[p.side] === null) named[p.side] = p.team;
    const v = Array.isArray(view) ? view : [];
    const found = [0, 1].map((side) => {
      const name = named[side] ?? (typeof v[side]?.name === "string" ? v[side].name : null);
      const i = name === null ? -1 : capture.indexOf(name);
      return i === -1 ? null : i;
    });
    const byNames = settle(found);
    if (byNames) return { team: byNames, key: "names" };
  }
  return { team: [0, 1], key: "order" };
}

/** A capture-order team_index as a play side under `team` (sideTeams(...).team); null stays null. */
const sideOfTeam = (team, t) => (bit(t) ? team.indexOf(t) : null);

/** The header's seats and sides from a finished game's participants: for each side, the first human with a
 *  player account on that side becomes a seat `{ display_name, avatar_url }` and `sides[side] = { seat }`.
 *  A participant's `team_index` is CAPTURE order; `team` (sideTeams(...).team, default the identity) says which
 *  capture team each side is. A side with no such human (CPU, a guest, an unattributed seat) has `{ seat: null }`.
 *  No attributed human at all: no seats and no sides, the renderer's "sides unknown" layout. */
export function participantSeats(participants, team = [0, 1]) {
  const list = Array.isArray(participants) ? participants : [];
  const seats = [];
  const sides = [0, 1].map((side) => {
    const p = list.find((x) => x && x.kind === "human" && x.player_id && bit(x.team_index) && x.team_index === team[side]);
    if (!p) return { seat: null };
    seats.push({ display_name: typeof p.display_name === "string" ? p.display_name : "", avatar_url: typeof p.avatar_url === "string" ? p.avatar_url : null });
    return { seat: seats.length - 1 };
  });
  return seats.length ? { seats, sides } : { seats: [], sides: [] };
}

/** The snapshot shape this renderer reads, built from one FINISHED game (the /games/:id page and the launcher's
 *  native game view both build it), all in play SIDE order: team identity from the plays' own `teams` when there
 *  are plays, logo and colours from the server's `game.teams_view` when it has one (else the capture's names with no
 *  logo); the capture's names, scores, seats and box lines moved from capture order onto sides by `sideTeams`. */
export function gameSnapshot(game, plays, lines, drives) {
  const fromPlays = (plays ?? []).find((p) => Array.isArray(p.body?.teams) && p.body.teams.length === 2)?.body.teams;
  const view = Array.isArray(game.teams_view) ? game.teams_view : [];
  const { team } = sideTeams(game, view);
  const teams = [0, 1].map((side) => {
    const tv = view[side] && typeof view[side] === "object" ? view[side] : {};
    const cap = game.teams?.[team[side]];
    const tgid = Number.isInteger(fromPlays?.[side]?.tgid) ? fromPlays[side].tgid : Number.isInteger(tv.tgid) ? tv.tgid : null;
    return {
      side,
      tgid,
      tmc: fromPlays?.[side]?.tmc ?? null,
      name: cap?.name ?? tv.name ?? `Side ${side + 1}`,
      abbrev: cap?.abbrev ?? tv.abbrev ?? null,
      primary: tv.primary ?? null,
      secondary: tv.secondary ?? null,
      logo_url: tv.logo_url ?? null,
    };
  });
  const score = [0, 1].map((side) => (Number.isFinite(game.teams?.[team[side]]?.score) ? game.teams[team[side]].score : 0));
  const { seats, sides } = participantSeats(game.participants, team);
  // The stored box lines' team_index is capture order too; the renderers read it as the side.
  const box = (lines ?? []).map((l) => (l && typeof l === "object" ? { ...l, team_index: sideOfTeam(team, l.team_index) } : l));
  return {
    // `single`: /v1/games/<id> carries no game_seq, so the page names no game number (no "Game 1", no G1 column).
    match: { status: "complete", best_of: 1, single: true },
    seats,
    games: [{ game_seq: 1, game_id: game.id, score, teams, sides, plays: plays ?? [], box: { provisional: false, lines: box }, drives: drives ?? [] }],
  };
}

const GAME_ID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
/** The permanent page of a recorded game, or null while it has none. */
export function gamePageHref(game) {
  const id = typeof game?.game_id === "string" ? game.game_id.toLowerCase() : "";
  return GAME_ID_RE.test(id) ? `/games/${id}` : null;
}

/** The newest game (highest game_seq) of a snapshot, or null. */
export function newestGame(snapshot) {
  let top = null;
  for (const g of snapshot?.games ?? []) if (!top || g.game_seq > top.game_seq) top = g;
  return top;
}

/** The newest play (highest seq) of a game, or null. */
export function newestPlay(game) {
  let top = null;
  for (const p of game?.plays ?? []) if (!top || seqOf(p) > seqOf(top)) top = p;
  return top;
}

/**
 * Mirrors src/pbp/safety.js's `safetyOf` (duplicated, not imported: this file ships to the browser as one
 * dependency-free module, served byte-for-byte from webRouter.js's fixed asset list). A safety is 2 points to the
 * defense, read from the game's own `tsst` stat -- the decoder can record the play itself as an ordinary 0-point
 * sack -- or from a newer decoder's `summary.safety`+`summary.points`. Never both for the same play.
 */
function safetyOf(body) {
  for (const c of Array.isArray(body?.stat_changes) ? body.stat_changes : []) {
    if (c && c.team && c.field === "tsst" && (c.who === 0 || c.who === 1)
        && Number.isFinite(c.before) && Number.isFinite(c.after) && c.after > c.before) {
      return { side: c.who, points: 2 };
    }
  }
  const s = body?.summary;
  if (s && s.safety === true && Number.isFinite(s.points) && s.points > 0) {
    const off = body?.offense_side;
    if (off === 0 || off === 1) return { side: 1 - off, points: s.points };
  }
  return null;
}

/**
 * Re-derives every play's `score_after` and the game's `score` from the whole play list, in seq order -- the same
 * rule as the server's pbp/livegame.js `scoreAfter` (points go to the offense, or to the defense on a turnover or
 * a safety).
 * Plays can arrive out of order, so a frame's own score_after may have been computed without a play that came
 * later; the game's score is therefore always the running total at the HIGHEST seq, never the last frame's.
 * A DISPUTED play's points do not count (ruling #2, the server's rule too); the play keeps the score before it.
 */
export function rescore(game) {
  const plays = [...(game?.plays ?? [])].filter((p) => Number.isInteger(seqOf(p))).sort((a, b) => seqOf(a) - seqOf(b));
  const score = [0, 0];
  for (const p of plays) {
    const b = p.body ?? {};
    const s = b.summary ?? {};
    const side = b.offense_side === 0 || b.offense_side === 1 ? b.offense_side : null;
    const game = Array.isArray(b.score) && b.score.length === 2
      && b.score.every((v) => Number.isSafeInteger(v) && v >= 0 && v <= 999) ? b.score : null;
    if (p.status !== "disputed" && game) {
      // The game's own score (runtime 1.2.11+) sets it outright, as the server's livegame.js gameScoreOf does.
      score[0] = game[0]; score[1] = game[1];
    } else if (p.status !== "disputed") {
      const safety = safetyOf(b);   // a safety scores for the defense only, never also as the offense's points
      if (!safety && side !== null && Number.isFinite(s.points) && s.points > 0) score[s.turnover ? 1 - side : side] += s.points;
      if (safety) score[safety.side] += safety.points;
    }
    p.score_after = [score[0], score[1]];
  }
  if (game) game.score = [score[0], score[1]];
  return game;
}

/**
 * Upserts one stream frame into a snapshot by (game_seq, seq) and rescores that game. Returns true when the frame
 * opened a game the snapshot did not have yet (the caller then refetches the snapshot for its sides and drives).
 */
export function upsertPlay(snapshot, frame, rev) {
  if (!snapshot || !Number.isInteger(frame?.game_seq)) return false;
  const seq = seqOf(frame);
  if (!Number.isInteger(seq)) return false;
  if (!Array.isArray(snapshot.games)) snapshot.games = [];
  let game = snapshot.games.find((g) => g.game_seq === frame.game_seq);
  let fresh = false;
  if (!game) {
    game = { game_seq: frame.game_seq, game_id: null, sides: null, score: [0, 0], drives: [], box: { provisional: true, lines: [] }, plays: [] };
    snapshot.games.push(game);
    fresh = true;
  }
  const entry = { rev, status: frame.status, body: frame.body, situation: frame.situation ?? null, field: frame.field ?? null,
    score_after: frame.score_after ?? null };
  const i = game.plays.findIndex((p) => seqOf(p) === seq);
  if (i >= 0) game.plays[i] = entry; else game.plays.push(entry);
  rescore(game);
  return fresh;
}

/** Which side each seat has: `[sideOfSeat0, sideOfSeat1]`, each 0|1|null. */
export function sideOfSeats(sides) {
  const out = [null, null];
  (Array.isArray(sides) ? sides : []).forEach((s, side) => { if (s && (s.seat === 0 || s.seat === 1) && side < 2) out[s.seat] = side; });
  return out;
}

/**
 * Who sits in each header box (09-30, Adam: no false "CPU"). `sides` is by side (`[{ seat }, { seat }]`), `seatCount`
 * the number of human seats. Returns `{ boxSide, seatOfBox, confirmed }`: boxSide[i] is the SIDE box i shows (null:
 * none claimed), seatOfBox[i] the seat under it (null: the CPU). The rule, mirrored in the launcher's
 * live-game-render.js and the Live bot's model.js:
 *   * a box is the CPU only when there are fewer human seats than sides -- a matchmade game (two seats) has none;
 *   * a seat whose side is known keeps its seat-number position on that side; with exactly one known and two seats,
 *     the other seat has the other side (pigeonhole);
 *   * with neither known, the seats show by position with no side claimed and `confirmed` false.
 * No sides array at all (pre-ports data): the legacy raw-position layout, `confirmed` true (nothing was claimed).
 */
export function seatLayout(sides, seatCount) {
  const humans = Math.max(0, Math.min(2, Number.isInteger(seatCount) ? seatCount : 0));
  if (!Array.isArray(sides) || !sides.length) return { boxSide: [null, null], seatOfBox: [0, 1], confirmed: true };
  const sideOf = sideOfSeats(sides);
  const known = [0, 1].filter((i) => sideOf[i] === 0 || sideOf[i] === 1);
  const human = (i) => (i < humans ? i : null);
  if (!known.length) {
    if (!humans) return { boxSide: [0, 1], seatOfBox: [null, null], confirmed: true };
    return { boxSide: [null, null], seatOfBox: [human(0), human(1)], confirmed: false };
  }
  if (known.length === 1) {
    const k = known[0];
    const boxSide = [null, null];
    const seatOfBox = [null, null];
    boxSide[k] = sideOf[k];
    boxSide[1 - k] = 1 - sideOf[k];
    seatOfBox[k] = k;
    seatOfBox[1 - k] = humans >= 2 ? 1 - k : null;
    return { boxSide, seatOfBox, confirmed: true };
  }
  return { boxSide: [sideOf[0], sideOf[1]], seatOfBox: [0, 1], confirmed: true };
}

/** A by-side score as `[seat0, seat1]`, or null when the sides are not known yet. */
export function seatScore(score, sides) {
  if (!Array.isArray(score)) return null;
  const [s0, s1] = sideOfSeats(sides);
  if (s0 === null || s1 === null) return null;
  return [num(score[s0]), num(score[s1])];
}

/** The display name of whoever has `side`, or "Side N". */
function sideName(side, seats, sides) {
  const seat = Array.isArray(sides) ? sides[side]?.seat : null;
  const name = (seat === 0 || seat === 1) ? seats?.[seat]?.display_name : null;
  return name || (side === 0 || side === 1 ? `Side ${side + 1}` : "Unknown");
}

const STATUS_WORD = { running: "Live", complete: "Final", held: "Under review", abandoned: "Abandoned", void: "Void", pairing: "Starting", lobby: "Starting" };

const initialOf = (name) => { const s = String(name ?? ""); return s ? Array.from(s)[0].toUpperCase() : ""; };

/** The letter badge an avatar falls back to: no avatar, or one whose image failed to load. */
export function letterBadge(doc, initial) {
  return el(doc, "span", "avatar blank", initial ? String(initial) : "?");
}

/**
 * A seat's avatar: the Discord image (only a Discord CDN URL, and not one already known to fail), else the letter
 * badge. The <img> carries `data-initial`, so the page controller's `error` listener can swap in the badge.
 */
function avatar(doc, seat, badAvatars) {
  const url = typeof seat?.avatar_url === "string" ? seat.avatar_url : "";
  const initial = initialOf(seat?.display_name);
  if (url.startsWith(AVATAR_PREFIX) && !badAvatars?.has(url)) {
    const img = el(doc, "img", "avatar");
    img.setAttribute("src", url);
    img.setAttribute("alt", "");
    img.setAttribute("width", "48");
    img.setAttribute("height", "48");
    img.setAttribute("data-initial", initial);
    return img;
  }
  return letterBadge(doc, initial);
}

/** A known team for a side of `game` (one with a name), else null. */
function teamOf(game, side) {
  const t = side === 0 || side === 1 ? game?.teams?.[side] : null;
  return t && typeof t.name === "string" && t.name ? t : null;
}

/** The team's abbrev badge, in its colour (teams.css paints `.teambadge` inside `.tg-<tgid>` with its ink). */
export function teamBadge(doc, abbrev) {
  return el(doc, "span", "teambadge", abbrev ? String(abbrev) : "?");
}

/** The team logo, or its abbrev badge when there is no usable logo (not ours, or already known to fail). */
function teamLogo(doc, team, badImages) {
  const url = typeof team.logo_url === "string" ? team.logo_url : "";
  if (LOGO_RE.test(url) && !badImages?.has(url)) {
    const img = el(doc, "img", "logo");
    img.setAttribute("src", url);
    img.setAttribute("alt", "");
    img.setAttribute("width", "56");
    img.setAttribute("height", "56");
    img.setAttribute("data-abbrev", String(team.abbrev ?? ""));
    return img;
  }
  return teamBadge(doc, team.abbrev);
}

/** What the header says about `game`: Live (the newest game of a running match, not yet recorded) or Final. */
function gameStatus(snapshot, game) {
  const status = String(snapshot?.match?.status ?? "");
  const newest = newestGame(snapshot);
  if (game && (game.game_id || (newest && game.game_seq < newest.game_seq))) return { word: "Final", live: false };
  return { word: STATUS_WORD[status] ?? status, live: status === "running" };
}

/**
 * The scoreboard for ONE game -- `game`, the one the viewer picked (the newest by default). Laid out BY SIDE: side 0
 * and side 1 each always get a box, from `game.teams` and `game.score` (by side) -- a side is never dropped just
 * because it has no controller. A box's Discord seat is whichever seat maps to that side (`sides[side].seat`); a
 * side with no seat shows "CPU" only when the match really has one (fewer human seats than sides); in a two-seat
 * game an unknown side takes the other seat, and with neither side known both seats show by position under a
 * "Sides not confirmed" note (seatLayout) -- never another side's seat. When `game.sides` names an actual seat for one of the two boxes, that box keeps its
 * historical seat-number position (so a seat's box does not jump left/right mid-match); a side left without a
 * claimed seat fills whatever position remains. With no `sides` array at all (pre-ports data), boxes fall back to
 * the snapshot's seats by raw position, exactly as before ports existed.
 * `badAvatars` / `badImages` are Sets of URLs that already failed to load in this page: those render as badges.
 */
export function renderHeader(snapshot, doc, { badAvatars = null, badImages = null, game: picked = null } = {}) {
  const match = snapshot?.match ?? {};
  const seats = snapshot?.seats ?? [];
  const game = picked ?? newestGame(snapshot);
  const root = el(doc, "div", "scoreboard");
  const row = el(doc, "div", "teams");
  const sideScore = Array.isArray(game?.score) ? [num(game.score[0]), num(game.score[1])] : null;
  // Something to show: a play, or a score (a recorded game with no live plays still has its final score).
  const hasPlays = (game?.plays?.length ?? 0) > 0 || Boolean(sideScore && (sideScore[0] || sideScore[1]));
  const sides = Array.isArray(game?.sides) ? game.sides : null;
  const hasSides = Boolean(sides && sides.length);
  // boxSide[i]: which SIDE the box at position i shows (null: no side claimed). seatOfBox[i]: which seats[] index
  // sits under it, or null for CPU. See seatLayout.
  const { boxSide, seatOfBox, confirmed } = seatLayout(sides, seats.length);
  const anySeatKnown = hasSides && [0, 1].some((i) => (boxSide[i] === 0 || boxSide[i] === 1) && seatOfBox[i] !== null);
  [0, 1].forEach((i) => {
    const side = boxSide[i];
    const team = teamOf(game, side);
    const tg = team && Number.isInteger(team.tgid) && team.tgid >= 0 ? ` tg-${team.tgid}` : "";
    const box = el(doc, "div", `team seat${i}${team ? " has-team" : ""}${tg}`);
    const who = el(doc, "div", "ident");
    if (team) {
      box.appendChild(el(doc, "span", "team-bar"));
      const id = el(doc, "div", "team-id");
      id.appendChild(teamLogo(doc, team, badImages));
      id.appendChild(el(doc, "span", "team-name", team.name));
      who.appendChild(id);
    }
    const seatIdx = seatOfBox[i];
    const seatBox = el(doc, "div", "seat-id");
    if (seatIdx === 0 || seatIdx === 1) {
      seatBox.appendChild(avatar(doc, seats[seatIdx], badAvatars));
      seatBox.appendChild(el(doc, "span", "name", seats[seatIdx]?.display_name ?? ""));
    } else {
      seatBox.appendChild(el(doc, "span", "name cpu", "CPU"));
    }
    who.appendChild(seatBox);
    box.appendChild(who);
    const points = hasPlays ? (anySeatKnown && sideScore ? sideScore[side] : DASH) : 0;
    box.appendChild(el(doc, "span", "points", points));
    row.appendChild(box);
  });
  root.appendChild(row);
  if (!confirmed) root.appendChild(el(doc, "div", "sides-note", "Sides not confirmed"));
  // Final review M3: no side has a known seat (an older runtime sends no ports, or neither side has a controller),
  // so no seat can be credited -- but the score by SIDE is still known: "Side 1 6 - 3 Side 2", or the sides' team
  // names when those are known. As soon as ANY side has a known seat, its box already carries the real score, so
  // this extra line does not repeat it (that used to duplicate the score on its own line for a mixed CPU game).
  if (!anySeatKnown && sideScore && hasPlays) {
    const label = (side) => teamOf(game, side)?.name ?? `Side ${side + 1}`;
    root.appendChild(el(doc, "div", "side-score", `${label(0)} ${sideScore[0]} ${DASH} ${sideScore[1]} ${label(1)}`));
  }

  const meta = el(doc, "div", "meta");
  const st = gameStatus(snapshot, game);
  meta.appendChild(el(doc, "span", `status ${st.live ? "live" : "done"}`, st.word));
  const live = seriesOutcome(cardInput(snapshot)).wins;
  if (match.mode === "ranked") {
    meta.appendChild(el(doc, "span", "game-no", `Game ${game?.game_seq ?? 1}`));
    meta.appendChild(el(doc, "span", "series", `Series ${num(live[0])}-${num(live[1])}`));   // hyphen, like the card text
  } else if ((snapshot.games ?? []).length > 1) {
    meta.appendChild(el(doc, "span", "game-no", `Game ${game?.game_seq ?? 1}`));
    meta.appendChild(el(doc, "span", "series", `${num(live[0])}-${num(live[1])}`));
  }
  if (match.mode) meta.appendChild(el(doc, "span", "mode", match.mode === "ranked" ? "Ranked" : match.mode === "casual" ? "Casual" : String(match.mode)));
  root.appendChild(meta);

  // A disputed play never sets the scoreboard's situation, the same as it never sets the score.
  const sit = newestPlay({ plays: (game?.plays ?? []).filter((p) => p.status !== "disputed") })?.situation;
  if (sit && st.live) root.appendChild(el(doc, "div", "situation", sit));
  return root;
}

/**
 * One line naming only the newest play of the newest game, for the page's polite live region (a screen reader then
 * hears each new play once, never the whole rebuilt scoreboard). "" before any play.
 */
export function newestPlayLine(snapshot) {
  const game = newestGame(snapshot);
  const p = newestPlay(game);
  if (!p) return "";
  const s = p.body?.summary ?? {};
  const text = typeof s.text === "string" && s.text ? s.text : (s.kind ? String(s.kind) : "Play");
  return `Game ${game.game_seq}, play ${seqOf(p)}: ${text}${p.status === "disputed" ? " (disputed)" : ""}`;
}

/**
 * When the page may ask again whether a recorded game is verified: an answer of "not yet" is kept until a new game
 * appears or the match's status changes (each check is a /v1 request).
 */
export function recheckKey(snapshot) {
  let top = 0;
  for (const g of snapshot?.games ?? []) if (Number.isInteger(g?.game_seq) && g.game_seq > top) top = g.game_seq;
  return `${snapshot?.match?.status ?? ""}|${top}`;
}

/**
 * Whether the 30-second tick refetches the snapshot (final review I2): not while the stream is OPEN -- it delivers
 * every play, and a new game_seq or the stream closing refetches at once anyway. With no stream, or one that is
 * reconnecting (CONNECTING) or closed, the tick is the page's only way to learn anything.
 */
export function refetchOnTick(es) {
  return !(es && es.readyState === 1);
}

/** DIGP -> portrait URL from box-score lines that carry one (the live box's lines do). */
export function portraitMap(lines) {
  const m = new Map();
  for (const l of Array.isArray(lines) ? lines : [])
    if (l && Number.isInteger(l.digp) && typeof l.portrait_url === "string" && PORTRAIT_RE.test(l.portrait_url)) m.set(l.digp, l.portrait_url);
  return m;
}

/** A small portrait <img> for a same-origin portrait URL, or null. `alt` is empty: the text beside it names them. */
function face(doc, url, className, size, badImages) {
  if (typeof url !== "string" || !PORTRAIT_RE.test(url) || badImages?.has(url)) return null;
  const img = el(doc, "img", className);
  img.setAttribute("src", url);
  img.setAttribute("alt", "");
  img.setAttribute("width", String(size));
  img.setAttribute("height", String(size));
  return img;
}

const isBig = (s) => s.touchdown === true || s.turnover === true || (s.yards_known === true && Number.isFinite(s.yards) && s.yards >= BIG_YARDS);
/** The player a big play is about: the receiver, else the rusher, else the passer (DIGP), or null. */
function starOf(s) {
  for (const d of [s.receiver, s.rusher, s.passer]) if (Number.isInteger(d) && d >= 0) return d;
  return null;
}

/**
 * A game's plays, newest first. Disputed plays carry a badge and NO score styling (their points do not count);
 * scores and turnovers are highlighted. A BIG play (TD, turnover, 20+ yards) shows its receiver's / rusher's /
 * passer's portrait when the box score has one.
 */
export function renderPlays(game, doc, { badImages = null } = {}) {
  const plays = [...(game?.plays ?? [])].sort((a, b) => seqOf(b) - seqOf(a));
  // A recorded game can have no live plays at all (final review I3): its box score and drives come from the record.
  if (!plays.length) return el(doc, "p", "empty", game?.game_id ? "No live plays reached us for this game." : "No plays yet.");
  const faces = portraitMap(game?.box?.lines);
  const list = el(doc, "ol", "plays");
  for (const p of plays) {
    const s = p?.body?.summary ?? {};
    const disputed = p.status === "disputed";
    const scored = s.touchdown === true || (Number.isFinite(s.points) && s.points > 0);
    const cls = ["play"];
    if (scored && !disputed) cls.push("score");
    if (s.turnover) cls.push("turnover");
    if (disputed) cls.push("disputed");
    const star = isBig(s) ? face(doc, faces.get(starOf(s)), "portrait", 48, badImages) : null;
    if (star) cls.push("has-portrait");
    const li = el(doc, "li", cls.join(" "));
    if (star) li.appendChild(star);

    const top = el(doc, "div", "play-top");
    top.appendChild(el(doc, "span", "seq", `#${seqOf(p)}`));
    if (p.situation) top.appendChild(el(doc, "span", "sit", p.situation));
    const after = seatScore(p.score_after, game?.sides);
    if (after) top.appendChild(el(doc, "span", "after", `${after[0]}${DASH}${after[1]}`));
    li.appendChild(top);

    const text = typeof s.text === "string" && s.text ? s.text : (s.kind ? String(s.kind) : "Play");
    li.appendChild(el(doc, "div", "text", text));
    if (p.status === "disputed") li.appendChild(el(doc, "span", "badge disputed", "disputed"));
    list.appendChild(li);
  }
  return list;
}

const TABLES = [
  { title: "Passing", has: (s) => num(s.pass_att) > 0,
    cols: [["C/ATT", (s) => `${num(s.pass_cmp)}/${num(s.pass_att)}`], ["YDS", (s) => num(s.pass_yds)], ["TD", (s) => num(s.pass_td)],
      ["INT", (s) => num(s.pass_int)], ["RTG", (s) => (Number.isFinite(s.pass_rtg) ? s.pass_rtg : DASH)]],
    sort: (s) => num(s.pass_yds) },
  // A player shows here on a carry OR a broken tackle OR yards after contact -- a decoder can log the latter
  // two on a snap with zero carries (e.g. a lateral recipient). AVG is rush_ypc, "-" at 0 carries, never 0/0.
  { title: "Rushing", has: (s) => num(s.rush_att) > 0 || num(s.broken_tackles) > 0 || num(s.yds_after_contact) > 0,
    cols: [["ATT", (s) => num(s.rush_att)], ["YDS", (s) => num(s.rush_yds)], ["AVG", (s) => (Number.isFinite(s.rush_ypc) ? s.rush_ypc : DASH)],
      ["TD", (s) => num(s.rush_td)], ["LONG", (s) => (Number.isFinite(s.rush_long) ? s.rush_long : DASH)],
      ["BTK", (s) => num(s.broken_tackles)], ["YCO", (s) => num(s.yds_after_contact)]],
    sort: (s) => num(s.rush_yds) },
  // A player shows here on a reception OR a drop.
  { title: "Receiving", has: (s) => num(s.rec) > 0 || num(s.drops) > 0,
    cols: [["REC", (s) => num(s.rec)], ["YDS", (s) => num(s.rec_yds)], ["TD", (s) => num(s.rec_td)],
      ["LONG", (s) => (Number.isFinite(s.rec_long) ? s.rec_long : DASH)], ["DROP", (s) => num(s.drops)]],
    sort: (s) => num(s.rec_yds) },
  { title: "Defense", has: (s) => num(s.tackles) + num(s.sacks) + num(s.def_int) + num(s.ff) + num(s.fr) > 0,
    cols: [["TKL", (s) => num(s.tackles)], ["SCK", (s) => num(s.sacks)], ["INT", (s) => num(s.def_int)], ["FF", (s) => num(s.ff)],
      ["PBU", (s) => num(s.pass_breakups)]],
    sort: (s) => num(s.tackles) + num(s.sacks) * 2 + num(s.def_int) * 3 },
];

/**
 * Passing / rushing / receiving / defense tables from box-score lines ({ name, team_index, stats }). `teams`, when
 * given, is a label per team_index and adds a team column. A row shows a small portrait from its own `portrait_url`
 * or, for verified lines (which carry none), from `portraits` (DIGP -> URL, the live box's).
 */
export function renderBox(lines, doc, teams = null, { portraits = null, badImages = null } = {}) {
  const list = (Array.isArray(lines) ? lines : []).filter((l) => l && l.stats && typeof l.stats === "object");
  const root = el(doc, "div", "box");
  let any = false;
  for (const t of TABLES) {
    const rows = list.filter((l) => t.has(l.stats))
      .sort((a, b) => num(a.team_index) - num(b.team_index) || t.sort(b.stats) - t.sort(a.stats));
    if (!rows.length) continue;
    any = true;
    const table = el(doc, "table", `stat ${t.title.toLowerCase()}`);
    table.appendChild(el(doc, "caption", null, t.title));
    const thead = el(doc, "thead");
    const hr = el(doc, "tr");
    hr.appendChild(el(doc, "th", "who", "Player"));
    if (teams) hr.appendChild(el(doc, "th", "side-col", "Team"));
    for (const [h] of t.cols) hr.appendChild(el(doc, "th", null, h));
    thead.appendChild(hr);
    table.appendChild(thead);
    const tbody = el(doc, "tbody");
    for (const l of rows) {
      const tr = el(doc, "tr", `t${l.team_index === 1 ? 1 : 0}`);
      const img = face(doc, l.portrait_url ?? portraits?.get(l.digp), "face", 28, badImages);
      if (img) {
        const td = el(doc, "td", "who with-face");
        td.appendChild(img);
        td.appendChild(el(doc, "span", "who-name", l.name ?? ""));
        tr.appendChild(td);
      } else tr.appendChild(el(doc, "td", "who", l.name ?? ""));
      if (teams) tr.appendChild(el(doc, "td", "side-col", teams[l.team_index] ?? ""));
      for (const [, f] of t.cols) tr.appendChild(el(doc, "td", null, f(l.stats)));
      tbody.appendChild(tr);
    }
    table.appendChild(tbody);
    root.appendChild(table);
  }
  if (!any) root.appendChild(el(doc, "p", "empty", "No stats yet."));
  return root;
}

const RESULT_WORD = { touchdown: "Touchdown", turnover: "Turnover", downs: "Downs", end: "End of game" };

/** Legacy drive table (unused by the restyled pages, R14): who had the ball, plays, yards and how it ended. */
export function renderDriveTable(drives, seats, sides, doc) {
  const list = Array.isArray(drives) ? drives : [];
  if (!list.length) return el(doc, "p", "empty", "No drives yet.");
  const table = el(doc, "table", "stat drives");
  table.appendChild(el(doc, "caption", null, list.some((d) => d?.provisional) ? "Drives (provisional)" : "Drives"));
  const thead = el(doc, "thead");
  const hr = el(doc, "tr");
  for (const h of ["#", "Offense", "Plays", "Yards", "Result"]) hr.appendChild(el(doc, "th", h === "Offense" ? "who" : null, h));
  thead.appendChild(hr);
  table.appendChild(thead);
  const tbody = el(doc, "tbody");
  list.forEach((d, i) => {
    const tr = el(doc, "tr", d?.result === "touchdown" ? "score" : d?.result === "turnover" ? "turnover" : null);
    tr.appendChild(el(doc, "td", null, i + 1));
    tr.appendChild(el(doc, "td", "who", sideName(d?.side, seats, sides)));
    tr.appendChild(el(doc, "td", null, num(d?.plays)));
    tr.appendChild(el(doc, "td", null, num(d?.yards)));
    tr.appendChild(el(doc, "td", null, RESULT_WORD[d?.result] ?? String(d?.result ?? "")));
    tbody.appendChild(tr);
  });
  table.appendChild(tbody);
  return table;
}

// ---- the live card and the final card (the bot's PNG cards, drawn as DOM + CSS) ---------------------------------
// The numbers come from live-card-core.js (a byte copy of the bot's core), so the page and Discord never disagree.
// Never a style attribute (the page's CSP has no 'unsafe-inline'): every position and every colour is a CSS custom
// property set through CSSOM (`style.setProperty`), and live.css reads them with var(). Colours are the core's
// `#rrggbb` strings, checked again before they are set; positions are numbers the core computed.

const HEX_RE = /^#[0-9a-f]{6}$/i;
/** Sets a colour custom property, only for a `#rrggbb` value (anything else is left to the CSS fallback). */
function colourVar(n, name, hex) {
  if (typeof hex === "string" && HEX_RE.test(hex)) n.style.setProperty(name, hex.toLowerCase());
}
/** A 0..1 fraction of the track as a percentage custom property. */
function pctVar(n, name, f) {
  if (Number.isFinite(f)) n.style.setProperty(name, `${(Math.min(1, Math.max(0, f)) * 100).toFixed(2)}%`);
}
function append(parent, ...kids) {
  for (const k of kids) if (k) parent.appendChild(k);
  return parent;
}

/** The core's CardInput from a page snapshot: plays by seq, each game's seat of each side (the bot's toCardInput). */
export function cardInput(snapshot) {
  const seats = Array.isArray(snapshot?.seats) ? snapshot.seats : [];
  const games = [...(snapshot?.games ?? [])].sort((a, b) => a.game_seq - b.game_seq).map((g) => {
    const { boxSide, seatOfBox, confirmed } = seatLayout(g.sides, seats.length);
    const bySide = [0, 1].map((side) => { const box = boxSide.indexOf(side); return box === -1 ? null : seatOfBox[box]; });
    const plays = (g.plays ?? []).map((p) => ({ seq: seqOf(p), status: p.status, body: p.body, situation: p.situation ?? null,
      field: p.field ?? null, score_after: p.score_after ?? null })).filter((p) => Number.isInteger(p.seq)).sort((a, b) => a.seq - b.seq);
    return { game_seq: g.game_seq, sides: g.sides, teams: g.teams, score: g.score, box: g.box,
      seatOfSide: fillSeats(confirmed ? bySide : [0, 1].map((i) => (i < seats.length ? i : null))), plays };
  });
  return { match: snapshot?.match ?? {}, seats, games };
}

const usableLogo = (url, badImages) => typeof url === "string" && LOGO_RE.test(url) && !badImages?.has(url);

/** A card logo <img> (class "logo", so the page's error listener swaps a failed one for its badge), or null. */
function logoImg(doc, url, abbrev, badImages, extra = "") {
  if (!usableLogo(url, badImages)) return null;
  const img = el(doc, "img", `logo${extra ? ` ${extra}` : ""}`);
  img.setAttribute("src", url);
  img.setAttribute("alt", "");
  img.setAttribute("data-abbrev", String(abbrev ?? ""));
  return img;
}
/** The logo, else the abbrev badge (live.css paints a card's badge in the team colour, --tb). */
const cardLogo = (doc, url, abbrev, badImages) => logoImg(doc, url, abbrev, badImages) ?? teamBadge(doc, abbrev);

/** A card portrait (the 192 px file): class "portrait", so a failed one is simply dropped. Null when unusable. */
function cardPortrait(doc, url, className, badImages) {
  const big = bigPortrait(url);
  if (typeof big !== "string" || !PORTRAIT_RE.test(big) || badImages?.has(big)) return null;
  const img = el(doc, "img", `${className} portrait`);
  img.setAttribute("src", big);
  img.setAttribute("alt", "");
  return img;
}

function scoreSide(doc, box, which, badImages) {
  const side = el(doc, "div", `lc-side ${which}${box.trailing ? " trailing" : ""}`);
  colourVar(side, "--tb", box.colour);
  const logo = cardLogo(doc, box.logo, box.abbrev, badImages);
  const pts = el(doc, "b", "lc-pts", box.value);
  return which === "a" ? append(side, logo, pts) : append(side, pts, logo);
}

function scoreBand(doc, d, badImages) {
  const mid = append(el(doc, "div", "lc-mid"),
    el(doc, "span", d.score.status === "LIVE" ? "lc-live" : "lc-final", d.score.status), el(doc, "span", "lc-sub", d.score.sub),
    d.score.unofficial ? el(doc, "span", "lc-uo", "UNOFFICIAL") : null);
  return append(el(doc, "div", "lc-score"), scoreSide(doc, d.score.a, "a", badImages), mid, scoreSide(doc, d.score.b, "b", badImages));
}

function cardRoot(doc, kind, P) {
  const root = el(doc, "div", "lc");
  root.setAttribute("data-kind", kind);
  (kind === "final" ? P.finalBg : P.bg).forEach((c, i) => colourVar(root, `--bg${i}`, c));
  P.face.forEach((c, i) => colourVar(root, `--face${i}`, c));
  for (const k of ["badge", "soft", "muted", "label", "line"]) colourVar(root, `--${k}`, P[k]);
  return root;
}

function playBand(doc, d, badImages) {
  const p = d.play;
  const av = el(doc, "div", "lc-av");
  const face = el(doc, "div", "lc-face");
  if (p.player?.colour) palette(p.player.colour).face.forEach((c, i) => colourVar(face, `--face${i}`, c));
  append(face, cardPortrait(doc, p.player?.portrait, "lc-portrait", badImages) ?? logoImg(doc, p.logo, "", badImages, "lc-facelogo"));
  append(av, face);
  if (p.player && usableLogo(p.logo, badImages)) append(av, append(el(doc, "div", "lc-badge"), logoImg(doc, p.logo, "", badImages)));

  const txt = el(doc, "div", "lc-txt");
  if (p.result) append(txt, el(doc, "div", `lc-res${p.tone === "good" ? "" : " muted"}`, p.result));
  append(txt, el(doc, "div", "lc-hl", p.headline));
  if (p.player) {
    const l1 = append(el(doc, "div", "lc-l1"), el(doc, "span", "lc-name", p.player.name));
    for (const [v, w] of p.stats ?? []) l1.appendChild(append(el(doc, "span", "lc-seg"), el(doc, "b", null, v), w ? el(doc, "span", null, w) : null));
    append(txt, l1);
    if (p.detail) {
      const l2 = el(doc, "div", "lc-l2");
      for (const part of p.detail) l2.appendChild(el(doc, part.bold ? "b" : "span", null, part.text));
      append(txt, l2);
    }
  } else if (p.text) {
    append(txt, append(el(doc, "div", "lc-l2"), el(doc, "span", null, p.text.length > 90 ? `${p.text.slice(0, 89)}…` : p.text)));
  }
  return append(el(doc, "div", "lc-play"), av, txt);
}

/** A ruler part: `cls` at fraction `x` of the track (and `w` wide). */
function mark(doc, cls, x, w = null, text = undefined) {
  const n = el(doc, "div", cls, text);
  pctVar(n, "--x", x);
  if (w != null) pctVar(n, "--w", w);
  return n;
}

function rulerBand(doc, r) {
  const rul = el(doc, "div", "lc-rul");
  colourVar(rul, "--cap-a", r.caps[0]);
  colourVar(rul, "--cap-b", r.caps[1]);
  append(rul, el(doc, "div", "lc-trk"), el(doc, "div", "lc-cap a"), el(doc, "div", "lc-cap b"));
  if (r.drive) rul.appendChild(mark(doc, "lc-drv", r.drive.from, r.drive.to - r.drive.from));
  for (const t of r.ticks) rul.appendChild(mark(doc, `lc-tick${t.mid ? " mid" : ""}`, t.x));
  if (r.start != null && r.drive) rul.appendChild(mark(doc, "lc-st", r.start));
  if (r.firstDown != null) {
    rul.appendChild(mark(doc, "lc-fd", r.firstDown));
    rul.appendChild(mark(doc, `lc-fl ${r.firstDownAlign ?? "center"}`, r.firstDown, null, "1st down"));
  }
  if (r.ball != null) rul.appendChild(mark(doc, "lc-ball", r.ball));
  // On a narrow card only the end labels and the ball's spot stay (CSS below). An end label within a quarter of the
  // track of the ball's spot would run into it there ("PIT 3" over "PIT"), so it is marked `near` and hidden too.
  const ballLabel = r.labels.find((l) => l.strong);
  for (const l of r.labels) {
    // The core places each label at [left, left + width] of the track; the CSS anchors it at that edge or its centre.
    const at = l.align === "left" ? l.left : l.align === "right" ? l.left + l.width : l.left + l.width / 2;
    const isEnd = l.x === 0 || l.x === 1;
    const near = isEnd && ballLabel && ballLabel !== l && Math.abs(ballLabel.x - l.x) < 0.25;
    const end = isEnd ? ` end${near ? " near" : ""}` : "";
    rul.appendChild(mark(doc, `lc-lb ${l.align}${l.strong ? " strong" : ""}${end}`, at, null, l.text));
  }
  return rul;
}

function situationBand(doc, d) {
  const band = el(doc, "div", "lc-sit");
  const s = d.situation;
  if (!s) return band;
  const dn = append(el(doc, "span", "lc-dn", s.down), s.spot ? el(doc, "span", "lc-spot", s.spot) : null);
  const dr = s.drive ? append(el(doc, "span", "lc-dr", "Drive · "), el(doc, "b", null, s.drive.plays),
    el(doc, "span", null, s.drive.plays === 1 ? " play · " : " plays · "), el(doc, "b", null, s.drive.yards), el(doc, "span", null, " yds")) : null;
  append(band, append(el(doc, "div", "lc-sh"), dn, dr));
  if (d.ruler) band.appendChild(rulerBand(doc, d.ruler));
  return band;
}

/** The card's shown play of a game: the newest non-disputed, else the newest; null with no plays. */
function shownPlay(game) {
  const plays = game?.plays ?? [];
  for (let i = plays.length - 1; i >= 0; i -= 1) if (plays[i].status !== "disputed") return plays[i];
  return plays[plays.length - 1] ?? null;
}

/** Before the first play: the score band (the teams, 0-0) over "Waiting for the first play". */
function waitingCard(doc, input, game, badImages) {
  const teams = [teamFor(game?.teams, 0), teamFor(game?.teams, 1)];
  const colours = sideColours(teams[0], teams[1]);
  const newest = Math.max(0, ...input.games.map((g) => g.game_seq));
  const box = (side) => ({ abbrev: teams[side]?.abbrev ?? (side ? "B" : "A"), logo: teams[side]?.logo_url ?? null, colour: colours[side], value: 0, trailing: false });
  const live = input.match?.status === "running" && (game?.game_seq ?? newest) >= newest;
  const d = { score: { a: box(0), b: box(1), status: live ? "LIVE" : "FINAL", sub: `Game ${game?.game_seq ?? 1}` } };
  const root = cardRoot(doc, "live", palette(colours[0]));
  bandStops(colours[0], colours[1]).forEach((c, i) => colourVar(root, `--a${i}`, c));
  const wait = append(el(doc, "div", "lc-play lc-wait"), el(doc, "div", "lc-hl", "Waiting for the first play"));
  return append(root, scoreBand(doc, d, badImages), wait, el(doc, "div", "lc-sit"));
}

/**
 * The live card for `game` (one of `snapshot.games`): its shown play (the newest non-disputed, else the newest) in
 * the bot's three bands. `badImages` (URLs that already failed in this page) draw as their fallbacks straight away.
 */
export function renderLiveCard(snapshot, game, doc, { badImages = null } = {}) {
  const input = cardInput(snapshot);
  const g = input.games.find((x) => x.game_seq === game?.game_seq) ?? null;
  const play = shownPlay(g);
  const d = play ? liveCard(input, g.game_seq, play.seq) : null;
  if (!d) return waitingCard(doc, input, g ?? game, badImages);
  const root = cardRoot(doc, "live", d.palette);
  d.band.forEach((c, i) => colourVar(root, `--a${i}`, c));
  return append(root, scoreBand(doc, d, badImages), playBand(doc, d, badImages), situationBand(doc, d));
}

/** The series final card: the scorebug rows, the winner's leaders and the team bars. `date`: when the match ended. */
export function renderFinalCard(snapshot, doc, { badImages = null, date = null } = {}) {
  const d = finalCard(cardInput(snapshot), { date });
  if (!d) return el(doc, "p", "empty", "No games yet.");
  const root = cardRoot(doc, "final", d.palette);
  root.style.setProperty("--n", String(d.header.games.length));

  const head = append(el(doc, "div", "fc-head"), el(doc, "span", "fc-hl", d.header.label));
  for (const g of d.header.games) head.appendChild(el(doc, "span", "fc-gh", g));
  append(root, append(head, el(doc, "span", "fc-sh", "SERIES")));

  for (const r of d.rows) {
    const row = el(doc, "div", `fc-row${r.winner ? " win" : ""}`);
    colourVar(row, "--row", r.colour);
    colourVar(row, "--tb", r.colour);
    row.appendChild(append(el(doc, "div", "fc-team"), cardLogo(doc, r.logo, r.abbrev, badImages), el(doc, "span", "fc-name", r.name)));
    for (const g of r.games) row.appendChild(el(doc, "span", `fc-g${g === "-" ? " na" : ""}`, g));
    root.appendChild(append(row, el(doc, "b", "fc-s", r.wins)));
  }

  // The forfeit note (a plain-text name, never markup): a fixed neutral grey, never the winner's team tint (like
  // the bot's NOTE_GREY), between the rows and the leaders/bars.
  if (d.note) {
    const note = el(doc, "div", "fc-note", d.note);
    colourVar(note, "--note", palette(NEUTRAL).label);
    root.appendChild(note);
  }

  const pls = el(doc, "div", "fc-pls");
  const winnerLogo = d.rows.find((r) => r.winner)?.logo;
  for (const l of d.leaders) {
    const hs = append(el(doc, "div", "fc-hs"), cardPortrait(doc, l.portrait, "fc-portrait", badImages) ?? logoImg(doc, winnerLogo, "", badImages, "fc-hslogo"));
    const line = append(el(doc, "span", "fc-ln"), el(doc, "b", null, l.name), el(doc, "span", null, ` · ${l.line}`));
    pls.appendChild(append(el(doc, "div", "fc-p"), hs, line));
  }
  const bars = el(doc, "div", "fc-bars");
  colourVar(bars, "--bar-a", palette(d.rows[0].colour).bar);
  colourVar(bars, "--bar-b", palette(d.rows[1].colour).bar);
  for (const b of d.bars) {
    const sum = b.a + b.b;
    const ia = el(doc, "i", "a");
    const ib = el(doc, "i", "b");
    ia.style.setProperty("--f", String(sum > 0 ? Math.max(0, b.a) : 1));
    ib.style.setProperty("--f", String(sum > 0 ? Math.max(0, b.b) : 1));
    append(bars, el(doc, "span", "fc-bl", b.label), el(doc, "span", "fc-bx", b.a), append(el(doc, "div", "fc-tr"), ia, ib), el(doc, "span", "fc-by", b.b));
  }
  // A leaderless final (split/held/abandoned) drops the leaders column AND its divider -- never an orphan divider
  // with nothing beside it -- so the bars (the lone child of fc-bot) take the whole row.
  const bot = el(doc, "div", "fc-bot");
  if (d.leaders.length) append(bot, pls, el(doc, "div", "fc-div"));
  bot.appendChild(bars);
  return append(root, bot);
}

// ---- the feed and the names line (plan task 18, rulings R18 and R25) ---------------------------------------------

/** The feed's 40 px portrait (the 96 px file; class "portrait", so a failed one is dropped), or null. */
function feedPortrait(doc, url, badImages) {
  if (typeof url !== "string" || !PORTRAIT_RE.test(url) || badImages?.has(url)) return null;
  const img = el(doc, "img", "portrait");
  img.setAttribute("src", url);
  img.setAttribute("alt", "");
  img.setAttribute("width", "40");
  img.setAttribute("height", "40");
  return img;
}

/**
 * Every play of `game` as the feed (newest first): a small headshot (else the team's logo), the result word, the
 * headline, then the white line (the player and his running stats) and the grey line, or the play's own text when
 * no player is known. Built from the core's `playRows`, so a row says what the card says for that play. A disputed
 * play keeps its badge and loses the score styling (its points do not count).
 */
export function renderFeed(snapshot, game, doc, { badImages = null } = {}) {
  // The game handed in is the one drawn (the page's picked game, with any stream frames already upserted).
  const games = (snapshot?.games ?? []).map((x) => (x.game_seq === game?.game_seq ? game : x));
  const g = cardInput({ ...snapshot, games }).games.find((x) => x.game_seq === game?.game_seq) ?? null;
  if (!g?.plays?.length) return el(doc, "p", "empty", game?.game_id ? "No live plays reached us for this game." : "No plays yet.");
  const teams = [teamFor(g.teams, 0), teamFor(g.teams, 1)];
  const rows = playRows(g, sideColours(teams[0], teams[1]));
  const bySeq = new Map(g.plays.map((p) => [p.seq, p]));
  const list = el(doc, "ol", "feed");
  list.setAttribute("data-game", String(g.game_seq));
  for (const r of rows) {
    const s = bySeq.get(r.seq)?.body?.summary ?? {};
    const scored = s.touchdown === true || (Number.isFinite(s.points) && s.points > 0);
    const cls = ["fr"];
    if (scored && !r.disputed) cls.push("score");
    if (s.turnover === true) cls.push("turnover");
    if (r.disputed) cls.push("disputed");
    const li = el(doc, "li", cls.join(" "));
    li.setAttribute("data-seq", String(r.seq));

    const side = bit(r.player?.side) ? r.player.side : bit(r.side) ? r.side : 0;
    const av = el(doc, "div", "fr-av");
    append(av, (r.player ? feedPortrait(doc, r.player.portrait, badImages) : null)
      ?? cardLogo(doc, teams[side]?.logo_url, teams[side]?.abbrev ?? (side ? "B" : "A"), badImages));
    li.appendChild(av);

    const txt = el(doc, "div", "fr-txt");
    if (r.result) append(txt, el(doc, "div", `fr-res${r.tone === "good" ? "" : " muted"}`, r.result));
    append(txt, el(doc, "div", "fr-hl", r.headline));
    if (r.player) {
      const l1 = append(el(doc, "div", "fr-l1"), el(doc, "span", "fr-name", r.player.name));
      for (const [v, w] of r.stats ?? []) l1.appendChild(append(el(doc, "span", "fr-seg"), el(doc, "b", null, v), w ? el(doc, "span", null, w) : null));
      append(txt, l1);
      if (r.detail) {
        const l2 = el(doc, "div", "fr-l2");
        for (const part of r.detail) l2.appendChild(el(doc, part.bold ? "b" : "span", null, part.text));
        append(txt, l2);
      }
    } else if (r.text) {
      append(txt, el(doc, "div", "fr-l2", r.text));
    }
    if (r.disputed) append(txt, el(doc, "span", "badge disputed", "disputed"));
    li.appendChild(txt);
    list.appendChild(li);
  }
  return list;
}

/** Live card M7: box lines indexed by play SIDE for the side-labelled box. A line that says its `side` (a live line,
 *  or a verified line since live card I1) is placed by it; a verified line's team_index is capture order. */
export function boxBySide(lines) {
  return (Array.isArray(lines) ? lines : []).map((l) => (l && bit(l.side) && l.team_index !== l.side ? { ...l, team_index: l.side } : l));
}

/** Live card M4: the next feed rows, each fresh row swapped for the row already on screen whose `sig` matches. */
export function keepRows(oldRows, freshRows, sig) {
  const old = new Map(oldRows.map((r) => [sig(r), r]));
  return freshRows.map((r) => old.get(sig(r)) ?? r);
}

/** Live card M4: make `list`'s children `next` touching only what changed: stale rows go, new rows go in at their
 *  place, and a row that stays is never moved (its scroll position and any text selection in it survive). */
export function patchList(list, next) {
  const keep = new Set(next);
  for (const c of [...list.children]) if (!keep.has(c)) list.removeChild(c);
  next.forEach((n, i) => { if (list.children[i] !== n) list.insertBefore(n, list.children[i] ?? null); });
}

/** The node the reader is looking at in an All plays list (R15): the first keyed node whose bottom is on screen, then,
 *  while it holds a keyed descendant that is also on screen, that descendant (an open drive gives way to its card).
 *  Null when nothing keyed is visible. */
export function readingAnchor(list) {
  const visible = (root) => [...root.querySelectorAll("[data-key]")].find((n) => n.getBoundingClientRect().bottom > 0) ?? null;
  let node = visible(list);
  for (let inner = node && visible(node); inner; inner = visible(node)) node = inner;
  return node;
}

/** Live card M8: how many of the newest feed rows a phone shows before "Show all N plays". */
export const FEED_PHONE_ROWS = 8;
/** The phone feed's expand button text for `n` rows, or null when every row already shows. */
export function feedMoreLabel(n) {
  return Number.isInteger(n) && n > FEED_PHONE_ROWS ? `Show all ${n} plays` : null;
}

const plainName = (seats, seat) => {
  const n = seats?.[seat]?.display_name;
  return typeof n === "string" && n.trim() ? n.replace(/\s+/g, " ").trim().slice(0, 60) : `Seat ${seat + 1}`;
};

/**
 * The Discord message's line as plain text (the bot's liveContent / finalContent, without markdown): the card never
 * names the players (spec §1), so the page shows this line above it. Live: "Alpha vs Bravo · Ranked" in the side
 * order of the newest game, "CPU" for a side with no human. After: "Bravo beat Alpha 2-0 · Ranked" (a one-game
 * match shows that game's score; level: "Alpha vs Bravo 1-1").
 */
export function namesLine(snapshot) {
  const seats = Array.isArray(snapshot?.seats) ? snapshot.seats : [];
  const mode = snapshot?.match?.mode === "ranked" ? "Ranked" : "Casual";
  const input = cardInput(snapshot);
  if (seriesOutcome(input).kind === "open") {
    const games = [...(snapshot?.games ?? [])].sort((a, b) => a.game_seq - b.game_seq);
    const { boxSide, seatOfBox, confirmed } = seatLayout(games[games.length - 1]?.sides, seats.length);
    const bySide = [0, 1].map((side) => { const box = boxSide.indexOf(side); return box === -1 ? null : seatOfBox[box]; });
    const name = (side) => { const seat = confirmed ? bySide[side] : side; return seat == null ? "CPU" : plainName(seats, seat); };
    return `${name(0)} vs ${name(1)} · ${mode}`;
  }
  return seriesLine(input, (seat) => plainName(seats, seat));
}

// ---- the restyled site (spec 2026-10-02): strip, series pill, game picker -------------------------------------
// Laid out BY SEAT (plan ruling R2): seat 0 left. Colours reach the page only as #rrggbb custom properties (CSSOM).

/** A seat's mark in the strip: its team's logo (or abbrev badge), else the player's avatar (Pick-Up has no team). */
function seatMark(doc, snapshot, s, badImages, badAvatars) {
  if (s.team) return cardLogo(doc, s.team.logo_url, s.team.abbrev ?? s.label, badImages);
  return avatar(doc, { ...(snapshot?.seats?.[s.seat] ?? {}), display_name: s.player || s.label }, badAvatars);
}

/** The line score table (spec §3): G1..Gn by seat; the picked game's column bold, a game not played greyed "-". */
function lineScoreTable(doc, input, game) {
  const ls = lineScore(input, game);
  const head = append(el(doc, "tr"), el(doc, "th", null, ""));
  ls.cols.forEach((c, i) => head.appendChild(el(doc, "th", i === ls.current ? "cur" : null, c)));
  const body = el(doc, "tbody");
  for (const r of ls.rows) {
    const tr = append(el(doc, "tr"), el(doc, "th", "ls-team", r.label));
    r.cells.forEach((c, i) => tr.appendChild(el(doc, "td", [c === "-" ? "na" : "", i === ls.current ? "cur" : ""].join(" ").trim() || null, c)));
    body.appendChild(tr);
  }
  return append(el(doc, "table", "ls"), append(el(doc, "thead"), head), body);
}

/** /games/:id's line score (R10): one column headed "T", each seat's label and the game's score. */
function singleScore(doc, input, game) {
  const st = seatTeams(input, game);
  const score = game ? scoreBySeat(game) : [0, 0];
  const head = append(el(doc, "tr"), el(doc, "th", null, ""), el(doc, "th", null, "T"));
  const body = el(doc, "tbody");
  for (const seat of [0, 1]) body.appendChild(append(el(doc, "tr"), el(doc, "th", "ls-team", st[seat].label), el(doc, "td", null, score[seat])));
  return append(el(doc, "table", "ls"), append(el(doc, "thead"), head), body);
}

/** The pill node for a CardInput, or null (seriesPill's words; the winner's colour as --pill). */
function pillNode(doc, input) {
  const p = seriesPill(input);
  if (!p) return null;
  const n = el(doc, "span", `pill${p.winner === null ? "" : " win"}`, p.text);
  if (p.winner !== null) colourVar(n, "--pill", seatColours(seatTeams(input))[p.winner]);
  return n;
}

/** The series pill (spec §3): "ATL wins the series 2-0" in the winner's colour, a plain pill otherwise, or null. */
export function renderSeriesPill(snapshot, doc) {
  return pillNode(doc, cardInput(snapshot));
}

/**
 * The scoreboard strip for `game` (the viewer's pick; the newest game by default): each seat's team logo, team and
 * player names and Anton score (the trailing seat dimmed), and in the middle the status, the line score and the
 * series pill. A seat with no team shows its player's name and avatar.
 */
export function renderStrip(snapshot, game, doc, { badImages = null, badAvatars = null } = {}) {
  const input = cardInput(snapshot);
  const g = input.games.find((x) => x.game_seq === game?.game_seq) ?? newestOf(input);
  const st = seatTeams(input, g);
  const colours = seatColours(st);
  const score = g ? scoreBySeat(g) : [0, 0];
  const lead = score[0] > score[1] ? 0 : score[1] > score[0] ? 1 : null;
  const root = el(doc, "div", `strip ${lead === null ? "level" : lead ? "lead-b" : "lead-a"}`);
  colourVar(root, "--ta", colours[0]);
  colourVar(root, "--tb", colours[1]);
  const side = (seat) => {
    const s = st[seat];
    const box = el(doc, "div", `st-team ${seat ? "b" : "a"}${lead !== null && lead !== seat ? " trailing" : ""}`);
    colourVar(box, "--tc", colours[seat]);
    const name = s.team ? [s.team.city, s.team.name].filter((x) => typeof x === "string" && x).join(" ") : (s.player || s.label);
    const nm = append(el(doc, "div", "st-nm"), el(doc, "span", "st-name", name), s.team && s.player ? el(doc, "small", "st-player", s.player) : null);
    const mark = seatMark(doc, snapshot, s, badImages, badAvatars);
    const pts = el(doc, "b", "st-pts", score[seat]);
    return seat ? append(box, pts, nm, mark) : append(box, mark, nm, pts);
  };
  const status = stripStatus(input, g);
  // An unofficial final: "Final · " then "Unofficial" in its own grey span (the card's FINAL · UNOFFICIAL).
  const stEl = el(doc, "div", `st-status${status.live ? " live" : ""}`, status.note ? `${status.text} · ` : status.text);
  if (status.note) stEl.appendChild(el(doc, "span", "st-unoff", status.note));
  const mid = append(el(doc, "div", "st-mid"), stEl,
    input.match?.single ? singleScore(doc, input, g) : lineScoreTable(doc, input, g), pillNode(doc, input));
  return append(root, side(0), mid, side(1));
}

/**
 * A plain-text game picker (spec §6): "Game 1 · Game 2 · ... · Series total", the picked one underlined (CSS), never
 * filled. `current` is a game_seq or "total". Null with fewer than two games (nothing to pick).
 */
export function renderPicker(games, current, doc, { total = false, name = "" } = {}) {
  const list = (Array.isArray(games) ? games : []).filter((g) => Number.isInteger(g?.game_seq));
  if (list.length < 2) return null;
  const nav = el(doc, "nav", "picker");
  nav.setAttribute("aria-label", "Pick a game");
  if (name) nav.setAttribute("data-for", name);
  const opt = (value, label) => {
    const b = el(doc, "button", null, label);
    b.setAttribute("type", "button");
    b.setAttribute("data-pick", String(value));
    b.setAttribute("aria-pressed", String(value === current));
    return b;
  };
  for (const g of list) nav.appendChild(opt(g.game_seq, `Game ${g.game_seq}`));
  if (total) nav.appendChild(opt("total", "Series total"));
  return nav;
}

// ---- Live Feed (spec §5): the latest-play panel (the live card minus its score band) and All plays by drive ------

/** The panel's play: `seq` when the game has it, else the card's shown play (the newest non-disputed). */
function panelPlay(g, seq) {
  return (Number.isInteger(seq) && g.plays.find((p) => p.seq === seq)) || shownPlay(g);
}

/**
 * The plays the panel steps through, oldest first (final review I2): every non-disputed play, or every play when all
 * are disputed. Its last seq is the play "Latest" shows, so the arrows, the label and both pages agree. Takes a page
 * game or a CardInput game (seqs via seqOf).
 */
export function steppableSeqs(game) {
  const all = (game?.plays ?? []).filter((p) => Number.isInteger(seqOf(p)));
  const ok = all.filter((p) => p.status !== "disputed");
  return (ok.length ? ok : all).map(seqOf).sort((a, b) => a - b);
}

/** Where the panel is on `seqs`: the shown seq, and whether it follows the newest steppable play. */
function panelAt(game, seqs, panelSeq) {
  const has = Number.isInteger(panelSeq) && (game?.plays ?? []).some((p) => seqOf(p) === panelSeq);
  const last = seqs[seqs.length - 1];
  const following = !has || panelSeq === last;
  return { cur: following ? last : panelSeq, following };
}

/** A play picked in All plays: null (follow) when it is the newest steppable play or unknown, else its seq. */
export function panelPickSeq(game, seq) {
  const seqs = steppableSeqs(game);
  return panelAt(game, seqs, seq).following ? null : seq;
}

/** « ◀ ▶ Live from `panelSeq`: the panel's next seq, null when that is the newest steppable play (follow again). */
export function panelStepSeq(game, panelSeq, how) {
  const seqs = steppableSeqs(game);
  if (!seqs.length) return null;
  const { cur } = panelAt(game, seqs, panelSeq);
  const older = seqs.filter((s) => s < cur), newer = seqs.filter((s) => s > cur);
  const to = { first: seqs[0], prev: older.length ? older[older.length - 1] : cur, next: newer.length ? newer[0] : cur, live: null }[how];
  if (to === undefined) return panelSeq;
  return to === null || to === seqs[seqs.length - 1] ? null : to;
}

/** The panel's play area: watermark, 112 px headshot with its team badge, result, headline, player and passer lines. */
function ppMain(doc, d, badImages) {
  const p = d.play;
  const main = el(doc, "div", "pp-main");
  const wm = logoImg(doc, p.logo, "", badImages, "pp-wm");
  if (wm) { wm.setAttribute("aria-hidden", "true"); main.appendChild(wm); }
  const face = append(el(doc, "div", "pp-face"),
    cardPortrait(doc, p.player?.portrait, "pp-portrait", badImages) ?? logoImg(doc, p.logo, "", badImages, "pp-facelogo"));
  const av = append(el(doc, "div", "pp-av"), face,
    p.player && usableLogo(p.logo, badImages) ? append(el(doc, "div", "pp-badge"), logoImg(doc, p.logo, "", badImages)) : null);
  const txt = el(doc, "div", "pp-txt");
  if (p.result) txt.appendChild(el(doc, "div", `pp-res${p.tone === "good" ? "" : " muted"}`, p.result));
  txt.appendChild(el(doc, "div", "pp-hl", p.headline));
  if (p.player) {
    const l1 = append(el(doc, "div", "pp-l1"), el(doc, "span", "pp-name", p.player.name));
    for (const [v, w] of p.stats ?? []) l1.appendChild(append(el(doc, "span", "pp-seg"), el(doc, "b", null, v), w ? el(doc, "span", null, w) : null));
    txt.appendChild(l1);
    if (p.detail) {
      const l2 = el(doc, "div", "pp-l2");
      for (const part of p.detail) l2.appendChild(el(doc, part.bold ? "b" : "span", null, part.text));
      txt.appendChild(l2);
    }
  } else if (p.text) {
    txt.appendChild(el(doc, "div", "pp-l2", p.text.length > 90 ? `${p.text.slice(0, 89)}…` : p.text));
  }
  return append(main, av, txt);
}

/**
 * The Latest play panel for `game` (spec §5): the live card's play and situation bands in the page's panel style.
 * `seq` (the viewer stepped back) shows that play; null follows the newest. The title row holds « ◀ ▶ Live; on a game
 * that is not live the fourth is "»", styled like the arrows (R7, game page v2 §5). The play's team colour washes the play area (--tc).
 */
export function renderPlayPanel(snapshot, game, doc, { seq = null, badImages = null } = {}) {
  const input = cardInput(snapshot);
  const g = input.games.find((x) => x.game_seq === game?.game_seq) ?? null;
  const wrap = el(doc, "section", "pp-wrap");
  wrap.setAttribute("aria-label", "Latest play");
  const cap = el(doc, "div", "cap");
  const box = el(doc, "div", "pp");
  if (!g?.plays?.length) {
    cap.appendChild(el(doc, "span", null, "Latest play"));
    box.appendChild(append(el(doc, "div", "pp-main pp-wait"),
      el(doc, "div", "pp-hl", game?.game_id ? "No live plays reached us for this game." : "Waiting for the first play")));
    return append(wrap, cap, box);
  }
  const seqs = steppableSeqs(g);
  const { cur, following } = panelAt(g, seqs, seq);
  const play = following ? shownPlay(g) : panelPlay(g, cur);
  const d = liveCard(input, g.game_seq, play.seq);
  const at = seqs.indexOf(play.seq);
  const older = seqs.some((s) => s < play.seq), newer = seqs.some((s) => s > play.seq);
  const live = gameIsLive(input, g);
  cap.appendChild(el(doc, "span", null, following ? "Latest play" : at === -1 ? "Disputed play" : `Play ${at + 1} of ${seqs.length}`));
  const nav = el(doc, "div", "pp-nav");
  nav.setAttribute("role", "group");
  nav.setAttribute("aria-label", "Step through the plays");
  const step = (name, label, glyph, disabled) => {
    const b = el(doc, "button", `pp-step${name === "live" && live ? " lv" : ""}`, glyph);
    b.setAttribute("type", "button");
    b.setAttribute("data-step", name);
    b.setAttribute("aria-label", label);
    if (name === "live" && live) b.setAttribute("aria-pressed", String(following));
    if (disabled) b.setAttribute("disabled", "");
    return b;
  };
  append(nav, step("first", "First play", "«", !older), step("prev", "Previous play", "◀", !older),
    step("next", "Next play", "▶", !newer),
    step("live", live ? "Back to the live play" : "Back to the last play", live ? "Live" : "»", false));
  cap.appendChild(nav);
  const tc = d.play.player?.colour ?? d.palette.team;
  colourVar(wrap, "--tc", tc);
  const P = palette(tc);
  P.face.forEach((c, i) => colourVar(wrap, `--face${i}`, c));
  colourVar(wrap, "--badge", P.badge);
  const sit = situationBand(doc, d);
  sit.className = "pp-sit";
  return append(wrap, cap, append(box, ppMain(doc, d, badImages), sit));
}

/** A play's own snap ("1st & 10 at ATL 7", R8/R9): the core's situation words on the play's field, else the server's
 *  text with team abbrevs (spotWords: never "own"/"opp", "Conversion try" in title case). */
function snapLine(play, teams) {
  const f = play.field;
  const off = play.body?.offense_side;
  const conv = play.body?.summary?.kind === "conversion" || play.body?.conversion === true;
  const ok = f && Number.isFinite(f.field_yds) && f.field_yds > 0 && Number.isFinite(f.own_yd) && bit(off);
  if (!ok) return spotWords(play.situation, off, teams);
  const before = { offense: off, down: conv ? null : f.down ?? null, to_go: conv ? null : f.to_go ?? null,
    own_yd: f.own_yd, field_yds: f.field_yds, label: conv ? "Conversion try" : null };
  const s = situationLine(before, teams, null, play);
  return s ? [s.down, s.spot].filter(Boolean).join(" ") : "";
}

/** One play card (spec §1): a button (click shows it in the panel) with headshot, headline + result word, the player
 *  and passer line, the down and spot, and the seat-ordered score. `last` adds "LAST PLAY" to the result word. */
function playItem(doc, p, r, g, teams, badImages, last = false) {
  const li = el(doc, "li", "ap-r");
  li.setAttribute("data-seq", String(r.seq));
  li.setAttribute("data-key", `p${r.seq}`);
  const btn = el(doc, "button", "ap-btn");
  btn.setAttribute("type", "button");
  btn.setAttribute("data-seq", String(r.seq));
  btn.setAttribute("aria-label", `Show play ${r.seq} in the panel: ${r.headline}`);
  const side = bit(r.player?.side) ? r.player.side : bit(r.side) ? r.side : 0;
  const av = append(el(doc, "span", "ap-av"), (r.player ? feedPortrait(doc, r.player.portrait, badImages) : null)
    ?? cardLogo(doc, teams[side]?.logo_url, teams[side]?.abbrev ?? (side ? "B" : "A"), badImages));
  const txt = el(doc, "span", "ap-txt");
  const words = [r.result, last ? "LAST PLAY" : null].filter(Boolean).join(" · ");
  txt.appendChild(append(el(doc, "span", "ap-hl", r.headline), words ? el(doc, "small", "ap-res", words) : null));
  if (r.player) {
    const l = append(el(doc, "span", "ap-l"), el(doc, "b", null, r.player.name),
      el(doc, "span", null, ` ${(r.stats ?? []).map(([v, w]) => (w ? `${v} ${w}` : String(v))).join(" · ")}`));
    if (r.detail) { l.appendChild(el(doc, "span", null, " · ")); for (const part of r.detail) l.appendChild(el(doc, part.bold ? "b" : "span", null, part.text)); }
    txt.appendChild(l);
  } else if (r.text) txt.appendChild(el(doc, "span", "ap-l", r.text));
  const snap = snapLine(p, teams);
  if (snap) txt.appendChild(el(doc, "span", "ap-sit", snap));
  const sc = Array.isArray(p.score_after) ? [p.score_after[sideOfSeat(g, 0)] ?? 0, p.score_after[sideOfSeat(g, 1)] ?? 0] : null;
  append(btn, av, txt, el(doc, "span", "ap-sc", sc ? `${sc[0]}-${sc[1]}` : ""));
  return append(li, btn);
}

/** A field position (own yards) as a fraction of the chart: the field sits between two 7% end zones (R7). */
const chartX = (yd, L) => 0.07 + 0.86 * (yd / L);

/** The drive chart (spec §2): team-colour end zones (offense left), the bar, a hollow start dot and a solid end dot,
 *  and beside it the result, the score after and "4 plays · 58 yds · ATL 7 to PIT 3". */
function driveChart(doc, d, words, scoreText, colours) {
  const c = d.chart;
  const field = el(doc, "div", "dc-field");
  field.setAttribute("aria-hidden", "true");
  colourVar(field, "--ez-a", colours[d.side]);
  colourVar(field, "--ez-b", colours[1 - d.side]);
  colourVar(field, "--tc", colours[d.side]);
  const a = chartX(Math.min(c.start, c.end), c.field_yds), b = chartX(Math.max(c.start, c.end), c.field_yds);
  append(field, el(doc, "i", "dc-ez a"), el(doc, "i", "dc-ez b"), el(doc, "i", "dc-yl y1"), el(doc, "i", "dc-yl y2"), el(doc, "i", "dc-yl y3"),
    mark(doc, "dc-run", a, b - a), mark(doc, "dc-st", chartX(c.start, c.field_yds)), mark(doc, "dc-en", chartX(c.end, c.field_yds)));
  const meta = `${d.plays} ${d.plays === 1 ? "play" : "plays"} · ${Math.round(d.yards)} yds · ${c.from} to ${c.to}`;
  const txt = append(el(doc, "div", "dc-txt"), el(doc, "b", null, words), el(doc, "span", "dc-sc", scoreText), el(doc, "span", "dc-m", meta));
  return append(el(doc, "div", "dc"), field, txt);
}

/**
 * All plays (spec §1, §2): the All plays / Scoring plays toggle, then `game`'s drives newest first, each a row (team
 * logo, how it ended, "4 plays, 52 yds", the score after by seat, a chevron). An open drive shows its chart and its
 * play cards newest first. `open` is the page's Map of explicit choices ("g<seq>:d<first_seq>" -> boolean, R5); by
 * default only the current drive of a live game is open (R4). `mode` is "all" or "scoring" (R1).
 */
export function renderAllPlays(snapshot, game, doc, { badImages = null, open = null, mode = "all" } = {}) {
  const games = (snapshot?.games ?? []).map((x) => (x.game_seq === game?.game_seq ? game : x));
  const input = cardInput({ ...snapshot, games });
  const g = input.games.find((x) => x.game_seq === game?.game_seq) ?? null;
  const wrap = el(doc, "section", "ap-wrap");
  wrap.setAttribute("aria-label", "All plays");
  const seg = el(doc, "nav", "ap-seg");
  seg.setAttribute("aria-label", "Which plays");
  for (const [m, label] of [["all", "All plays"], ["scoring", "Scoring plays"]]) {
    const b = el(doc, "button", null, label);
    b.setAttribute("type", "button");
    b.setAttribute("data-mode", m);
    b.setAttribute("aria-pressed", String(m === mode));
    seg.appendChild(b);
  }
  if (!g?.plays?.length) return append(wrap, seg, el(doc, "p", "empty", game?.game_id ? "No live plays reached us for this game." : "No plays yet."));
  const teams = [teamFor(g.teams, 0), teamFor(g.teams, 1)];
  const colours = sideColours(teams[0], teams[1]);
  const rows = new Map(playRows(g, colours).map((r) => [r.seq, r]));
  const st = seatTeams(input, g);
  const all = driveList(input, g, { live: gameIsLive(input, g) });
  const drives = mode === "scoring" ? scoringDrives(all) : all;
  const list = el(doc, "ol", "ap");
  list.setAttribute("data-game", String(g.game_seq));
  list.setAttribute("data-mode", mode);
  if (!snapshot?.match?.single && input.games.length > 1) { const qh = el(doc, "li", "ap-qh", `Game ${g.game_seq}`); qh.setAttribute("data-key", "qh"); list.appendChild(qh); }
  if (!drives.length) list.appendChild(el(doc, "li", "empty", mode === "scoring" ? (gameIsLive(input, g) ? "No scoring plays yet." : "No scoring plays.") : "No plays yet."));
  for (const d of drives) {
    const key = `g${g.game_seq}:${d.key}`;
    const isOpen = open?.has(key) ? open.get(key) === true : d.current;
    const inPlay = d.current && d.end === "live";
    const team = bit(d.side) && teams[d.side]?.name ? teams[d.side] : null;
    const words = DRIVE_END_WORDS[d.end] ?? "";
    const scoreText = `${st[0].label} ${d.score[0]} · ${st[1].label} ${d.score[1]}`;
    const li = el(doc, "li", `dv${isOpen ? " open" : ""}`);
    li.setAttribute("data-key", d.key);
    const hd = el(doc, "button", "dv-hd");
    hd.setAttribute("type", "button");
    hd.setAttribute("data-drive", key);
    hd.setAttribute("aria-expanded", String(isOpen));
    hd.setAttribute("aria-controls", `dv-${g.game_seq}-${d.key}`);
    const meta = `${d.plays} ${d.plays === 1 ? "play" : "plays"}, ${Math.round(d.yards)} yds${inPlay && d.chart ? ` · started ${d.chart.from}` : ""}`;
    const sc = el(doc, "span", "dv-sc");
    const lead = d.score[0] === d.score[1] ? null : d.score[0] > d.score[1] ? 0 : 1;
    append(sc, el(doc, "span", null, st[0].label), el(doc, "span", null, st[1].label),
      el(doc, "b", lead === 1 ? "lo" : "hi", d.score[0]), el(doc, "b", lead === 0 ? "lo" : "hi", d.score[1]));
    const ch = el(doc, "span", "dv-ch", isOpen ? "⌃" : "⌄");
    ch.setAttribute("aria-hidden", "true");
    append(hd, append(el(doc, "span", "dv-logo"), team ? cardLogo(doc, team.logo_url, team.abbrev, badImages) : null),
      append(el(doc, "span", "dv-txt"), el(doc, "b", "dv-res", words), el(doc, "span", "dv-m", meta)),
      inPlay ? el(doc, "span", "dv-tag", "IN PROGRESS") : sc, ch);
    li.appendChild(hd);
    if (isOpen) {
      const body = el(doc, "div", "dv-body");
      body.setAttribute("id", `dv-${g.game_seq}-${d.key}`);
      if (d.chart) body.appendChild(driveChart(doc, d, words, scoreText, colours));
      const cards = el(doc, "ol", "dv-plays");
      for (const seq of [...d.seqs].reverse()) {
        const p = g.plays.find((x) => x.seq === seq);
        const r = rows.get(seq);
        if (p && r) cards.appendChild(playItem(doc, p, r, g, teams, badImages, d.current && seq === d.newest));
      }
      li.appendChild(append(body, cards));
    }
    list.appendChild(li);
  }
  return append(wrap, seg, list);
}

// ---- Box Score (spec §6): the picked game or the series total, two seat columns, TEAM rows ---------------------

/** One box table (or its empty words) as a section: logo + "Atlanta Passing", then the table ending in TEAM. */
function boxSection(doc, col, t, badImages) {
  const sec = el(doc, "section", "bx-sec");
  sec.appendChild(append(el(doc, "h3", "bx-sh"), col.team ? logoImg(doc, col.team.logo_url, col.team.abbrev, badImages) : null, el(doc, "span", null, t.title)));
  if (t.empty) return append(sec, el(doc, "p", "bx-none", t.empty));
  const head = append(el(doc, "tr"), el(doc, "th", "who", ""));
  for (const h of t.heads) head.appendChild(el(doc, "th", null, h));
  const body = el(doc, "tbody");
  for (const r of t.rows) body.appendChild(append(el(doc, "tr"), el(doc, "td", "who", r.name), ...r.cells.map((v) => el(doc, "td", null, v))));
  body.appendChild(append(el(doc, "tr", "tot"), el(doc, "td", "who", "TEAM"), ...t.total.map((v) => el(doc, "td", null, v))));
  return append(sec, append(el(doc, "table", "tbl"), el(doc, "caption", "sr", t.title), append(el(doc, "thead"), head), body));
}

/** The Box Score tab for `pick` (a game_seq or "total"): the note over a series total, then the two seat columns. */
export function renderBoxScore(snapshot, pick, doc, { badImages = null } = {}) {
  const m = boxScore(cardInput(snapshot), pick);
  const root = el(doc, "div", "bx");
  if (!m.any) return append(root, el(doc, "p", "empty", "No box score for this game yet."));
  const cols = el(doc, "div", "bx-cols");
  for (const c of m.columns) cols.appendChild(append(el(doc, "div", "bx-col"), ...c.tables.map((t) => boxSection(doc, c, t, badImages))));
  return append(root, cols);
}

// ---- Team Stats (game page v2 spec §3): the ESPN table -----------------------------------------------------------------

/** A Team Stats head cell: the team's logo and its short label. */
function teamHead(doc, h, badImages) {
  return append(el(doc, "th", "th-team"), h.team ? logoImg(doc, h.team.logo_url, h.team.abbrev, badImages) : null, el(doc, "span", null, h.label));
}

/** The Team Stats tab for `pick` (a game_seq or "total"), the ESPN table (spec §3, R8): stat name, then one column per
 *  seat; the better value bright (`hi`), the other grey (`lo`); a tie both bright. No bars, no shaded rows. */
export function renderTeamStats(snapshot, pick, doc, { badImages = null } = {}) {
  const t = teamStats(cardInput(snapshot), pick);
  const root = el(doc, "div", "ts");
  if (!t.any) return append(root, el(doc, "p", "empty", "No team stats for this game yet."));
  const head = append(el(doc, "tr"), el(doc, "th", "who", "Team stats"), teamHead(doc, t.heads[0], badImages), teamHead(doc, t.heads[1], badImages));
  const body = el(doc, "tbody");
  for (const r of t.rows) {
    body.appendChild(append(el(doc, "tr"), el(doc, "td", "who", r.label),
      ...r.values.map((v, i) => el(doc, "td", `v ${r.better[i] ? "hi" : "lo"}`, v))));
  }
  return append(root, append(el(doc, "div", "ts-box"),
    append(el(doc, "table", "ts-t"), el(doc, "caption", "sr", "Team stats"), append(el(doc, "thead"), head), body)));
}
