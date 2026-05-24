// `/try` is a fully client-side surface — no SSR, prerendered to a static
// shell that hydrates in the browser.  Bundling decisions live in
// `svelte.config.js`; this just toggles SSR off for the entire subtree.
export const prerender = true;
export const ssr = false;
