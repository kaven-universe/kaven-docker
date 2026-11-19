# Release Process

Kaven Docker uses separate development and release histories:

- `origin/main` contains the full development history.
- `github/release` contains only the current release source snapshot and is used by GitHub Actions to build the installer.

The GitHub `release` branch is an orphan branch. Every release replaces it with a new parentless snapshot commit. Older release sources remain available through their version tags.

## Release trigger

`.github/workflows/release.yml` runs when an `X.Y.Z` tag is pushed. The workflow uses `tauri-apps/tauri-action` to:

1. Build the Windows NSIS installer.
2. Sign and publish the Tauri updater bundle and candidate `latest.json`.
3. Create the GitHub Release.
4. Keep the Release as a draft until checksums, SBOMs, and reviewed notes are attached.
5. Publish the completed Release as the latest stable version.

The GitHub `latest.json` asset is release evidence and a candidate manifest;
installed applications never use it as their discovery endpoint. The
canonical manifest is published separately through `public.data.kaven.xyz`
after release verification.

Configure these protected GitHub `release` Environment secrets before any
release build:

- `TAURI_SIGNING_PRIVATE_KEY`: the complete Minisign private-key content.
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: the private-key password.

The private key must match the public key in `src-tauri/tauri.conf.json`. Never
store the private key, password, or their encoded forms in the repository,
workflow logs, release assets, or source snapshot.

The dedicated local signing material is stored outside the repository:

- Private key: `C:\Users\Kaven\.tauri\kaven-docker.key`
- Public key: `C:\Users\Kaven\.tauri\kaven-docker.key.pub`
- Local password cache: `C:\Users\Kaven\.tauri\kaven-docker.password.dpapi`

The password file is protected with Windows DPAPI and can only be decrypted by
the Windows user that created it on the same computer. It is not a portable
backup and normally cannot be decrypted after copying it to another computer.
Store the original password separately in a password manager or offline secure
backup, and back it up together with the private key. Losing either prevents
signing future updates for installed clients.

Use a verified `X.Y.Z` tag push as the only release trigger. The workflow has
no `workflow_dispatch` path, preventing a manually supplied tag from being
built from unrelated source.

## 1. Preflight checks

Check the worktree, branch, and remotes:

```powershell
git status --short --branch
git branch -vv
git remote -v
git remote get-url origin
git remote get-url github
git -C C:\Users\Kaven\nguowk\public.data.kaven.xyz status --short
```

The GitHub release remote should be:

```text
git@github.com:kaven-universe/kaven-docker.git
```

Add it if it is not configured:

```powershell
git remote add github git@github.com:kaven-universe/kaven-docker.git
```

Fetch the release references and confirm that the new tag does not exist:

```powershell
git fetch --no-tags github refs/heads/release:refs/remotes/github/release
git fetch github --tags
git ls-remote --tags github refs/tags/0.2.1
```

The last command must produce no output. If the tag exists, stop and inspect the GitHub Release and Actions runs. Do not overwrite a published release tag.

## 2. Update the version

For this release, change the version to `0.2.1` in all of these files:

- `package.json`
- `package-lock.json` (top-level and root package entries)
- `src-tauri/Cargo.toml`
- `src-tauri/Cargo.lock` (the `kaven-docker` package entry)
- `src-tauri/tauri.conf.json`

Confirm that no prerelease version remains:

```powershell
rg -n '0\.2\.1-' package.json package-lock.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json
./scripts/test-release-identity.ps1 -Tag 0.2.1
```

## 3. Build and test

```powershell
npm ci
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

Every command must pass before continuing. Also verify that regenerating Tauri
schemas produces no unexpected diff and complete the installation, upgrade,
uninstall, credential-preservation, failed-import, and interrupted-transfer
recovery checks before publishing.

For a local signed updater build, provide the same protected key through the
process environment, then clear the variables when the build is complete:

```powershell
$password = Get-Content -Raw -LiteralPath 'C:\Users\Kaven\.tauri\kaven-docker.password.dpapi' |
  ConvertTo-SecureString
