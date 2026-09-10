// Stats, as a page: every game recorded on this PC as a box score, a
// summary of the whole run in the manner of a year-end wrap, and the Discord
// account the results are sent under. Moved out of Settings on 2026-09-10.
//
// The numbers come from the recorder's captures (reborn_core::stats::Capture),
// archived by the launcher when a game is ingested. The page adds nothing the
// capture does not carry: touchdowns by kind, picks, tackles, sacks, fumbles,
// team totals. Which side the player controlled is not recorded yet, so the
// team and player summaries count both sides and say so.

import { call, el, fact, facts, markCurrent, partial, reason, refusal, wireCoreFold } from "./shell.js";

const state = {};
let problem = null;
let busy = false;
let said = null;
let open = null; // key of the game whose box score is open

function ok(x) {
  return x && !x.error;
}

async function load() {
  const reads = {
    identity: call("identity_status"),
    queue: call("stats_queue"),
    games: call("stats_games"),
  };
  const keys = Object.keys(reads);
  const settled = await Promise.allSettled(Object.values(reads));
  keys.forEach((k, i) => {
    state[k] = settled[i].status === "fulfilled" ? settled[i].value : { error: reason(settled[i].reason) };
  });
}

async function act(label, cmd, args, after) {
  if (busy) return;
  busy = true;
  problem = null;
  draw();
  try {
    after(await call(cmd, args));
  } catch (e) {
    problem = `${label}: ${reason(e)}`;
  }
  await load();
  busy = false;
  draw();
}

