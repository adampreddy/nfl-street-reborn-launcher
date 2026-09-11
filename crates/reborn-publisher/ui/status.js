// Home: what players get right now, anything this computer has not sent
// to the server yet (with the one Upload now button), what is missing on
// this computer, and what was done recently.
//
// Every value drawn here comes from `publisher_status`,
// `publisher_environment` or `publisher_recent`, read through `shell.js`'s
// `call`, which never throws: a rejected command becomes `{ok: false,
// error}` and is drawn as a sentence, never a blank. Every field that can
// be null (a server that could not be reached, a computer that has never
// published, an empty journal) says why instead of rendering nothing.
//
// This page computes no publishing rule. `status.rs` already decided what
// is pending and why; this file puts it in words a person reads.

import { el, call, refusal, partial, facts, fact, fmtBytes, markCurrent, thingName, thingWord } from "./shell.js";
import { job } from "./job.js";

markCurrent();

const screen = document.getElementById("screen");

// The order the things players get are listed in: the launcher first (it
// is the program), then what it downloads, most often changed first.
const ORDER = ["launcher", "roster", "textures", "game-patch", "runtime", "mods", "offline-content", "online-content", "controller-db"];

function block(title, ...children) {
  return el("section", { class: "block" }, el("h2", { text: title }), ...children);
}

/// An RFC 3339 UTC timestamp (what the journal stamps every record with),
/// in the reader's own local time.
function localTime(iso) {
  if (typeof iso !== "string" || iso === "") return "unknown time";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return `${iso} (not a readable time)`;
  return d.toLocaleString([], { month: "numeric", day: "numeric", hour: "numeric", minute: "2-digit" });
}

// -- What players get right now -------------------------------------------

const playersHost = el("div");

function renderPlayers(answer) {
  if (!answer.ok) {
    playersHost.replaceChildren(el("div", { class: "home-card" }, el("h2", { text: "What players get right now" }), refusal(`The server could not be read: ${answer.error}`)));
    return;
  }
  const value = answer.value || {};
  const hosted = value.hosted || null;
  const launcher = value.launcher || null;
  const problems = Array.isArray(value.problems) ? value.problems : [];
  const children = [el("h2", { text: "What players get right now" })];

  const rows = [];
  if (launcher && launcher.version) rows.push(["launcher", launcher.version]);
  const pins = hosted && Array.isArray(hosted.pins) ? hosted.pins : [];
  for (const p of pins) if (p && p.component && p.component !== "launcher") rows.push([p.component, p.version]);
  rows.sort((a, b) => (ORDER.indexOf(a[0]) + 1 || 99) - (ORDER.indexOf(b[0]) + 1 || 99));
  if (rows.length) {
    children.push(
      el("div", { class: "home-rows" }, ...rows.map(([id, v]) => el("div", { class: "home-row" }, el("span", { text: thingName(id) }), el("strong", { text: v || "not known" }))))
    );
  }

  if (hosted) {
    if (hosted.verified) {
      children.push(el("p", { class: "sentence ok", text: `Signed and checked. This is content release ${hosted.sequence}.` }));
    } else {
      const why = problems.find((p) => p.includes("signature")) || "It could not be confirmed as genuine.";
      children.push(el("p", { class: "sentence bad", role: "alert", text: `The release on the server did NOT pass its signature check. ${why}` }));
    }
  } else {
    const why = problems.find((p) => p.toLowerCase().includes("release")) || "The server's release could not be read.";
    children.push(el("p", { class: "sentence bad", role: "alert", text: `No release could be read from the server. ${why}` }));
  }
  if (!launcher) {
    const why = problems.find((p) => p.toLowerCase().includes("launcher")) || "The launcher's own update file could not be read.";
    children.push(partial(`The launcher version on the server could not be read. ${why}`));
  }

  const shown = problems.filter((p) => !p.includes("signature") && !p.toLowerCase().includes("release") && !p.toLowerCase().includes("launcher"));
  for (const p of shown) children.push(partial(p));

  children.push(
    el(
      "details",
      { class: "how" },
      el("summary", { text: "Details" }),
      facts([
        fact("Content release version", hosted ? hosted.version : null),
        fact("Content release number", hosted ? hosted.sequence : null),
        fact("Launcher size", launcher ? fmtBytes(launcher.bytes) : null),
        fact("Launcher SHA-256", launcher ? launcher.sha256 : null),
        fact("Launcher file", launcher ? launcher.file_name : null),
      ])
    )
  );
  playersHost.replaceChildren(el("div", { class: "home-card" }, ...children));
}

// -- Not yet on the server --------------------------------------------------

const pendingHost = el("div");
const jobHost = el("div");

function pendingSentence(p) {
  const name = thingName(p.component);
  const code = String(p.reason || "").toLowerCase();
  if (code.includes("not_served") || code.includes("notserved")) {
    return `${name} ${p.local}: the server's release does not name it yet.`;
  }
  if (code.includes("older")) {
    return `${name} ${p.local}: the server still has ${p.hosted || "an older version"}.`;
  }
  if (code.includes("newer")) {
    return `${name} ${p.local}: this computer signed a newer release than the server has.`;
  }
  const serverSide = p.hosted ? `the server has ${p.hosted}` : "the server does not have it";
  return `${name} ${p.local}: ${serverSide}. ${p.reason_text || ""}`.trim();
}

