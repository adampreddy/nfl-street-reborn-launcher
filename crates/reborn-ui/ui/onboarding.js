// First run, as screens.
//
// This is a feel-the-flow build, not the product interface. It exists to answer
// questions a harness row cannot: how many steps is this, what does each one
// actually ask, and what does a screen look like when the launcher has to admit
// it does not know something.
//
// Two rules, both correctness rather than style:
//
//   Every value on screen comes from a command. Nothing is a placeholder, and
//   nothing is filled in with a plausible default. A field nothing measured is
//   drawn as absent, because a number that looks right in review and is wrong on
//   contact is the failure this whole module was shaped to avoid.
//
//   The core owns the order. This file never decides which screen is next: it
//   asks, draws what it is told, and reports what came back. A second copy of
//   the stage order here would be a second source of truth, and the one on disk
//   would win at the worst moment.

const bridge = window.__TAURI__ ? window.__TAURI__.core : null;

const LEGACY_ROOT = "C:\\Users\\Adam\\AppData\\Roaming\\NFL Street Reborn";

// Filled by refresh(), which is the only thing that talks to onboarding_progress.
let state = null;
// Per screen scratch, cleared on every move so an answer cannot leak forward.
let pending = {};

// ---------------------------------------------------------------------------
// Talking to the core
// ---------------------------------------------------------------------------

function log(what, value) {
  const pre = document.getElementById("out");
  const body = typeof value === "string" ? value : JSON.stringify(value, null, 2);
  pre.textContent = `${what}\n${body}\n\n${pre.textContent}`.slice(0, 20000);
}

async function call(cmd, args = {}) {
  if (!bridge) throw new Error("the launcher core is not attached");
  const answer = await bridge.invoke(cmd, { legacyRoot: LEGACY_ROOT, ...args });
  log(cmd, answer === undefined ? "ok" : answer);
  return answer;
}

/// Ask where the flow stands, then draw it. Every move ends here.
async function refresh() {
  try {
    state = await call("onboarding_progress");

    // Showing a stage IS reaching it, and the journal has to say so. Two
    // stages refuse their own work without the marker, preparation and
    // completion, and without this the flow stopped dead on both with a
    // refusal written for a programmer: "call reach() first".
    //
    // Safe to do on every draw. reach is idempotent and deliberately
    // permissive, so re-showing a screen records nothing new, and the marker
    // changes nothing about what is drawn.
    if (state.next.kind === "show") {
      await call("onboarding_reach", { stage: state.next.stage });
    }

    await prefetch(state.next.stage);
    draw();
  } catch (e) {
    fail(e);
  }
}

/// Fetch what a screen needs before drawing it.
///
/// Only the screens that read something outside the journal. The controller
/// screen is deliberately NOT here: a pad list goes stale the moment a wireless
/// pad sleeps, so that one is fetched on a button the player can press again
/// rather than once on the way in.
async function prefetch(slug) {
  if (slug === "controls_profile" && !pending.controls) {
    try {
      // The suggestion comes from the pad that was actually found, so this
      // needs the runtime path for the search to cover every backend.
      const list = await call("controls_options", {
        runtime: pending.runtime || null,
      });
      pending.controls = { list };
    } catch (e) {
      pending.controls = { error: typeof e === "string" ? e : e.message };
    }
  }
}

function fail(e) {
  const msg = typeof e === "string" ? e : e.message || JSON.stringify(e);
  log("error", msg);
  document.getElementById("screen").replaceChildren(
    el("h1", null, "Something went wrong"),
    // The core's own sentence. Rewriting it here would lose the part that says
    // what to do about it.
    el("div", "refusal", msg),
  );
}

// ---------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------

function el(tag, cls, text) {
  const node = document.createElement(tag);
  if (cls) node.className = cls;
  if (text !== undefined && text !== null) node.textContent = text;
  return node;
}

/// The step strip, from the plan the core returns rather than a list here.
function drawSteps() {
  const host = document.getElementById("steps");
  host.replaceChildren();
  const current = state.next.stage;

  for (const step of state.plan) {
    const label = step.stage.replace(/_/g, " ");
    const node = el("span", null, label);
    if (step.stage === current) node.className = "now";
    else if (step.satisfied) node.className = "done";
    host.appendChild(node);
  }

  // Stages this machine does not have, named rather than silently dropped: a
  // flow that quietly omits a step is one nobody can check.
  for (const aside of state.set_aside || []) {
    const node = el("span", "skipped", aside.what.replace(/_/g, " "));
    node.title = aside.why;
    host.appendChild(node);
  }
}

