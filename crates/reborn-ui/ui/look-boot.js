// Before first paint: the look the player chose, from the last answer the core gave.
// Classic script on purpose (not a module) so it runs before the page paints.
// Glass defaults OFF until the core confirms it, so a window is never transparent with nothing behind it.
(function () {
  var look = { glass: false, slow: true };
  try { var c = JSON.parse(localStorage.getItem("reborn:look") || "null"); if (c && typeof c === "object") { look.glass = c.glass === true; look.slow = c.slow !== false; } } catch (e) {}
  var d = document.documentElement.dataset;
  d.glass = look.glass ? "on" : "off";
  d.motion = look.slow ? "slow" : "quick";
})();
