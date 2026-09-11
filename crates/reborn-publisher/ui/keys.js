// The Keys page: signing keys, the vault, and this app's own settings.
//
// The rule everything here follows: a stored secret is never read back into
// this page. `vault_state` reports "stored" or "missing" and nothing else,
// `vault_store` reports only whether the store succeeded, and nothing in
// this file ever holds a stored value longer than it takes to send it to
// `vault_store`; a field is cleared the moment a store or forget settles.
//
// No job lives here (this page never touches `job.js`): every command
// below either reads a fact or performs one
// small, immediate action, so a button click calls a command and then
// re-reads everything from scratch, the same way the rest of this app
// treats a command's answer as the only truth on screen.

import { call, el, fact, facts, refusal, partial, markCurrent } from "./shell.js";

markCurrent();

const screen = document.getElementById("screen");

// Draft input the user is typing, kept here (not in the DOM) so a full
// re-render triggered by an unrelated button never loses what has not been
// submitted yet.
let genDir = "";
const vaultDrafts = { site_token: "", r2_access_key: "", r2_secret: "" };
const settingsDrafts = { bucket_endpoint: "", bucket_name: "", public_base: "", launcher_manifest: "", drop_root: "" };
let settingsDraftsSeeded = false;

// One-shot feedback from the last action in each block, shown until the
// next action in that block replaces or clears it.
let lastGenerateMessage = null;
let lastVaultMessage = null;
let lastSettingsMessage = null;

async function refresh() {
  const [keysAnswer, vaultAnswer, settingsAnswer] = await Promise.all([
    call("keys_state"),
    call("vault_state"),
    call("publisher_settings"),
  ]);
  render(keysAnswer, vaultAnswer, settingsAnswer);
}

function render(keysAnswer, vaultAnswer, settingsAnswer) {
  screen.textContent = "";
  screen.append(keysBlock(keysAnswer), vaultBlock(vaultAnswer), settingsBlock(settingsAnswer));
}

// ---------------------------------------------------------------------------
// Signing keys
// ---------------------------------------------------------------------------

function trustNote(publisherKeyState) {
  switch (publisherKeyState.trust) {
    case "everyday":
      return el("p", { class: "said", text: "Trusted: this is the everyday key every shipped launcher accepts." });
    case "rotation":
      return partial(
        "This file holds the ROTATION key, not the everyday key. It should sit offline, not be configured here as the signing key."
      );
    case "trusted":
      return el("p", {
        class: "said",
        text: "Trusted, but in neither the everyday nor the rotation slot. That is unexpected; check TRUSTED_KEYS.",
      });
    case "untrusted":
      return refusal(
        "This key is not in the shipped launcher's trusted keys. A release it signs will be rejected by every launcher in the field."
      );
    case "unreadable":
      return refusal("This file exists but is not a readable key.");
    case "missing":
      return refusal("The configured publisher.key file is not on disk.");
    case "error":
      return refusal(`The key file could not be checked: ${publisherKeyState.error || "unknown error"}`);
    case "not_configured":
    default:
      return partial("No signing key is configured yet. Generate one below, or use Browse in Settings to point at an existing publisher.key.");
  }
}

