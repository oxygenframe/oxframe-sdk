---
name: release-to-video
description: "Turn release notes or a changelog into a short video: title card with the version, the changes as lists, an end card. Use for release announcements, changelog videos, or setting up oxygenframe/release-video-action in CI."
---
# Release notes to video

Uses the `oxframe` skill (workflow, full API cheat sheet); install both with `npx skills add oxygenframe/oxframe-sdk`.

## In CI, with no code

```yaml
on:
  release:
    types: [published]
jobs:
  video:
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - uses: oxygenframe/release-video-action@v1
```

The action turns headings into title cards and bullets into lists, runs
`oxframe check`, renders with `--cpu` and attaches `release.mp4` to the release.
A release created by a workflow with `GITHUB_TOKEN` does not trigger
`on: release`: add the step to that job with `tag: ${{ github.ref_name }}`.
`portrait: true` renders 9:16; `credit: false` drops "made by Oxframe".

## As a scene of your own

```rust
#[oxframe::scene]
async fn release(view: View) {
    let title = view.add(TitleCard::new("v2.4.0").subtitle("What's new"));
    title.enter().await;
    title.next().await;
    wait_for(1.5).await;
    title.exit().await;
    let changes = view.add(BulletList::new(["Faster startup", "Dark theme", "Bug fixes"]));
    changes.enter().await;
    for _ in 0..3 {
        changes.next().await;
        wait_for(1.2).await;
    }
    changes.exit().await;
    let end = view.add(EndCard::new("Update now").note("example.com"));
    end.enter().await;
    end.next().await;
    wait_for(1.0).await;
}
```

Keep one idea per card and at most five bullets per list; split longer
sections into several lists. Hold each card long enough to read, or
`oxframe check` reports `too_fast`.
