# modus-sdk

Guest SDK and CLI for **Modus ABI 2** plugins: crate `modus-sdk`, binary `modus`, signing crate `modus-sign`, and the WIT contract in `wit/`.

Author path: `new` → `dev` → `pack` → install `.mplug` into the host app. Language — Rust. Do not put secrets in the package.

## Layout

```text
guest/     # crate modus-sdk (path dependency for plugins)
cli/       # binary modus
sign/      # crate modus-sign (pack --sign / host verify)
wit/       # modus:abi@2.0.0 — world plugin
```

## Build

```powershell
rustup target add wasm32-wasip1
cargo build -p modus-sdk --features consumer
cargo run -p modus --release -- --help
```

Convenience (from this repo root):

```powershell
cargo run --manifest-path cli/Cargo.toml --release -- <command>
```

## Plugin dependency

```toml
[dependencies]
modus-sdk = { path = "../modus-sdk/guest", default-features = false, features = ["consumer"] }
```

Exactly one role feature. Docs for authors: companion hub **modus-docs** (tutorial, ref, api).

## License

MIT (see crate manifests).
