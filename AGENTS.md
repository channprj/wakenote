# WakeNote Agent Instructions

## Local releases

- GitHub Actions is disabled at the repository level to avoid hosted build and release costs. Keep it disabled; adding or enabling hosted workflows requires an explicit user request.
- `pnpm release:publish` reuses local artifacts matching the current commit, version, architecture and checksums, or builds them first. Commit synchronized versions before building. Use `pnpm release:build` to prepare artifacts separately; both build paths produce a DMG, checksums, and source-commit metadata under the ignored `release/` directory for the current Mac's architecture.
- Push the branch and annotated version tag, then validate publication with `pnpm release:publish --dry-run`. Dry-run reports whether a build is needed without building or publishing. Use `pnpm release:publish` when GitHub Release publication is requested, or `pnpm release:publish --draft` when a draft is requested. Automatic rebuilds preserve replaced local output under `.previous-*` directories; publication never pushes Git refs or overwrites existing GitHub releases.
- The current macOS packaging path uses ad-hoc signing and does not notarize the app. Do not describe artifacts as Developer ID signed or notarized.

## Versioning

- Every repository update must include a Headatever patch-version bump. This applies to code, tests, documentation, configuration, assets, and this file itself.
- Follow Headatever's `head.yymmdd.patch` rules: increment the patch on the same local date, reset it to `0` on a new local date, and change the head only when the user explicitly requests a major bump.
- Use the installed `headatever` skill and its bundled `scripts/headatever.sh`; never edit `VERSION` by hand.
- Before creating the release commit or annotated `v<version>` tag, synchronize the same version across `VERSION`, `package.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, and `src-tauri/tauri.conf.json`.
- Keep the functional update and its release/version metadata in separate Conventional Commits. Push the branch and annotated tag, then verify local, tracking, and live-remote parity before declaring the update complete.