function heading(title, next) {
  const out = [el("h1", null, title)];
  if (next.position && next.total) {
    out.push(el("p", "position", `Step ${next.position} of ${next.total}`));
  }
  return out;
}

/// Facts as a table, with absent values drawn as absent.
function factsTable(rows) {
  const table = el("table", "facts");
  for (const [label, value] of rows) {
    const tr = el("tr");
    tr.appendChild(el("th", null, label));
    const td = el("td");
    if (value === null || value === undefined) {
      td.appendChild(el("span", "absent", "not measured"));
    } else {
      td.textContent = value;
    }
    tr.appendChild(td);
    table.appendChild(tr);
  }
  return table;
}

function radios(name, options) {
  const out = [];
  for (const opt of options) {
    const label = el("label", "choice");
    const input = document.createElement("input");
    input.type = "radio";
    input.name = name;
    input.value = opt.value;
    if (pending[name] === opt.value) input.checked = true;
    input.addEventListener("change", () => {
      pending[name] = opt.value;
      updateNav();
    });
    label.appendChild(input);
    label.appendChild(el("span", "what", opt.what));
    if (opt.why) label.appendChild(el("span", "why", opt.why));
    out.push(label);
  }
  return out;
}

// ---------------------------------------------------------------------------
// The screens
//
// One entry per stage slug. Each returns the nodes to draw, and says whether
// Next can fire and what it does. `answer` is what Next calls, and a stage with
// no answer is one that only needs acknowledging.
// ---------------------------------------------------------------------------