function renderPending(answer) {
  if (!answer.ok) {
    pendingHost.replaceChildren(refusal(`What is pending could not be read: ${answer.error}`));
    return;
  }
  const value = answer.value || {};
  const local = value.local || null;
  const pending = Array.isArray(value.pending) ? value.pending : [];
  const problems = Array.isArray(value.problems) ? value.problems : [];
  const children = [];
  if (!local) {
    const why = problems.find((p) => p.toLowerCase().includes("origin") || p.toLowerCase().includes("release.json")) || "This computer has not published anything yet.";
    children.push(partial(why));
  } else if (pending.length === 0) {
    children.push(el("p", { class: "sentence ok", text: "The server has everything this computer published." }));
  } else {
    children.push(el("ul", { class: "plain-list" }, ...pending.map((p) => el("li", { text: pendingSentence(p) }))));
  }
  if (local && Array.isArray(local.unpinned) && local.unpinned.length > 0) {
    const names = local.unpinned.map((u) => `${thingWord(u.component)} ${u.version}`).join(", ");
    children.push(partial(`Published on this computer but not part of any release, so players never see it: ${names}.`));
  }
  pendingHost.replaceChildren(...children);
}

job(jobHost, {
  operation: "upload",
  verb: () => "Upload now",
  plan: () => call("upload_plan"),
  run: (dryRun) => call("upload_run", { dry_run: dryRun }),
  verify: () => call("publisher_status"),
  describePlan: (plan) => {
    if (!plan || typeof plan !== "object") return ["The check came back with nothing to describe."];
    if (!plan.objects) return ["The server already has everything this computer published. Nothing needs sending."];
    const lines = [`${plan.objects} file(s), ${fmtBytes(plan.bytes)}, to send to the server.`];
    const items = Array.isArray(plan.items) ? plan.items : [];
    const named = items.slice(0, 4).map((i) => i.key).join(", ");
    if (named) lines.push(items.length > 4 ? `Including ${named} and ${items.length - 4} more.` : `Files: ${named}.`);
    return lines;
  },
  describeDone: () => "Done. The server now has everything this computer published.",
  describeVerified: (value) => {
    const pending = value && Array.isArray(value.pending) ? value.pending : null;
    if (pending === null) return "the server could not be read back.";
    return pending.length === 0 ? "the server serves what this computer has." : `${pending.length} thing(s) still pending.`;
  },
  confirmLabel: "Yes, upload",
  onSettled: () => refreshStatus(),
});

// -- This computer --------------------------------------------------------

const machineHost = el("div");

function machineRow(item) {
  const isVault = item.kind === "vault";
  const li = el("li", { text: `${item.label}: missing. ${item.note || ""}`.trim() });
  if (!isVault) {
    li.append(
      " ",
      el("button", {
        text: "Browse",
        onclick: async (ev) => {
          ev.currentTarget.disabled = true;
          await call("settings_browse", { which: item.key });
          refreshMachine();
        },
      })
    );
  } else {
    li.append(" ", el("a", { href: "keys.html", text: "Store it on Keys and settings." }));
  }
  return li;
}

function renderMachine(answer) {
  if (!answer.ok) {
    machineHost.replaceChildren(refusal(`This computer could not be checked: ${answer.error}`));
    return;
  }
  const value = answer.value || {};
  const items = Array.isArray(value.items) ? value.items : [];
  const problems = Array.isArray(value.problems) ? value.problems : [];
  if (items.length === 0) {
    machineHost.replaceChildren(partial("Nothing was checked. The check itself found nothing to report."));
    return;
  }
  const missing = items.filter((i) => !i.found);
  const children = [];
  if (missing.length === 0) {
    children.push(el("p", { class: "sentence ok", text: "Everything this computer needs is in place." }));
  } else {
    children.push(el("ul", { class: "plain-list" }, ...missing.map(machineRow)));
  }
  for (const p of problems) children.push(partial(p));
  children.push(
    el(
      "details",
      { class: "how" },
      el("summary", { text: "Everything checked" }),
      facts(items.map((i) => fact(i.label, i.found ? (i.kind === "vault" ? "stored" : i.path) : "missing")))
    )
  );
  machineHost.replaceChildren(...children);
}

// -- Recent -----------------------------------------------------------------

const recentHost = el("div");

const OPERATION_WORDS = {
  upload: "Upload to the server",
  status: "Upload to the server",
  content: "Textures, runtime or mods",
  launcher: "Launcher update",
  roster: "Roster",
  history: "Roll back",
  game_build: "Build an ISO",
  game_patch: "Game patch",
  set: "Full installer set",
};

function recentSentence(r) {
  const what = OPERATION_WORDS[r.operation] || r.operation;
  const how = r.dry_run ? "checked only" : r.outcome || "unknown";
  return `${localTime(r.at)}: ${what}, ${how}. ${r.summary || ""}`.trim();
}

function renderRecent(answer) {
  if (!answer.ok) {
    recentHost.replaceChildren(refusal(`Recent history could not be read: ${answer.error}`));
    return;
  }
  const value = answer.value || {};
  if (value.error) {
    recentHost.replaceChildren(refusal(`The history file could not be read: ${value.error}`));
    return;
  }
  const records = Array.isArray(value.records) ? value.records : [];
  if (records.length === 0) {
    recentHost.replaceChildren(partial("Nothing has been published from this computer yet."));
    return;
  }
  const problems = Array.isArray(value.problems) ? value.problems : [];
  recentHost.replaceChildren(el("ul", { class: "plain-list" }, ...records.map((r) => el("li", { text: recentSentence(r) }))), ...problems.map((p) => partial(p)));
}

// -- Page -------------------------------------------------------------------

screen.replaceChildren(
  playersHost,
  block("Not yet on the server", pendingHost, jobHost),
  block("This computer", machineHost),
  block("Recent", recentHost)
);

async function refreshStatus() {
  const answer = await call("publisher_status");
  renderPlayers(answer);
  renderPending(answer);
  renderRecent(await call("publisher_recent", { n: 10 }));
}

async function refreshMachine() {
  renderMachine(await call("publisher_environment"));
}

refreshStatus();
refreshMachine();
