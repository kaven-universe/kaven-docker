# Kaven Docker User and Feature Guide

## 1. Overview

Kaven Docker is a Windows desktop client for Docker Registry HTTP API V2 services. It connects directly to registries without using the local Docker CLI. You can browse images, inspect or delete manifests, and copy selected tags between registries.

The first release supports Windows. The interface is available in Simplified Chinese and English. Vue 3 provides the UI, while Tauri/Rust handles networking, authentication, TLS, proxies, native credential storage, and image transfers.

## 2. Features

| Area | Current capability |
| --- | --- |
| Registry connections | Add, edit, and remove multiple registries |
| Public presets | Docker Hub, GHCR, Quay.io, Kubernetes Registry, and MCR |
| Authentication | Anonymous and username/password, including Bearer challenge handling |
| TLS | HTTPS, custom CA files, certificate verification override, and opt-in HTTP |
| Image browsing | Repositories, tags, manifests, digests, platforms, layers, size, creation time, and labels |
| Copy actions | Image name, tag, full reference, digest, and `docker pull` command |
| Deletion | Delete a manifest when the server enables deletion |
| Synchronization | Copy selected tags from a saved source to a private Registry |
| Multi-platform images | Preserve the index/list and every referenced platform manifest |
| Proxies | Reusable global or per-Registry HTTP, HTTPS, and SOCKS profiles |
| Diagnostics | Per-stage progress, byte progress, detailed events, and persistent log files |
| Localization | Simplified Chinese and English |

## 3. Adding a Registry

Select the `+` button next to **Registries** in the sidebar.

### 3.1 Public presets

The add dialog includes these anonymous presets:

- Docker Hub: `https://registry-1.docker.io`
- GitHub Container Registry: `https://ghcr.io`
- Quay.io: `https://quay.io`
- Kubernetes Registry: `https://registry.k8s.io`
- Microsoft Container Registry: `https://mcr.microsoft.com`

Most public registries do not expose the global Catalog API. After adding one, use the `+` above its repository list and enter an owner or namespace. Kaven Docker automatically discovers repositories on Docker Hub and Quay; GHCR discovery uses the GitHub token configured on the connection and requires `read:packages`. Select the repositories to add. Manual names such as `library/nginx`, `owner/image`, or `dotnet/runtime` remain available as a fallback. Do not include a host, tag, or digest.

To remove a saved repository from Kaven Docker, select it and use the remove button beside the repository count. This only removes the local saved entry; it does not delete the remote repository or any images.

Edit the connection and enable username/password authentication when a repository requires credentials or an access token.

### 3.2 Private registries

Configure:

- A display name used only inside Kaven Docker.
- A root URL such as `https://registry.example.com` or `http://192.168.1.20:5000`.
- Anonymous or username/password authentication.
- An optional local PEM CA file for privately issued certificates.
- Explicit insecure HTTP permission when the URL uses `http://`.
- An optional TLS verification override for temporary diagnostics.
- A request timeout from 3 to 120 seconds.
- Global, direct, or saved-profile proxy behavior.

Enter only the service root. Do not append `/v2/`, a repository, or a tag. Kaven Docker requests `/v2/` after saving to test the connection.

## 4. Browsing Images

Choose a Registry in the sidebar, a repository in the catalog column, and a tag in the versions column. The detail panel can show:

- A complete `docker pull` command
- Image name, tag, and full Registry reference
- Manifest digest and media type
- Image size and creation time
- Operating system, architecture, and variant
- Config and filesystem layers
- OCI/Docker image labels
- Platforms and child manifest digests for a multi-platform image

Use the copy button beside a reference field to place its value on the clipboard.

### 4.1 Unknown creation time

A manifest index or Docker manifest list normally has no creation timestamp of its own. Kaven Docker follows its platform manifests and reads their image configuration blobs. `Unknown` can still be correct when the upstream config has no `created` value, access to a required blob is denied, or the Registry returns non-standard or incomplete metadata.

## 5. Deleting a Manifest

Choose a tag and select **Delete** in the detail panel. Kaven Docker resolves the tag to its content digest before sending the delete request.

The Registry server must permit manifest deletion. CNCF Distribution commonly requires:

```yaml
storage:
  delete:
    enabled: true
```

Deleting a manifest does not necessarily reclaim disk space immediately. Server-side garbage collection is normally required. Multiple tags may share content, and deletion cannot be undone through Kaven Docker.

## 6. Synchronizing Images

Select **Sync images** in the top toolbar. Choose a source Registry and repository, a private target Registry and repository, and one or more tags.

Use **Batch sync** to select multiple source repositories. Each selected repository has its own target repository name and tag selection. Kaven Docker automatically selects `latest` and `stable` when those tags exist; review or change the selections before starting. The source and target clients are initialized once and repositories are processed sequentially. During the run, Kaven Docker shows overall progress, the current image, and remaining, succeeded, and failed counts. One failed image does not stop later items; after processing finishes, use **Retry failed** to synchronize only the failed images again.

Public registries can currently be sources but not targets. Publishing to hosted registries involves provider-specific credentials, permissions, and policies, so the first release restricts targets to generic private registries.

### 6.1 Transfer stages

1. Initialize independent source and target clients.
2. Read selected manifests and recursively resolve multi-platform indexes/lists.
3. Check target blobs, skip existing content, and transfer missing blobs through a persistent, content-addressed local cache in bounded chunks.
4. Write child manifests before their parent indexes/lists.
5. Publish the selected target tags.

