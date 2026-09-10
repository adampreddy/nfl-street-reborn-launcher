// Stats, as a page: the Discord account and the game results sent from this
// PC. Moved out of Settings on 2026-09-10 (Adam: "the stats tab in settings
// needs a separate tab"). Signing in stays on the Play page, where a game
// is about to be played.

import { call, el, fact, facts, markCurrent, partial, reason, refusal, wireCoreFold } from "./shell.js";

const state = {};
let problem = null;
let busy = false;
let said = null;

function ok(x) {
  return x && !x.error;
}

async function load() {
  const reads = {
    identity: call("identity_status"),
    queue: call("stats_queue"),
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
  const sec = el("section", { class: "block" }, el("h2", { text: "Game results" }));
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
  parts.push(accountBlock(), resultsBlock());
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
