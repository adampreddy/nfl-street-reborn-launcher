#!/usr/bin/env python3
"""End-to-end first run of the Linux launcher, the way a Deck player does it.

Drives the REAL launcher binary through WebDriver (tauri-driver ->
WebKitWebDriver), in a fresh HOME, against the live content server, from the
Welcome screen to the Play page and then Play. Every screen is screenshotted
and its text logged, so a stuck screen says where and why.

The one stand-in: the native file picker cannot be clicked through WebDriver,
so the launcher answers `pick_path` with REBORN_E2E_PICK_GAME (the game file).
Everything else is the launcher's own code and the server's own content.

Usage (inside the jammy build root, with Xvfb on $DISPLAY and tauri-driver
listening on 127.0.0.1:4444):  e2e_first_run.py <launcher-binary> <game-file> <out-dir>
"""
import base64, json, os, sys, time, urllib.error, urllib.request

APP, GAME, OUT = sys.argv[1], sys.argv[2], sys.argv[3]
os.makedirs(OUT, exist_ok=True)
BASE = "http://127.0.0.1:4444"
LOG = open(os.path.join(OUT, "steps.log"), "w", buffering=1)


def log(*a):
    line = " ".join(str(x) for x in a)
    print(line, flush=True)
    LOG.write(line + "\n")


def wd(method, path, body=None, timeout=600):
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(BASE + path, data=data, method=method, headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return json.loads(r.read() or b"{}").get("value")
    except urllib.error.HTTPError as e:
        raise RuntimeError(f"WebDriver {method} {path}: {e.read().decode(errors='replace')[:800]}")


session = wd("POST", "/session", {"capabilities": {"alwaysMatch": {"tauri:options": {"application": APP}}}})["sessionId"]
S = f"/session/{session}"


def js(script, *args):
    return wd("POST", S + "/execute/sync", {"script": script, "args": list(args)})


def shot(name):
    png = wd("GET", S + "/screenshot")
    with open(os.path.join(OUT, f"{name}.png"), "wb") as f:
        f.write(base64.b64decode(png))


# The file picker stand-in is the launcher's REBORN_E2E_PICK_GAME (set by
# e2e-first-run.sh): the page's invoke is frozen and cannot be swapped.

STATE = """
const vis = e => !!(e.offsetWidth || e.offsetHeight || e.getClientRects().length);
const next = document.getElementById('next');
return {
  url: location.pathname,
  heading: (document.querySelector('h1,h2') || {}).textContent || '',
  text: document.body.innerText.slice(0, 1500),
  next: next ? {label: next.textContent.trim(), disabled: next.disabled} : null,
  buttons: [...document.querySelectorAll('button')].filter(vis).map(b => b.textContent.trim()).filter(Boolean),
  radios: [...document.querySelectorAll('input[type=radio]')].map(r => ({v: r.value, checked: r.checked})),
};
"""

CLICK_TEXT = """
const want = arguments[0];
const b = [...document.querySelectorAll('button')].find(b => b.textContent.trim() === want && !b.disabled);
if (!b) return false; b.click(); return true;
"""

SELECT_LATER = """
const s = [...document.querySelectorAll('select')].find(s => [...s.options].some(o => o.value === 'none'));
if (!s || s.value) return false;
s.value = 'none'; s.dispatchEvent(new Event('change', {bubbles: true})); return true;
"""

PICK_FIRST = """
const r = document.querySelector('input[type=radio]:not(:disabled)');
if (!r) return false; r.click(); return true;
"""

used = set()
last_key, same_since, step = None, time.time(), 0
deadline = time.time() + 60 * 60
while time.time() < deadline:
    st = js(STATE)
    key = (st["url"], st["heading"], st["text"][:300], json.dumps(st["next"]))
    if key != last_key:
        step += 1
        log(f"\n=== step {step} {st['url']} :: {st['heading'].strip()}")
        log("text:", st["text"].replace("\n", " | ")[:900])
        log("buttons:", st["buttons"], "next:", st["next"])
        shot(f"{step:02d}")
        last_key, same_since = key, time.time()
    if "couldn't be completed" in st["text"] or "could not" in st["text"].lower() and "Reborn could not convert" not in st["text"]:
        log("!!! ERROR ON SCREEN"); shot(f"{step:02d}-error")
    if st["url"].endswith("index.html") and "Play" in st["text"] and "onboarding" not in st["url"]:
        log("=== reached the Play page"); shot("play-page"); break
    # Choices, once each: Start fresh; the game file.
    acted = False
    for r in st["radios"]:
        if r["v"] == "fresh" and not r["checked"] and "radio-fresh" not in used:
            js("const r=document.querySelector('input[type=radio][value=fresh]'); r.click(); return 1;")
            used.add("radio-fresh"); acted = True
    # A step that waits on a choice nobody made (graphics preset, button
    # textures): take the first option, as a player clicking through would.
    if not acted and st["next"] and st["next"]["disabled"] and st["radios"] and not any(r["checked"] for r in st["radios"]):
        if js(PICK_FIRST): acted = True; log("chose the first option")
    # No pad in a headless run: the controller step's own way past.
    if not acted and "select-later" not in used and js(SELECT_LATER):
        used.add("select-later"); acted = True; log("chose: Set up a controller later")
    for label in ("Browse for NFL Street 2", "Skip for now", "Skip"):
        if not acted and label in st["buttons"] and label not in used:
            if js(CLICK_TEXT, label): used.add(label); acted = True; log("clicked", label)
    if not acted and st["next"] and not st["next"]["disabled"] and "wait" not in st["next"]["label"].lower():
        js("document.getElementById('next').click(); return 1;"); log("clicked next:", st["next"]["label"])
    if time.time() - same_since > 15 * 60:
        log("!!! STUCK 15 min on this screen"); shot(f"{step:02d}-stuck"); break
    time.sleep(3)
else:
    log("!!! out of time")
LOG.close()
try:
    wd("DELETE", S)
except Exception:
    pass
