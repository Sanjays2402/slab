<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { listRecent, formatRelTime, type RecentFile } from "$lib/recent";
  import { setUiConfig, ACCENT_COLORS, BUILT_IN_THEMES, type Density } from "$lib/theme";
  import { recordMru, mruRanks, clearMru } from "$lib/cmdMru";
  import { openPanelWindow } from "$lib/windows";
  import { isInTauri } from "$lib/tauri";
  import { vimEnabled } from "$lib/vim/mode";
  import { LOCALES, setLocale, t } from "$lib/i18n";
  import { pluginsStore, runPluginCommand, type CommandOutcome } from "$lib/plugins";
  import { applyPluginTheme } from "$lib/pluginThemes";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { notify } from "$lib/notify";
  import {
    scorePaletteEntry,
    splitHighlight,
    classifyPaletteNav,
    nextPaletteIndex,
    paletteKeymapId,
    suggestPaletteFallback,
    parsePaletteScope,
    entryMatchesScope,
    describePaletteScope,
    classifyPaletteGroupNav,
    nextGroupIndex,
    recentReadingProgress,
    describePaletteCount,
    paletteActionVerb,
    toggleCollapsedGroup,
    partitionCollapsedGroups,
    collapseAllGroups,
    isEveryGroupCollapsed,
    describeCollapseState,
    soloExpandGroup,
    type PaletteRange,
    type PaletteFallback,
    type RecentProgress,
  } from "$lib/paletteSearch";
  import { loadCollapsedGroups, saveCollapsedGroups } from "$lib/paletteCollapsed";
  import { prettyBindingFor, keymapView, type ActionId } from "$lib/keymap";
  import { get } from "svelte/store";

  // Cabinet (v1.1.0) Slice 5 — panels that can be detached into their own
  // native window. Must stay in sync with `DETACHABLE_PANELS` in
  // `+page.svelte`. Duplicated as a typed string-literal union here so the
  // palette doesn't have to import that route module at runtime.
  const DETACHABLE_PANELS = new Set<string>([
    "reader",
    "library",
    "beacon",
    "search",
    "pii",
    "pages",
    "pages-list",
    "diff",
    "stack",
    "slides",
    "tables",
    "markdown",
    "voice",
  ]);

  type Action = {
    id: string;
    title: string;
    subtitle?: string;
    icon: string;
    group: string;
    run: () => void;
    keywords?: string;
    // Lumen II Slice 4: optional reading-progress chip for recent-file rows.
    progress?: RecentProgress;
  };

  type Props = {
    open: boolean;
    panels: { id: string; label: string; icon: string; ready: boolean }[];
    activePanel: string;
    onClose: () => void;
    onSelectPanel: (id: string) => void;
    onOpenRecent: (file: RecentFile, opts?: { newTab?: boolean }) => void;
    onShowShortcuts?: () => void;
  };

  let {
    open = $bindable(false),
    panels,
    activePanel,
    onClose,
    onSelectPanel,
    onOpenRecent,
    onShowShortcuts,
  }: Props = $props();

  let query = $state("");
  let selected = $state(0);
  let inputEl: HTMLInputElement | undefined = $state();
  let listEl: HTMLElement | undefined = $state();
  let recents = $state<RecentFile[]>([]);
  // Glass Slice 5: MRU ranks for actions (id → rank, lower = more recent).
  let mru = $state<Record<string, number>>({});
  // Foundry Slice 9: live snapshot of plugin contributions. Subscribed so
  // the palette re-derives when a plugin is enabled / disabled.
  let pluginsSnap = $state(get(pluginsStore));
  const unsubPlugins = pluginsStore.subscribe((s) => (pluginsSnap = s));

  function refreshRecents() {
    recents = listRecent();
  }
  function refreshMru() {
    mru = mruRanks();
  }

  // Foundry Slice 9 — dispatch a plugin command. For URL outcomes,
  // open via tauri opener plugin; for shell outcomes, surface a toast
  // with status + truncated stdout/stderr so the user knows it ran.
  async function dispatchPluginCommand(
    pluginId: string,
    commandId: string,
    label: string,
  ): Promise<void> {
    try {
      const outcome: CommandOutcome = await runPluginCommand(pluginId, commandId);
      if (outcome.kind === "url") {
        await openUrl(outcome.url);
        notify.info(label, { detail: `Opened ${outcome.url}` });
        return;
      }
      const stdoutShort = outcome.stdout.trim().slice(0, 200);
      const stderrShort = outcome.stderr.trim().slice(0, 200);
      if (outcome.status === "ok") {
        notify.success(label, {
          detail: stdoutShort || `exit 0 · ${outcome.duration_ms}ms`,
        });
      } else if (outcome.status === "nonzeroexit") {
        notify.warning(label, {
          detail: `${stderrShort || stdoutShort || "non-zero exit"} · exit ≠ 0`,
        });
      } else if (outcome.status === "timeout") {
        notify.error(label, { detail: "Command timed out (30s)" });
      } else {
        notify.error(label, { detail: stderrShort || "Failed to spawn" });
      }
    } catch (e) {
      notify.error(label, { detail: e instanceof Error ? e.message : String(e) });
    }
  }

  $effect(() => {
    if (open) {
      query = "";
      selected = 0;
      refreshRecents();
      refreshMru();
      // focus next tick once mounted
      queueMicrotask(() => inputEl?.focus());
    }
  });

  // Build the action list each time inputs change.
  let actions = $derived.by<Action[]>(() => {
    const out: Action[] = [];
    for (const p of panels) {
      if (!p.ready) continue;
      out.push({
        id: `panel:${p.id}`,
        title: p.label,
        subtitle: p.id === activePanel ? "Current panel" : `Switch to ${p.label}`,
        icon: p.icon,
        group: "Panels",
        run: () => onSelectPanel(p.id),
        keywords: `${p.label} panel ${p.id}`,
      });
    }
    // v3.28.0 Quill Hub: surface each Forms sub-tab as its own
    // palette action so users can `Cmd+K` → "fill form" → enter,
    // even if they've never seen the sidebar entry. This is the
    // discoverability layer for the four-tab unified workflow.
    const FORMS_SUBTABS: { id: "detect" | "design" | "fill" | "smartfill" | "batch"; title: string; subtitle: string; icon: string; keywords: string }[] = [
      {
        id: "detect",
        title: "Forms: Auto-Detect fields",
        subtitle: "Propose form fields on a flat PDF",
        icon: "✨",
        keywords: "quill autodetect detect propose form acroform",
      },
      {
        id: "design",
        title: "Forms: Designer",
        subtitle: "Draw and edit AcroForm fields by hand",
        icon: "✎",
        keywords: "quill designer draw author author form acroform fields",
      },
      {
        id: "fill",
        title: "Forms: Fill a PDF",
        subtitle: "Type values into an existing AcroForm",
        icon: "📝",
        keywords: "fill inspect acroform values type form",
      },
      {
        id: "smartfill",
        title: "Forms: Smart Fill from source doc",
        subtitle: "Drop a resume/contact/CSV — AI maps to fields",
        icon: "🪄",
        keywords: "quill smart fill ai magic auto-fill resume mapping intelligent beacon",
      },
      {
        id: "batch",
        title: "Forms: Batch with CSV",
        subtitle: "Mail-merge a CSV across many copies",
        icon: "⋮",
        keywords: "quill batch csv merge mail batch many copies",
      },
    ];
    for (const sub of FORMS_SUBTABS) {
      out.push({
        id: `panel:forms:${sub.id}`,
        title: sub.title,
        subtitle: sub.subtitle,
        icon: sub.icon,
        group: "Forms",
        run: () => {
          // Set the sub-tab BEFORE switching the active panel so the
          // Hub mounts on the right child first paint (no flicker).
          import("$lib/quill").then((m) => m.setActiveTab(sub.id));
          onSelectPanel("forms");
        },
        keywords: `forms ${sub.keywords}`,
      });
    }

    // v3.31.0 Atlas Lite: Home / Recents palette entries. Cmd+0 lands you
    // back on the Recents Home from anywhere; Cmd+Shift+0 opens the file
    // you were most recently reading. Each pinned/recent file is also a
    // direct palette entry so power users can fuzzy-find by name.
    out.push({
      id: "home:open",
      title: "Go to Recents Home",
      subtitle: "Hero card · Continue reading · pinned & recent files",
      icon: "🏠",
      group: "Home",
      run: () => {
        // Closing the active document falls back to RecentsHome.
        window.dispatchEvent(new CustomEvent("slab:home-open"));
      },
      keywords: "home recents continue reading start landing dashboard",
    });
    out.push({
      id: "home:continue",
      title: "Continue reading",
      subtitle: "Jump to your most recently read document",
      icon: "▶",
      group: "Home",
      run: () => {
        window.dispatchEvent(new CustomEvent("slab:home-continue"));
      },
      keywords: "resume continue last opened recent reading",
    });
    // Pinned + recent files become direct palette entries — opens are
    // dispatched through the same `slab:open-recent` channel the
    // ReaderPanel already listens on.
    try {
      const recents = listRecent();
      for (const r of recents.slice(0, 20)) {
        out.push({
          id: `recent:${r.path}`,
          title: `Open ${r.name}`,
          subtitle: r.path,
          icon: r.pinned ? "📌" : "📄",
          group: r.pinned ? "Pinned" : "Recent",
          run: () => onOpenRecent(r, { newTab: lastActivationNewTab }),
          keywords: `${r.name} ${r.path} ${r.pinned ? "pinned" : "recent"} open file`,
        });
      }
    } catch { /* recent module not loadable — skip dynamic entries */ }


    // v3.29.0: replay the Forms onboarding tour from anywhere. This is
    // how users who skipped the auto-fire (or want a refresher) get back
    // to the 30-second walkthrough.
    out.push({
      id: "forms:tour",
      title: "Forms: Show welcome tour",
      subtitle: "Replay the 30-second tour of the Forms workspace",
      icon: "🍰",
      group: "Forms",
      run: () => {
        import("$lib/quill-tour").then((m) => m.replayTour());
        onSelectPanel("forms");
      },
      keywords: "forms tour onboarding welcome help walkthrough guide intro",
    });

    // Cabinet Slice 5: "Open <panel> in new window" — only inside Tauri
    // (no detached windows in vanilla browser dev), and only for panels
    // that have a real detached experience.
    if (isInTauri()) {
      for (const p of panels) {
        if (!p.ready) continue;
        if (!DETACHABLE_PANELS.has(p.id)) continue;
        out.push({
          id: `panel-window:${p.id}`,
          title: `Open ${p.label} in new window`,
          subtitle: "Detach into its own native window",
          icon: "⤢",
          group: "Windows",
          run: () => {
            void openPanelWindow(p.id);
          },
          keywords: `${p.label} ${p.id} detach window new open float separate panel cabinet`,
        });
      }
    }
    for (const r of recents) {
      // Lumen II Slice 4: surface per-document reading position as a chip.
      const prog = recentReadingProgress(r);
      out.push({
        id: `recent:${r.path}`,
        title: r.name,
        subtitle: `Open · ${formatRelTime(r.openedAt)}${!prog.hasProgress && r.pageCount ? ` · ${r.pageCount} pages` : ""}`,
        icon: "▥",
        group: "Recent files",
        run: () => onOpenRecent(r, { newTab: lastActivationNewTab }),
        keywords: `${r.name} ${r.path} pdf recent`,
        progress: prog.hasProgress ? prog : undefined,
      });
    }
    // Theme quick actions
    for (const th of BUILT_IN_THEMES) {
      out.push({
        id: `theme:${th.id}`,
        title: `Theme: ${th.label}`,
        subtitle: "Switch appearance",
        icon: th.icon,
        group: "Appearance",
        run: () => void setUiConfig({ theme: th.id }),
        keywords: `theme appearance ${th.id} ${th.label} light dark auto`,
      });
    }
    // Foundry Slice 9 — plugin-contributed themes. Activating one swaps
    // a runtime style tag; picking any built-in clears it via
    // clearPluginTheme inside setUiConfig.
    for (const th of pluginsSnap.themes) {
      out.push({
        id: `plugin-theme:${th.plugin_id}:${th.id}`,
        title: `Theme: ${th.label}`,
        subtitle: `From plugin ${th.plugin_id}${th.dark ? " · dark" : ""}`,
        icon: "◇",
        group: "Appearance",
        run: () => {
          void applyPluginTheme(th.plugin_id, th.id, th.css).catch((e) => {
            console.warn("[slab] applyPluginTheme failed", e);
          });
        },
        keywords: `theme appearance ${th.id} ${th.label} ${th.plugin_id} plugin custom`,
      });
    }
    for (const a of ACCENT_COLORS) {
      out.push({
        id: `accent:${a.id}`,
        title: `Accent: ${a.label}`,
        subtitle: a.hex,
        icon: "●",
        group: "Appearance",
        run: () => void setUiConfig({ accent: a.id }),
        keywords: `accent color ${a.id} ${a.label}`,
      });
    }
    const densities: { id: Density; label: string; icon: string }[] = [
      { id: "comfortable", label: "Comfortable", icon: "▭" },
      { id: "compact", label: "Compact", icon: "▬" },
    ];
    for (const d of densities) {
      out.push({
        id: `density:${d.id}`,
        title: `Density: ${d.label}`,
        subtitle: "Spacing",
        icon: d.icon,
        group: "Appearance",
        run: () => void setUiConfig({ density: d.id }),
        keywords: `density spacing ${d.id} ${d.label}`,
      });
    }
    if (onShowShortcuts) {
      out.push({
        id: "help:shortcuts",
        title: "Keyboard shortcuts",
        subtitle: "Show the full reference (?)",
        icon: "⌨",
        group: "Help",
        run: () => onShowShortcuts!(),
        keywords: "keyboard shortcuts help reference cheatsheet bindings",
      });
    }
    // Atlas v2.2.0 — palette shortcut to the cross-document search panel.
    // The action also focuses the input via the `slab:focus-library-search`
    // event so the user can type immediately after the palette closes.
    out.push({
      id: "library:search",
      title: "Search library",
      subtitle: "Find a word across every PDF you've added (⇧⌘F)",
      icon: "🔎",
      group: "Library",
      run: () => {
        onSelectPanel("library-search");
        // Defer until after the route swap so the SearchPanel input is mounted.
        queueMicrotask(() => {
          window.dispatchEvent(new CustomEvent("slab:focus-library-search"));
        });
      },
      keywords: "search library find query fts full text cross document indemnify clause atlas",
    });
    // v3.35.0 "Atlas Presets" — open the preset picker from anywhere.
    // Dispatches a window event that CollectionsSidebar listens to;
    // we use an event rather than `bind:this` because the sidebar may
    // not yet be mounted (user could be on the reader panel).
    out.push({
      id: "library:preset-picker",
      title: "Add smart collection from preset…",
      subtitle: "Tax 2025, Invoices, Contracts pending, Receipts… (⇧⌘P)",
      icon: "★",
      group: "Library",
      run: () => {
        onSelectPanel("library");
        queueMicrotask(() => {
          window.dispatchEvent(new CustomEvent("slab:open-preset-picker"));
        });
      },
      keywords: "preset smart collection template tax invoice receipt contract legal research manual scanned untagged atlas built-in starter",
    });
    // v3.37.0 "Atlas Smart Folders Hub" — unified panel for built-in
    // + personal presets with drag-to-reorder, pin, bulk export.
    out.push({
      id: "library:smart-folders-hub",
      title: "Smart Folders Hub…",
      subtitle: "Manage built-in + personal smart folders — pin, reorder, export pack (⇧⌘F)",
      icon: "🗂",
      group: "Library",
      run: () => {
        onSelectPanel("library");
        queueMicrotask(() => {
          window.dispatchEvent(new CustomEvent("slab:open-smart-folders-hub"));
        });
      },
      keywords: "smart folders hub manage organize pin reorder drag preset built-in personal pack export bulk atlas",
    });
    // v3.52.0 "Atlas OCR-Queue" — dedicated panel for the auto-OCR
    // pipeline: dashboard counts, failure inbox with persisted reasons,
    // re-queue/retry, pending preview.
    out.push({
      id: "library:ocr-queue",
      title: "OCR Queue…",
      subtitle: "Pending docs, failure inbox with reasons, re-queue (⇧⌘O)",
      icon: "◳",
      group: "Library",
      run: () => {
        onSelectPanel("library");
        queueMicrotask(() => {
          window.dispatchEvent(new CustomEvent("slab:open-ocr-queue"));
        });
      },
      keywords: "ocr queue scan tesseract retry failed pending inbox dashboard auto recognise text searchable image atlas",
    });
    // v3.54.0 "Atlas Beacon-Cache" — dedicated inspector for the
    // embedding index: per-model breakdown, stale-path detection,
    // multi-select forget, full indexed-PDF table.
    out.push({
      id: "library:beacon-cache",
      title: "Beacon Cache…",
      subtitle: "Inspect & prune the semantic-search index: per-model, stale paths, bulk forget",
      icon: "◉",
      group: "Library",
      run: () => {
        onSelectPanel("library");
        queueMicrotask(() => {
          window.dispatchEvent(new CustomEvent("slab:open-beacon-cache"));
        });
      },
      keywords: "beacon cache embedding index inspector vector semantic search prune forget stale mixed model nomic mxbai chunks pdfs storage cleanup atlas",
    });
    out.push({
      id: "library:new-smart",
      title: "New smart collection…",
      subtitle: "Build a custom rule with the advanced builder (⇧⌘N)",
      icon: "✦",
      group: "Library",
      run: () => {
        onSelectPanel("library");
        queueMicrotask(() => {
          window.dispatchEvent(new CustomEvent("slab:open-smart-builder"));
        });
      },
      keywords: "smart collection new rule builder nested and or not advanced atlas",
    });
    // Hopper v3.22.0 — backfill ("Hopper Loop"). The palette entry
    // opens the Hopper panel so the user can pick a watch + click
    // "Test on this folder". The deep-link to a specific watch's
    // backfill flow is handled by the `slab:open-hopper-backfill`
    // event below + Cmd+Shift+B keyboard handler in App.svelte.
    out.push({
      id: "hopper:backfill",
      title: "Hopper: Backfill folder with rules",
      subtitle: "Apply your routing rules to PDFs already in a watched folder (⇧⌘H)",
      icon: "📂",
      group: "Hopper",
      run: () => {
        onSelectPanel("hopper");
        queueMicrotask(() => {
          window.dispatchEvent(new CustomEvent("slab:open-hopper-backfill"));
        });
      },
      keywords:
        "hopper backfill folder retroactive existing files apply rules batch bulk paralegal legal discovery sort route move",
    });
    // Stack v3.23.0 — visual redline diff of two PDFs. Three palette
    // entries because each one corresponds to a distinct user intent
    // (compare, run again, export).
    out.push({
      id: "stack:compare",
      title: "Stack: Compare two PDFs",
      subtitle: "Word-level redline + side-by-side visual diff (⇧⌘D)",
      icon: "≢",
      group: "Stack",
      run: () => onSelectPanel("diff"),
      keywords:
        "stack diff compare redline litera changes contracts legal markup additions deletions revisions",
    });
    out.push({
      id: "stack:export",
      title: "Stack: Export diff report (PDF)",
      subtitle: "Save the current diff as a shareable PDF report",
      icon: "📄",
      group: "Stack",
      run: () => {
        onSelectPanel("diff");
        queueMicrotask(() => {
          window.dispatchEvent(new CustomEvent("slab:stack-export-report"));
        });
      },
      keywords:
        "stack diff export report pdf save share redline markup paralegal legal",
    });
    out.push({
      id: "stack:redline",
      title: "Stack: Export shareable redline (PDF)",
      subtitle: "Bake the word-level redline into one PDF — recipients don't need Slab",
      icon: "🟢",
      group: "Stack",
      run: () => {
        onSelectPanel("diff");
        queueMicrotask(() => {
          window.dispatchEvent(new CustomEvent("slab:stack-export-redline"));
        });
      },
      keywords:
        "stack diff export redline pdf share legal litera compare baked markup recipients green strikethrough word level",
    });
    out.push({
      id: "stack:rerun",
      title: "Stack: Re-run last comparison",
      subtitle: "Diff the two PDFs you compared most recently",
      icon: "🔁",
      group: "Stack",
      run: () => {
        onSelectPanel("diff");
        queueMicrotask(() => {
          window.dispatchEvent(new CustomEvent("slab:stack-rerun"));
        });
      },
      keywords: "stack diff redo again same previous compare",
    });
    // Stack Pro v3.24.0 — three-way compare. The Litera-Compare killer.
    out.push({
      id: "stack:diff3",
      title: "Stack Pro: Three-way compare (base / mine / theirs)",
      subtitle: "Merge two divergent PDF revisions against a common ancestor (⇧⌘3)",
      icon: "⫲",
      group: "Stack",
      run: () => onSelectPanel("diff3"),
      keywords:
        "stack pro three way 3-way diff3 base mine theirs merge conflict litera compare legal contracts revision branch ancestor common",
    });
    // Bind v3.18.0 — PDF → EPUB 3. The screenshot-bait wedge: Calibre is
    // the only mainstream PDF→EPUB tool and it's a 2008 GUI; Acrobat
    // doesn't ship EPUB at all.
    out.push({
      id: "bind:convert",
      title: "Convert PDF to EPUB",
      subtitle: "For Kindle, Apple Books, Kobo — offline, free",
      icon: "📖",
      group: "Convert",
      run: () => onSelectPanel("bind"),
      keywords:
        "bind epub ebook kindle apple books kobo calibre reflowable e-reader reader chapter convert export offline",
    });
    // Theater v2.3.0 — open the presenter-mode control panel. Shortcut
    // ⇧⌘T (⇧^T on win/linux). The detailed key cheat sheet lives inside
    // the panel itself once it's open.
    out.push({
      id: "theater:open",
      title: "Start Theater (presenter mode)",
      subtitle: "Turn the current PDF into slides — laser, ink, blackout (⇧⌘T)",
      icon: "🎬",
      group: "Theater",
      run: () => onSelectPanel("theater"),
      keywords:
        "theater presenter present slides projector audience laser pointer blackout whiteout ink annotate spotlight talk teach lecture",
    });
    // Theater Slice 5 — one-shot detach. Skips the panel and spawns the
    // audience + control windows immediately. Useful when the operator
    // already started a session and just wants the second display now.
    out.push({
      id: "theater:detach",
      title: "Open Theater audience window",
      subtitle:
        "Spawn the fullscreen audience + presenter control windows",
      icon: "🖥",
      group: "Theater",
      run: async () => {
        try {
          const { theaterOpenWindows } = await import("./theater");
          await theaterOpenWindows(null);
        } catch (e) {
          // eslint-disable-next-line no-console
          console.warn("[palette] theater detach failed", e);
        }
      },
      keywords:
        "theater detach audience second screen monitor projector window presenter control dual display",
    });
    // Glass Slice 7: jump straight to the customisation panel.
    out.push({
      id: "settings:keymap",
      title: "Customize keyboard shortcuts",
      subtitle: "Rebind any global action",
      icon: "⌨",
      group: "Settings",
      run: () => onSelectPanel("keymap"),
      keywords: "shortcuts keymap rebind customize keys hotkeys bindings",
    });
    // Foundry Slice 10: jump to the plugins manager.
    out.push({
      id: "settings:plugins",
      title: t("plugins.cmdOpen"),
      subtitle: t("plugins.subtitle"),
      icon: "🧩",
      group: "Settings",
      run: () => onSelectPanel("plugins"),
      keywords:
        "plugins extensions themes locales commands ai providers manifest foundry install uninstall enable disable reload",
    });
    // Glass II Slice 1: toggle Vim modal bindings
    out.push({
      id: "settings:toggle-vim",
      title: get(vimEnabled) ? "Disable Vim bindings" : "Enable Vim bindings",
      subtitle: "Modal keyboard navigation across Reader, Library, Beacon",
      icon: "⌨",
      group: "Settings",
      run: () => vimEnabled.set(!get(vimEnabled)),
      keywords: "vim bindings modal keyboard normal insert visual hjkl",
    });
    // Glass II Slice 5: switch UI language. One entry per locale so the
    // palette behaves like Settings → Language but with keyboard-first.
    // Each title is bilingual ("Español (Spanish)") for discoverability
    // regardless of the user's current language.
    for (const loc of LOCALES) {
      out.push({
        id: `settings:lang:${loc.id}`,
        title: `Switch language: ${loc.label}`,
        subtitle: t("settings.language.desc"),
        icon: "⌘",
        group: "Language",
        run: () => setLocale(loc.id),
        keywords: `language locale i18n translate ${loc.label} ${loc.id} english español spanish français french arabic عربية`,
      });
    }
    // Glass Slice 6: re-trigger the onboarding tour
    out.push({
      id: "help:onboarding",
      title: "Show onboarding tour",
      subtitle: "5-step walkthrough",
      icon: "🍰",
      group: "Help",
      run: () => {
        window.dispatchEvent(new CustomEvent("slab:show-onboarding"));
      },
      keywords: "onboarding tour welcome walkthrough tutorial intro first launch",
    });
    // Foundry Slice 9 — plugin-contributed commands. Alphabetised by
    // label so the order stays predictable across plugins.
    const pluginCmds = [...pluginsSnap.commands].sort((a, b) => a.label.localeCompare(b.label));
    for (const c of pluginCmds) {
      out.push({
        id: `plugin-cmd:${c.plugin_id}:${c.id}`,
        title: c.label,
        subtitle: `From plugin ${c.plugin_id}`,
        icon: c.url ? "↗" : "⌘",
        group: "Plugin commands",
        run: () => void dispatchPluginCommand(c.plugin_id, c.id, c.label),
        keywords: `plugin ${c.plugin_id} ${c.id} ${c.label} ${c.url ? "url link open" : "shell command"}`,
      });
    }
    // Foundry Slice 9 — informational surface for plugin AI providers.
    // Hook-up of materialised provider through Beacon's runtime is a
    // v1.3.x follow-up. For now: running the action copies the base
    // URL so users can paste it into curl / Settings, plus a discovery
    // toast.
    for (const p of pluginsSnap.aiProviders) {
      out.push({
        id: `plugin-ai:${p.plugin_id}:${p.id}`,
        title: `AI provider: ${p.label}`,
        subtitle: `${p.kind} · ${p.base_url}`,
        icon: "✦",
        group: "Plugin AI providers",
        run: async () => {
          try {
            await navigator.clipboard.writeText(p.base_url);
            notify.info(p.label, { detail: `${p.base_url} copied to clipboard` });
          } catch {
            notify.info(p.label, { detail: p.base_url });
          }
        },
        keywords: `plugin ai provider llm ${p.plugin_id} ${p.id} ${p.label} ${p.kind} ${p.base_url}`,
      });
    }
    // Glass Slice 5: MRU management
    if (Object.keys(mru).length > 0) {
      out.push({
        id: "settings:clear-mru",
        title: "Clear command history",
        subtitle: `${Object.keys(mru).length} remembered command${Object.keys(mru).length === 1 ? "" : "s"}`,
        icon: "↺",
        group: "Settings",
        run: () => {
          clearMru();
          mru = {};
        },
        keywords: "clear mru reset recent commands history forget",
      });
    }
    return out;
  });

  // Lumen Slice 1: scoring moved to the tested pure core in
  // `$lib/paletteSearch`. `scorePaletteEntry` ranks each action on the
  // higher of its (weighted) title and keyword scores and returns the
  // character ranges that matched the *title* — consumed by Slice 2's
  // live highlighting. Ranges for the current query are memoised here
  // so the render pass doesn't re-score.
  let titleRangeCache = new Map<string, PaletteRange[]>();

  // Lumen II Slice 2: typed scope sigils. A leading ">", "@", or "#"
  // narrows the list to commands / files / appearance (VSCode ⌘P style)
  // and the rest of the query becomes the search term. Parsed once here so
  // both the filter and the input pill read the same decomposition.
  let scopeParse = $derived(parsePaletteScope(query));

  let filtered = $derived.by(() => {
    titleRangeCache = new Map();
    const { scope, term } = scopeParse;
    // Apply the scope filter first; "all" passes everything through.
    const inScope =
      scope === "all" ? actions : actions.filter((a) => entryMatchesScope(a.group, scope));

    if (!term.trim()) {
      // Empty term (blank query, or a bare sigil): MRU floats to top, in
      // MRU order. Actions not in MRU keep their natural order after.
      const recent: Action[] = [];
      const rest: Action[] = [];
      for (const a of inScope) {
        if (a.id in mru) recent.push(a);
        else rest.push(a);
      }
      recent.sort((a, b) => mru[a.id] - mru[b.id]);
      // Cap the "Recent" pseudo-group to the 6 most recent, the rest fall back
      // into their natural group so the palette doesn't feel front-loaded.
      const top = recent.slice(0, 6);
      const overflow = recent.slice(6);
      return [...top, ...overflow, ...rest];
    }
    const q = term.trim();
    const scored = inScope
      .map((a: Action) => {
        const r = scorePaletteEntry(q, { title: a.title, keywords: a.keywords });
        if (r.score > 0) titleRangeCache.set(a.id, r.titleRanges);
        return { a, score: r.score };
      })
      .filter((x) => x.score > 0)
      .sort((a, b) => {
        // Primary: fuzzy score. Tie-breaker: MRU rank (lower = more recent).
        if (b.score !== a.score) return b.score - a.score;
        const ar = mru[a.a.id] ?? 9999;
        const br = mru[b.a.id] ?? 9999;
        return ar - br;
      });
    return scored.map((x) => x.a);
  });

  /** Title split into highlight segments for the current query. */
  function titleSegments(a: Action) {
    return splitHighlight(a.title, titleRangeCache.get(a.id) ?? []);
  }

  // Lumen Slice 5: the bound keyboard chord for a row (Raycast-style hint
  // that teaches the shortcut while you mouse). Resolves the palette
  // action id -> keymap action id -> pretty chord for the active
  // platform. `keymapTick` re-reads when the keymap store changes so a
  // rebind reflects live. Empty string when the row has no global chord.
  let keymapTick = $state(0);
  const unsubKeymap = keymapView.subscribe(() => keymapTick++);
  function rowChord(a: Action): string {
    void keymapTick; // establish reactive dependency
    const kid = paletteKeymapId(a.id);
    if (!kid) return "";
    return prettyBindingFor(kid as ActionId);
  }

  // Group preserving filtered order. When the term is empty AND there are
  // MRU entries, the first N items get pulled into a synthetic "Recently
  // used" group so the user sees their muscle-memory commands first. Uses
  // the scoped term so a bare ">" / "@" / "#" still shows the MRU header.
  let grouped = $derived.by(() => {
    const map = new Map<string, Action[]>();
    const showMruHeader = !scopeParse.term.trim() && Object.keys(mru).length > 0;
    let mruShown = 0;
    const mruCap = 6;
    for (const a of filtered) {
      if (showMruHeader && a.id in mru && mruShown < mruCap) {
        const key = "Recently used";
        if (!map.has(key)) map.set(key, []);
        map.get(key)!.push(a);
        mruShown++;
        continue;
      }
      if (!map.has(a.group)) map.set(a.group, []);
      map.get(a.group)!.push(a);
    }
    return Array.from(map.entries());
  });

  // Lumen III Slice 1: group collapse. In browse (empty-query) mode the
  // user can FOLD a section header so its rows tuck away; the set of
  // collapsed group names lives here and now PERSISTS across sessions
  // (v3.57.0 — seeded from localStorage on mount, written back on every
  // toggle) so a folded "Appearance" stays folded after a restart, exactly
  // like Raycast. Collapse is DISABLED while a query is active — folding a
  // group during search would hide matching results, which is a footgun —
  // so the effective set is empty then.
  let collapsedGroups = $state<Set<string>>(loadCollapsedGroups());
  const collapseActive = $derived(!scopeParse.term.trim());
  let collapsedView = $derived(
    partitionCollapsedGroups(
      grouped,
      collapseActive ? collapsedGroups : new Set<string>(),
    ),
  );
  /** The flat list the keyboard cursor walks — items from open groups only,
      so arrows never land on a folded (hidden) row. */
  let visibleList = $derived(collapsedView.visible);

  function toggleGroup(group: string): void {
    collapsedGroups = toggleCollapsedGroup(collapsedGroups, group);
    // Persist the new fold set so it survives a restart (best-effort).
    saveCollapsedGroups(collapsedGroups);
    // Re-seed the cursor to the top so it can't strand on a now-hidden row
    // (the clamp effect would also catch it, but this keeps it predictable).
    selected = 0;
  }

  // Round 51 Slice 2: Alt-click a header to SOLO-expand it — fold every
  // other section so one group fills the surface (the inverse of
  // collapse-all). Re-Alt-clicking an already-solo group pops everything
  // back open. soloExpandGroup owns the symmetric toggle; the component
  // just persists + re-seeds the cursor, exactly like toggleGroup.
  function soloGroup(group: string): void {
    collapsedGroups = soloExpandGroup(grouped, collapsedGroups, group);
    saveCollapsedGroups(collapsedGroups);
    selected = 0;
  }

  // Lumen III Slice 2: collapse-all / expand-all. With per-group fold +
  // cross-session persistence in place, a power user wants to clear the
  // whole browse surface to its headers in one keystroke (Cmd/Ctrl+E) or
  // a header-bar toggle, then drill back. `allCollapsed` tracks whether
  // every CURRENT group is folded so the control flips between "Collapse
  // all" and "Expand all". Only meaningful in browse mode (collapse is
  // disabled during search), so it reads the same grouped list collapse
  // applies to.
  const allCollapsed = $derived(
    collapseActive && isEveryGroupCollapsed(grouped, collapsedGroups),
  );

  // Lumen III Slice 3: legible bulk-collapse state in the footer. With
  // collapse-all in place, a power user wants to know at a glance how much
  // of the surface is folded — describeCollapseState turns the grouped list
  // + fold set into "N of M sections open" (or "All M collapsed"). Only
  // meaningful in browse mode; "" when nothing is folded so the footer
  // falls back to its result count.
  const collapseState = $derived(
    collapseActive
      ? describeCollapseState(grouped, collapsedGroups)
      : describeCollapseState([], new Set<string>()),
  );

  function toggleAllGroups(): void {
    // Fold everything, or — when already all-folded — clear the set open.
    collapsedGroups = allCollapsed ? new Set<string>() : collapseAllGroups(grouped);
    saveCollapsedGroups(collapsedGroups);
    selected = 0;
  }

  // Lumen II Slice 3: flat start index of each rendered group, so
  // Cmd/Ctrl+Arrow can leap the cursor between section heads. With
  // collapse applied, the heads come straight from the visible-list
  // partition so a folded group contributes no (unreachable) head.
  let groupStarts = $derived(collapsedView.starts);

  // Lumen II Slice 5: context-aware footer. The count pulses with the live
  // result list, and the Enter hint's verb tracks the selected row ("Open"
  // a file, "Switch to" a panel, "Apply" a theme, "Run" a command) so the
  // user sees what Return will do before committing.
  let resultCountLabel = $derived(describePaletteCount(filtered.length));
  let enterVerb = $derived(paletteActionVerb(visibleList[selected] ?? null));

  // Clamp selection when filter shrinks list (selected indexes the
  // visible/cursor list, which collapse can shrink below `filtered`).
  $effect(() => {
    if (selected >= visibleList.length) selected = Math.max(0, visibleList.length - 1);
  });

  // Lumen II Slice 1: empty-state fallback. When the live filter comes back
  // empty for a non-blank query, offer either a typo-corrected "did you
  // mean" (the closest shorter prefix that matches) or, failing that, a
  // curated set of starter commands — so the palette is never a dead end.
  // The actual rows are resolved back to live Action objects (by id) so
  // Enter/click runs the real handler. Computed only when filtered is empty.
  let fallback = $derived.by<PaletteFallback>(() => {
    if (filtered.length > 0 || !scopeParse.term.trim()) {
      return { kind: "none", relaxed: "", ids: [] };
    }
    // Search within the active scope so a scoped miss suggests scoped rows.
    const inScope =
      scopeParse.scope === "all"
        ? actions
        : actions.filter((a) => entryMatchesScope(a.group, scopeParse.scope));
    return suggestPaletteFallback(
      scopeParse.term,
      inScope.map((a) => ({ id: a.id, title: a.title, keywords: a.keywords })),
    );
  });

  /** Resolve fallback ids back to live Action objects, dropping any miss. */
  let fallbackActions = $derived.by<Action[]>(() => {
    if (fallback.kind === "none") return [];
    const byId = new Map(actions.map((a) => [a.id, a] as const));
    const out: Action[] = [];
    for (const id of fallback.ids) {
      const a = byId.get(id);
      if (a) out.push(a);
    }
    return out;
  });

  /** Run a fallback row directly (records MRU + closes, like runSelected). */
  function runFallback(a: Action) {
    if (a.id !== "settings:clear-mru") recordMru(a.id);
    onClose();
    queueMicrotask(() => a.run());
  }

  // True when the most recent activation (Enter or click) held Cmd/Ctrl.
  // Recent-file palette entries read this to decide between reuse-active-tab
  // (default) and open-in-new-tab (Cmd/Ctrl held). Cleared on every run.
  let lastActivationNewTab = false;

  function runSelected() {
    const a = visibleList[selected];
    if (!a) return;
    // Record into MRU before running so the next palette open shows it on top.
    // Skip the "clear MRU" action itself so it doesn't become its own bait.
    if (a.id !== "settings:clear-mru") recordMru(a.id);
    onClose();
    queueMicrotask(() => a.run());
  }

  function onKey(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === "Escape") {
      e.preventDefault();
      onClose();
      return;
    }
    if (e.key === "Enter") {
      e.preventDefault();
      lastActivationNewTab = e.metaKey || e.ctrlKey;
      runSelected();
      return;
    }
    // Lumen II Slice 3: Cmd/Ctrl+Arrow leaps the cursor between section
    // heads (Linear/Finder-style group jump) over the big action catalog.
    // Checked BEFORE the modifier bail so the chord isn't swallowed.
    const groupIntent = classifyPaletteGroupNav(e);
    if (groupIntent) {
      e.preventDefault();
      selected = nextGroupIndex(groupStarts, selected, groupIntent, visibleList.length);
      scrollSelectedIntoView();
      return;
    }
    // Lumen III Slice 2: Cmd/Ctrl+E folds every section to its header, or
    // — when already all-folded — expands them all. Only in browse mode
    // (collapse is disabled during search). Checked before the modifier
    // bail so the chord isn't swallowed.
    if ((e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey && (e.key === "e" || e.key === "E")) {
      if (collapseActive) {
        e.preventDefault();
        toggleAllGroups();
        scrollSelectedIntoView();
        return;
      }
    }
    // Lumen Slice 3: Raycast-grade list movement. Arrows wrap at the
    // ends, Home/End leap to either extreme, PageUp/PageDown page through
    // a long list — all resolved by the tested pure core. Modifiers fall
    // through (Cmd+ArrowDown etc. stays available for future chords).
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    const intent = classifyPaletteNav(e);
    if (intent) {
      e.preventDefault();
      selected = nextPaletteIndex(intent, selected, visibleList.length);
      scrollSelectedIntoView();
    }
  }

  // Keep the active row visible as the cursor moves (wrap to top/bottom,
  // paging, Home/End can all push it out of the scroll viewport).
  function scrollSelectedIntoView() {
    queueMicrotask(() => {
      const el = listEl?.querySelector<HTMLElement>(".palette-item.active");
      el?.scrollIntoView({ block: "nearest" });
    });
  }

  onMount(() => {
    window.addEventListener("keydown", onKey);
  });
  onDestroy(() => {
    window.removeEventListener("keydown", onKey);
    unsubPlugins();
    unsubKeymap();
  });
