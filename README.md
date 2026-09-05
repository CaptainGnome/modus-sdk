# modus-sdk

Guest SDK and CLI for **Modus ABI 2** plugins: crate `modus-sdk`, binary `modus`, signing crate `modus-sign`, and the WIT contract in `wit/`.

**Repos:** [modus-sdk](https://github.com/CaptainGnome/modus-sdk) (this repo) · docs hub [modus-docs](https://github.com/CaptainGnome/modus-docs)

Author path: `new` → `dev` → `pack` → install `.mplug` into the host app. Language — Rust. Do not put secrets in the package.

## Clone

```powershell
git clone https://github.com/CaptainGnome/modus-sdk.git
git clone https://github.com/CaptainGnome/modus-docs.git
cd modus-sdk
```

## Layout

```text
guest/     # crate modus-sdk (path dependency for plugins)
cli/       # binary modus
sign/      # crate modus-sign (pack --sign / host verify)
wit/       # modus:abi@2.0.0 — world plugin
```

## Build

```powershell
rustup target add wasm32-unknown-unknown
cargo build -p modus-sdk --features consumer
cargo run --manifest-path cli/Cargo.toml --release -- --help
```

## Plugin dependency

Plugin in a sibling folder of this clone:

```toml
[dependencies]
modus-sdk = { path = "../modus-sdk/guest", default-features = false, features = ["consumer"] }
```

Exactly one role feature. Author docs: [modus-docs](https://github.com/CaptainGnome/modus-docs) (tutorial, ref, api, examples).

## License

MIT (see crate manifests).
