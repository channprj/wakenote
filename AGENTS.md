# WakeNote Agent Instructions

## Versioning

- Every repository update must include a Headatever patch-version bump. This applies to code, tests, documentation, configuration, assets, and this file itself.
- Follow Headatever's `head.yymmdd.patch` rules: increment the patch on the same local date, reset it to `0` on a new local date, and change the head only when the user explicitly requests a major bump.
- Use the installed `headatever` skill and its bundled `scripts/headatever.sh`; never edit `VERSION` by hand.
- Before creating the release commit or annotated `v<version>` tag, synchronize the same version across `VERSION`, `package.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, and `src-tauri/tauri.conf.json`.
- Keep the functional update and its release/version metadata in separate Conventional Commits. Push the branch and annotated tag, then verify local, tracking, and live-remote parity before declaring the update complete.
