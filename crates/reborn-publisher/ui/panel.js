// The one operation panel: plan, then run, then verify, drawn the same way
// on every page. This file decides nothing about what an operation does;
// every fact it shows, the phase, the log lines, the plan's own summary,
// comes from polling `op_status`, which is `ops.rs`'s own record of what is
// actually happening. A page cannot make this panel show something that did
// not happen, because it never invents anything: it only draws answers.
//
// # The contract a page implements
//
// `panel(host, config)` mounts into `host`. `config` is:
//
//   operation: string
//     This page's operation name, matching the page name half of the `op`
//     strings its commands pass to `ops::run_operation` (for example
//     `"roster"`). Used to tell "something else is busy" apart from "my own
//     operation is busy", and for nothing else: this file draws whatever
//     `op_status` says regardless.
//
//   describe: string
//     One sentence, shown at the top of the panel, saying what Plan/Run/
//     Verify do here. Static text; this file does not interpret it.
//
//   plan(): Promise<{ok, value?, error?}>
//   run(dryRun): Promise<{ok, value?, error?}>
//   verify(): Promise<{ok, value?, error?}>
//     One function per step, written by the page, each of which calls
//     exactly one named command (through `shell.js`'s `call`) with
//     whatever arguments that command needs, gathered from the page's own
//     inputs. This is what keeps this file generic across seven pages with
//     seven different argument shapes: the page already knows its own
//     fields, so it hands this file a closure instead of a command name and
//     a static argument list. `run` alone takes the dry-run flag, because
//     the checkbox that carries it lives in this panel, not on the page.
//
// A page's real commands, per the plan this app was built from, return
// almost immediately (`ops::run_operation` starts a worker thread and
// returns), so a resolved `plan`/`run`/`verify` promise most often means
// only "the request was accepted or refused up front", not "the operation
// finished". The finish is discovered by polling, same as everything else
// this panel shows.

import { call, el } from "./shell.js";

const POLL_MS = 400;
const BUSY_PHASES = new Set(["planning", "running", "verifying"]);

/**
 * @param {HTMLElement} host
 * @param {{
 *   operation: string,
 *   describe: string,
 *   plan: () => Promise<{ok: boolean, value?: unknown, error?: string}>,
 *   run: (dryRun: boolean) => Promise<{ok: boolean, value?: unknown, error?: string}>,
 *   verify: () => Promise<{ok: boolean, value?: unknown, error?: string}>,
 * }} config
 */
