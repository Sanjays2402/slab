<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import ReaderPanel from "$lib/panels/ReaderPanel.svelte";
  import MergePanel from "$lib/panels/MergePanel.svelte";
  import SplitPanel from "$lib/panels/SplitPanel.svelte";
  import SplitPatternPanel from "$lib/panels/SplitPatternPanel.svelte";
  import PagesVisualPanel from "$lib/panels/PagesVisualPanel.svelte";
  import PagesListPanel from "$lib/panels/PagesListPanel.svelte";
  import EditTextPanel from "$lib/panels/EditTextPanel.svelte";
  import CompressPanel from "$lib/panels/CompressPanel.svelte";
  import ExtractPanel from "$lib/panels/ExtractPanel.svelte";
  import EncryptPanel from "$lib/panels/EncryptPanel.svelte";
  import WatermarkPanel from "$lib/panels/WatermarkPanel.svelte";
  import ConvertPanel from "$lib/panels/ConvertPanel.svelte";
  import MetadataPanel from "$lib/panels/MetadataPanel.svelte";
  import PageNumbersPanel from "$lib/panels/PageNumbersPanel.svelte";
  import SignPanel from "$lib/panels/SignPanel.svelte";
  import CropPanel from "$lib/panels/CropPanel.svelte";
  import InsertPanel from "$lib/panels/InsertPanel.svelte";
  import HeaderFooterPanel from "$lib/panels/HeaderFooterPanel.svelte";
  import RedactPanel from "$lib/panels/RedactPanel.svelte";
  import NupPanel from "$lib/panels/NupPanel.svelte";
  import MarkdownPanel from "$lib/panels/MarkdownPanel.svelte";
  import GrayscalePanel from "$lib/panels/GrayscalePanel.svelte";
  import PageLabelsPanel from "$lib/panels/PageLabelsPanel.svelte";
  import AutoRedactPanel from "$lib/panels/AutoRedactPanel.svelte";
  import FlattenPanel from "$lib/panels/FlattenPanel.svelte";
  import SanitizePanel from "$lib/panels/SanitizePanel.svelte";
  import RepairPanel from "$lib/panels/RepairPanel.svelte";
  import BeaconChatPanel from "$lib/panels/BeaconChatPanel.svelte";
  import BeaconSearchPanel from "$lib/panels/BeaconSearchPanel.svelte";
  import BeaconPiiPanel from "$lib/panels/BeaconPiiPanel.svelte";
  import LibraryPanel from "$lib/panels/LibraryPanel.svelte";
  import TablesPanel from "$lib/panels/TablesPanel.svelte";
  import DiffPanel from "$lib/panels/DiffPanel.svelte";
  import SlidesPanel from "$lib/panels/SlidesPanel.svelte";
  import SettingsPanel from "$lib/panels/SettingsPanel.svelte";
  import KeymapPanel from "$lib/panels/KeymapPanel.svelte";
  import CommandPalette from "$lib/CommandPalette.svelte";
  import ShortcutsOverlay from "$lib/ShortcutsOverlay.svelte";
  import { isInTauri } from "$lib/tauri";
  import { matches } from "$lib/keymap";
  import { basename } from "$lib/types";
  import type { RecentFile } from "$lib/recent";

  type Feature = {
    id: string;
    label: string;
    icon: string;
    ready: boolean;
  };

  const features: Feature[] = [
    { id: "reader", label: "Reader", icon: "▥", ready: true },
    { id: "library", label: "Library", icon: "❐", ready: true },
    { id: "beacon", label: "Beacon AI", icon: "✦", ready: true },
    { id: "search", label: "Beacon Search", icon: "⌕", ready: true },
    { id: "pii", label: "PII Redact", icon: "🔒", ready: true },
    { id: "merge", label: "Merge", icon: "⧉", ready: true },
    { id: "split", label: "Split", icon: "⎯", ready: true },
    { id: "split-chapter", label: "Split by Chapter", icon: "✂", ready: true },
    { id: "pages", label: "Pages", icon: "▦", ready: true },
    { id: "pages-list", label: "Pages (list)", icon: "≣", ready: true },
    { id: "edit-text", label: "Edit Text", icon: "✎", ready: true },
    { id: "compress", label: "Compress", icon: "▼", ready: true },
    { id: "extract", label: "Extract", icon: "❡", ready: true },
    { id: "encrypt", label: "Encrypt", icon: "▣", ready: true },
    { id: "watermark", label: "Watermark", icon: "○", ready: true },
    { id: "convert", label: "Convert", icon: "↔", ready: true },
    { id: "metadata", label: "Metadata", icon: "ⓘ", ready: true },
    { id: "numbers", label: "Numbers", icon: "№", ready: true },
    { id: "sign", label: "Sign", icon: "✍", ready: true },
    { id: "crop", label: "Crop", icon: "⊟", ready: true },
    { id: "insert", label: "Insert", icon: "＋", ready: true },
    { id: "headerfooter", label: "Header/Footer", icon: "≡", ready: true },
    { id: "redact", label: "Redact", icon: "▮", ready: true },
    { id: "autoredact", label: "Auto-Redact", icon: "⊘", ready: true },
    { id: "nup", label: "N-up", icon: "▦", ready: true },
    { id: "markdown", label: "Markdown → PDF", icon: "Ⓜ", ready: true },
    { id: "grayscale", label: "Grayscale", icon: "◐", ready: true },
    { id: "labels", label: "Page Labels", icon: "ⅰ", ready: true },
    { id: "flatten", label: "Flatten", icon: "▤", ready: true },
    { id: "sanitize", label: "Sanitize", icon: "⊗", ready: true },
    { id: "repair", label: "Repair", icon: "✚", ready: true },
    { id: "ocr", label: "OCR", icon: "👁", ready: true },
    { id: "tables", label: "Tables → CSV", icon: "⊞", ready: true },
    { id: "diff", label: "Diff", icon: "≢", ready: true },
    { id: "slides", label: "Slides", icon: "▷", ready: true },
    { id: "settings", label: "Settings", icon: "⚙", ready: true },
    { id: "keymap", label: "Shortcuts", icon: "⌨", ready: true },
  ];

  let active = $state("reader");
  let paletteOpen = $state(false);
  let shortcutsOpen = $state(false);

  // ---------- Reader tabs (Lathe Slice 5) ----------
  //
  // Each tab is its own ReaderPanel instance with its own pdfjs viewer, so we
  // can keep document state alive across tab switches. Inactive tabs are
  // kept in the DOM but hidden via `display: none` — switching is just
  // updating `activeTabId`. This is the same pattern the Reader panel uses
  // internally for the document <-> empty-state swap.
  type Tab = {
    id: string;
    initialPath: string | null;
    title: string;
  };
  let nextTabSeq = 0;
  function newTabId(): string {
    nextTabSeq += 1;
    return `tab-${Date.now().toString(36)}-${nextTabSeq}`;
  }
  const initialTab: Tab = { id: newTabId(), initialPath: null, title: "New tab" };
  let tabs = $state<Tab[]>([initialTab]);
  let activeTabId = $state(initialTab.id);

  function setActiveTab(id: string) {
    activeTabId = id;
    active = "reader";
  }

  function openNewTab(initialPath: string | null = null) {
    const tab: Tab = {
      id: newTabId(),
      initialPath,
      title: initialPath ? basename(initialPath) : "New tab",
    };
    tabs = [...tabs, tab];
    setActiveTab(tab.id);
  }

  function closeTab(id: string) {
    // Always keep at least one tab open. If the last tab is closed,
    // reset it to an empty "New tab" instead of disappearing.
    if (tabs.length === 1) {
      tabs = [{ id: newTabId(), initialPath: null, title: "New tab" }];
      activeTabId = tabs[0].id;
      return;
    }
    const idx = tabs.findIndex((t) => t.id === id);
    if (idx < 0) return;
    const next = tabs.filter((t) => t.id !== id);
    tabs = next;
    if (activeTabId === id) {
      // Activate the tab that took the closed one's slot, or the last one.
      activeTabId = next[Math.min(idx, next.length - 1)].id;
    }
  }

  /** Called by ReaderPanel after a successful load. Updates the tab label. */
  function onTabTitleChange(id: string, title: string) {
    tabs = tabs.map((t) => (t.id === id ? { ...t, title } : t));
  }

  async function pickAndOpenInNewTab() {
    if (!isInTauri()) {
      // In browser dev, just create an empty tab — the user can pick via the
      // dropzone inside the panel.
      openNewTab(null);
      return;
    }
    const picked = await open({
      multiple: false,
      filters: [
        { name: "PDF only", extensions: ["pdf"] },
        {
          name: "Documents (PDF, Office, HTML, EPUB, CSV, JSON, XML, RTF, ODT)",
          extensions: [
            "pdf", "docx", "pptx", "xlsx", "xls",
            "html", "htm", "epub",
            "csv", "json", "xml", "rtf", "odt",
            "png", "jpg", "jpeg", "gif", "bmp", "tif", "tiff", "webp",
            "wav", "mp3", "m4a", "flac", "ogg",
          ],
        },
      ],
    });
    if (typeof picked !== "string") return;
    openNewTab(picked);
  }

  // Pending recent-file open request — Reader panel reads this and reacts.
  // When triggered from the palette we route it to the *active* tab so the
  // user's current view changes in place. (To open a recent in a new tab
  // instead, hold Cmd while picking from the palette — TODO follow-up.)
  function requestOpenRecent(file: RecentFile) {
    active = "reader";
    queueMicrotask(() => {
      window.dispatchEvent(new CustomEvent("slab:open-recent", { detail: file }));
    });
  }

  function onGlobalKey(e: KeyboardEvent) {
    // Glass Slice 7: every shortcut now flows through the user-customisable
    // keymap. `matches()` resolves against ~/.slab/config.toml [keymap].
    if (matches(e, "palette.open")) {
      e.preventDefault();
      paletteOpen = !paletteOpen;
      return;
    }
    // "?" opens the shortcuts overlay (but not while typing in a field).
    if (matches(e, "shortcuts.show")) {
      const target = e.target as HTMLElement | null;
      const inField =
        target && (target.matches("input,textarea") || target.isContentEditable);
      if (!inField) {
        e.preventDefault();
        shortcutsOpen = !shortcutsOpen;
        return;
      }
    }
    // Tab shortcuts only fire when the Reader panel is the active feature.
    // Otherwise we'd hijack ⌘T for users wanting (e.g.) browser dev tools.
    if (active !== "reader") return;
    // Skip when typing in form fields so we don't steal ⌘T from inputs.
    const target = e.target as HTMLElement | null;
    if (target && (target.matches("input,textarea") || target.isContentEditable)) {
      // Still allow tabs.close to fire even when an input has focus — feels native.
      if (!matches(e, "tabs.close")) return;
    }
    if (matches(e, "tabs.new")) {
      e.preventDefault();
      void pickAndOpenInNewTab();
      return;
    }
    if (matches(e, "tabs.close")) {
      e.preventDefault();
      closeTab(activeTabId);
      return;
    }
    // tabs.goto1 … tabs.goto9. We use a small loop so users can rebind any
    // single one without us iterating in a fragile hardcoded order.
    const gotoIds = [
      "tabs.goto1", "tabs.goto2", "tabs.goto3",
      "tabs.goto4", "tabs.goto5", "tabs.goto6",
      "tabs.goto7", "tabs.goto8", "tabs.goto9",
    ] as const;
    for (let n = 1; n <= 9; n++) {
      if (matches(e, gotoIds[n - 1])) {
        if (n <= tabs.length) {
          e.preventDefault();
          setActiveTab(tabs[n - 1].id);
        }
        return;
      }
    }
    // Cycle through tabs. tabs.prev is checked first because Shift+Tab also
    // matches the modifier-less "Tab" pattern of tabs.next on some envs;
    // checking the more-specific binding first avoids that ambiguity.
    if (matches(e, "tabs.prev")) {
      e.preventDefault();
      const idx = tabs.findIndex((t) => t.id === activeTabId);
      if (idx < 0) return;
      const nextIdx = (idx - 1 + tabs.length) % tabs.length;
      setActiveTab(tabs[nextIdx].id);
      return;
    }
    if (matches(e, "tabs.next")) {
      e.preventDefault();
      const idx = tabs.findIndex((t) => t.id === activeTabId);
      if (idx < 0) return;
      const nextIdx = (idx + 1) % tabs.length;
      setActiveTab(tabs[nextIdx].id);
    }
  }

  // Middle-click on a tab closes it (browser convention).
  function onTabMouseDown(e: MouseEvent, id: string) {
    if (e.button === 1) {
      e.preventDefault();
      closeTab(id);
    }
  }

  // LibraryPanel dispatches this when the user clicks a card. We open
  // the doc in a fresh Reader tab and flip the active feature so they
  // see it immediately.
  function onLibraryOpen(e: Event) {
    const detail = (e as CustomEvent<{ path: string }>).detail;
    if (!detail || typeof detail.path !== "string") return;
    openNewTab(detail.path);
  }

  onMount(() => {
    window.addEventListener("keydown", onGlobalKey);
    window.addEventListener("slab:open-library-doc", onLibraryOpen as EventListener);
  });
  onDestroy(() => {
    window.removeEventListener("keydown", onGlobalKey);
    window.removeEventListener("slab:open-library-doc", onLibraryOpen as EventListener);
  });