function keysBlock(answer) {
  if (!answer.ok) {
    return el(
      "section",
      { class: "block" },
      el("h2", { text: "Signing keys" }),
      refusal(`The key state could not be read: ${answer.error}`)
    );
  }
  const state = answer.value;
  // Every field the real command sends is a populated object or array, never
  // null, but a page that assumes that throws the moment that stops being
  // true. Guarded the same way `status.js` and `content.js` guard their own
  // list and object answers.
  const trusted = Array.isArray(state.trusted) ? state.trusted : [];
  const publisherKey = state.publisher_key || {};
  const rotationKey = state.rotation_key || {};

  const children = [
    el("h2", { text: "Signing keys" }),
    el("p", { class: "hint", text: "The public keys a shipped launcher trusts, compiled into every player's build." }),
    facts(trusted.map((t) => fact(t.role, t.public_hex))),
    el("p", { class: "hint", text: "The everyday key on this machine." }),
    facts([
      fact("Path", publisherKey.path),
      fact("On disk", publisherKey.path === null || publisherKey.path === undefined ? null : publisherKey.exists ? "yes" : "no"),
      ...(publisherKey.public_hex ? [fact("Public key", publisherKey.public_hex)] : []),
    ]),
    trustNote(publisherKey),
  ];

  if (state.same_folder_warning) {
    children.push(
      partial(
        "rotation.key sits in the same folder as publisher.key. A rotation key stored beside the everyday key leaks in the same breach; move it offline."
      )
    );
  }

  children.push(
    el("p", { class: "hint", text: "The rotation key, generated alongside it and meant to move offline." }),
    facts([
      fact("Path", rotationKey.path),
      fact("On disk", rotationKey.path === null || rotationKey.path === undefined ? null : rotationKey.exists ? "yes" : "no"),
    ])
  );

  if (lastGenerateMessage) {
    children.push(
      lastGenerateMessage.ok ? el("p", { class: "said", text: lastGenerateMessage.text }) : refusal(lastGenerateMessage.text)
    );
    if (lastGenerateMessage.lines && lastGenerateMessage.lines.length) {
      children.push(el("pre", { class: "panel-log", text: lastGenerateMessage.lines.join("\n") }));
    }
  }

  children.push(
    el("p", {
      class: "hint",
      text: "Generates a brand new everyday key and rotation key together. Refuses if either file already exists in the chosen folder.",
    }),
    el(
      "div",
      { class: "row" },
      el("input", {
        type: "text",
        placeholder: "folder to write publisher.key and rotation.key into",
        value: genDir,
        oninput: (e) => (genDir = e.target.value),
      }),
      el("button", { class: "write", text: "Generate publisher.key + rotation.key", onclick: onGenerate })
    )
  );

  return el("section", { class: "block" }, ...children);
}

async function onGenerate() {
  const dir = genDir.trim();
  if (!dir) {
    lastGenerateMessage = { ok: false, text: "Choose a folder to write the two new keys into first." };
    await refresh();
    return;
  }
  const answer = await call("keys_generate", { dir });
  lastGenerateMessage = answer.ok
    ? { ok: true, text: `Wrote ${answer.value.publisher_path} and ${answer.value.rotation_path}.`, lines: answer.value.lines }
    : { ok: false, text: answer.error };
  await refresh();
}

// ---------------------------------------------------------------------------
// Vault
// ---------------------------------------------------------------------------

function storedFact(label, stored) {
  return el("tr", {}, el("th", { text: label }), el("td", { class: stored ? null : "absent", text: stored ? "stored" : "missing" }));
}

function vaultFieldRow(key, label, hint) {
  const input = el("input", {
    type: "password",
    placeholder: "value to store",
    value: vaultDrafts[key],
    oninput: (e) => (vaultDrafts[key] = e.target.value),
  });
  return el(
    "div",
    { class: "row" },
    el("span", { text: label }),
    input,
    el("button", { class: "write", text: "Store", onclick: () => onVaultStore(key) }),
    el("button", { text: "Forget", onclick: () => onVaultForget(key) }),
    el("small", { class: "hint", text: hint })
  );
}

function vaultBlock(answer) {
  if (!answer.ok) {
    return el(
      "section",
      { class: "block" },
      el("h2", { text: "Vault" }),
      refusal(`The vault state could not be read: ${answer.error}`)
    );
  }
  const state = answer.value;

  const children = [
    el("h2", { text: "Vault" }),
    el("p", {
      class: "hint",
      text: "A stored value is never shown again, only whether it is stored. There is no reveal button.",
    }),
  ];

  if (state.offer_import) {
    children.push(
      partial(
        "An rclone.conf for R2 was found. Import its access key id and secret access key into the vault now? The file itself is only read, never changed."
      ),
      el(
        "div",
        { class: "row" },
        el("button", { class: "write", text: "Import from rclone.conf", onclick: onImportR2 }),
        el("button", { text: "Not now", onclick: onDismissImport })
      )
    );
  }

  if (lastVaultMessage) {
    children.push(
      lastVaultMessage.ok ? el("p", { class: "said", text: lastVaultMessage.text }) : refusal(lastVaultMessage.text)
    );
  }

  children.push(
    facts([
      // Guarded the same way `publisherKey`/`rotationKey` are above: each of
      // these three is always a populated object from the real command, but
      // a page that assumes that throws the moment it stops being true.
      storedFact("Site token", (state.site_token || {}).stored),
      storedFact("R2 access key id", (state.r2_access_key || {}).stored),
      storedFact("R2 secret access key", (state.r2_secret || {}).stored),
    ]),
    vaultFieldRow("site_token", "Site token", "the league site's API export token"),
    vaultFieldRow("r2_access_key", "R2 access key id", "the server's write credentials, first half"),
    vaultFieldRow("r2_secret", "R2 secret access key", "the server's write credentials, second half")
  );

  return el("section", { class: "block" }, ...children);
}

