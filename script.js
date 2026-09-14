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
})();
