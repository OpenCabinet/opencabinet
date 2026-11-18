<!--
SPDX-FileCopyrightText: 2026 OpenCabinet contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# Windows installer

`opencabinet.iss` is the [Inno Setup](https://jrsoftware.org/isinfo.php)
script for the OpenCabinet Designer installer. Inno Setup is free, widely
used and produces a single small `.exe` that installs for the current user
without administrator rights, which many shop computers do not give.

## Test builds

The **Windows installer** workflow builds the installer, installs it silently,
checks that the program starts, uninstalls it and keeps the installer as a
download for 30 days. It runs on every pull request that changes the
application, and by hand:

1. Open **Actions → Windows installer → Run workflow** and choose `main`.
2. When the run finishes, download the `OpenCabinet-Designer-…-windows-x64`
   artifact from the run's page and unzip it.

Test builds are numbered after the workspace's version plus `+dev.N`, such
as `0.1.0-beta.1+dev.42`, where `N` is the run number. Releases are built
by the **Release** workflow when a version tag is pushed (see
[`docs/releasing.md`](../../docs/releasing.md)).

## The SmartScreen warning

The installer is not signed yet, so Windows shows "Windows protected your
PC" the first time it runs. Choose **More info → Run anyway**. Signing
needs a code signing certificate, a decision for a later release.

## Building locally

```bash
cargo build --release -p opencabinet-app
```

```bash
ISCC /DAppVersion=0.1.0-beta.1+dev packaging/windows/opencabinet.iss
```

The installer is written to `target/installer/`.
