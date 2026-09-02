# The beta environment

Two layers, for two different questions.

| Layer | Answers | Where it runs |
|---|---|---|
| **Beta channel** | Does the launcher work, on real hardware, for testers, without touching anyone's real install? | Any Windows PC, beside a stable or 2.1 install |
| **Sandbox smoke test** | Does the installer behave on a machine that has never seen Reborn, and on one upgrading from 2.1, and does uninstalling keep the saves? | A disposable Windows Sandbox on this PC |

## 1. The beta channel

A beta build is a normal build with one difference: a `channel` file beside
the executable containing `beta`. The launcher reads it at start and appends
` Beta` to every root it owns:

| | Stable | Beta |
|---|---|---|
| Program files | `%LOCALAPPDATA%\Programs\NFL Street Reborn Launcher` | `...\NFL Street Reborn Launcher Beta` |
| Data root | `%APPDATA%\NFL Street Reborn Launcher` | `%APPDATA%\NFL Street Reborn Launcher Beta` |
| Cache | `%LOCALAPPDATA%\NFL Street Reborn\cache` | `%LOCALAPPDATA%\NFL Street Reborn Beta\cache` |
| Uninstall entry | 2.1's AppId, `{DCD6B9B9-...}` | its own, `{7C3E9A5D-...}` |
| Programs and Features name | NFL Street Reborn | NFL Street Reborn Beta |

So a beta never writes where the stable launcher reads, never replaces the
2.1 uninstall entry, and uninstalling it removes only itself. That is what
makes it safe to hand to testers, and safe to install on the development PC
beside the real 2.1 install.

What a beta still shares, on purpose: the game itself. Dolphin runs against
the same User folder a stable launch would resolve (the 2.1 one, when it
exists), so saves, controller profiles and the texture folder are the
player's real ones. Mods composed by a beta land in the beta's own data root,
and the launcher points the emulator's texture folder at them by junction;
the stable launcher points it back at its own tree on its next launch. Nothing
is deleted either way.

Build one:

```powershell
.\installer\windows\build.ps1 -Channel beta -Version 3.0.0-beta.1 `
    -Runtime "C:\path\to\Reborn Dolphin" -GameImages "C:\path\to\NSR.iso"
```

The result is `dist\NFL Street Reborn Beta Launcher Setup 3.0.0-beta.1.exe`.
Leave out `-GameImages` and the launcher looks for an image in an existing
2.1 install instead.

To run any build as beta without installing it, set `REBORN_CHANNEL=beta` in
the environment. The environment wins over the marker file, so an installed
beta can also be run as stable for a comparison.

## 2. The sandbox smoke test

```powershell
.\Tools\beta-sandbox.ps1
```

This boots Windows Sandbox, a disposable Windows that starts from a pristine
image every time, with three folders mapped in: `dist\` (the installers, read
only), `installer\beta\` (read only) and `target\beta-out\` (read-write). At
logon the sandbox runs `sandbox-smoke.ps1`, which:

1. Lays a **fake 2.1 install**: the `%APPDATA%\NFL Street Reborn` folder, a
   memory card file inside it, a texture, and the uninstall registry entry
   under 2.1's AppId, exactly as the real installer leaves them.
2. Runs the **stable installer silently** as an upgrade over it, and checks
   that it landed in Programs and not on the 2.1 data folder, that the
   uninstall entry now points at Programs, that the runtime shipped, and that
   the memory card is byte for byte what it was.
3. **Uninstalls** it silently and checks the memory card and the 2.1 texture
   folder survived. This is the bug the 2.1 uninstaller has.
4. Runs the **beta installer** and checks it landed in its own folder, carries
   its channel marker, has its own uninstall entry, left the 2.1 entry alone,
   and that its `rebornctl status` names the beta channel and data root.

Results land on this machine in `target\beta-out\result.txt`, one PASS or
FAIL per check, with `smoke.log` and the installer logs beside it. The sandbox
window stays open with the result showing, so a person can open the launcher
inside it and look around. Close the window and it is gone.

**If Windows Sandbox is not enabled** the script says so and prints the one
command that enables it. That needs an administrator PowerShell and a restart:

```powershell
Enable-WindowsOptionalFeature -Online -FeatureName "Containers-DisposableClientVM" -All
```

The sandbox has no WebView2 runtime, which the launcher's window needs. The
installer carries Microsoft's bootstrapper and installs it when it is absent,
which needs the network; the sandbox has networking on by default. The check
for it is in the results, and a machine without network fails that check and
nothing else.

## What this does not cover

- Playing the game inside the sandbox. It has a virtual GPU, and Dolphin may
  run slowly or not at all on it. The sandbox proves the installer; the beta
  channel proves the launcher, on real hardware.
- macOS and Linux. Their builds come from CI and have not been run.
- Upgrading a real 2.1 install in place. The fake one has the folder, the
  card and the registry entry, which is everything the installer looks at,
  but the real thing is 8 GB and lives on this PC. Install the STABLE build
  here only when it is the build you mean to keep.
