// Tiny helper: are we running inside a Tauri window, or vanilla browser?
// Used to gate native-only APIs (dialog, fs) and fall back to <input type="file">
// + downloadable blobs when running in `vite dev` for live development.
export function isInTauri(): boolean {
  return (
    typeof window !== "undefined" &&
    "__TAURI_INTERNALS__" in (window as unknown as Record<string, unknown>)
  );
}
