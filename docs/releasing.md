<!--
SPDX-FileCopyrightText: 2026 OpenCabinet contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# Releasing

A release is prepared by a pull request and published by a maintainer by
hand. Nothing reaches users until a person presses **Publish**.

1. **Prepare.** In one pull request, set `version` in the workspace's
   `Cargo.toml` (SemVer; `-beta.N` for a test release) and move the
   changes under `## [Unreleased]` in `CHANGELOG.md` to a section
   `## [X.Y.Z] - YYYY-MM-DD`, with the link definitions at the end of the
   file. Merge it as usual.
2. **Tag.** On the merged `main`, push the version tag:

   ```bash
   git tag -a v0.1.0-beta.1 -m "OpenCabinet Designer 0.1.0-beta.1"
   ```

   ```bash
   git push origin v0.1.0-beta.1
   ```

3. **Draft.** The **Release** workflow checks that the tag matches
   `Cargo.toml` and has notes, builds and tests the Windows installer,
   packs the sample kitchen as `sample-kitchen.ocab`, writes
   `SHA256SUMS.txt`, and creates a **draft** release with them and the
   version's notes. Versions with a `-` are marked as pre-releases.
4. **Publish.** A maintainer reads the draft on the
   [releases page](https://github.com/OpenCabinet/opencabinet/releases),
   installs the installer from it on a Windows computer, and publishes
   it.

If the workflow fails, fix the cause in a pull request, delete the tag
(`git push origin :refs/tags/v0.1.0-beta.1` and `git tag -d v0.1.0-beta.1`)
and push it again on the fixed `main`.