// ---------------------------------------------------------------------------
// The arithmetic. A touchdown is a touchdown whoever scored it and however.
const tds = (s) => (s.pass_td || 0) + (s.rush_td || 0) + (s.rec_td || 0) + (s.pick_six || 0) + (s.fum_td || 0);
const when = (iso) => { const d = new Date(iso); return isNaN(d) ? iso : d.toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" }); };
const games = () => (ok(state.games) && Array.isArray(state.games) ? state.games : []);

function summary(list) {
  const teams = new Map(); // name -> games
  const players = new Map(); // name -> { name, team, games, stats totals }
  for (const g of list) {
    for (const t of g.teams || []) teams.set(t.name, (teams.get(t.name) || 0) + 1);
    for (const p of g.players || []) {
      const row = players.get(p.name) || { name: p.name, team: p.team, games: 0, td: 0, int: 0, tackles: 0, sacks: 0, ff: 0 };
      row.games += 1; row.td += tds(p.stats); row.int += p.stats.interceptions || 0; row.tackles += p.stats.tackles || 0; row.sacks += p.stats.sacks || 0; row.ff += p.stats.forced_fumbles || 0;
      row.team = p.team || row.team;
      players.set(p.name, row);
    }
  }
  const top = (key) => [...players.values()].filter((p) => p[key] > 0).sort((a, b) => b[key] - a[key] || b.games - a.games)[0] || null;
  const favorite = [...teams.entries()].sort((a, b) => b[1] - a[1])[0] || null;
  const mostUsed = [...players.values()].sort((a, b) => b.games - a.games || b.td - a.td)[0] || null;
  const points = list.reduce((n, g) => n + (g.teams || []).reduce((m, t) => m + (t.score || 0), 0), 0);
  return { games: list.length, points, favorite, mostUsed, scorer: top("td"), hawk: top("int"), enforcer: top("tackles"), rusher: top("sacks") };
}

// ---------------------------------------------------------------------------
// The pieces.
function card(label, value, detail) {
  return el("div", { class: "wrap-card" }, el("span", { class: "wrap-label", text: label }), el("strong", { class: "wrap-value", text: value }), detail ? el("span", { class: "wrap-detail", text: detail }) : null);
}

function wrappedBlock() {
  const sec = el("section", { class: "block wrapped" }, el("h2", { text: "Your Reborn, so far" }));
  const g = state.games;
  if (!ok(g)) { sec.append(partial(`Your games could not be read: ${g ? g.error : "not loaded"}`)); return sec; }
  const list = games();
  if (!list.length) {
    sec.append(el("p", { class: "note", text: "Play a game and it shows up here: the score, every player's line, and who carried you." }));
    return sec;
  }
  const s = summary(list);
  const grid = el("div", { class: "wrap-grid" });
  grid.append(card("Games recorded", String(s.games), `${s.points} points scored across them`));
  if (s.favorite) grid.append(card("Team you saw most", s.favorite[0], `${s.favorite[1]} of ${s.games} games, either side`));
  if (s.scorer) grid.append(card("Top scorer", s.scorer.name, `${s.scorer.td} touchdowns · ${s.scorer.team}`));
  if (s.mostUsed) grid.append(card("On the field most", s.mostUsed.name, `${s.mostUsed.games} games · ${s.mostUsed.team}`));
  if (s.hawk) grid.append(card("Ball hawk", s.hawk.name, `${s.hawk.int} interceptions`));
  if (s.enforcer) grid.append(card("Enforcer", s.enforcer.name, `${s.enforcer.tackles} tackles`));
  if (s.rusher) grid.append(card("Pass rusher", s.rusher.name, `${s.rusher.sacks} sacks`));
  sec.append(grid);
  sec.append(el("p", { class: "note", text: "Both teams count until the recorder marks which side you played." }));
  return sec;
}

function totalsTable(g) {
  const totals = Array.isArray(g.team_totals) ? g.team_totals : null;
  if (!totals || totals.length !== 2) return null;
  const rows = [["Score", "score"], ["Touchdowns", "total_td"], ["Rushing TD", "rush_td"], ["Passing TD", "pass_td"], ["Defensive TD", "def_td"], ["Sacks", "sacks"], ["Interceptions", "int"], ["Forced fumbles", "ff"], ["First downs", "first_downs"], ["Gamebreakers", "gamebreakers"], ["Style points", "style"]];
  const table = el("table", { class: "box-table totals" });
  table.append(el("thead", {}, el("tr", {}, el("th", { text: "" }), el("th", { text: g.teams?.[0]?.name || "Home" }), el("th", { text: g.teams?.[1]?.name || "Away" }))));
  const body = el("tbody");
  for (const [label, key] of rows) body.append(el("tr", {}, el("th", { text: label }), el("td", { text: String(totals[0][key] ?? 0) }), el("td", { text: String(totals[1][key] ?? 0) })));
  table.append(body);
  return table;
}

function playersTable(g, side) {
  const list = (g.players || []).filter((p) => p.side === side).sort((a, b) => tds(b.stats) - tds(a.stats) || (b.stats.tackles || 0) - (a.stats.tackles || 0));
  if (!list.length) return null;
  const teamName = list[0].team || g.teams?.[side]?.name || `Side ${side + 1}`;
  const table = el("table", { class: "box-table players" });
  table.append(el("caption", { text: teamName }));
  table.append(el("thead", {}, el("tr", {}, ...["Player", "TD", "Pass", "Rush", "Rec", "INT", "Tkl", "Sack", "FF", "Fum"].map((h) => el("th", { text: h })))));
  const body = el("tbody");
  for (const p of list) {
    const s = p.stats;
    body.append(el("tr", {}, el("th", { text: p.name }), ...[tds(s), s.pass_td, s.rush_td, s.rec_td, s.interceptions, s.tackles, s.sacks, s.forced_fumbles, s.fumbles].map((v) => el("td", { text: String(v || 0) }))));
  }
  table.append(body);
  return table;
}

function gameRow(g) {
  const [a, b] = [g.teams?.[0], g.teams?.[1]];
  const isOpen = open === g.key;
  const row = el("div", { class: "game-row" + (isOpen ? " open" : "") });
  const head = el("button", { type: "button", class: "game-head", "aria-expanded": String(isOpen), onclick: () => { open = isOpen ? null : g.key; draw(); } },
    el("span", { class: "game-when", text: when(g.captured_at) }),
    el("span", { class: "game-score" }, el("span", { class: "game-team", text: a?.name || "?" }), el("strong", { text: `${a?.score ?? 0} – ${b?.score ?? 0}` }), el("span", { class: "game-team", text: b?.name || "?" })),
    el("span", { class: "game-source", text: /online/i.test(g.source || "") ? "Online" : "Offline" }));
  row.append(head);
  if (isOpen) {
    const box = el("div", { class: "box-score" });
    const totals = totalsTable(g);
    if (totals) box.append(totals);
    for (const side of [0, 1]) { const t = playersTable(g, side); if (t) box.append(t); }
    if (g.sides_verified === false) box.append(el("p", { class: "note", text: "The recorder could not confirm which side each team was on for this game." }));
    row.append(box);
  }
  return row;
}

function gamesBlock() {
  const sec = el("section", { class: "block" }, el("h2", { text: "Games" }));
  const g = state.games;
  if (!ok(g)) { sec.append(partial(`Your games could not be read: ${g ? g.error : "not loaded"}`)); return sec; }
  const list = games();
  if (!list.length) { sec.append(el("p", { class: "note", text: "No games recorded yet." })); return sec; }
  const wrap = el("div", { class: "game-list" });
  for (const game of list) wrap.append(gameRow(game));
  sec.append(wrap);
  return sec;
}

function accountBlock() {
  const sec = el("section", { class: "block" }, el("h2", { text: "Account" }));
  const id = state.identity;
  if (!ok(id)) sec.append(partial(`Your account could not be read: ${id ? id.error : "not loaded"}`));
  sec.append(facts([fact("Discord", ok(id) ? (id.signed_in ? `Signed in as ${id.display_name}` : "not signed in") : null)]));
  if (ok(id) && id.signed_in) {
    const box = el("input", { type: "checkbox", id: "show-offline", onchange: (e) => act("Offline visibility", "identity_set_show_offline", { show: !!e.target.checked }, () => { said = "Saved."; }) });
    box.checked = !!id.show_offline;
    sec.append(el("label", { for: "show-offline", class: "name" }, box, el("span", { text: " Show my offline games on my profile" })));
    sec.append(el("div", { class: "mod" }, el("button", { type: "button", text: "Sign out", disabled: busy, onclick: () => act("Sign out", "identity_sign_out", {}, () => { said = "Signed out."; }) })));
  } else {
    sec.append(el("p", { class: "note", text: "Sign in from the Play page to track your stats from your games." }));
  }
  return sec;
}

function resultsBlock() {
  const sec = el("section", { class: "block" }, el("h2", { text: "Sending results" }));
  const q = state.queue;
  if (!ok(q)) sec.append(partial(`Results could not be read: ${q ? q.error : "not loaded"}`));
  sec.append(facts([fact("Waiting to send", ok(q) ? String(q.depth ?? 0) : null)]));
  sec.append(el("p", { class: "note", text: "Results are sent on their own after each game. Send now if you were offline when you played." }));
  sec.append(el("div", { class: "mod" }, el("button", { type: "button", class: "write", text: "Send results now", disabled: busy, onclick: () => act("Send", "stats_submit", {}, (r) => { said = r.note; }) })));
  if (said) sec.append(el("div", { class: "said" }, el("p", { text: said })));
  return sec;
}

function draw() {
  const screen = document.getElementById("screen");
  const parts = [];
  if (problem) parts.push(refusal(problem));
  parts.push(wrappedBlock(), gamesBlock(), accountBlock(), resultsBlock());
  parts.push(el("button", { type: "button", text: "Refresh", disabled: busy, onclick: refresh }));
  screen.replaceChildren(...parts);
}

async function refresh() {
  problem = null;
  try {
    await load();
  } catch (e) {
    problem = reason(e);
  }
  draw();
}

markCurrent();
wireCoreFold();
refresh();
