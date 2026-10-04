// Restore only a known website accent; storage is an optional preference.
(function () {
  "use strict";
  var colors = ["orange", "blue", "purple", "green", "pink", "teal", "amber", "lime", "slate"];
  var current = "orange";
  try {
    var saved = localStorage.getItem("slab.site.accent.v1");
    if (colors.indexOf(saved) !== -1) current = saved;
  } catch (_) {}
  document.documentElement.dataset.accent = current;
})();