async function onVaultStore(key) {
  const value = vaultDrafts[key] || "";
  if (!value.trim()) {
    lastVaultMessage = { ok: false, text: "Type a value before storing." };
    await refresh();
    return;
  }
  const answer = await call("vault_store", { which: key, value });
  vaultDrafts[key] = "";
  lastVaultMessage = answer.ok ? { ok: true, text: "Stored." } : { ok: false, text: answer.error };
  await refresh();
}

async function onVaultForget(key) {
  const answer = await call("vault_forget", { which: key });
  lastVaultMessage = answer.ok ? { ok: true, text: "Forgotten." } : { ok: false, text: answer.error };
  await refresh();
}

async function onImportR2() {
  const answer = await call("vault_store", { which: "r2_import", value: "" });
  if (!answer.ok) {
    lastVaultMessage = { ok: false, text: answer.error };
  } else {
    const imported = answer.value.imported || {};
    const parts = [];
    if (imported.r2_access_key) parts.push("access key id");
    if (imported.r2_secret) parts.push("secret access key");
    if (imported.bucket_endpoint) parts.push("bucket endpoint");
    lastVaultMessage = { ok: true, text: parts.length ? `Imported: ${parts.join(", ")}.` : "Nothing new was found to import." };
  }
  await refresh();
}

async function onDismissImport() {
  // The offer is already marked seen server-side the moment `vault_state`
  // reported it; refreshing is enough to make it not come back.
  await refresh();
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

function browseRow(label, which, value) {
  return el(
    "div",
    { class: "row" },
    el("span", { text: label }),
    el("span", { class: value ? null : "absent", text: value || "not set" }),
    el("button", { text: "Browse", onclick: () => onBrowse(which) })
  );
}

function textRow(label, key, hint) {
  return el(
    "div",
    { class: "row" },
    el("span", { text: label }),
    el("input", {
      type: "text",
      value: settingsDrafts[key],
      oninput: (e) => (settingsDrafts[key] = e.target.value),
    }),
    el("button", { class: "write", text: "Save", onclick: () => onSaveSetting(key) }),
    hint ? el("small", { class: "hint", text: hint }) : null
  );
}

function settingsBlock(answer) {
  if (!answer.ok) {
    return el(
      "section",
      { class: "block" },
      el("h2", { text: "Settings" }),
      refusal(`Settings could not be read: ${answer.error}`)
    );
  }
  const s = answer.value;
  if (!settingsDraftsSeeded) {
    settingsDrafts.bucket_endpoint = s.bucket_endpoint || "";
    settingsDrafts.bucket_name = s.bucket_name || "";
    settingsDrafts.public_base = s.public_base || "";
    settingsDrafts.launcher_manifest = s.launcher_manifest || "";
    settingsDrafts.drop_root = s.drop_root || "";
    settingsDraftsSeeded = true;
  }

  const children = [el("h2", { text: "Settings" })];
  if (lastSettingsMessage) {
    children.push(
      lastSettingsMessage.ok ? el("p", { class: "said", text: lastSettingsMessage.text }) : refusal(lastSettingsMessage.text)
    );
  }

  children.push(
    browseRow("Published files (this computer's copy)", "origin", s.origin),
    browseRow("Signing key", "publisher_key", s.publisher_key),
    browseRow("Reborn Builder", "builder", s.builder),
    browseRow("xdelta3", "xdelta", s.xdelta),
    textRow("Server endpoint", "bucket_endpoint", "the R2 endpoint URL, not a secret"),
    textRow("Server bucket name", "bucket_name"),
    textRow("Public base URL", "public_base"),
    textRow("Launcher manifest URL", "launcher_manifest"),
    textRow("Drop folders", "drop_root", "one subfolder per thing: textures\\GN7E69, runtime, mods\\packages, roster, game")
  );

  return el("section", { class: "block" }, ...children);
}

async function onBrowse(which) {
  const answer = await call("settings_browse", { which });
  lastSettingsMessage = answer.ok ? null : { ok: false, text: answer.error };
  await refresh();
}

async function onSaveSetting(key) {
  const value = settingsDrafts[key] ?? "";
  const answer = await call("publisher_set_setting", { key, value });
  lastSettingsMessage = answer.ok ? { ok: true, text: "Saved." } : { ok: false, text: answer.error };
  await refresh();
}

refresh();
