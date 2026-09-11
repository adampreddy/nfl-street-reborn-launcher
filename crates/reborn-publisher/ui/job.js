// One job, one button. Every publishing page mounts this instead of the old
// plan / dry run / run / verify panel, and the shape is the same everywhere:
//
//   [ Publish textures 1.2.1 ]
//
//   press it -> the check runs (the page's `plan` command; it writes
//   nothing) -> a card says in plain words what would change for players,
//   with [Cancel] [Yes, publish] -> Yes runs the page's `run` command for
//   real (dry run off), with a bar while bytes move -> the page's `verify`
//   runs on its own -> "Done. ... Verified from the server at 10:41 PM."
//
// A refusal or a failure is one red sentence and a Copy button. The raw
// log is always there, behind a closed "Details".
//
// Nothing here decides a publishing rule, and nothing here invents a fact:
// every phase, line, byte count and sentence comes from polling
// `op_status` (the backend's own record) or from the page's own
// `describe*` functions applied to the values the backend returned.
//
// # The contract a page implements
//
// `job(host, config)` mounts into `host`. `config`:
//
//   operation: string
//     This page's operation name (the page half of the `op` strings its
//     commands pass to `ops::run_operation`, for example "roster"). This
//     job draws ONLY states whose `op_status.operation` equals this. Another
//     page's busy operation shows as one line naming that page; another
//     page's finished operation is ignored entirely. (Before this rule, the
//     last thing done on any page followed the operator to every page.)
//
//   verb: () => string
//     The primary button's text, computed at draw time from the page's own
//     inputs ("Publish textures 1.2.1"). Returning "" disables the button.
//
//   plan(): Promise<{ok, value?, error?}>
//   run(dryRun: boolean): Promise<{ok, value?, error?}>
//   verify?: () => Promise<{ok, value?, error?}>
//     One function per step, each calling exactly one command with the
//     page's own inputs. `run` is always called with `false`: the check IS
//     the dry run. `verify` is optional; when absent the done card has no
//     "Verified" line. A verify that is a plain read (not an operation)
//     resolves with its value, which `describeVerified` turns into words.
//
//   describePlan(plan, state): string[]
//     Plain-words lines for the card, from the plan value the backend
//     returned. A line starting with "Warning:" is drawn in the warning
//     style.
//   describeDone(state): string
//     The one sentence after a successful run ("Done. Players now get
//     textures 1.2.1.").
//   describeVerified?(value, state): string
//     For a plain-read verify: the sentence from its value. An operation
//     verify uses the backend's own summary.
//   confirmLabel?: string | (state) => string     default "Yes, publish"
//   emptyLabel?: string        the button's text while `verb()` is "", in
//                              the form of what to do first ("Choose a folder")
//   onSettled?(state)          called whenever this job's operation settles
//                              (the page may want to refresh a list).
//
// Polling starts only AFTER a request is accepted, never before: started
// before, the first poll read the previous operation's finished state and
// stopped, and the page sat on a spinner while the backend finished alone.

import { call, el, fmtBytes } from "./shell.js";

const JOB_POLL_MS = 400;
const JOB_BUSY = new Set(["planning", "running", "verifying"]);

/// The page a backend operation name belongs to, for the "X is publishing.
/// Open X to watch." line. Names not listed fall back to the raw name.
export const OPERATION_PAGES = {
  upload: "Home",
  status: "Home",
  content: "Textures, runtime, mods",
  launcher: "Launcher update",
  roster: "Roster",
  history: "Roll back",
  game_build: "Build an ISO",
  game_patch: "Build an ISO",
  set: "Full installer set",
};

function pageOf(operation) {
  return OPERATION_PAGES[operation] || operation;
}

