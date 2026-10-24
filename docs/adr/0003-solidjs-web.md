# 0003. SolidJS for the web client

- Status: Accepted
- Date: 2026-10-23

## Context
The web client is the primary UI (desktop, phone, TV browsers, admin). It needs small bundles (≤ 150 KB gz initial), smooth virtualized grids on weak devices, and mature video playback through MSE/hls.js. The options considered were SolidJS, Svelte, React, and Rust→WASM (Leptos/Dioxus).

## Decision
Use SolidJS + TypeScript (strict) with Vite, `@solidjs/router`, TanStack Query/Virtual, Kobalte (accessible headless components), `openapi-fetch` with types generated from the server's OpenAPI spec, and hls.js.

## Consequences
- Fine-grained reactivity gives small bundles and fast updates without a virtual DOM.
- The TS/MSE ecosystem (hls.js, JASSUB, Cast SDK) is directly usable.
- Rust→WASM was rejected for now: the ecosystem around video playback and the contributor pool are much larger in TS. This can be revisited later.
- Contributors need TypeScript skills in addition to Rust.