const SCREENS = {
  welcome: () => ({
    nodes: [
      el(
        "p",
        null,
        "This sets up NFL Street Reborn: your controller, how it looks, and " +
          "which mods are on. Nothing is changed on your machine until the " +
          "step that says so.",
      ),
      el(
        "p",
        null,
        state.next.resumed
          ? "You have been here before, so this picks up where it stopped."
          : "It takes a couple of minutes and you can go back at any point.",
      ),
    ],
    ready: () => true,
    answer: () => call("onboarding_acknowledge"),
  }),

  migration_offer: () => ({
    nodes: [
      el(
        "p",
        null,
        `An NFL Street Reborn 2.1 install was found at ${
          state.legacy_root_checked || "an earlier location"
        }.`,
      ),
      el(
        "p",
        null,
        "Bringing it over copies your saves and settings. Nothing is deleted " +
          "either way, so the old install stays where it is.",
      ),
      ...(state.migration_blocked
        ? [el("div", "refusal", state.migration_blocked)]
        : []),
      ...radios("migration", [
        { value: "yes", what: "Bring it over", why: "Copies saves and settings" },
        { value: "no", what: "Start fresh", why: "The old install is left alone" },
      ]),
    ],
    // Declining is always available, even when the copy is blocked: leaving
    // the old install alone is exactly what a blocked migration should let you
    // do, and refusing to let anyone past would strand them here.
    ready: () => pending.migration === "no" || (pending.migration === "yes" && !state.migration_blocked),
    answer: () => {
      if (pending.migration === "no") return call("onboarding_decline_migration");
      // Records the decision only. The copy is a separate, confirmed step,
      // because it moves memory cards and a Next button is not consent for
      // that.
      return call("onboarding_accept_migration", { legacyRoot: LEGACY_ROOT });
    },
  }),

  migration_copy: () => ({
    nodes: [
      el(
        "p",
        null,
        "This copies your memory cards, screenshots and textures out of the " +
          "2.1 install. Nothing in the old install is changed or removed, and " +
          "anything already here is left alone rather than overwritten.",
      ),
      el(
        "div",
        "partial",
        "This is the step that writes. It is recorded before it starts, so if " +
          "it is interrupted the launcher knows it was inside it and picks up " +
          "rather than starting the copy again.",
      ),
    ],
    ready: () => true,
    nextLabel: "Copy it over",
    answer: async () => {
      const done = await call("migration_apply", {
        legacyRoot: LEGACY_ROOT,
        confirm: true,
      });
      if (done.journal_note) {
        // The files moved but the journal would not take it. Saying nothing
        // would leave the flow looking stuck for an invisible reason.
        document
          .getElementById("screen")
          .appendChild(el("div", "partial", done.journal_note));
      }
      return done;
    },
  }),

  game_verification: () => ({
    nodes: [
      el(
        "p",
        null,
        "The game image is what actually gets booted. The launcher measures it " +
          "rather than taking your word for the size.",
      ),
      // The point Adam raised: a 2.1 install proves ownership and already has a
      // patched image, so this stage should not be asking for one.
      ...(state.legacy_root_checked
        ? [
            el(
              "div",
              "partial",
              "A 2.1 install was found, so the image comes from there and " +
                "nothing needs patching. This screen should not be asking at " +
                "all, which is the next thing to fix.",
            ),
          ]
        : []),
      pathBox("image", "Full path to the .iso or .rvz, or leave blank"),
    ],
    ready: () => true,
    answer: () => call("onboarding_confirm_game", { image: pending.image || null }),
  }),

  system_check: () => ({
    nodes: [
      el(
        "p",
        null,
        "How the game looks. Every one of these runs; the difference is how " +
          "hard your machine has to work.",
      ),
      // Nothing measures this machine, so there is no recommendation. Drawing a
      // "recommended for your PC" badge here would be inventing one.
      el(
        "div",
        "partial",
        "Nothing has measured this machine yet, so none of these is marked " +
          "recommended. A badge here would be a guess wearing a badge.",
      ),
      ...radios("preset", [
        { value: "performance", what: "Performance", why: "Lowest load" },
        { value: "balanced", what: "Balanced" },
        { value: "quality", what: "Quality" },
        { value: "ultra", what: "Ultra", why: "Highest load" },
      ]),
    ],
    ready: () => !!pending.preset,
    answer: () => call("onboarding_choose_graphics", { preset: pending.preset }),
  }),

  controller_setup: () => {
    const pads = pending.pads;
    const nodes = [
      el(
        "p",
        null,
        "Pick the pad to use as player one. The launcher takes its name from " +
          "the emulator, so the buttons in game match what you press.",
      ),
    ];

    if (!pads) {
      nodes.push(el("p", "note", "Looking for pads."));
    } else if (pads.error) {
      nodes.push(el("div", "refusal", pads.error));
    } else {
      if (!pads.searched_everywhere) {
        // The recurring honest case, and Adam's point about the forced Dolphin:
        // without a runtime path only XInput is searched.
        nodes.push(
          el(
            "div",
            "partial",
            `Not every backend was searched (${
              pads.unsearched.join("; ") || "reason not given"
            }), so a pad the game would see may be missing from this list. ` +
              `The list came from: ${pads.source}.`,
          ),
        );
      }
      if (pads.list.length === 0) {
        nodes.push(
          el(
            "div",
            "partial",
            "No pad found. A wireless pad that has gone to sleep is not " +
              "listed, so wake it and press Look again.",
          ),
        );
      }
      nodes.push(
        ...radios(
          "port",
          pads.list.map((p, i) => ({
            value: String(i),
            // The family is the only human readable part. There is no friendly
            // name, on purpose: the device string is what binds, so it is shown
            // rather than hidden behind something prettier that binds nothing.
            what: `${p.family} pad on port ${i + 1}`,
            why: p.device_string,
          })),
        ),
      );
      const decline = el("label", "choice");
      const input = document.createElement("input");
      input.type = "radio";
      input.name = "port";
      input.value = "none";
      if (pending.port === "none") input.checked = true;
      input.addEventListener("change", () => {
        pending.port = "none";
        updateNav();
      });
      decline.appendChild(input);
      decline.appendChild(el("span", "what", "Carry on without a pad"));
      decline.appendChild(
        el(
          "span",
          "why",
          "Recorded as a decision, so the launcher can say the game will start " +
            "into nothing rather than letting you find out",
        ),
      );
      nodes.push(decline);
    }

    const look = el("button", null, "Look for pads");
    look.addEventListener("click", async () => {
      try {
        const report = await call("devices", { runtime: pending.runtime || null });
        pending.pads = {
          list: report.pads || [],
          // The core's own word for it. Recomputing this from the unsearched
          // list would be a second opinion on a question it already answered.
          searched_everywhere: report.complete,
          unsearched: report.unsearched || [],
          source: report.source,
        };
      } catch (e) {
        pending.pads = { error: typeof e === "string" ? e : e.message };
      }
      draw();
    });
    nodes.push(el("p", null, ""), look);

    return {
      nodes,
      ready: () => pending.port !== undefined,
      answer: () =>
        pending.port === "none"
          ? call("onboarding_decline_controller", {
              because: "no pad was connected during first run",
            })
          : call("onboarding_choose_controller", {
              port: Number(pending.port),
              runtime: pending.runtime || null,
            }),
    };
  },

  controls_profile: () => {
    const opts = pending.controls;
    const nodes = [
      el(
        "p",
        null,
        "Which button art the game shows. This only changes the icons, not what " +
          "the buttons do, and you can change it later.",
      ),
    ];

    if (!opts) {
      nodes.push(el("p", "note", "Reading the profiles."));
    } else if (opts.error) {
      nodes.push(el("div", "refusal", opts.error));
    } else {
      // Suggested, never applied unsolicited. Someone on an Xbox pad may well
      // want GameCube art, since the game is a GameCube game and its own
      // tutorials draw it that way.
      if (pending.profile === undefined) {
        const s = opts.list.find((o) => o.suggested);
        if (s) pending.profile = s.slug;
      }
      nodes.push(
        ...radios(
          "profile",
          opts.list.map((o) => ({
            value: o.slug,
            what: o.suggested ? `${o.label} (suggested for your pad)` : o.label,
            why:
              o.pack === null
                ? `${o.describes}. Applies nothing, so the game's own art shows`
                : o.describes,
          })),
        ),
      );
      if (!opts.list.some((o) => o.suggested)) {
        nodes.push(
          el(
            "div",
            "partial",
            "No pad was found, so none of these is suggested. Xbox is preselected " +
              "as the default rather than as a recommendation.",
          ),
        );
      }
    }

    return {
      nodes,
      ready: () => !!pending.profile,
      answer: () => call("onboarding_choose_controls", { profile: pending.profile }),
    };
  },

  preparation: () => {
    const done = new Set((state.tasks_done || []).map((t) => t.slug));
    const remaining = state.preparation_remaining || [];
    const ran = pending.ran || {};

    const list = el("ul", "tasklist");
    for (const t of [...(state.tasks_done || []), ...remaining]) {
      const li = el("li", done.has(t.slug) ? "done" : null, t.label);
      // What it actually did, not a tick. "set 5 keys" and "nothing to change"
      // are both success and mean different things.
      if (ran[t.slug]) li.appendChild(el("span", "why", ran[t.slug]));
      list.appendChild(li);
    }

    const nodes = [
      el(
        "p",
        null,
        "This saves the answers you just gave into the emulator's own settings. " +
          "Nothing is installed or downloaded: the installer already put the " +
          "files on your machine.",
      ),
      list,
    ];

    if (pending.prepError) nodes.push(el("div", "refusal", pending.prepError));

    if (remaining.length) {
      const run = el("button", "write", `Run ${remaining.length} step(s)`);
      run.addEventListener("click", async () => {
        run.disabled = true;
        pending.prepError = null;
        pending.ran = pending.ran || {};
        for (const t of remaining) {
          try {
            const out = await call("onboarding_run_task", {
              task: t.slug,
              runtime: pending.runtime || null,
              game: pending.image || null,
            });
            pending.ran[out.task] = out.did;
            if (out.step.outcome === "refused") {
              pending.prepError = out.step.refused_because;
              break;
            }
          } catch (e) {
            // Stop at the first failure rather than carrying on. The steps
            // are ordered, and running the check after a failed write would
            // report on a configuration that was never made.
            pending.prepError = typeof e === "string" ? e : e.message;
            break;
          }
        }
        await refresh();
      });
      nodes.push(run);
    }

    return {
      nodes,
      ready: () => (state.preparation_remaining || []).length === 0,
      hint: remaining.length
        ? "Run the steps to continue."
        : "Everything is saved.",
      answer: () => call("onboarding_prepared"),
    };
  },

  completion: () => {
    const s = state.summary;
    return {
      nodes: [
        el("p", null, "Setup is done. This is what was decided."),
        factsTable([
          ["Controller", s.controller || (s.no_controller_because ? `none: ${s.no_controller_because}` : null)],
          ["Port", s.controller_port === null ? null : s.controller_port + 1],
          ["Graphics", s.graphics],
          [
            "Recommended",
            s.graphics_was_recommended === null
              ? null
              : s.graphics_was_recommended
                ? "yes, the recommendation was taken"
                : "no, a different preset was chosen",
          ],
          ["Detected hardware", s.detected_hardware],
          ["Game size", s.game_bytes === null ? null : `${s.game_bytes.toLocaleString()} bytes`],
          [
            "Mods",
            s.mods_chosen === null
              ? null
              : s.mods_chosen.length === 0
                ? "none, by choice"
                : s.mods_chosen.join(", "),
          ],
          ["Tasks completed", (s.tasks_done || []).length],
        ]),
        ...((s.absent || []).length
          ? [
              el(
                "div",
                "partial",
                `Nothing measured: ${s.absent.join(", ")}. These are printed as ` +
                  "absent rather than filled in with something plausible.",
              ),
            ]
          : []),
      ],
      ready: () => true,
      nextLabel: "Finish",
      answer: () => call("onboarding_finish", { confirm: true }),
    };
  },
};

