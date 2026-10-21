# Notes for coding agents

## Releasing

All crates and `oxframe-cli` share one version (see CHANGELOG.md).

1. `./scripts/release-check.sh` must pass. It runs this repository's CI and
   then the CLI (its project template, end to end) and the site (examples,
   docs, the compile service's sandbox template, e2e) against this
   checkout, and builds the container images on it with podman (or
   docker): the compile service's image is run with no network and must
   build a real scene. It also checks that the marketing pages quote only
   the timings in `oxframe-web/apps/site/src/lib/measurements.json`. It
   also renders this version's changelog as an MP4
   (`target/release-check/release.mp4`) through the sibling
   `release-video-action` checkout, the CLI, and this SDK. CI alone cannot
   catch an API change that breaks the other two repositories or the images.
2. Bump `[workspace.package] version` and the internal dependency versions,
   add the CHANGELOG.md section (breaking changes with "before → after").
3. Commit, tag `vX.Y.Z`, push the tag: the release workflow publishes to
   crates.io. Wait until the crates are there.
4. CLI: bump its version and `SDK_VERSION`, publish, tag.
5. Site: archive the previous docs (`bun scripts/archive-docs.ts <old
   version>` in oxframe-web/apps/site), bump `apps/site/src/lib/sdk.ts`,
   `bun run sync-examples`, push. The site and compile.oxframe.dev
   redeploy.