</script>

<aside class="sidebar">
  <div class="brand">
    <span class="logo">▤</span>
    <span class="brand-name">Slab</span>
    <span class="brand-tag">local · offline · free</span>
  </div>

  <nav>
    {#each features as f (f.id)}
      <button
        class="nav-item"
        class:active={active === f.id}
        class:locked={!f.ready}
        disabled={!f.ready}
        onclick={() => (active = f.id)}
      >
        <span class="nav-icon">{f.icon}</span>
        <span class="nav-label">{f.label}</span>
        {#if !f.ready}<span class="badge">soon</span>{/if}
      </button>
    {/each}
  </nav>

  <button class="palette-trigger" onclick={() => (paletteOpen = true)} title="Command palette">
    <span class="pt-icon">⌘</span>
    <span class="pt-label">Jump to anything</span>
    <span class="pt-kbd">⌘K</span>
  </button>

  <div class="footer">
    <span class="version">v1.0.0</span>
  </div>
</aside>

<main class="content">
  {#if active === "reader"}
    <!-- Tab strip — only shown for the Reader feature. -->
    <div class="tabstrip" role="tablist" aria-label="Open PDFs">
      {#each tabs as t (t.id)}
        <div
          class="tab"
          class:active={t.id === activeTabId}
          role="presentation"
          onmousedown={(e) => onTabMouseDown(e, t.id)}
          title={t.title}
        >
          <button
            class="tab-label"
            role="tab"
            tabindex={t.id === activeTabId ? 0 : -1}
            aria-selected={t.id === activeTabId}
            onclick={() => setActiveTab(t.id)}
          >
            <span class="tab-icon">▥</span>
            <span class="tab-title">{t.title}</span>
          </button>
          <button
            class="tab-close"
            aria-label="Close tab"
            title="Close (⌘W)"
            onclick={(e) => {
              e.stopPropagation();
              closeTab(t.id);
            }}
          >×</button>
        </div>
      {/each}
      <button
        class="tab-new"
        title="New tab (⌘T)"
        aria-label="Open a PDF in a new tab"
        onclick={pickAndOpenInNewTab}
      >+</button>
    </div>

    <!-- Render every tab; only the active one is visible. Keeping the
         hidden tabs mounted preserves their pdfjs viewer state (current
         page, zoom, find state, outline) across switches. -->
    <div class="reader-stack">
      {#each tabs as t (t.id)}
        <div class="reader-slot" class:active={t.id === activeTabId}>
          <ReaderPanel
            tabId={t.id}
            active={t.id === activeTabId}
            initialPath={t.initialPath}
            onTitleChange={(title) => onTabTitleChange(t.id, title)}
          />
        </div>
      {/each}
    </div>
  {:else if active === "beacon"}
    <BeaconChatPanel />
  {:else if active === "library"}
    <LibraryPanel />
  {:else if active === "search"}
    <BeaconSearchPanel />
  {:else if active === "pii"}
    <BeaconPiiPanel />
  {:else if active === "merge"}
    <MergePanel />
  {:else if active === "split"}
    <SplitPanel />
  {:else if active === "split-chapter"}
    <SplitPatternPanel />
  {:else if active === "pages"}
    <PagesVisualPanel />
  {:else if active === "pages-list"}
    <PagesListPanel />
  {:else if active === "edit-text"}
    <EditTextPanel />
  {:else if active === "compress"}
    <CompressPanel />
  {:else if active === "extract"}
    <ExtractPanel />
  {:else if active === "encrypt"}
    <EncryptPanel />
  {:else if active === "watermark"}
    <WatermarkPanel />
  {:else if active === "convert"}
    <ConvertPanel />
  {:else if active === "metadata"}
    <MetadataPanel />
  {:else if active === "numbers"}
    <PageNumbersPanel />
  {:else if active === "sign"}
    <SignPanel />
  {:else if active === "crop"}
    <CropPanel />
  {:else if active === "insert"}
    <InsertPanel />
  {:else if active === "headerfooter"}
    <HeaderFooterPanel />
  {:else if active === "redact"}
    <RedactPanel />
  {:else if active === "autoredact"}
    <AutoRedactPanel />
  {:else if active === "nup"}
    <NupPanel />
  {:else if active === "markdown"}
    <MarkdownPanel />
  {:else if active === "grayscale"}
    <GrayscalePanel />
  {:else if active === "labels"}
    <PageLabelsPanel />
  {:else if active === "flatten"}
    <FlattenPanel />
  {:else if active === "sanitize"}
    <SanitizePanel />
  {:else if active === "repair"}
    <RepairPanel />
  {:else if active === "tables"}
    <TablesPanel />
  {:else if active === "diff"}
    <DiffPanel />
  {:else if active === "slides"}
    <SlidesPanel />
  {:else if active === "settings"}
    <SettingsPanel />
  {:else if active === "keymap"}
    <KeymapPanel />
  {/if}
</main>

<CommandPalette
  bind:open={paletteOpen}
  panels={features}
  activePanel={active}
  onClose={() => (paletteOpen = false)}
  onSelectPanel={(id) => {
    active = id;
    paletteOpen = false;
  }}
  onOpenRecent={(file) => {
    paletteOpen = false;
    requestOpenRecent(file);
  }}
  onShowShortcuts={() => {
    paletteOpen = false;
    shortcutsOpen = true;
  }}
/>

<ShortcutsOverlay bind:open={shortcutsOpen} onClose={() => (shortcutsOpen = false)} />

<style>
  .sidebar {
    width: var(--sidebar-w);
    background: var(--bg-2);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    padding: 14px 10px;
    flex-shrink: 0;
  }

  .brand {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 4px 8px 18px;
  }
  .logo {
    color: var(--accent);
    font-size: 18px;
  }
  .brand-name {
    font-weight: 700;
    font-size: 15px;
    letter-spacing: 0.2px;
  }
  .brand-tag {
    font-size: 10px;
    text-transform: uppercase;
    color: var(--text-3);
    letter-spacing: 0.5px;
  }

  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    overflow-y: auto;
  }

  .nav-item {
    display: flex;
    align-items: center;
    gap: 10px;
    text-align: left;
    background: transparent;
    border: 1px solid transparent;
    color: var(--text-2);
    padding: 7px 10px;
    border-radius: var(--r-sm);
    font-size: 13px;
  }
  .nav-item:hover:not(:disabled) {
    background: var(--bg-3);
    color: var(--text);
  }
  .nav-item.active {
    background: var(--bg-3);
    color: var(--text);
    border-color: var(--border);
  }
  .nav-item.locked {
    opacity: 0.55;
  }
  .nav-icon {
    width: 18px;
    text-align: center;
    color: var(--accent);
    opacity: 0.9;
  }
  .nav-label {
    flex: 1;
  }
  .badge {
    font-size: 9px;
    text-transform: uppercase;
    color: var(--text-3);
    background: var(--bg);
    padding: 2px 5px;
    border-radius: 4px;
    letter-spacing: 0.5px;
  }

  .palette-trigger {
    display: flex;
    align-items: center;
    gap: 8px;
    background: var(--bg);
    color: var(--text-3);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 7px 10px;
    font-size: 12px;
    margin: 10px 0 8px;
  }
  .palette-trigger:hover {
    color: var(--text);
    background: var(--bg-3);
  }
  .pt-icon {
    color: var(--accent);
  }
  .pt-label {
    flex: 1;
    text-align: left;
  }
  .pt-kbd {
    font-size: 10px;
    background: var(--bg-2);
    border: 1px solid var(--border);
    padding: 1px 5px;
    border-radius: 3px;
    letter-spacing: 0.5px;
  }

  .footer {
    padding: 8px 10px;
    border-top: 1px solid var(--border);
    font-size: 11px;
    color: var(--text-3);
  }

  .content {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow-y: hidden;
    padding: 28px 36px 36px;
    min-height: 0;
  }

  /* ---------- Tab strip (Reader only) ---------- */
  .tabstrip {
    display: flex;
    align-items: stretch;
    gap: 2px;
    margin: -8px -8px 12px;
    padding: 0 2px;
    border-bottom: 1px solid var(--border);
    overflow-x: auto;
    flex-shrink: 0;
    scrollbar-width: thin;
  }
  .tab {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: transparent;
    border: 1px solid transparent;
    border-bottom: none;
    border-top-left-radius: 6px;
    border-top-right-radius: 6px;
    margin-bottom: -1px;
    max-width: 240px;
    min-width: 120px;
    position: relative;
  }
  .tab:hover {
    background: var(--bg-3);
  }
  .tab.active {
    background: var(--bg);
    border-color: var(--border);
    z-index: 1;
  }
  .tab-label {
    flex: 1;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    background: transparent;
    border: none;
    padding: 6px 4px 7px 10px;
    font-size: 12px;
    color: var(--text-2);
    cursor: pointer;
    min-width: 0;
  }
  .tab:hover .tab-label,
  .tab.active .tab-label {
    color: var(--text);
  }
  .tab-icon {
    color: var(--accent);
    opacity: 0.9;
    font-size: 11px;
    flex-shrink: 0;
  }
  .tab-title {
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    text-align: left;
  }
  .tab-close {
    background: transparent;
    border: none;
    color: var(--text-3);
    font-size: 14px;
    line-height: 1;
    padding: 2px 6px;
    margin-right: 4px;
    border-radius: 3px;
    cursor: pointer;
    flex-shrink: 0;
  }
  .tab-close:hover {
    background: var(--bg-3);
    color: var(--text);
  }
  .tab.active .tab-close:hover {
    background: var(--bg-2);
  }
  .tab-new {
    background: transparent;
    border: none;
    color: var(--text-3);
    font-size: 16px;
    line-height: 1;
    padding: 0 10px;
    margin-left: 2px;
    cursor: pointer;
    align-self: center;
    border-radius: 4px;
  }
  .tab-new:hover {
    background: var(--bg-3);
    color: var(--text);
  }

  /* ---------- Reader stack ---------- */
  /* All tabs are mounted; only the active one is displayed. Keeps pdfjs
     viewer state alive across switches. */
  .reader-stack {
    flex: 1;
    min-height: 0;
    position: relative;
    display: flex;
    flex-direction: column;
  }
  .reader-slot {
    display: none;
    flex: 1;
    min-height: 0;
    flex-direction: column;
  }
  .reader-slot.active {
    display: flex;
  }
</style>
