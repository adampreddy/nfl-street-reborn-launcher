// The Status page.
//
// Task 1 draws the four blocks Task 2's spec names, each holding a plain
// note that its data is not wired up yet, rather than calling
// `publisher_status`, `publisher_environment` or `publisher_recent`: those
// three commands are still `unimplemented!("Task 2")` in this build, and a
// page that awaited one of them here, on the page every window opens to,
// would sit on a promise that can never settle. Task 2 replaces the body
// of every block below with the real calls; the shape (four blocks, this
// order) is what it inherits.

import { el, partial, markCurrent } from "./shell.js";

markCurrent();

function block(title, note) {
  return el("section", { class: "block" }, el("h2", { text: title }), partial(note));
}

const screen = document.getElementById("screen");
screen.append(
  block(
    "Live",
    "Not wired up yet. Task 2 reads this from publisher_status: the hosted " +
      "release's version, sequence and pins, the launcher's version, size " +
      "and hash, the download link, and whether the signature verified."
  ),
  block(
    "Local origin",
    "Not wired up yet. Task 2 lists anything published locally that the " +
      "bucket does not yet serve, each marked pending, with an Upload " +
      "button that runs the upload operation through the panel."
  ),
  block(
    "This machine",
    "Not wired up yet. Task 2 reads this from publisher_environment: " +
      "publisher.key, the Builder, xdelta, cargo, Inno Setup, the R2 " +
      "credentials, the site token, the base image and the import bundle, " +
      "each found (with its path) or missing (with a Browse button)."
  ),
  block(
    "Recent",
    "Not wired up yet. Task 2 reads the last ten journal records from " +
      "publisher_recent, newest first."
  )
);
