# Dependency Risk Register

Review date: 2026-11-18

`npm run audit:npm` rejects every unregistered high-severity finding. The one
accepted npm advisory is limited to build and test tooling, has no patched
release, and cannot receive attacker-provided glob expressions in the shipped
desktop application. Its machine-readable acceptance is stored in
`dependency-risk-acceptances.json` so an expiry or any additional advisory
fails CI.

`cargo audit` reports no known vulnerability in the supported Windows target.
Its allowed warnings are absent from the resolved Windows graph: the Linux
desktop and credential-store crates are target-specific, while `chrono` is an
unused optional entry retained in the lock graph by `serde_with`. Application
update scheduling uses only standard-library Unix timestamps.

| Advisory | Dependency | Scope and compensating control | Expiry |
| --- | --- | --- | --- |
| GHSA-fc5q-39xc-vjpf | `picomatch 4.0.7` | High-severity ReDoS affects the glob parser through the current latest release. The dependency is used only by lint, type-check, build, and test tooling; it is not bundled into the Tauri application and receives no untrusted runtime patterns. The machine-readable allowlist accepts only this advisory and fails on any additional high-severity finding. | 2027-02-17 |
| RUSTSEC-2026-0543 | `chrono 0.4.45` | The direct application dependency was removed. Cargo retains it only as an unused optional `serde_with` lock entry; `cargo tree --target x86_64-pc-windows-msvc -i chrono` confirms it is absent from the supported dependency graph. | 2027-02-17 |
| RUSTSEC-2024-0370 | `proc-macro-error 1.0.4` | Unmaintained build-time macro dependency through GTK 0.18. Kaven Docker currently supports and releases Windows only; it is absent from the Windows dependency graph. Track the Tauri GTK migration before enabling Linux builds. | 2027-02-17 |
| RUSTSEC-2024-0429 | `glib 0.18.5` | Unsound iterator implementation in the Linux GTK stack. Kaven Docker does not build or release Linux, and application code does not invoke this API. Re-evaluate with every Tauri upgrade and before Linux support. | 2027-02-17 |
| RUSTSEC-2026-0371 | `zbus_names 4.3.5` | Unmaintained Linux D-Bus type dependency reached through the Secret Service credential backend. It is absent from the supported Windows dependency graph. Track the keyring/zbus replacement before enabling Linux releases. | 2027-02-17 |
| RUSTSEC-2026-0370 | `zvariant 5.15.1` | Unmaintained Linux D-Bus serialization dependency reached through the Secret Service credential backend. It is absent from the supported Windows dependency graph. Track the keyring/zbus replacement before enabling Linux releases. | 2027-02-17 |
| RUSTSEC-2026-0372 | `zvariant_derive 5.15.1` | Unmaintained derive dependency paired with `zvariant` in the Linux Secret Service graph. It is absent from the supported Windows dependency graph and is reviewed with the same upgrade plan. | 2027-02-17 |

An expired entry blocks release until it is removed through an upgrade or
renewed with current evidence, a new expiry date, and explicit review.
