# Contributing

1. Fork, branch, make your change.
2. Build and test locally:

   ```sh
   cargo build --workspace
   cargo test  --workspace
   cargo fmt --all --check
   cargo clippy --workspace --all-targets -- -D warnings
   ```

   These are exactly what CI runs — a green local run means a green PR.
3. Open a pull request describing the change and why.

Adding a language? See [Adding a language](README.md#adding-a-language) —
it's a self-contained change behind the `Language` trait.

No CLA, no ceremony. Small, focused PRs over large ones.
