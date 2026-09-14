// Slab site interactions: mobile menu, copy-to-clipboard, scroll reveal.
(function () {
  "use strict";

  // Mobile menu
  var menuBtn = document.getElementById("menuBtn");
  var mobileLinks = document.getElementById("mobileLinks");
  if (menuBtn && mobileLinks) {
    menuBtn.addEventListener("click", function () {
      var open = mobileLinks.classList.toggle("open");
      menuBtn.setAttribute("aria-expanded", open ? "true" : "false");
      menuBtn.setAttribute("aria-label", open ? "Close menu" : "Open menu");
    });
    mobileLinks.querySelectorAll("a").forEach(function (a) {
      a.addEventListener("click", function () {
        mobileLinks.classList.remove("open");
        menuBtn.setAttribute("aria-expanded", "false");
      });
    });
  }

  // Copy to clipboard
  var toast = document.getElementById("toast");
  var toastTimer = null;
  function showToast(msg) {
    if (!toast) return;
    toast.textContent = msg || "Copied to clipboard";
    toast.classList.add("show");
    clearTimeout(toastTimer);
    toastTimer = setTimeout(function () { toast.classList.remove("show"); }, 1800);
  }
  function copyText(text, done) {
    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard.writeText(text).then(done, function () { fallback(); });
    } else {
      fallback();
    }
    function fallback() {
      var ta = document.createElement("textarea");
      ta.value = text;
      ta.style.position = "fixed";
      ta.style.opacity = "0";
      document.body.appendChild(ta);
      ta.select();
      try { document.execCommand("copy"); } catch (e) {}
      document.body.removeChild(ta);
      done();
    }
  }
  document.querySelectorAll("[data-copy]").forEach(function (el) {
    el.addEventListener("click", function () {
      copyText(el.getAttribute("data-copy"), function () { showToast("Copied to clipboard"); });
    });
  });

  // Sticky mobile CTA: show after the hero, hide at the download section
  var stickyCta = document.getElementById("stickyCta");
  var downloadSec = document.getElementById("download");
  if (stickyCta) {
    var past = false, atDownload = false;
    function renderCta() {
      stickyCta.classList.toggle("show", past && !atDownload);
    }
    window.addEventListener("scroll", function () {
      past = window.scrollY > window.innerHeight * 0.75;
      renderCta();
    }, { passive: true });
    if (downloadSec && "IntersectionObserver" in window) {
      new IntersectionObserver(function (entries) {
        atDownload = entries[0].isIntersecting;
        renderCta();
      }, { threshold: 0.15 }).observe(downloadSec);
    }
  }

  // Image loading states
  document.querySelectorAll("img").forEach(function (img) {
    if (!img.complete) {
      img.classList.add("is-loading");
      img.addEventListener("load", function () { img.classList.remove("is-loading"); });
      img.addEventListener("error", function () { img.classList.remove("is-loading"); });
    }
  });

  // Scroll reveal
  var revealEls = document.querySelectorAll(".reveal");
  if ("IntersectionObserver" in window && revealEls.length) {
    var io = new IntersectionObserver(function (entries) {
      entries.forEach(function (entry) {
        if (entry.isIntersecting) {
          entry.target.classList.add("in");
          io.unobserve(entry.target);
        }
      });
    }, { threshold: 0.12, rootMargin: "0px 0px -40px 0px" });
    revealEls.forEach(function (el) { io.observe(el); });
  } else {
    revealEls.forEach(function (el) { el.classList.add("in"); });
  }

  // Live GitHub star count (graceful fallback keeps the static label)
  var ghStar = document.getElementById("ghStar");
  if (ghStar) {
    fetch("https://api.github.com/repos/Sanjays2402/slab")
      .then(function (r) { return r.ok ? r.json() : null; })
      .then(function (d) {
        if (d && typeof d.stargazers_count === "number") {
          var n = d.stargazers_count;
          var s = n >= 1000 ? (n / 1000).toFixed(1).replace(/\.0$/, "") + "k" : String(n);
          ghStar.textContent = "\u2605 " + s + " \u2014 Star on GitHub";
        }
      })
      .catch(function () {});
  }

  // Count-up stats
  document.querySelectorAll(".num[data-count]").forEach(function (el) {
    var target = parseInt(el.dataset.count, 10) || 0;
    var seen = new IntersectionObserver(function (entries, ob) {
      entries.forEach(function (e) {
        if (!e.isIntersecting) return;
        ob.disconnect();
        var start = null, dur = 1100;
        function step(ts) {
          if (!start) start = ts;
          var p = Math.min((ts - start) / dur, 1);
          el.textContent = Math.round(target * (1 - Math.pow(1 - p, 3)));
          if (p < 1) requestAnimationFrame(step);
        }
        requestAnimationFrame(step);
      });
    }, { threshold: 0.4 });
    seen.observe(el);
  });

  // Playground: the real 65-tool palette, filterable
  var TOOLS = ["Toolbox","Reader","Library","Search Library","Beacon AI","Beacon Search","PII Redact","Citations","Study","Glossary","Voice","Merge","Split","Split by Chapter","Pages","Pages (list)","Edit Text","Compress","Extract","Encrypt","Watermark","Convert","Metadata","Numbers","Sign","Loom (PDF/UA)","Crop","Insert","Header/Footer","Bates","Legal Stamp","Signet","Batch Sign","Redact","Veil","Compact","Streamline (Fast Web View)","Reflow (PDF \u2192 Word)","Tabulate (PDF \u2192 Excel)","Markdown (PDF \u2192 MD / HTML)","Bind (PDF \u2192 EPUB)","Auto-Redact","N-up","Markdown \u2192 PDF","Grayscale","Page Labels","Flatten","Sanitize","Repair","OCR","Tables \u2192 CSV","Diff","Compare 3-way","Compare","Archive (PDF/A)","Press (PDF/X-4)","Forms","Quill Batch (CSV merge)","Quill Designer (author fields)","Quill Auto-Detect (\u2728 propose fields)","Atelier (Recipes)","Hopper (Watched Folders)","Loupe (PDF/A check)","Slides","Theater"];
  var pInput = document.getElementById("paletteInput");
  var pList = document.getElementById("paletteList");
  var pCount = document.getElementById("paletteCount");
  function esc(s) { return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;"); }
  if (pInput && pList) {
    var matches = [], sel = 0;
    function paint() {
      pList.innerHTML = matches.length
        ? matches.map(function (t, i) {
            return '<li data-i="' + i + '" class="' + (i === sel ? "on" : "") + '"><span class="k">\u29c9</span>' + esc(t) + "</li>";
          }).join("")
        : '<li class="empty">No tool matches &mdash; <a href="#download">it&rsquo;s still free though.</a></li>';
      if (pCount) pCount.textContent = matches.length + " of " + TOOLS.length + " tools";
    }
    function filter() {
      var q = pInput.value.trim().toLowerCase();
      matches = TOOLS.filter(function (t) { return t.toLowerCase().indexOf(q) !== -1; }).slice(0, 8);
      sel = 0;
      paint();
    }
    function pick(i) {
      var t = matches[i];
      if (t) showToast("\u201c" + t + "\u201d lives in the app \u2014 download Slab to use it.");
    }
    pList.addEventListener("click", function (e) {
      var li = e.target.closest("li[data-i]");
      if (li) pick(parseInt(li.dataset.i, 10));
    });
    pList.addEventListener("mousemove", function (e) {
      var li = e.target.closest("li[data-i]");
      if (li) { sel = parseInt(li.dataset.i, 10); paint(); }
    });
    pInput.addEventListener("input", filter);
    pInput.addEventListener("keydown", function (e) {
      if (e.key === "ArrowDown") { e.preventDefault(); sel = Math.min(sel + 1, matches.length - 1); paint(); }
      else if (e.key === "ArrowUp") { e.preventDefault(); sel = Math.max(sel - 1, 0); paint(); }
      else if (e.key === "Enter") { e.preventDefault(); pick(sel); }
    });
    filter();
  }

})();
