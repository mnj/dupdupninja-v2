# GitHub automation

- CI workflow: `.github/workflows/ci.yml`
- Release workflow (tag-driven): `.github/workflows/release.yml`

## Releases

Push a version tag like `v0.1.0` to trigger a build on Linux/macOS/Windows and attach artifacts to a GitHub Release.

## Allowed failures (temporary)

Until the macOS and Windows pipelines are fully stabilized, their CI/release jobs are marked as allowed-to-fail so Linux artifacts can still be produced.
