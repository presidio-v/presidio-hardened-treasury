# Fuzzing

`cargo-fuzz` (libFuzzer) harnesses for the evidence boundary. The crate is kept
out of the main workspace because it needs a nightly toolchain.

| Target | What it checks |
|--------|----------------|
| `canonical_json` | Arbitrary bytes → JSON → `treasury_evidence::canonical_bytes` never panics, and canonical output is a fixed point (re-canonicalizing gives the same bytes) |

Run locally:

```sh
cargo install cargo-fuzz
cd fuzz
cargo +nightly fuzz run canonical_json -- -max_total_time=60
```

CI runs each target for 60 seconds on every pull request (`fuzz` job in
`.github/workflows/ci.yml`).
