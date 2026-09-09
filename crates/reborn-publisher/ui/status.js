// The Status page: what the bucket serves, what sits in the local origin,
// and what this machine has ready to publish with.
//
// Four blocks, in the order `status.rs`'s doc names: Live, Local origin,
// This machine, Recent. Every value drawn here comes from one of
// `publisher_status`, `publisher_environment` or `publisher_recent`, read
// through `shell.js`'s `call`, which never throws: a rejected command
// becomes `{ok: false, error}` and is drawn as a `refusal()`, never an
// uncaught exception that would leave the rest of the page blank too.
// Every field that can be null (a bucket that could not be reached, a
// local origin that does not exist yet, a journal with nothing in it) is
// checked before it is read, and every such absence says why instead of
// rendering nothing.
//
// This page computes no publishing rule. `status.rs` already turned
// `PendingReason` into a sentence and diffed the local pins against the
// unpinned components on disk; this file only reads what came back.

import { el, call, partial, refusal, facts, fact, fmtBytes, markCurrent } from "./shell.js";
import { panel } from "./panel.js";

markCurrent();

const screen = document.getElementById("screen");

function block(title, ...children) {
  return el("section", { class: "block" }, el("h2", { text: title }), ...children);
}

/// An RFC 3339 UTC timestamp (what the journal stamps every record with),
/// shown in the reader's own local time and labeled as such: a bare
/// timestamp reads as local to whoever looks at it, and this one is not.
function localTime(iso) {
  if (typeof iso !== "string" || iso === "") return "unknown time";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return `${iso} (not a readable time)`;
  return `${d.toLocaleString()} local time`;
}

function problemsList(problems) {
  return (Array.isArray(problems) ? problems : []).map((p) => partial(p));
}

// -- Live ---------------------------------------------------------------

function renderLive(answer) {
  if (!answer.ok) {
    return block("Live", refusal(`Status could not be read: ${answer.error}`));
  }
  const value = answer.value || {};
  const hosted = value.hosted || null;
  const launcher = value.launcher || null;
  const problems = Array.isArray(value.problems) ? value.problems : [];
  const children = [];

  // The signature line leads the page: per this task's brief, an
  // unverified hosted release is the most important thing here.
  if (hosted) {
    if (hosted.verified) {
      children.push(
        el("p", {
          class: "said",
          text: `The hosted release's signature verified (version ${hosted.version}, sequence ${hosted.sequence}).`,
        })
      );
    } else {
      const why = problems.find((p) => p.includes("signature")) || "It could not be confirmed as genuine.";
      children.push(
        el("p", {
          class: "refusal",
          role: "alert",
          text: `The hosted release's signature did NOT verify. ${why}`,
        })
      );
    }
  } else {
    const why =
      problems.find((p) => p.toLowerCase().includes("release")) ||
      "The bucket's release could not be read.";
    children.push(el("p", { class: "refusal", role: "alert", text: `No hosted release could be read. ${why}` }));
  }

  children.push(
    facts([
      fact("Release version", hosted ? hosted.version : null),
      fact("Release sequence", hosted ? hosted.sequence : null),
      fact("Launcher version", launcher ? launcher.version : null),
      fact("Launcher size", launcher ? fmtBytes(launcher.bytes) : null),
      fact("Launcher SHA-256", launcher ? launcher.sha256 : null),
      fact("Download link (file)", launcher ? launcher.file_name : null),
    ])
  );

  if (hosted && Array.isArray(hosted.pins) && hosted.pins.length > 0) {
    children.push(el("p", { class: "hint", text: "Pins the bucket serves:" }));
    children.push(facts(hosted.pins.map((p) => fact(p.component, p.version))));
  } else if (!launcher) {
    const why =
      problems.find((p) => p.toLowerCase().includes("launcher")) ||
      "The launcher's own update channel could not be read.";
    children.push(partial(`No launcher release could be read. ${why}`));
  }

  const shown = problems.filter((p) => !p.includes("signature"));
  children.push(...problemsList(shown));

  return block("Live", ...children);
}

// -- Local origin ---------------------------------------------------------

function pendingRow(p) {
  const bucketSide = p.hosted ? `the bucket has ${p.hosted}` : "the bucket does not serve it";
  return el(
    "tr",
    {},
    el("th", { text: p.component }),
    el("td", {}, `this machine has ${p.local}, ${bucketSide}. ${p.reason_text}.`)
  );
}

