<div align="center">

# fluxa-core

The platform-agnostic Rust core behind [Fluxa](https://github.com/KhooLy/Fluxa), a media-streaming app.<br/>
State management · Stream policy · Addon protocol · Effect-driven I/O

[![Stars](https://img.shields.io/github/stars/KhooLy/fluxa-core?style=flat-square&color=fff&labelColor=111)](https://github.com/KhooLy/fluxa-core/stargazers)
[![Issues](https://img.shields.io/github/issues/KhooLy/fluxa-core?style=flat-square&color=fff&labelColor=111)](https://github.com/KhooLy/fluxa-core/issues)
[![License](https://img.shields.io/github/license/KhooLy/fluxa-core?style=flat-square&color=fff&labelColor=111)](LICENSE)

[What it does](#what-it-does) · [Architecture](#architecture) · [Building from source](#building-from-source) · [Stack](#stack)

</div>

---

## What it does

`fluxa-core` holds all of Fluxa's domain logic — content discovery, stream selection,
playback state, profiles, library, calendar, and external sync with Trakt/Simkl — so the
same Rust codebase can run unmodified on Android, desktop, and (via WASM) the web. It
contains no platform-specific code and never performs I/O itself: it takes an action and
returns state plus a list of typed *effects*, and the host platform executes those
effects and reports results back.

```
Host  →  dispatch(action_json)
      ←  { state, effects: [{ id, type, payload }] }
Host  →  executes each effect (HTTP / storage / player / ...)
      →  completeEffect({ effectId, result })
      ←  { state, effects: [...] }
```

This repo also contains a companion crate, **`fluxa-streaming-engine/`**, which handles
the runtime streaming side: torrent download (via `librqbit`), local HTTP proxying, and
Dolby Vision / HDR10+ stream rewriting.

## Who uses this

| Platform | Repo | How it links |
| --- | --- | --- |
| Android (mobile + TV) | [Fluxa](https://github.com/KhooLy/Fluxa) | JNI (primary, ~157 functions) + a small UniFFI surface |
| Desktop (Linux/macOS/Windows) | [FluxaDesktop](https://github.com/KhooLy/FluxaDesktop) | Plain Rust dependency — calls `FluxaCore`/`core_invoke` directly, no FFI marshaling |
| iOS / tvOS | not in this workspace | UniFFI (`bindings/uniffi.rs`) |
| webOS | not in this workspace | WASM (`bindings/wasm.rs`, `wasm` feature) |

See [`docs/integrating.md`](docs/integrating.md) for how each platform actually wires
this crate in, including how to add a new capability for a given platform.

## Architecture

- **`headless_engine/`** — the primary state machine. State is a typed `EngineState`
  struct made of per-feature sub-structs (home, detail, player, library, search, ...);
  cross-module writes go through `pub(super)` setters, never raw field access.
- **`app_state.rs`** — a second, simpler engine for overlapping concerns, used by
  Android via UniFFI. The split is intentional, not duplication to be cleaned up.
- Three uncoordinated exposure mechanisms, one per platform's needs: `core_api::FluxaCore`
  (minimal, desktop-only), `ffi::core_invoke` (string-routed dispatcher, desktop + Swift),
  and `bindings/jni.rs` (Android, no equivalent elsewhere).

Full architecture notes, the effect catalog, and the wire-format reference live in
[`docs/`](docs/):

- [`docs/overview.md`](docs/overview.md) — architecture, state engines, module map
- [`docs/effect-loop.md`](docs/effect-loop.md) — the dispatch/effect/completeEffect cycle
- [`docs/effects.md`](docs/effects.md) — every `EffectKind` and its payload shape
- [`docs/integrating.md`](docs/integrating.md) — per-platform integration guide
- [`docs/building.md`](docs/building.md) — features, commands, cross-compilation

## Building from source

```bash
git clone https://github.com/KhooLy/fluxa-core.git
cd fluxa-core
cargo build                  # default (native) features — what Android uses
cargo test --lib             # ~190 tests, fast
```

**Prerequisites**

- Rust stable
- For Android cross-compilation: `rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android`

```bash
cargo check --no-default-features --features wasm   # sanity-check the webOS/WASM path
cd fluxa-streaming-engine && cargo build             # the companion crate builds independently
```

See [`docs/building.md`](docs/building.md) for the full feature matrix, UniFFI binding
generation, and release-build details.

## Repo layout

```
src/                    domain logic, headless_engine, FFI bindings
fluxa-streaming-engine/ torrent + Dolby Vision/HDR10+ stream rewriting (separate crate)
fuzz/                   cargo-fuzz targets for parsers (episode matching, manifests, percent-decode)
docs/                   architecture, effects reference, integration guide
```

## Stack

[Rust](https://www.rust-lang.org/) · [JNI](https://docs.rs/jni) · [UniFFI](https://mozilla.github.io/uniffi-rs/) · [wasm-bindgen](https://rustwasm.github.io/wasm-bindgen/) · [axum](https://github.com/tokio-rs/axum) · [tokio](https://tokio.rs/) · [librqbit](https://github.com/ikatson/rqbit) · [dolby_vision](https://github.com/quietvoid/dovi_tool) · [serde](https://serde.rs/)

---

**Legal** — fluxa-core is a domain-logic library for a client-side interface to user-installed Stremio addons. It does not host, serve, or distribute any media content, and never makes a network call itself — all I/O is performed by the host platform. Fluxa is not affiliated with any addon developer, repository, or content provider. Users are responsible for ensuring they have the right to access what they stream.

## Related projects

- [Fluxa for Android](https://github.com/KhooLy/Fluxa) — the Android counterpart consuming this crate
- [FluxaDesktop](https://github.com/KhooLy/FluxaDesktop) — the desktop counterpart consuming this crate
