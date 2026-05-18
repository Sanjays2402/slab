// See https://svelte.dev/docs/kit/types#app for SvelteKit's app types.
// This file extends the global typespace with build-time constants
// injected via Vite's `define` (see vite.config.js).

declare global {
  /** Slab application semver from package.json, injected at build time. */
  const __APP_VERSION__: string;

  namespace App {
    // interface Error {}
    // interface Locals {}
    // interface PageData {}
    // interface PageState {}
    // interface Platform {}
  }
}

export {};
