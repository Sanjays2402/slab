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
      out.push({
        id: `recent:${r.path}`,
        title: r.name,
        subtitle: `Open · ${formatRelTime(r.openedAt)}${r.pageCount ? ` · ${r.pageCount} pages` : ""}`,
        icon: "▥",
        group: "Recent files",
        run: () => onOpenRecent(r, { newTab: lastActivationNewTab }),
        keywords: `${r.name} ${r.path} pdf recent`,
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

  // Lightweight fuzzy match: every character of the query must appear in order
  // in the haystack. Score = (matched / haystack.length), with bonus for
  // prefix and contiguous matches.
  function fuzzyScore(q: string, hay: string): number {
    if (!q) return 1;
    const Q = q.toLowerCase();
    const H = hay.toLowerCase();
    if (H.startsWith(Q)) return 2 + 1 / H.length;
    if (H.includes(Q)) return 1.5 + 1 / H.length;
    let qi = 0;
    let lastIdx = -1;
    let contiguous = 0;
    let bestContiguous = 0;
    for (let hi = 0; hi < H.length && qi < Q.length; hi++) {
      if (H[hi] === Q[qi]) {
        if (hi === lastIdx + 1) contiguous++;
        else contiguous = 1;
        bestContiguous = Math.max(bestContiguous, contiguous);
        lastIdx = hi;
        qi++;
      }
    }
    if (qi < Q.length) return 0;
    return 1 + bestContiguous / Q.length + 0.1 / H.length;
  }

  let filtered = $derived.by(() => {
    if (!query.trim()) {
      // Empty-query view: MRU floats to top, in MRU order.
      // Actions not in MRU keep their natural order after.
      const recent: Action[] = [];
      const rest: Action[] = [];
      for (const a of actions) {
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
    const q = query.trim();
    const scored = actions
      .map((a) => ({ a, score: fuzzyScore(q, `${a.title} ${a.keywords ?? ""}`) }))
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

  // Group preserving filtered order. When query is empty AND there are MRU
  // entries, the first N items get pulled into a synthetic "Recently used"
  // group so the user sees their muscle-memory commands first.
  let grouped = $derived.by(() => {
    const map = new Map<string, Action[]>();
    const showMruHeader = !query.trim() && Object.keys(mru).length > 0;
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

  // Clamp selection when filter shrinks list
  $effect(() => {
    if (selected >= filtered.length) selected = Math.max(0, filtered.length - 1);
  });

  // True when the most recent activation (Enter or click) held Cmd/Ctrl.
  // Recent-file palette entries read this to decide between reuse-active-tab
  // (default) and open-in-new-tab (Cmd/Ctrl held). Cleared on every run.
  let lastActivationNewTab = false;

  function runSelected() {
    const a = filtered[selected];
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
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      selected = Math.min(filtered.length - 1, selected + 1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      selected = Math.max(0, selected - 1);
    } else if (e.key === "Enter") {
      e.preventDefault();
      lastActivationNewTab = e.metaKey || e.ctrlKey;
      runSelected();
    }
  }

  onMount(() => {
    window.addEventListener("keydown", onKey);
  });
  onDestroy(() => {
    window.removeEventListener("keydown", onKey);
    unsubPlugins();
  });
</script>

{#if open}
  <div class="palette-scrim" onclick={onClose} role="presentation"></div>
  <div class="palette" role="dialog" aria-modal="true" aria-label="Command palette">
    <div class="palette-input-row">
      <span class="palette-kbd-leading">⌘K</span>
      <input
        bind:this={inputEl}
        bind:value={query}
        placeholder="Jump to anything…"
        aria-label="Command palette search"
        autocomplete="off"
        spellcheck="false"
      />
      <button class="palette-close" onclick={onClose} title="Close (Esc)">esc</button>
    </div>
    <div class="palette-list">
      {#if filtered.length === 0}
        <div class="palette-empty">No matches for “{query}”</div>
      {:else}
        {#each grouped as [group, items] (group)}
          <div class="palette-group-label">{group}</div>
          {#each items as a (a.id)}
            {@const idx = filtered.indexOf(a)}
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
                <span class="palette-title">{a.title}</span>
                {#if a.subtitle}<span class="palette-subtitle">{a.subtitle}</span>{/if}
              </span>
              {#if idx === selected}<span class="palette-enter">↵</span>{/if}
            </button>
          {/each}
        {/each}
      {/if}
    </div>
    <div class="palette-footer">
      <span><kbd>↑</kbd><kbd>↓</kbd> navigate</span>
      <span><kbd>↵</kbd> select</span>
      <span><kbd>esc</kbd> close</span>
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
  .palette-group-label {
    padding: 8px 12px 4px;
    font-size: 10px;
    text-transform: uppercase;
    color: var(--text-3);
    letter-spacing: 0.6px;
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
  .palette-footer {
    display: flex;
    gap: 14px;
    padding: 8px 12px;
    border-top: 1px solid var(--border);
    font-size: 10px;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.5px;
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
