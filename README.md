# Kaven Docker

Kaven Docker is a Windows-first desktop application for managing Docker Registry HTTP API V2 services. The interface is built with Vue 3, while Tauri and Rust handle Registry requests, authentication, TLS, proxy connections, credential storage, and image transfers.

The application currently supports:

- Private Registry and common public Registry connections
- Repository, tag, manifest, platform, layer, and image metadata browsing
- Copying image names, tags, digests, full references, and pull commands
- Manifest deletion
- Selected-tag synchronization from any saved source to a private Registry
- Multi-platform image synchronization with reusable HTTP, HTTPS, and SOCKS proxies
- Per-stage synchronization progress, request diagnostics, and persistent log files
- Simplified Chinese and English interfaces

> Kaven Docker performs explicit image copies between Registry repositories. It is not currently a transparent Docker pull-through cache. See the user guide for the difference.

## Documentation

- [中文使用及功能说明](docs/USER_GUIDE.zh-CN.md)
- [English user and feature guide](docs/USER_GUIDE.en.md)

## Development

Requirements:

- Windows 10 or Windows 11
- Node.js 20 or later
- Rust stable with the MSVC toolchain
- Microsoft C++ Build Tools and WebView2 Runtime, as required by Tauri 2

Install dependencies and start the application:

```powershell
npm install
npm run tauri dev
```

Build the Windows NSIS installer:

```powershell
npm run tauri build
```

Run frontend type checking and production bundling:

```powershell
npm run build
```

VS Code launch profiles are included in `.vscode/launch.json`. **Tauri: Run Dev (no debugger)** does not require a Rust debugger extension. **Tauri: Debug Rust (CodeLLDB)** requires the CodeLLDB extension.

## Current Scope

Windows is the supported platform for the first release. The code uses cross-platform Vue, Tauri, Rust, and native credential-store abstractions so that macOS and Linux support can be added later.

License information has not yet been added to this repository.