Content is deduplicated by digest. Shared layers are transferred once, and blobs already present at the target are skipped. Downloaded blobs are retained in the application cache and verified against their SHA-256 or SHA-512 digest before every reuse, avoiding repeat downloads when syncing to another target. The maximizable progress dialog shows a progress bar for each stage, byte-level blob progress, cache hits, event logs, and final counts. Select **Stop sync** to cancel a running transfer; closing the sync dialog also cancels the current network transfer first.

### 6.2 Multi-platform behavior

Kaven Docker copies the top-level index/list, all referenced platform manifests, and their config and layer blobs. It does not limit the transfer to the platform of the Windows computer running Kaven Docker.

### 6.3 Source and target proxies

The source and target use separate clients and their own Registry settings:

- Source manifest reads and blob downloads use the source Registry proxy, credentials, CA, and timeout.
- Target blob checks/uploads and manifest writes use the target Registry proxy, credentials, CA, and timeout.

For example, Docker Hub can use a local SOCKS5 proxy while an intranet target connects directly.

### 6.4 Image names after synchronization

An explicit copy changes the Registry reference used to retrieve the image:

```text
Source: docker.io/library/nginx:latest
Target: registry.example.com/library/nginx:latest
```

Pull the copied image with:

```powershell
docker pull registry.example.com/library/nginx:latest
```

The content digests can remain identical, but the Registry host is client routing information and is not embedded in the manifest. Keeping the `docker pull nginx:latest` experience requires a server-side Docker Hub pull-through cache plus Docker Engine `registry-mirrors` configuration. That differs from Kaven Docker's current explicit synchronization.

## 7. Proxy Profiles

Open network settings from the lower-left toolbar. Create reusable profiles by selecting a protocol and entering a hostname, port, and optional credentials. Users do not have to construct a proxy URL.

| Protocol | Default port | Authentication | DNS resolution |
| --- | ---: | --- | --- |
| HTTP | 8080 | Username/password | Usually client-side |
| HTTPS | 443 | Username/password | Usually client-side |
| SOCKS4 | 1080 | Not supported | Client-side |
| SOCKS4A | 1080 | Not supported | Proxy-side |
| SOCKS5 | 1080 | Username/password | Client-side |
| SOCKS5H | 1080 | Username/password | Proxy-side |

Each Registry selects one mode:

- **Follow global settings** uses the current default profile, or connects directly when no default is selected.
- **Direct connection** bypasses the global proxy.
- **Saved proxy** always uses the selected profile.

A profile referenced by global settings or a Registry cannot be deleted until those references are changed. SOCKS5H is useful when DNS should be resolved by the proxy; intranet targets will normally use a direct connection.

## 8. Security and Local Data

Registry and proxy passwords are stored in the operating system credential manager, not in JSON files. Connection names, URLs, usernames, and non-secret settings are stored in the application configuration directory.

On Windows, the default directory is:

```text
%APPDATA%\io.kaven.docker\
```

It contains:

```text
registries.json    Registry settings without passwords
proxies.json       Proxy profiles without passwords
settings.json      Global settings
```

Synchronization logs use Tauri's application log directory. On Windows, this is normally:

```text
%LOCALAPPDATA%\io.kaven.docker\logs\sync\
```

Use custom CA trust where possible. Skipping TLS verification or allowing HTTP reduces connection security and should only be used in controlled environments.

## 9. Logs and Troubleshooting

Synchronization logs include source/target endpoints and proxy modes, manifest requests, blob checks/downloads/uploads, manifest writes, tag publication, HTTP errors, Registry response bodies, and underlying network error chains. Passwords are not intentionally logged. Copy the visible log with the button in its header; the persistent log path is shown below it.

Common checks:

- **Connection refused/timeout:** verify the root URL, port, proxy, DNS, VPN, firewall, CA, and timeout.
- **401:** verify credentials or tokens and source pull/target push permissions.
- **403:** the account may lack catalog, pull, push, or delete permission.
- **Catalog unavailable:** public registries commonly disable `_catalog`; add the repository name manually.
- **Delete rejected:** enable deletion, verify permissions, and allow `DELETE` through the reverse proxy.
- **Blob download failure:** check source permissions, the source proxy, and free space in the application cache directory.
- **Blob upload failure:** check target permissions, reverse-proxy body limits, buffering, timeouts, and `Location` header rewriting.
- **Manifest write failure:** the target may reject the media type or believe a referenced blob is missing.

## 10. Language

Use the language control in the lower-left toolbar to switch between Simplified Chinese and English. The choice is kept in WebView Local Storage. Repository names, tags, labels, and raw server errors are not translated.

## 11. Current Limitations

- Windows is the only validated release platform.
- No scheduled or background synchronization while the app is closed.
- No automatic whole-Registry mirroring or target tag pruning.
- Public registries cannot be synchronization targets.
- No transparent pull-through cache or Docker Engine configuration.
- No server-side Registry garbage collection.
- No container, Docker daemon, Compose, or Kubernetes workload management.
- Stopping a synchronization interrupts the current transfer, but blobs, manifests, or tags already written to the target Registry are not rolled back automatically.

## 12. Development and Build

Requirements: Windows 10/11, Node.js 20 or later, stable Rust with the MSVC toolchain, Microsoft C++ Build Tools, and Microsoft Edge WebView2 Runtime.

```powershell
npm install
npm run tauri dev
```

VS Code provides **Tauri: Run Dev (no debugger)** without an LLDB dependency and **Tauri: Debug Rust (CodeLLDB)** for Rust debugging. The pre-launch task reuses port `1420` only when the existing Vite process belongs to this workspace.

Validate and build:

```powershell
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri build
```

The Windows NSIS installer is written under `src-tauri/target/release/bundle/nsis/`.