function pathBox(key, placeholder) {
  const box = document.createElement("input");
  box.type = "text";
  box.placeholder = placeholder;
  box.value = pending[key] || "";
  box.spellcheck = false;
  box.style.width = "100%";
  box.addEventListener("input", () => {
    pending[key] = box.value.trim();
  });
  return box;
}

// ---------------------------------------------------------------------------
// The frame
// ---------------------------------------------------------------------------

let current = null;

function draw() {
  drawSteps();
  const host = document.getElementById("screen");

  if (state.complete || state.next.kind === "already_complete") {
    host.replaceChildren(
      el("h1", null, "Already set up"),
      el(
        "p",
        null,
        "First run was finished, and it latches once so it is not offered " +
          "again. Start over from the harness to walk it another time.",
      ),
    );
    current = null;
    updateNav();
    return;
  }

  if (state.next.kind === "reconcile") {
    // This stage writes files and the last run stopped inside it, so its work
    // may have partly happened. Saying so is the whole point of the state.
    host.replaceChildren(
      ...heading("Picking up where this stopped", state.next),
      el("div", "partial", state.next.what_to_do),
    );
    current = null;
    updateNav();
    return;
  }

  const slug = state.next.stage;
  const build = SCREENS[slug];
  if (!build) {
    host.replaceChildren(
      el("h1", null, slug.replace(/_/g, " ")),
      el(
        "div",
        "refusal",
        `No screen is built for the ${slug} stage yet. The core is offering it, ` +
          "so this is a gap here and not a gap there.",
      ),
    );
    current = null;
    updateNav();
    return;
  }

  current = build();
  host.replaceChildren(...heading(titleFor(slug), state.next), ...current.nodes);
  updateNav();
}

