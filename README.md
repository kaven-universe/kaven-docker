# Kaven Docker

Kaven Docker is a Windows-first desktop application for managing Docker Registry HTTP API V2 services. The interface is built with Vue 3, while Tauri and Rust handle Registry requests, authentication, TLS, proxy connections, credential storage, and image transfers.

Status: stable. Windows 10 and Windows 11 are the currently supported platforms.

The application currently supports:

- Private Registry and common public Registry connections
- Repository, tag, manifest, platform, layer, and image metadata browsing
- Copying image names, tags, digests, full references, and pull commands
- Manifest deletion
- Selected-tag synchronization from any saved source to a private Registry
- Multi-platform image synchronization with reusable HTTP, HTTPS, and SOCKS proxies
- Per-stage synchronization progress, request diagnostics, and persistent log files
- Simplified Chinese and English interfaces
- Signed in-application updates through the canonical Kaven update channel, with automatic daily checks and manual checks from the Help menu

> Kaven Docker performs explicit image copies between Registry repositories. It is not currently a transparent Docker pull-through cache. See the user guide for the difference.

## Documentation

- [中文使用及功能说明](docs/USER_GUIDE.zh-CN.md)
- [English user and feature guide](docs/USER_GUIDE.en.md)
- [Release process](docs/RELEASE.md)
- [Engineering verification and security](docs/ENGINEERING.md)
- [Security policy](SECURITY.md)
- [Architecture decisions](docs/decisions/)

## Development

Requirements:

- Windows 10 or Windows 11
- Node.js 24 and npm 11
- Rust 1.99.1 with the MSVC toolchain
- Microsoft C++ Build Tools and WebView2 Runtime, as required by Tauri 2

Install dependencies and start the application:

```powershell
npm ci
npm run tauri dev
```

Build the Windows NSIS installer:

```powershell
npm run tauri build
```

Run the complete local verification procedure:

```powershell
npm run lint
npm run typecheck
npm test
npm run test:audit-policy
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features --locked -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --all-targets --all-features --locked
npm run verify:schemas
npm run audit:npm
cargo audit --file src-tauri/Cargo.lock
```

The release workflow runs the same applicable gates before it receives write
permission to publish an installer and its signed updater artifacts. The
updater reads its canonical manifest only through
`https://public.data.kaven.xyz/json/kaven-docker/latest.json`; installer URLs
inside that manifest may point to GitHub Releases or another host, but every
installer must pass the embedded Minisign public-key verification before
installation. Generated Tauri schemas are committed; `npm run verify:schemas`
regenerates them through the pinned Rust/Tauri toolchain and rejects any diff.

VS Code launch profiles are included in `.vscode/launch.json`. **Tauri: Run Dev (no debugger)** does not require a Rust debugger extension. **Tauri: Debug Rust (CodeLLDB)** requires the CodeLLDB extension.

## Current Scope

Windows is the supported platform for the first release. The code uses cross-platform Vue, Tauri, Rust, and native credential-store abstractions so that macOS and Linux support can be added later.

## License

Copyright 2026 Kaven. Licensed under the [Apache License 2.0](LICENSE).
