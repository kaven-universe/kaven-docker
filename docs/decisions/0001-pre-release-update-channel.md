# 0001: Temporary pre-release update channel

Status: Superseded by 0003

## Context

The current release-candidate workflow publishes immutable Windows installers
through GitHub Releases. Kaven Docker does not yet have an updater signing key,
canonical `public.data.kaven.xyz` manifest, or an in-application signed update
flow implementing `RUNTIME-UPDATE-001` through `RUNTIME-UPDATE-012`.

## Decision

Release candidates use explicit user-initiated installer downloads. The
application does not perform background update checks or replace its own
files. This is a temporary deviation from the desktop update rules and must
not carry into the first stable release.

## Consequences and compensating controls

- Users must discover and install release candidates manually.
- Published tags and installer assets are immutable.
- The release workflow runs formatting, linting, type checking, tests, builds,
  and dependency audits before publication.
- Release notes identify compatibility and recovery behavior.

## Alternatives

Shipping an unsigned or build-time-injected updater endpoint was rejected
because it would weaken the product trust boundary.

## Review condition

Before publishing the first stable release, establish the checked-in canonical
manifest endpoint and public verification key, protected signing process,
explicit confirmation UI, active-operation gate, stale-manifest recheck,
bounded progress, safe shutdown, failure recovery, and end-to-end update tests.

These conditions were implemented by decision 0003.