function titleFor(slug) {
  return {
    welcome: "Welcome",
    migration_offer: "An earlier install was found",
    migration_copy: "Bringing it over",
    game_verification: "The game",
    system_check: "How it looks",
    controller_setup: "Your controller",
    controls_profile: "Controls",
    preparation: "Setting up",
    completion: "Ready",
  }[slug] || slug.replace(/_/g, " ");
}

function updateNav() {
  const next = document.getElementById("next");
  const back = document.getElementById("back");
  const hint = document.getElementById("hint");

  // Called, not read. ready() has to see the answer the player just gave, and
  // reading a boolean captured when the screen was built cannot.
  next.disabled = !current || !current.ready();
  next.textContent = (current && current.nextLabel) || "Next";
  hint.textContent = (current && current.hint) || "";

  // Back is asked, never assumed: the previous stage may be irreversible, in
  // which case there is no way back and the button should say so by being off.
  back.disabled = true;
  if (state && state.next && state.next.stage) {
    call("onboarding_back", { stage: state.next.stage })
      .then((answer) => {
        back.disabled = !answer.to;
        if (!answer.to && answer.refused_because) back.title = answer.refused_because;
      })
      .catch(() => {});
  }
}

document.getElementById("next").addEventListener("click", async () => {
  if (!current || !current.ready()) return;
  const btn = document.getElementById("next");
  btn.disabled = true;
  try {
    const answer = await current.answer();
    // A refusal is not an error. It is the core saying no with a reason, and
    // the reason belongs on the screen the player is looking at.
    const step = answer && answer.step ? answer.step : answer;
    if (step && step.outcome === "refused") {
      document
        .getElementById("screen")
        .appendChild(el("div", "refusal", step.refused_because));
      btn.disabled = false;
      return;
    }
    pending = {};
    await refresh();
  } catch (e) {
    fail(e);
  }
});

document.getElementById("back").addEventListener("click", async () => {
  try {
    const answer = await call("onboarding_back", { stage: state.next.stage });
    if (!answer.to) return;
    pending = {};
    await call("onboarding_reach", { stage: answer.to });
    await refresh();
  } catch (e) {
    fail(e);
  }
});

document.getElementById("clear").addEventListener("click", () => {
  document.getElementById("out").textContent = "";
});

if (!bridge) {
  document.getElementById("screen").replaceChildren(
    el("h1", null, "The core is not attached"),
    el(
      "div",
      "refusal",
      "This window loaded but the launcher core did not, so nothing can be " +
        "called. Nothing on screen would be real.",
    ),
  );
} else {
  refresh();
}
