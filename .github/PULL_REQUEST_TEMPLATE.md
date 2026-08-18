<!-- Keep the title short and imperative. -->

## What & why

<!-- What does this change do, and why? The "why" is the part a diff can't show. -->

## Type of change

- [ ] Bug fix
- [ ] New feature
- [ ] Refactor / cleanup
- [ ] Performance
- [ ] Docs / tooling

## How it was tested

- [ ] `cargo test --lib` passes
- [ ] `cargo fmt --all` clean
- [ ] `cargo clippy --all-targets -- -D warnings` clean
- [ ] `cargo check --no-default-features --features wasm` passes (web/webOS path)
- [ ] Fuzz target run/added if a parser grammar changed

## Checklist

- [ ] No I/O added to core — network/storage/clock work is modeled as an effect, not done inside the crate
- [ ] No new `unwrap` / `expect` / `panic` / slice-index on a fallible path
- [ ] New behaviour has a test; a bug fix includes the test that would have caught it
- [ ] No comments added to code
- [ ] Considered every consumer (Android / Desktop / Web) — noted below if one needs matching shell work

## Consumers affected

<!-- Which shells need a change to use this? "None — internal only", or list them. -->

## Related issues

<!-- "Closes #123", or remove this line. -->
