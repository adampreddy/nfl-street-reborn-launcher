// Local captures contain both teams. Do not infer the user's side or wins.
import { call, el, markCurrent, partial, reason, refusal, wireCoreFold } from "./shell.js";

const state = {};
const PAGE_SIZE = 6;
let filter = "all", search = "", page = 0;
let busy = false, refreshing = false, problem = null, said = null;
let gameDialog = null;
const ok = value => value && !value.error;
const games = () => Array.isArray(state.games) ? state.games : [];
// Capture source is always "dolphin". The queue records the actual game mode.
const mode = game => state.modes?.get(game.key) || "unknown";
const modeLabel = game => ({ online: "Online", offline: "Offline", unknown: "Unknown" })[mode(game)];
const teamName = (game, side) => game.teams?.[side]?.name || `Team ${side + 1}`;
const number = value => typeof value === "number" && Number.isFinite(value) ? value.toLocaleString() : "–";

function dateParts(iso) {
  const date = new Date(iso);
  if (!Number.isFinite(date.getTime())) return ["Date unavailable", ""];
  return [date.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" }),
    date.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" })];
}

async function load() {
  const keys = ["identity", "queue", "games"];
  const results = await Promise.allSettled([call("identity_status"), call("stats_queue"), call("stats_games")]);
  keys.forEach((key, i) => { state[key] = results[i].status === "fulfilled" ? results[i].value : { error: reason(results[i].reason) }; });
  state.modes = new Map((state.queue?.entries || [])
    .filter(entry => entry.capture_folder === "online" || entry.capture_folder === "offline")
    .map(entry => [entry.key, entry.capture_folder]));
}

async function act(label, command, args, after) {
  if (busy) return;
  busy = true; problem = null; draw();
  try { after(await call(command, args)); }
  catch (error) { problem = `${label}: ${reason(error)}`; }
  await load();
  busy = false; draw();
}

function metric(label, value) {
  return el("div", { class: "stats-metric" }, el("strong", { text: number(value) }), el("span", { text: label }));
}

function gameRow(game) {
  const [date, time] = dateParts(game.captured_at);
  const score = side => number(game.teams?.[side]?.score);
  const button = el("button", {
    type: "button", class: "stats-game", id: `stats-game-${encodeURIComponent(game.key)}`,
    "aria-haspopup": "dialog",
    "aria-label": `View game: ${teamName(game, 0)} ${score(0)}, ${teamName(game, 1)} ${score(1)}. ${date}, ${modeLabel(game)}.`,
    onclick: () => openGame(game, button),
  },
  el("span", { class: "stats-game-date" }, el("span", { text: date }), el("small", { text: time })),
  el("span", { class: "stats-matchup" }, ...[0, 1].map(side => el("span", { class: "stats-matchup-team" },
    el("span", { text: teamName(game, side) }), el("strong", { text: score(side) })))),
  el("span", { class: "stats-game-mode", text: modeLabel(game) }),
  el("span", { class: "stats-game-arrow", "aria-hidden": "true" }));
  return button;
}

function filteredGames() {
  const term = search.trim().toLocaleLowerCase();
  return games().filter(game => (filter === "all" || mode(game) === filter) && (!term ||
    [...(game.teams || []), ...(game.players || [])].some(item => String(item.name || "").toLocaleLowerCase().includes(term))))
    .sort((a, b) => (Date.parse(b.captured_at) || 0) - (Date.parse(a.captured_at) || 0));
}

// Keep search inputs and surrounding content mounted while results change.
function renderList() {
  const host = document.getElementById("stats-results");
  if (!host) return;
  const list = filteredGames();
  const pages = Math.max(1, Math.ceil(list.length / PAGE_SIZE));
  page = Math.max(0, Math.min(page, pages - 1));
  const start = page * PAGE_SIZE;
  const rows = list.slice(start, start + PAGE_SIZE).map(gameRow);
  if (!rows.length) rows.push(el("div", { class: "stats-no-results", role: "status" },
    el("p", { text: "No games match your search." }),
    el("button", { type: "button", text: "Clear filters", onclick: () => {
      search = ""; filter = "all"; page = 0;
      document.getElementById("stats-search").value = "";
      document.getElementById("stats-filter").value = "all";
      renderList(); document.getElementById("stats-search").focus();
    } })));
  const pagination = el("div", { class: "stats-pagination" },
    el("span", { role: "status", text: list.length ? `${start + 1}–${Math.min(start + PAGE_SIZE, list.length)} of ${list.length} games` : "0 games" }));
  if (pages > 1) {
    const changePage = delta => { page += delta; renderList(); document.querySelector(".stats-game")?.focus(); };
    pagination.append(el("div", { class: "stats-page-controls" },
      el("button", { type: "button", text: "Previous", disabled: page === 0, onclick: () => changePage(-1) }),
      el("span", { text: `${page + 1} / ${pages}`, "aria-label": `Page ${page + 1} of ${pages}` }),
      el("button", { type: "button", text: "Next", disabled: page + 1 === pages, onclick: () => changePage(1) })));
  }
  host.replaceChildren(el("div", { class: "stats-game-list" }, ...rows), pagination);
}

function historyBlock() {
  const section = el("section", { class: "stats-history", "aria-labelledby": "stats-history-title" });
  const counts = games();
  const online = counts.filter(game => mode(game) === "online").length;
  const offline = counts.filter(game => mode(game) === "offline").length;
  const unknown = counts.length - online - offline;
  section.append(el("div", { class: "stats-history-heading" },
    el("h2", { id: "stats-history-title", text: "Game history" }),
    counts.length ? el("div", { class: "stats-counts", "aria-label": "Recorded game totals" },
      metric("recorded", counts.length), metric("offline", offline), metric("online", online), unknown ? metric("unknown", unknown) : null) : null,
    el("button", { type: "button", id: "stats-refresh", text: refreshing ? "Refreshing…" : "Refresh", disabled: refreshing || busy, onclick: refresh })));
  if (!ok(state.games)) {
    section.append(el("div", { class: "stats-state" }, partial("Game history could not be loaded. Try refreshing.")));
    return section;
  }
  if (!counts.length) {
    section.append(el("div", { class: "stats-empty" },
      el("span", { class: "stats-empty-score", "aria-hidden": "true", text: "– : –" }),
      el("h3", { text: "No games recorded yet" }),
      el("p", { text: "Finish a game to see its score, team totals, and player stats here." }),
      el("a", { href: "index.html", class: "button", text: "Go to Play" })));
    return section;
  }
  const input = el("input", { type: "search", id: "stats-search", placeholder: "Search teams or players", "aria-label": "Search games", value: search,
    oninput: event => { search = event.target.value; page = 0; renderList(); } });
  const select = el("select", { id: "stats-filter", "aria-label": "Game type", onchange: event => { filter = event.target.value; page = 0; renderList(); } });
  for (const [value, text] of [["all", "All games"], ["offline", "Offline"], ["online", "Online"]])
    select.append(el("option", { value, text, selected: filter === value }));
  section.append(el("div", { class: "stats-filters" }, input, select),
    el("div", { class: "stats-list-labels", "aria-hidden": "true" }, el("span", { text: "Played" }), el("span", { text: "Matchup" }), el("span", { text: "Mode" })),
    el("div", { id: "stats-results" }));
  return section;
}

function totalsTable(game) {
  if (!Array.isArray(game.team_totals) || game.team_totals.length !== 2)
    return el("p", { class: "stats-detail-empty", text: "Team totals were not recorded for this game. The recorded score is shown above." });
  const table = el("table", { class: "stats-table stats-totals" }, el("caption", { class: "stats-sr-only", text: "Team comparison" }),
    el("thead", {}, el("tr", {}, el("th", { scope: "col", text: teamName(game, 0) }), el("th", { scope: "col", text: "Stat" }), el("th", { scope: "col", text: teamName(game, 1) }))));
  const rows = [["Touchdowns", "total_td"], ["Passing TD", "pass_td"], ["Rushing TD", "rush_td"], ["Defensive TD", "def_td"],
    ["First downs", "first_downs"], ["Sacks", "sacks"], ["Interceptions", "int"], ["Forced fumbles", "ff"], ["Gamebreakers", "gamebreakers"], ["Style points", "style"]];
  table.append(el("tbody", {}, ...rows.map(([label, key]) => el("tr", {},
    el("td", { text: number(game.team_totals[0]?.[key]) }), el("th", { scope: "row", text: label }), el("td", { text: number(game.team_totals[1]?.[key]) })))));
  return table;
}

function playersTable(game, side, category) {
  const list = (game.players || []).filter(player => player.side === side);
  if (!list.length) return el("p", { class: "stats-detail-empty", text: "Player stats were not recorded for this team." });
  const columns = category === "offense"
    ? [["Passing TD", "pass_td"], ["Rushing TD", "rush_td"], ["Receiving TD", "rec_td"], ["Fumbles", "fumbles"]]
    : [["Tackles", "tackles"], ["Sacks", "sacks"], ["Interceptions", "interceptions"], ["Forced fumbles", "forced_fumbles"], ["Defensive TD", "defensive_td"]];
  const table = el("table", { class: "stats-table stats-players" }, el("caption", { class: "stats-sr-only", text: `${teamName(game, side)} ${category}` }));
  table.append(el("thead", {}, el("tr", {}, ...["Player", ...columns.map(([label]) => label)].map(text => el("th", { scope: "col", text })))));
  table.append(el("tbody", {}, ...list.map(player => {
    const stats = player.stats || {};
    const value = key => key === "defensive_td"
      ? (typeof stats.pick_six === "number" && typeof stats.fum_td === "number" ? stats.pick_six + stats.fum_td : null) : stats[key];
    return el("tr", {}, el("th", { scope: "row", text: player.name || "Unknown player" }), ...columns.map(([, key]) => el("td", { text: number(value(key)) })));
  })));
  return table;
}

function openGame(game, trigger) {
  if (gameDialog?.open) return;
  let view = "teams", side = 0, category = "offense";
  const [date, time] = dateParts(game.captured_at);
  const dialog = el("dialog", { class: "stats-dialog", "aria-labelledby": "stats-dialog-title" });
  gameDialog = dialog;
  const close = el("button", { type: "button", class: "stats-close", text: "Close", onclick: () => dialog.close() });
  dialog.append(el("div", { class: "stats-dialog-heading" }, el("div", {},
    el("h2", { id: "stats-dialog-title", text: "Game details" }), el("p", { text: [date, time, modeLabel(game)].filter(Boolean).join(" · ") })), close),
    el("div", { class: "stats-scoreboard" }, el("strong", { class: "stats-scoreboard-team", text: teamName(game, 0) }),
      el("span", { class: "stats-scoreboard-score", text: `${number(game.teams?.[0]?.score)} – ${number(game.teams?.[1]?.score)}` }),
      el("strong", { class: "stats-scoreboard-team", text: teamName(game, 1) })));
  const tabs = el("div", { class: "stats-view-tabs", "aria-label": "Box score views" });
  const body = el("div", { class: "stats-detail-body" });
  function renderView() {
    for (const button of tabs.children) button.setAttribute("aria-pressed", String(button.value === view));
    body.replaceChildren();
    if (view === "teams") body.append(totalsTable(game));
    else {
      const teams = el("div", { class: "stats-team-picker", "aria-label": "Player stats team" });
      for (const index of [0, 1]) teams.append(el("button", { type: "button", id: `stats-side-${index}`, text: teamName(game, index), "aria-pressed": String(side === index), onclick: () => {
        side = index; renderView(); document.getElementById(`stats-side-${index}`).focus({ preventScroll: true });
      } }));
      const categories = el("div", { class: "stats-category-picker", "aria-label": "Player stats category" });
      for (const [value, text] of [["offense", "Offense"], ["defense", "Defense"]]) categories.append(el("button", { type: "button", id: `stats-${value}`, text, "aria-pressed": String(category === value), onclick: () => {
        category = value; renderView(); document.getElementById(`stats-${value}`).focus({ preventScroll: true });
      } }));
      body.append(teams, categories, el("div", { class: "stats-table-scroll" }, playersTable(game, side, category)));
    }
    if (game.sides_verified !== true) body.append(el("p", { class: "stats-detail-note", text: "Team assignments could not be verified for this recording." }));
  }
  for (const [value, text] of [["teams", "Team stats"], ["players", "Player stats"]]) tabs.append(el("button", { type: "button", value, text, "aria-pressed": String(view === value), onclick: () => { view = value; renderView(); } }));
  dialog.append(tabs, body);
  dialog.addEventListener("close", () => { gameDialog = null; dialog.remove(); trigger.focus({ preventScroll: true }); });
  dialog.addEventListener("click", event => {
    if (event.target !== dialog) return;
    const rect = dialog.getBoundingClientRect();
    if (event.clientX < rect.left || event.clientX > rect.right || event.clientY < rect.top || event.clientY > rect.bottom) dialog.close();
  });
  renderView();
  document.getElementById("screen").append(dialog);
  dialog.showModal(); close.focus({ preventScroll: true });
}

function accountBlock() {
  const identity = state.identity, queue = state.queue;
  const status = !ok(identity) ? "Account unavailable" : identity.signed_in ? `Signed in as ${identity.display_name || "your Discord account"}` : "Not signed in";
  const details = el("details", { class: "stats-account" }, el("summary", {},
    el("span", { text: "Profile & sync" }), el("span", { class: "stats-account-status", text: status })));
  const profile = el("div", { class: "stats-account-section" }, el("h3", { text: "Profile" }));
  if (!ok(identity)) profile.append(partial("Your account could not be loaded. Try refreshing."));
  else if (identity.signed_in) {
    profile.append(el("label", { class: "stats-visibility", for: "show-offline" },
      el("input", { type: "checkbox", id: "show-offline", checked: !!identity.show_offline, disabled: busy,
        onchange: event => act("Offline visibility", "identity_set_show_offline", { show: !!event.target.checked }, () => { said = "Profile settings saved."; }) }),
      el("span", { text: "Show my offline games on my profile" })),
      el("button", { type: "button", id: "stats-sign-out", text: "Sign out", disabled: busy, onclick: () => act("Sign out", "identity_sign_out", {}, () => { said = "Signed out."; }) }));
  } else profile.append(el("p", { text: "Connect your Discord account to add these results to your profile." }), el("a", { href: "index.html", class: "button", text: "Sign in on Play" }));
  const sync = el("div", { class: "stats-account-section" }, el("h3", { text: "Result sync" }));
  if (!ok(queue)) sync.append(partial("Result sync status could not be loaded."));
  else sync.append(el("p", { text: queue.depth ? `${queue.depth} result${queue.depth === 1 ? " is" : "s are"} waiting to sync.` : "No results waiting to sync." }));
  sync.append(el("p", { class: "stats-secondary", text: "Results sync automatically after each game." }),
    el("button", { type: "button", id: "stats-send", text: "Send results now", disabled: busy, onclick: () => act("Sync", "stats_submit", {}, result => { said = result?.note || "Sync complete."; }) }));
  details.append(el("div", { class: "stats-account-grid" }, profile, sync));
  if (said) details.append(el("p", { class: "stats-sync-note", role: "status", text: said }));
  return details;
}

function draw() {
  const screen = document.getElementById("screen"), focusedId = document.activeElement?.id;
  const accountOpen = !!screen.querySelector?.(".stats-account")?.open;
  const account = accountBlock(); account.open = accountOpen;
  screen.replaceChildren(...(problem ? [refusal(problem)] : []), historyBlock(), account);
  if (games().length) renderList();
  if (focusedId) document.getElementById(focusedId)?.focus?.({ preventScroll: true });
}

async function refresh() {
  if (refreshing || busy) return;
  problem = null;
  refreshing = true;
  const button = document.getElementById("stats-refresh");
  if (button) { button.disabled = true; button.textContent = "Refreshing…"; }
  await load(); refreshing = false; draw();
}

markCurrent();
wireCoreFold();
refresh();