function timeNow() {
  return new Date().toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

async function copyText(text) {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch (_) {
    try {
      const ta = document.createElement("textarea");
      ta.value = text;
      document.body.append(ta);
      ta.select();
      const ok = document.execCommand("copy");
      ta.remove();
      return ok;
    } catch (_) {
      return false;
    }
  }
}

export function job(host, config) {
  const { operation, verb, plan, run, verify, describePlan, describeDone, describeVerified } = config;
  const confirmLabel = (state) => {
    const c = config.confirmLabel;
    if (typeof c === "function") return String(c(state) || "Yes, publish");
    return c || "Yes, publish";
  };

  // What this job is doing, as this file sees it. `phase` is the job's
  // own step, not the backend's: "idle", "checking", "proposed",
  // "publishing", "verifying", "done", "stopped" (refused or failed).
  let phase = "idle";
  let last = null; // the last op_status state that belonged to this job
  let verifiedLine = null; // the sentence the verify step produced
  let stopped = null; // { sentence } for a refusal or failure
  let note = null; // one transient line (a cancel that could not happen)
  let otherBusy = null; // the page name of another operation in flight
  let pollHandle = null;

  const primary = el("button", { type: "button", class: "write job-primary", onclick: onPrimary });
  const cancelBtn = el("button", { type: "button", class: "job-cancel", text: "Cancel", onclick: onCancel });
  const status = el("p", { class: "job-status" });
  const bar = el("div", { class: "job-bar" }, el("div", { class: "job-bar-fill" }));
  const barLabel = el("p", { class: "job-bar-label" });
  const card = el("div", { class: "job-card" });
  const log = el("pre", { class: "panel-log job-log" });
  const details = el("details", { class: "job-details" }, el("summary", { text: "Details" }), log);
  const noteLine = el("p", { class: "job-note" });

  host.append(
    el(
      "div",
      { class: "job" },
      el("div", { class: "job-actions" }, primary, cancelBtn, status),
      bar,
      barLabel,
      card,
      noteLine,
      details
    )
  );

  function busy() {
    return phase === "checking" || phase === "publishing" || phase === "verifying";
  }

  function draw() {
    const text = typeof verb === "function" ? verb() : String(verb || "");
    primary.textContent = text || config.emptyLabel || "Nothing to publish yet";
    primary.disabled = busy() || !!otherBusy || !text;
    primary.hidden = phase === "proposed";
    cancelBtn.hidden = !busy();

    status.hidden = !(busy() || otherBusy);
    if (otherBusy) {
      status.textContent = `${otherBusy} is publishing. Open ${otherBusy} to watch.`;
    } else if (phase === "checking") {
      status.textContent = "Checking...";
    } else if (phase === "publishing") {
      status.textContent = "Publishing...";
    } else if (phase === "verifying") {
      status.textContent = "Confirming from the server...";
    }

    const progress = last && last.progress;
    const showBar = phase === "publishing" && progress && progress.total > 0;
    bar.hidden = !showBar;
    barLabel.hidden = !showBar;
    if (showBar) {
      const pct = Math.max(0, Math.min(100, Math.floor((progress.done / progress.total) * 100)));
      bar.firstChild.style.width = `${pct}%`;
      barLabel.textContent = `${progress.label || "Uploading"}: ${fmtBytes(progress.done)} of ${fmtBytes(progress.total)} (${pct}%)`;
    }

    card.replaceChildren();
    card.className = "job-card";
    card.hidden = true;
    if (phase === "proposed" && last) {
      card.hidden = false;
      card.classList.add("proposed");
      const lines = safeLines(() => describePlan(last.plan, last));
      for (const line of lines) {
        const warning = /^Warning:/i.test(line);
        card.append(el("p", { class: warning ? "job-line warning" : "job-line", text: line }));
      }
      card.append(
        el(
          "div",
          { class: "job-confirm" },
          el("button", { type: "button", text: "Cancel", onclick: onDismiss }),
          el("button", { type: "button", class: "write", text: confirmLabel(last), onclick: onConfirm })
        )
      );
    } else if (phase === "done" && last) {
      card.hidden = false;
      card.classList.add("ok");
      card.append(el("p", { class: "job-line", text: safeText(() => describeDone(last)) }));
      if (verifiedLine) card.append(el("p", { class: "job-line", text: verifiedLine }));
      card.append(el("button", { type: "button", text: "OK", onclick: onDismiss }));
    } else if (phase === "stopped" && stopped) {
      card.hidden = false;
      card.classList.add("bad");
      card.append(el("p", { class: "job-line", role: "alert", text: stopped.sentence }));
      card.append(
        el(
          "div",
          { class: "job-confirm" },
          el("button", { type: "button", text: "Copy", onclick: onCopy }),
          el("button", { type: "button", text: "OK", onclick: onDismiss })
        )
      );
    }

    noteLine.hidden = !note;
    noteLine.textContent = note || "";

    const lines = last && Array.isArray(last.lines) ? last.lines : [];
    log.textContent = lines.join("\n");
    details.hidden = !last;
  }

  function safeLines(fn) {
    try {
      const out = fn();
      return Array.isArray(out) ? out.map(String) : [String(out)];
    } catch (e) {
      return [`The plan could not be described: ${e && e.message ? e.message : e}`];
    }
  }

  function safeText(fn) {
    try {
      return String(fn());
    } catch (e) {
      return `Done, but the result could not be described: ${e && e.message ? e.message : e}`;
    }
  }

  // -- the backend's record, read the same way every page reads it --------

  function mine(state) {
    return !!state && state.operation === operation;
  }

  function applyState(state) {
    const backendBusy = !!state && JOB_BUSY.has(state.phase);
    if (mine(state)) {
      otherBusy = null;
      last = state;
      return true;
    }
    otherBusy = backendBusy ? pageOf(state.operation) : null;
    return false;
  }

  /// One settled backend phase, turned into this job's own next step.
  function settle(step, state) {
    if (state.phase === "refused" || state.phase === "failed") {
      stopped = { sentence: state.summary || (step === "checking" ? "The check stopped without saying why." : "It stopped without saying why.") };
      phase = "stopped";
      return "stopped";
    }
    if (step === "checking") {
      phase = "proposed";
      return "proposed";
    }
    if (step === "publishing") return "published";
    if (step === "verifying") {
      verifiedLine = state.summary ? `Verified from the server at ${timeNow()}: ${state.summary}` : `Verified from the server at ${timeNow()}.`;
      phase = "done";
      return "done";
    }
    return "settled";
  }

  function stopPolling() {
    if (pollHandle) {
      clearInterval(pollHandle);
      pollHandle = null;
    }
  }

  /// Poll until this job's operation settles; resolve with the settled
  /// state. Another page's operation being busy is drawn while waiting.
  function follow(step) {
    stopPolling();
    return new Promise((resolve) => {
      const tick = async () => {
        const answer = await call("op_status");
        if (!answer.ok) {
          note = `The operation status could not be read: ${answer.error}`;
          draw();
          return;
        }
        const state = answer.value;
        const isMine = applyState(state);
        draw();
        if (isMine && !JOB_BUSY.has(state.phase)) {
          stopPolling();
          if (config.onSettled) {
            try {
              config.onSettled(state);
            } catch (_) {
              /* a page's own refresh must not break the job */
            }
          }
          resolve(state);
        } else if (!isMine && !JOB_BUSY.has(state.phase)) {
          // The backend settled on something that was never ours (a
          // request accepted, then the state replaced by another page's
          // operation in the same instant). Stop rather than spin.
          stopPolling();
          resolve(null);
        }
      };
      pollHandle = setInterval(tick, JOB_POLL_MS);
      tick();
    });
  }

  function refusedUpFront(answer, what) {
    stopped = { sentence: answer.error || `${what} was refused.` };
    phase = "stopped";
    draw();
  }

  // -- the four clicks ------------------------------------------------------

  async function onPrimary() {
    if (busy()) return;
    note = null;
    stopped = null;
    verifiedLine = null;
    phase = "checking";
    draw();
    const answer = await plan();
    if (!answer.ok) return refusedUpFront(answer, "The check");
    const state = await follow("checking");
    if (!state) {
      phase = "idle";
      note = "The check was interrupted by another operation.";
      draw();
      return;
    }
    settle("checking", state);
    draw();
  }

  async function onConfirm() {
    if (busy()) return;
    note = null;
    phase = "publishing";
    draw();
    const answer = await run(false);
    if (!answer.ok) return refusedUpFront(answer, "Publishing");
    const state = await follow("publishing");
    if (!state) {
      phase = "idle";
      note = "Publishing was interrupted by another operation.";
      draw();
      return;
    }
    if (settle("publishing", state) !== "published") {
      draw();
      return;
    }
    if (!verify) {
      phase = "done";
      draw();
      return;
    }
    phase = "verifying";
    draw();
    const check = await verify();
    if (!check.ok) {
      verifiedLine = `Published, but the check from the server said: ${check.error}`;
      phase = "done";
      draw();
      return;
    }
    // Two kinds of verify: an operation (op_status turns to "verifying"
    // for this job) or a plain read whose answer IS the result.
    const peek = await call("op_status");
    const isOp = peek.ok && mine(peek.value) && (JOB_BUSY.has(peek.value.phase) || peek.value.phase === "done" || peek.value.phase === "refused" || peek.value.phase === "failed") && peek.value !== last && peek.value.summary !== (last && last.summary);
    if (isOp && peek.value.phase === "verifying") {
      const settled = await follow("verifying");
      if (settled) {
        if (settled.phase === "refused" || settled.phase === "failed") {
          verifiedLine = `Published, but the check from the server said: ${settled.summary}`;
        } else {
          verifiedLine = settled.summary ? `Verified from the server at ${timeNow()}: ${settled.summary}` : `Verified from the server at ${timeNow()}.`;
        }
      }
    } else if (isOp) {
      const settled = peek.value;
      last = settled;
      if (settled.phase === "refused" || settled.phase === "failed") {
        verifiedLine = `Published, but the check from the server said: ${settled.summary}`;
      } else {
        verifiedLine = settled.summary ? `Verified from the server at ${timeNow()}: ${settled.summary}` : `Verified from the server at ${timeNow()}.`;
      }
    } else {
      const sentence = describeVerified ? safeText(() => describeVerified(check.value, last)) : "";
      verifiedLine = sentence ? `Verified from the server at ${timeNow()}: ${sentence}` : `Verified from the server at ${timeNow()}.`;
    }
    phase = "done";
    draw();
  }

  async function onCancel() {
    const answer = await call("op_cancel");
    note = answer.ok ? "Stopping..." : answer.error;
    draw();
  }

  function onDismiss() {
    phase = "idle";
    stopped = null;
    note = null;
    draw();
  }

  async function onCopy() {
    const lines = last && Array.isArray(last.lines) ? last.lines : [];
    const text = [stopped ? stopped.sentence : "", "", ...lines].join("\n").trim();
    note = (await copyText(text)) ? "Copied." : "Could not copy. Open Details and select the text instead.";
    draw();
  }

  // -- first look ---------------------------------------------------------
  //
  // Reflect what is already true: this job's own operation still running
  // from before a reload is picked up and followed; another page's busy
  // operation is named; anything else, including another page's finished
  // state, is left alone.
  (async () => {
    const answer = await call("op_status");
    if (!answer.ok) {
      note = `The operation status could not be read: ${answer.error}`;
      draw();
      return;
    }
    const state = answer.value;
    if (mine(state) && JOB_BUSY.has(state.phase)) {
      last = state;
      phase = state.phase === "planning" ? "checking" : state.phase === "running" ? "publishing" : "verifying";
      draw();
      const settled = await follow(phase);
      if (settled) settle(phase, settled);
      draw();
      return;
    }
    applyState(mine(state) ? { ...state, operation: "" } : state);
    draw();
  })();

  draw();

  return {
    /// Redraw with the page's current inputs (the verb may have changed).
    refresh: draw,
  };
}
