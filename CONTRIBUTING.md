# Contributing to fluxa-core

Thanks for wanting to help. fluxa-core is the platform-agnostic Rust brain shared by
[Fluxa for Android](https://github.com/FluxaMedia/fluxa), [Fluxa Desktop](https://github.com/FluxaMedia/fluxa-desktop),
and the web/webOS targets. A change here can reach every platform at once, so this
guide is mostly about keeping that shared surface honest.

## The one rule that matters most

**fluxa-core never performs I/O.** No HTTP, no file system, no sockets, no clock reads
that aren't passed in. The crate decides *what* should happen and returns a typed
effect; the host platform (Kotlin, the Tauri Rust shell, the browser) actually does it
and hands the result back.

```text
host -> dispatch(action)
     -> core updates state, emits an effect
     -> host performs the network/storage/auth work
     -> host -> complete(effect result)
     -> core updates state
```

If your feature needs the network or disk, model it as an effect. A `reqwest` call, a
`std::fs` read, or a `SystemTime::now()` inside core is a bug, not a shortcut — it
breaks the WASM build and makes behaviour untestable and non-deterministic. This is the
first thing a review will check.

## Belongs here vs. belongs in a shell

| In fluxa-core | In a platform shell |
|---|---|
| State machines and transitions | The actual HTTP / Room / localStorage call |
| Addon manifest & resource parsing | Playing the video, drawing the UI |
| Stream discovery planning & policy | OAuth token exchange, secure storage |
| Ranking, scoring, personalization | Notifications, tray, window management |
| Library / watchlist / progress rules | Anything with a platform SDK in the type |
| Scrobble decisions, calendar detection | |
| Content identity & episode-locator parsing | |

When unsure, prefer core — but only if it stays pure. A decision that *requires* a
platform type to express is a sign it belongs in the shell.

## Getting set up

```bash
git clone https://github.com/FluxaMedia/fluxa-core.git
cd fluxa-core
cargo build          # default (native) features — what the shells consume
cargo test --lib     # fast, the suite you run constantly
```

You only need Rust stable. For the cross-compilation paths:

```bash
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
cargo check --no-default-features --features wasm   # the webOS / web path
cd fluxa-streaming-engine && cargo build            # the companion crate
```

See [`docs/building.md`](docs/building.md) for the full feature matrix and UniFFI
binding generation.

## Before you open a PR

Run these and make sure they pass cleanly:

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --lib
cargo check --no-default-features --features wasm
```

The workspace lints treat `unwrap`, `expect`, `panic`, and slice indexing as warnings on
purpose — a panic in core takes down whichever app is hosting it. Return `Option`/`Result`
and let the host decide what to do with a failure. If you genuinely need to index or
unwrap, prove it can't fail and keep it local.

For parser-facing changes (episode matching, manifests, percent-decode), the
[`fuzz/`](fuzz/) targets exist for a reason — run the relevant one and add a case if you
touched the grammar.

## Style

- **No comments in code.** Not inline, not doc comments, not block comments. If something
  needs explaining, it goes in the commit message or the PR description. Rename the thing
  or restructure it until it reads on its own.
- **English only**, everywhere developer-facing: identifiers, file names, commit messages,
  PR titles and descriptions, test names, logs.
- Match the surrounding code. This crate has a consistent voice — test names describe the
  scenario (`ciphered_tracks_resolve_before_mobile_quality_selection`), not the function.
- Keep changes focused. One logical change per commit; don't fold a refactor into a fix.

## Commits and pull requests

- Write a real commit message: what changed and *why*, in imperative mood. The "why" is
  the part that can't be recovered from the diff.
- Keep the tree green at every commit if you can — a bisect through a broken build helps
  no one.
- In the PR, say which platforms you considered. A core change that's correct for Android
  can still be wrong for the web build's constraints; call out what you checked.
- New behaviour needs a test. Bug fixes should come with the test that would have caught
  the bug.

## Reporting bugs

Open an issue with enough to reproduce: the action dispatched, the state before, and the
effect or state you expected versus what you got. Because core is deterministic, a good
report is usually a failing test in prose.

## Legal

fluxa-core is domain-logic only — it hosts, serves, and distributes nothing, and makes no
network call itself. Don't add anything that changes that. Contributions are licensed
under the repository's GPLv3.

Questions are welcome on [Discord](https://discord.gg/wan9FeDEfe).