export function panel(host, config) {
  const { operation, describe, plan, run, verify } = config;

  let planHasReturned = false;
  let runHasReturned = false;
  let pollHandle = null;

  const description = el("p", { class: "panel-describe", text: describe || "" });
  const busyNote = el("p", { class: "partial", hidden: true });

  const phaseLabel = el("span", { class: "panel-phase idle", text: "Idle" });
  const summaryLine = el("p", { class: "panel-summary" });
  const planSummary = el("p", { class: "panel-plan-summary", hidden: true });

  const dryRunCheckbox = el("input", { type: "checkbox", checked: true });
  const dryRunLabel = el("label", { class: "inline" }, dryRunCheckbox, " Dry run (check nothing, write nothing)");

  const planBtn = el("button", { class: "write", text: "Plan", onclick: onPlan });
  const runBtn = el("button", { class: "write", text: "Run", disabled: true, onclick: onRun });
  const verifyBtn = el("button", { class: "write", text: "Verify", disabled: true, onclick: onVerify });
  const cancelBtn = el("button", { text: "Cancel", disabled: true, onclick: onCancel });
  const clearBtn = el("button", { text: "Clear", disabled: true, onclick: onClear });

  const log = el("pre", { class: "panel-log" });

  host.append(
    el(
      "div",
      { class: "panel" },
      description,
      busyNote,
      el("div", { class: "panel-status" }, phaseLabel, summaryLine),
      planSummary,
      el("div", { class: "panel-buttons" }, planBtn, dryRunLabel, runBtn, verifyBtn, cancelBtn, clearBtn),
      log
    )
  );

  function setBusy(busy) {
    planBtn.disabled = busy;
    runBtn.disabled = busy || !planHasReturned;
    verifyBtn.disabled = busy || !runHasReturned;
    cancelBtn.disabled = !busy;
  }

  function planSummaryText(value) {
    if (value === null || value === undefined) return "";
    if (typeof value === "object" && typeof value.summary === "string") return value.summary;
    if (typeof value === "string") return value;
    try {
      return JSON.stringify(value);
    } catch (_) {
      return String(value);
    }
  }

  function renderState(state) {
    const phase = (state && state.phase) || "idle";
    const busy = BUSY_PHASES.has(phase);
    const theirs = busy && state && state.operation && state.operation !== operation;

    phaseLabel.textContent = phase.charAt(0).toUpperCase() + phase.slice(1);
    phaseLabel.className = `panel-phase ${phase}`;

    const lines = Array.isArray(state && state.lines) ? state.lines : [];
    log.textContent = lines.join("\n");

    summaryLine.textContent = (state && state.summary) || "";
    summaryLine.className = phase === "refused" || phase === "failed" ? "panel-summary refusal" : "panel-summary";

    const hasPlan = state && Object.prototype.hasOwnProperty.call(state, "plan") && state.plan !== null && state.plan !== undefined;
    planSummary.hidden = !hasPlan;
    if (hasPlan) planSummary.textContent = planSummaryText(state.plan);

    if (theirs) {
      busyNote.hidden = false;
      busyNote.textContent = `${state.operation} is running. Wait for it to finish before starting this operation.`;
    } else {
      busyNote.hidden = true;
    }

    clearBtn.disabled = busy;
    setBusy(busy);
  }

  async function poll() {
    const answer = await call("op_status");
    if (!answer.ok) {
      busyNote.hidden = false;
      busyNote.textContent = `The operation status could not be read: ${answer.error}`;
      return;
    }
    renderState(answer.value);
    const phase = (answer.value && answer.value.phase) || "idle";
    if (!BUSY_PHASES.has(phase)) stopPolling();
  }

  function startPolling() {
    if (pollHandle) return;
    pollHandle = setInterval(poll, POLL_MS);
    poll();
  }

  function stopPolling() {
    if (pollHandle) {
      clearInterval(pollHandle);
      pollHandle = null;
    }
  }

  function showImmediateError(answer) {
    if (answer.ok) return;
    summaryLine.textContent = answer.error;
    summaryLine.className = "panel-summary refusal";
  }

  // Polling starts AFTER the request is accepted, never before. Started
  // before, the first poll read the previous operation's finished state (or
  // "idle" on a fresh app), decided nothing was running, and stopped; the
  // plan then ran to the end in the backend with nobody listening, and the
  // page sat on "Planning" with an empty log. Seen on Adam's PC on the first
  // Textures plan, 2026-09-10. The buttons go busy at once by hand, so a
  // slow request cannot be clicked twice.
  async function onPlan() {
    setBusy(true);
    const answer = await plan();
    planHasReturned = true;
    showImmediateError(answer);
    startPolling();
  }

  async function onRun() {
    setBusy(true);
    const answer = await run(dryRunCheckbox.checked);
    runHasReturned = true;
    showImmediateError(answer);
    startPolling();
  }

  async function onVerify() {
    setBusy(true);
    const answer = await verify();
    showImmediateError(answer);
    startPolling();
  }

  async function onCancel() {
    const answer = await call("op_cancel");
    if (!answer.ok) {
      busyNote.hidden = false;
      busyNote.textContent = answer.error;
    }
  }

  async function onClear() {
    const answer = await call("op_clear");
    if (!answer.ok) {
      busyNote.hidden = false;
      busyNote.textContent = answer.error;
      return;
    }
    planHasReturned = false;
    runHasReturned = false;
    renderState({ phase: "idle", lines: [], summary: "", plan: null, operation: "" });
  }

  // Reflect whatever is already true (this page's own operation left
  // running from before a reload, or another page's still in flight)
  // rather than assuming a freshly mounted panel means a freshly idle app.
  poll();
}
