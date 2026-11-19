# 0003: Public-data signed update channel

Status: Accepted

## Context

Kaven Docker installers are built and published through GitHub Releases, but
the application needs a stable, owner-controlled update-discovery endpoint.
As with Kaven Git, discovery must not depend directly on GitHub's definition
of the latest release or expose the application to a mutable third-party
redirect boundary.

## Decision

The application uses the Tauri updater and a dedicated Kaven Docker Minisign
key pair. It checks only this canonical endpoint:

```text
https://public.data.kaven.xyz/json/kaven-docker/latest.json
```

Redirects while fetching this manifest are restricted to HTTPS locations on
`public.data.kaven.xyz`. The manifest's installer `url` and optional ordered
`urls` may use GitHub Releases or any other distribution host, because every
candidate must serve identical bytes and pass the embedded signature check.

The rest of the update behavior follows Kaven Git: automatic checks are
backend-scheduled, failures use bounded retry backoff, installation rechecks
the signed manifest and expected version, ordered mirror addresses are tried
in turn, progress is visible, and active image synchronization blocks
installation.

## Consequences

- GitHub Actions receives the signing key and password through the protected
  `release` Environment and publishes the signed installer artifacts.
- The public-data repository retains an immutable manifest for every version,
  a version list, and the canonical `latest.json` pointer.
- The canonical manifest is replaced only after the installer, signature, and
  installation lifecycle have been verified.
- Installer hosting can change without changing the client trust root.
- Losing the private key prevents installed clients from accepting future
  updates without a separately distributed migration release.
