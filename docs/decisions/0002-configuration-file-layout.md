# 0002: Use one authoritative desktop configuration

Status: Implemented on 2026-11-18

## Context

The release-candidate implementation stored Registry connections, proxy
profiles, and global settings in three JSON files. That temporary deviation
made a complete configuration import a multi-file transaction and did not
provide durable recovery across a process or power interruption.

## Decision

Use the positive-versioned `config.json` as the only authoritative user
configuration. All backend mutations replace that file atomically. Imports
validate the complete bundle before performing the same single-file atomic
replacement.

Before the UI starts, installations with the pre-release three-file layout
write a durable `recovery/config-v1-migration.json` journal, materialize and
verify `config.json`, archive the old files under
`recovery/config-v0-backup/`, and finally remove the journal. Startup resumes
any prepared journal before loading normal configuration. A newer or invalid
configuration version is rejected without mutation.

## Consequences

- Cross-file partial configuration updates are no longer possible.
- The recovery journal contains only non-secret configuration; credentials
  remain in the operating-system credential manager.
- The archived version-zero files provide a manual downgrade path and are not
  treated as competing authoritative sources.
- Unit tests cover normal migration, interrupted-startup recovery, and
  rejection of a newer schema without mutation.
