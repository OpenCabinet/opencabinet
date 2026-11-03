<!--
SPDX-FileCopyrightText: 2026 OpenCabinet contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# Requests for comments

An RFC proposes a large or hard-to-reverse change before anyone builds it:
the plugin API, the file format, new official modules or governance. Smaller
technical decisions go directly to an [ADR](../adr/).

## Process

1. Copy [`template.md`](template.md) to `NNNN-short-title.md` using the next
   free number, and open a pull request titled `docs(rfc): NNNN short title`.
2. The pull request stays open for a **public comment period of at least
   14 days**, announced in Discussions.
3. The maintainers accept, reject or postpone the RFC, writing down the
   reasoning in the pull request. Accepted RFCs are merged with the status
   updated.
4. Implementation happens in separate pull requests that reference the RFC.

## Index

| RFC | Title | Status |
| --- | --- | --- |
| [0001](0001-project-data-model.md) | Project data model | Draft |
| 0002 | Module and plugin architecture | Planned (phase 1b) |