$credential = [pscredential]::new('kaven-docker', $password)
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content -Raw -LiteralPath 'C:\Users\Kaven\.tauri\kaven-docker.key'
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = $credential.GetNetworkCredential().Password
npm run tauri build
Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY
Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD
$credential = $null
$password.Dispose()
```

## 4. Commit and push the development branch

```powershell
git diff --check
git add -- package.json package-lock.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json
git commit -m "chore(release): prepare 0.2.1"
git push origin main
```

At this point, `origin/main` should contain all feature commits and the version preparation commit.

## 5. Create the single-commit release snapshot

Create a parentless commit directly from the current `HEAD` tree. This does not switch branches or modify the worktree:

```powershell
$releaseVersion = "0.2.1"
$tree = (git rev-parse 'HEAD^{tree}').Trim()
$snapshot = (git commit-tree $tree -m "Kaven Docker $releaseVersion release snapshot").Trim()
```

Verify the snapshot. `parents=` must be empty, and `tree=` must match the `HEAD` tree:

```powershell
git show --no-patch --format="commit=%H%nparents=%P%ntree=%T%nsubject=%s" $snapshot
git rev-parse 'HEAD^{tree}'
```

Read the current GitHub snapshot and safely replace `release` with `--force-with-lease`:

```powershell
$releaseLine = git ls-remote --heads github refs/heads/release
$previousSnapshot = ($releaseLine -split '\s+')[0]
git push --force-with-lease="refs/heads/release:$previousSnapshot" github "${snapshot}:refs/heads/release"
```

Before creating the tag, verify that GitHub `release` points to the new snapshot:

```powershell
git ls-remote --heads github refs/heads/release
```

The returned commit must equal `$snapshot`.

## 6. Create and push the tag last

Only create the tag after the release snapshot has been verified:

```powershell
git tag -a $releaseVersion $snapshot -m "Kaven Docker $releaseVersion"
git show --no-patch $releaseVersion
git push github "refs/tags/$releaseVersion"
```

Pushing the tag starts GitHub Actions immediately. Never push a temporary or unverified tag.

## 7. Verify the build and Release

Open the [Release workflow](https://github.com/kaven-universe/kaven-docker/actions/workflows/release.yml) and confirm that there is exactly one run for the version and that its `head_sha` is the release snapshot commit.

After the build completes, verify that:

- The Actions run concludes with `success`.
- The Release is named `Kaven Docker 0.2.1`.
- The Release is published, is not a prerelease, and is marked latest.
- The Release contains the Windows NSIS installer.
- The asset name matches `Kaven-Docker_[version]_[arch][setup][ext]`.
- The Release contains `latest.json`, the updater archive, and its `.sig`
  signature generated from the protected signing key.
- The Release contains `npm.cdx.json`, `kaven-docker.cdx.json`, and
  `SHA256SUMS`, and every listed checksum matches the downloaded asset.
- The candidate `latest.json` identifies the published version, contains the
  complete signature, and every updater URL opens without authentication.
- An installed signed test build with a lower version detects the release, shows bounded
  download progress, verifies the signature, installs quietly, restarts, and
  reports the new version. A deliberately invalid signature must be rejected.
- Starting an update while image synchronization is active must be blocked;
  starting synchronization once installation begins must also be blocked.

The installer or updater archive may remain hosted on GitHub Releases. The
application does not discover it through GitHub directly; publication of the
canonical public-data manifest is a separate required step.

Finally, verify the remote references and local worktree:

```powershell
git ls-remote --heads github refs/heads/release
git ls-remote --tags github refs/tags/0.2.1
git status --short --branch
```

## 8. Publish version history and the canonical manifest

Update `C:\Users\Kaven\nguowk\public.data.kaven.xyz` only after the GitHub
artifacts and installation lifecycle have been verified. Preserve every
historical manifest.

1. Use the verified candidate `latest.json` as the basis for
   `json/kaven-docker/<version>.json`; refuse to overwrite an existing version.
2. Confirm its `url` and optional ordered `urls` serve identical signed bytes.
   The download hosts may be GitHub Releases or any other service.
3. Prepend the release to `json/kaven-docker/versions.json` and set `latest` to
   the new version.
4. Replace `json/kaven-docker/latest.json` last.
5. Parse all three files and verify version, publication time, URL, signature,
   and descending history order.
6. Commit only the public-data manifest files, deploy that repository through
   its normal publication process, then verify the live canonical response.

```powershell
$PublicDataRoot = 'C:\Users\Kaven\nguowk\public.data.kaven.xyz'
$ReleaseVersion = '0.2.1'
git -C $PublicDataRoot diff --check
git -C $PublicDataRoot status --short
git -C $PublicDataRoot add -- "json/kaven-docker/$ReleaseVersion.json" json/kaven-docker/versions.json json/kaven-docker/latest.json
git -C $PublicDataRoot commit -m "Publish-Kaven-Docker-$ReleaseVersion-update-manifests"
```

The application always checks:

```text
https://public.data.kaven.xyz/json/kaven-docker/latest.json
```

The live site may be cached. Verify the deployed response itself announces the
new version and signed artifact URL before declaring the release complete.

## Recovery

### The tag points to the wrong commit

A formal tag is immutable. Never move, delete, or reuse a pushed version tag.
Cancel the incorrect Actions run, withdraw any draft or published Release,
fix and verify the source on `main`, and prepare the next unused higher numeric
version. Record the withdrawn version in the release notes and update-channel
metadata so clients cannot confuse it with the replacement.

### The release branch changed after it was checked

`--force-with-lease` rejects an unexpected remote update. Do not replace it with plain `--force`. Fetch the branch, inspect the change, and generate the snapshot again.

### The build fails

For a transient runner or network failure before publication, keep the tag and
rerun the same workflow. The draft Release may be updated, but it must remain
unpublished until every asset and check succeeds. Do not push the tag again.

If source changes are required, delete the failed draft, fix and verify the
problem on `main`, and prepare the next unused numeric version, such as
`0.2.1`; never move a published version tag or replace published assets.