</script>

{#if open}
  <div class="palette-scrim" onclick={onClose} role="presentation"></div>
  <div class="palette" role="dialog" aria-modal="true" aria-label="Command palette">
    <div class="palette-input-row">
      <span class="palette-kbd-leading">⌘K</span>
      {#if scopeParse.scope !== "all"}
        <span class="palette-scope-pill" aria-label={`Scoped to ${describePaletteScope(scopeParse.scope)}`}>
          {describePaletteScope(scopeParse.scope)}
        </span>
      {/if}
      <input
        bind:this={inputEl}
        bind:value={query}
        placeholder={scopeParse.scope === "all" ? "Jump to anything…  (> commands, @ files, # appearance)" : "Filter…"}
        aria-label="Command palette search"
        autocomplete="off"
        spellcheck="false"
      />
      <button class="palette-close" onclick={onClose} title="Close (Esc)">esc</button>
    </div>
    <div class="palette-list" bind:this={listEl}>
      {#if filtered.length === 0}
        {#if fallbackActions.length > 0}
          <div class="palette-fallback-note">
            {#if fallback.kind === "typo"}
              No matches for “{scopeParse.term}”. Showing results for
              <span class="palette-fallback-relax">{fallback.relaxed}</span>
            {:else}
              No matches for “{scopeParse.term}”. Try one of these
            {/if}
          </div>
          <div class="palette-group-label">
            {fallback.kind === "typo" ? "Did you mean" : "Suggested"}
          </div>
          {#each fallbackActions as a (a.id)}
            <button
              class="palette-item"
              onclick={(e: MouseEvent) => {
                lastActivationNewTab = e.metaKey || e.ctrlKey;
                runFallback(a);
              }}
            >
              <span class="palette-icon">{a.icon}</span>
              <span class="palette-text">
                <span class="palette-title">{a.title}</span>
                {#if a.subtitle}<span class="palette-subtitle">{a.subtitle}</span>{/if}
              </span>
            </button>
          {/each}
        {:else}
          <div class="palette-empty">No matches for “{scopeParse.term}”</div>
        {/if}
      {:else}
        {#each collapsedView.display as section (section.group)}
          <button
            type="button"
            class="palette-group-label palette-group-toggle"
            class:collapsed={section.collapsed}
            aria-expanded={!section.collapsed}
            disabled={!collapseActive}
            onclick={(e: MouseEvent) => (e.altKey ? soloGroup(section.group) : toggleGroup(section.group))}
            title={collapseActive
              ? section.collapsed
                ? `Expand ${section.group} (Alt-click: show only this)`
                : `Collapse ${section.group} (Alt-click: show only this)`
              : section.group}
          >
            {#if collapseActive}
              <span class="palette-group-chevron" aria-hidden="true">{section.collapsed ? "▸" : "▾"}</span>
            {/if}
            <span class="palette-group-name">{section.group}</span>
            <span class="palette-group-count" aria-hidden="true">{section.count}</span>
          </button>
          {#each section.items as a (a.id)}
            {@const idx = visibleList.indexOf(a)}
            {@const chord = rowChord(a)}
            <button
              class="palette-item"
              class:active={idx === selected}
              onmouseenter={() => (selected = idx)}
              onclick={(e: MouseEvent) => {
                lastActivationNewTab = e.metaKey || e.ctrlKey;
                runSelected();
              }}
            >
              <span class="palette-icon">{a.icon}</span>
              <span class="palette-text">
                <span class="palette-title">{#each titleSegments(a) as seg}{#if seg.hit}<mark class="palette-hl">{seg.text}</mark>{:else}{seg.text}{/if}{/each}</span>
                {#if a.subtitle}<span class="palette-subtitle">{a.subtitle}</span>{/if}
              </span>
              {#if a.progress}
                <span
                  class="palette-progress"
                  class:finished={a.progress.finished}
                  aria-label={a.progress.finished ? "Finished reading" : `Read ${a.progress.percent} percent, page ${a.progress.page} of ${a.progress.total}`}
                >
                  {#if !a.progress.finished}
                    <span class="palette-progress-track">
                      <span class="palette-progress-fill" style={`width:${a.progress.percent}%`}></span>
                    </span>
                  {/if}
                  <span class="palette-progress-label">{a.progress.label}</span>
                </span>
              {/if}
              {#if chord}<span class="palette-chord" aria-label={`Shortcut ${chord}`}>{chord}</span>{/if}
              {#if idx === selected}<span class="palette-enter">↵</span>{/if}
            </button>
          {/each}
        {/each}
      {/if}
    </div>
    <div class="palette-footer">
      {#if collapseActive && collapsedView.display.length > 1}
        <span class="palette-foldall-group">
          <button
            type="button"
            class="palette-foldall"
            onclick={() => {
              toggleAllGroups();
              scrollSelectedIntoView();
            }}
            title={allCollapsed ? "Expand all sections (⌘E)" : "Collapse all sections (⌘E)"}
            aria-label={allCollapsed ? "Expand all sections" : "Collapse all sections"}
          >
            <span class="palette-foldall-glyph" aria-hidden="true">{allCollapsed ? "▸" : "▾"}</span>
            {allCollapsed ? "Expand all" : "Collapse all"}
          </button>
          {#if collapseState.label}
            <span class="palette-foldall-count" aria-live="polite">{collapseState.label}</span>
          {/if}
        </span>
      {:else}
        <span class="palette-footer-count">{resultCountLabel}</span>
      {/if}
      <span class="palette-footer-keys">
        <span><kbd>↑</kbd><kbd>↓</kbd> navigate</span>
        <span><kbd>⌘</kbd><kbd>↑</kbd><kbd>↓</kbd> section</span>
        {#if collapseActive}<span><kbd>⌘</kbd><kbd>E</kbd> fold all</span>{:else}<span><kbd>⇞</kbd><kbd>⇟</kbd> page</span>{/if}
        <span><kbd>↵</kbd> {enterVerb || "select"}</span>
        <span><kbd>esc</kbd> close</span>
      </span>
    </div>
  </div>
{/if}

<style>
  .palette-scrim {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.4);
    backdrop-filter: blur(4px);
    z-index: 90;
  }
  .palette {
    position: fixed;
    top: 14vh;
    left: 50%;
    transform: translateX(-50%);
    width: min(620px, 92vw);
    max-height: 70vh;
    z-index: 100;
    background: var(--bg-2);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    box-shadow: 0 30px 80px rgba(0, 0, 0, 0.45);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .palette-input-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
  }
  .palette-input-row input {
    flex: 1;
    background: transparent;
    border: 0;
    outline: 0;
    color: var(--text);
    font-size: 15px;
    padding: 4px 0;
  }
  .palette-kbd-leading {
    font-size: 10px;
    text-transform: uppercase;
    color: var(--text-3);
    background: var(--bg-3);
    border: 1px solid var(--border);
    padding: 3px 6px;
    border-radius: 4px;
    letter-spacing: 0.5px;
  }
  /* Lumen II Slice 2: typed-scope pill. When a leading sigil scopes the
     search (> commands, @ files, # appearance) this accent chip replaces
     the implicit "all" so the active class is unmistakable. */
  .palette-scope-pill {
    font-size: 11px;
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent) 40%, var(--border));
    padding: 3px 8px;
    border-radius: 4px;
    letter-spacing: 0.3px;
    white-space: nowrap;
    font-weight: 600;
  }
  .palette-close {
    background: var(--bg-3);
    border: 1px solid var(--border);
    color: var(--text-3);
    border-radius: 4px;
    padding: 3px 7px;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .palette-list {
    flex: 1;
    overflow-y: auto;
    padding: 6px 4px;
  }
  .palette-empty {
    padding: 20px;
    text-align: center;
    color: var(--text-3);
    font-size: 13px;
  }
  /* Lumen II Slice 1: empty-state fallback ("did you mean" / suggested).
     The note reads as a gentle recovery line above the offered rows; the
     relaxed query is tinted with the accent so the correction is legible. */
  .palette-fallback-note {
    padding: 14px 14px 6px;
    color: var(--text-3);
    font-size: 12px;
    line-height: 1.45;
  }
  .palette-fallback-relax {
    color: var(--accent);
    font-weight: 600;
  }
  .palette-group-label {
    padding: 8px 12px 4px;
    font-size: 10px;
    text-transform: uppercase;
    color: var(--text-3);
    letter-spacing: 0.6px;
  }
  /* Lumen III: the group label is now a collapse toggle (a real <button>
     so it's keyboard + screen-reader reachable). Reset button chrome and
     lay out chevron / name / count in a row. */
  .palette-group-toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    background: transparent;
    border: 0;
    cursor: pointer;
    text-align: left;
    font: inherit;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: var(--text-3);
  }
  .palette-group-toggle:disabled {
    cursor: default;
  }
  .palette-group-toggle:not(:disabled):hover {
    color: var(--text-2);
  }
  .palette-group-chevron {
    display: inline-flex;
    width: 10px;
    justify-content: center;
    font-size: 9px;
    opacity: 0.7;
    transition: transform 120ms ease;
  }
  .palette-group-name {
    flex: 1;
  }
  .palette-group-count {
    font-variant-numeric: tabular-nums;
    font-size: 9px;
    padding: 0 5px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--text-3) 22%, transparent);
    color: var(--text-3);
  }
  .palette-group-toggle.collapsed .palette-group-name {
    opacity: 0.75;
  }
  .palette-item {
    width: 100%;
    background: transparent;
    border: 0;
    color: var(--text-2);
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 12px;
    border-radius: var(--r-sm);
    text-align: left;
    cursor: pointer;
  }
  .palette-item.active {
    background: var(--bg-3);
    color: var(--text);
  }
  .palette-icon {
    width: 20px;
    text-align: center;
    color: var(--accent);
    font-size: 14px;
  }
  .palette-text {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }
  .palette-title {
    font-size: 13px;
    color: inherit;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* Lumen Slice 2: live query highlight on the matched title chars.
     Reset the browser's yellow <mark> default and tint with the accent
     so the matched substring reads as "this is why it ranked". */
  .palette-hl {
    background: color-mix(in srgb, var(--accent) 22%, transparent);
    color: var(--accent);
    border-radius: 2px;
    padding: 0 0.5px;
    font-weight: 600;
  }
  .palette-item.active .palette-hl {
    background: color-mix(in srgb, var(--accent) 32%, transparent);
  }
  .palette-subtitle {
    font-size: 11px;
    color: var(--text-3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .palette-enter {
    font-size: 12px;
    color: var(--accent);
  }
  /* Lumen II Slice 4: recent-file reading-progress chip. A thin accent
     track + percent label so a recent PDF reads as "continue at p.12/80".
     Finished docs drop the bar and show a muted "Finished" pill. */
  .palette-progress {
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .palette-progress-track {
    width: 42px;
    height: 4px;
    border-radius: 2px;
    background: var(--bg-3);
    overflow: hidden;
  }
  .palette-progress-fill {
    display: block;
    height: 100%;
    border-radius: 2px;
    background: var(--accent);
    min-width: 2px;
  }
  .palette-progress-label {
    font-size: 10px;
    font-variant-numeric: tabular-nums;
    color: var(--text-3);
    white-space: nowrap;
  }
  .palette-progress.finished .palette-progress-label {
    color: color-mix(in srgb, var(--accent) 70%, var(--text-3));
    font-weight: 600;
  }
  .palette-item.active .palette-progress-label {
    color: var(--text-2);
  }
  /* Lumen Slice 5: bound-shortcut hint on the right of a row. Monospace
     key-cap styling matching the footer kbd vocabulary; muted by default,
     brightening on the active row so it reads without shouting. */
  .palette-chord {
    flex-shrink: 0;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    color: var(--text-3);
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 2px 6px;
    letter-spacing: 0.5px;
    white-space: nowrap;
  }
  .palette-item.active .palette-chord {
    color: var(--text-2);
    border-color: color-mix(in srgb, var(--accent) 40%, var(--border));
  }
  .palette-footer {
    display: flex;
    gap: 14px;
    padding: 8px 12px;
    border-top: 1px solid var(--border);
    font-size: 10px;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    align-items: center;
  }
  /* Lumen II Slice 5: result count on the left, key hints on the right. */
  .palette-footer-count {
    flex-shrink: 0;
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }
  /* Lumen III Slice 2: collapse-all / expand-all toggle, replacing the
     count in browse mode. A quiet text button matching the footer scale. */
  .palette-foldall {
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    background: transparent;
    border: none;
    font: inherit;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-2);
    cursor: pointer;
    padding: 0;
    transition: color 80ms;
  }
  .palette-foldall:hover {
    color: var(--accent, #7c8cff);
  }
  .palette-foldall-glyph {
    font-size: 9px;
    line-height: 1;
  }
  /* Lumen III Slice 3: bulk-collapse legibility. The fold-all button + a
     live "N of M sections open" count sit together so the fold state is
     legible at a glance. */
  .palette-foldall-group {
    flex-shrink: 0;
    display: inline-flex;
    align-items: baseline;
    gap: 8px;
  }
  .palette-foldall-count {
    font-size: 10px;
    letter-spacing: 0.4px;
    color: var(--text-3, var(--text-2));
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .palette-footer-keys {
    display: flex;
    gap: 14px;
    margin-left: auto;
    align-items: center;
  }
  .palette-footer kbd {
    background: var(--bg-3);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 1px 4px;
    margin-right: 3px;
    font-family: inherit;
  }
</style>