function uploadPanel() {
  const host = el("div", {});
  panel(host, {
    operation: "status",
    describe: "Push whatever this machine has published locally that the bucket does not yet serve.",
    plan: () => call("publisher_status"),
    run: async () => ({
      ok: false,
      error:
        "This build has no command that re-uploads from Status directly. Publish the pending " +
        "component again from its own page (Roster, Content, Game or Launcher); that page's Run " +
        "step uploads for you.",
    }),
    verify: () => call("publisher_status"),
  });
  return host;
}

function renderLocalOrigin(answer) {
  if (!answer.ok) {
    return block("Local origin", refusal(`Status could not be read: ${answer.error}`));
  }
  const value = answer.value || {};
  const local = value.local || null;
  const pending = Array.isArray(value.pending) ? value.pending : [];
  const problems = Array.isArray(value.problems) ? value.problems : [];
  const children = [];

  if (!local) {
    const why =
      problems.find((p) => p.toLowerCase().includes("origin") || p.toLowerCase().includes("release.json")) ||
      "No local origin was found on this machine.";
    children.push(partial(why));
  } else {
    children.push(facts([fact("Local version", local.version), fact("Local sequence", local.sequence)]));
    if (Array.isArray(local.unpinned) && local.unpinned.length > 0) {
      const names = local.unpinned.map((u) => `${u.component} ${u.version}`).join(", ");
      children.push(
        partial(
          `${local.unpinned.length} component(s) on disk are not part of the signed local release: ${names}. ` +
            "Published, but never carried into a release: a half-finished publish."
        )
      );
    }
  }

  if (pending.length === 0) {
    children.push(
      partial(
        local
          ? "Nothing published locally is pending. The bucket serves what this machine has."
          : "Pending components cannot be computed without a local origin."
      )
    );
  } else {
    children.push(el("table", { class: "facts" }, ...pending.map(pendingRow)));
  }

  children.push(uploadPanel());

  return block("Local origin", ...children);
}

// -- This machine ---------------------------------------------------------

function machineRow(item) {
  const isVault = item.kind === "vault";
  const td = el("td", { class: item.found ? null : "absent" });
  td.append(document.createTextNode(item.found ? (isVault ? "stored" : item.path) : "missing"));
  if (item.note) {
    td.append(el("span", { class: "hint", text: item.note }));
  }
  if (!item.found && !isVault) {
    td.append(
      el("button", {
        text: "Browse",
        onclick: async (ev) => {
          ev.currentTarget.disabled = true;
          await call("settings_browse", { which: item.key });
          renderPage();
        },
      })
    );
  }
  return el("tr", {}, el("th", { text: item.label }), td);
}

function renderMachine(answer) {
  if (!answer.ok) {
    return block("This machine", refusal(`The environment could not be read: ${answer.error}`));
  }
  const value = answer.value || {};
  const items = Array.isArray(value.items) ? value.items : [];
  const problems = Array.isArray(value.problems) ? value.problems : [];

  if (items.length === 0) {
    return block("This machine", partial("Nothing was checked. The environment check itself found nothing to report."));
  }

  return block("This machine", el("table", { class: "facts" }, ...items.map(machineRow)), ...problemsList(problems));
}

// -- Recent ---------------------------------------------------------------

function recentRow(r) {
  const dryRun = r.dry_run ? " (dry run)" : "";
  const summary = r.summary || "(no summary)";
  return fact(localTime(r.at), `${r.operation}${dryRun}, ${r.outcome}: ${summary}`);
}

function renderRecent(answer) {
  if (!answer.ok) {
    return block("Recent", refusal(`Recent history could not be read: ${answer.error}`));
  }
  const value = answer.value || {};
  if (value.error) {
    return block("Recent", refusal(`The journal could not be read: ${value.error}`));
  }
  const records = Array.isArray(value.records) ? value.records : [];
  if (records.length === 0) {
    return block("Recent", partial("No journal records yet. Nothing has been published from this machine."));
  }
  return block("Recent", facts(records.map(recentRow)), ...problemsList(value.problems));
}

// -- Page -------------------------------------------------------------

async function renderPage() {
  const [statusAnswer, envAnswer, recentAnswer] = await Promise.all([
    call("publisher_status"),
    call("publisher_environment"),
    call("publisher_recent", { n: 10 }),
  ]);

  screen.replaceChildren(
    renderLive(statusAnswer),
    renderLocalOrigin(statusAnswer),
    renderMachine(envAnswer),
    renderRecent(recentAnswer)
  );
}

renderPage();
