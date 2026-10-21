# Pull request

## Summary

<!-- What changes, and why. One or two sentences. -->

## Related issue

<!-- Fixes #0 -->

## Type of change

- [ ] Bug fix
- [ ] New feature
- [ ] Refactor with no behaviour change
- [ ] Documentation
- [ ] Build, packaging or CI
- [ ] Machine model (`massey-core`) change

## How was this tested?

<!-- Commands you ran, windows sizes you tried, programs you loaded.
     For machine-model changes, list the opcodes or examples you covered. -->

```sh
cargo fmt --all -- --check
cargo clippy -p massey-gui --all-targets -- -D warnings
cargo test --workspace
```

## Screenshots

<!-- Required for any visual change: before and after. -->

## Checklist

- [ ] The change is focused; unrelated refactors are in a separate PR.
- [ ] Tests cover the new behaviour, or the PR explains why no test applies.
- [ ] `cargo fmt`, `clippy` and `cargo test --workspace` pass locally.
- [ ] User-visible changes have a `CHANGELOG.md` entry under `## [Unreleased]`.
- [ ] Documentation (`docs/` or `README.md`) is updated where behaviour changed.
- [ ] No `unwrap()` was added on user input.
- [ ] Nothing references an internal-only path, secret or personal machine layout.
