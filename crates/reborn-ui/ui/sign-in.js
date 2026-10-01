// Discord sign-in by device code, for pages other than Play (Play keeps its own copy in home.js).
// Same rules as Play: one start at a time, one poll at a time, the code and address are always shown,
// and a finished or expired sign-in always clears the old message.
import { call, reason } from "./shell.js";

export function createSignIn({ onChange, onDone }) {
  const s = { active: false, starting: false, polling: false, timer: null, deviceCode: null, userCode: null, verifyUrl: null, message: null };
  function stop(message) {
    if (s.timer) clearInterval(s.timer);
    s.timer = null; s.active = false; s.deviceCode = null; s.userCode = null; s.verifyUrl = null;
    s.message = message || null;
  }
  async function start() {
    if (s.active || s.starting) return;
    s.starting = true; s.message = null; onChange();
    try {
      const r = await call("identity_sign_in_start");
      s.active = true; s.deviceCode = r.device_code; s.userCode = r.user_code; s.verifyUrl = r.verify_url;
      onChange();
      s.timer = setInterval(async () => {
        if (s.polling || !s.active) return;
        s.polling = true;
        try {
          const p = await call("identity_sign_in_poll", { deviceCode: s.deviceCode });
          if (p.state === "done") { stop(); await onDone(); }
          else if (p.state === "expired") { stop("The sign-in code expired. Try again."); onChange(); }
        } catch (e) { stop(reason(e)); onChange(); }
        finally { s.polling = false; }
      }, Math.max(2, r.interval || 3) * 1000);
    } catch (e) { s.message = reason(e); }
    finally { s.starting = false; onChange(); }
  }
  globalThis.addEventListener?.("pagehide", () => stop());
  return { state: s, start, cancel: () => { stop(); onChange(); } };
}
