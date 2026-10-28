# Contributing

Thanks for contributing to the OWS runtime!

## Development

Prerequisites: a recent stable Rust toolchain (see `rust-toolchain.toml`).

```sh
cargo build --workspace
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo doc --workspace --all-features
```

## Guidelines

- Keep the core engine independent of HTTP frameworks, databases, brokers and
  cloud providers; add capabilities as adapters behind the core traits.
- Preserve the OWS specification semantics. If a behavior is in doubt, the
  specification wins.
- Add regression tests for every bug you fix.
- Ensure `cargo test --workspace --no-default-features` and
  `cargo test --workspace --all-features` both pass.
- Run `cargo fmt --all` and keep `clippy -D warnings` clean.

## Pre-commit hooks

Install [`prek`](https://github.com/j178/prek) and enable the repository hooks:

```sh
cargo install --locked prek
prek install
```

Commits that include Rust files will run `cargo fmt --all -- --check` and
`cargo clippy --workspace --all-targets --all-features -- -D warnings`. Run the
hooks manually with `prek run --all-files`.

## Conformance

If you change execution semantics, run the CTK:

```sh
cargo run -p ows-cli -- conformance --ctk <path-to-ows-spec/ctk/features>
```

## Commits

Use clear, conventional commit messages describing the change and its rationale.
