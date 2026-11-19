<!--
SPDX-FileCopyrightText: 2026 OpenCabinet contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# Contributing to OpenCabinet

Thank you for your interest! The project is in its earliest phase, so the
most valuable contributions right now are:

- **Millwork expertise.** How do you build cabinets? Which construction
  methods, allowances and conventions must a designer support? Share it in
  [Discussions](../../discussions).
- **Feedback on design documents.** Comment on open
  [RFCs](docs/rfcs/) and [ADRs](docs/adr/).
- **Documentation fixes**, from typos to clearer explanations.

## Ground rules

- Be kind and follow the [Code of Conduct](CODE_OF_CONDUCT.md).
- The working language of the repository is **English**. Spanish is welcome
  in Discussions; translations of the documentation are a goal.
- For anything larger than a small fix, open an issue or discussion first
  so we can agree on the approach before you invest time.

## Clean-room policy

OpenCabinet is developed independently. **Never contribute code, icons,
help texts, templates, libraries, documentation or sample files taken from
proprietary software** (AutoCAD, Bluebeam, Microvellum, Cabinet Vision,
Mozaik or any other), nor drawings belonging to clients or employers.
Implementing an equivalent *feature* from public knowledge is fine; copying
its *expression* is not. Test files are created from scratch for this
project.

## Developer Certificate of Origin (DCO)

We do not ask you to sign a CLA. Instead, every commit must carry a
`Signed-off-by` line certifying the
[Developer Certificate of Origin 1.1](https://developercertificate.org/):
you wrote the change or otherwise have the right to submit it under the
project's licenses. You keep your copyright.

Add the line automatically with `-s`:

```sh
git commit -s -m "docs: clarify edge banding allowances"
```

The sign-off must match the commit author's name and email. CI rejects pull
requests with unsigned commits. To fix the last commit, run
`git commit --amend -s --no-edit`; for several commits, run
`git rebase --signoff main` and force-push your branch.

## Commit messages and pull request titles

We use [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/).
Pull requests are squash-merged, so the **pull request title** becomes the
commit on `main` and must follow the format:

```text
<type>(<optional scope>): <description>
```

| Type | Use for |
| --- | --- |
| `feat` | A new user-facing feature |
| `fix` | A bug fix |
| `docs` | Documentation only |
| `refactor` | Code change that neither fixes a bug nor adds a feature |
| `perf` | Performance improvement |
| `test` | Adding or correcting tests |
| `build` | Build system or dependencies |
| `ci` | CI configuration |
| `chore` | Maintenance that fits nothing above |
| `revert` | Reverting a previous commit |

Mark breaking changes with `!` after the type (`feat!: ...`) and explain
them in a `BREAKING CHANGE:` footer.

## Licensing of your contribution

Every file must start with an [SPDX](https://spdx.dev/) header declaring its
copyright and license, as required by the [REUSE](https://reuse.software/)
specification. Use the license of the component you are changing (see the
[README](README.md#license)). For example, in a Markdown file:

<!-- REUSE-IgnoreStart -->
```html
<!--
SPDX-FileCopyrightText: 2026 OpenCabinet contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->
```
<!-- REUSE-IgnoreEnd -->

Files that cannot hold comments (images, JSON) are declared in a
`REUSE.toml` file at the repository root.

## Pull request process

1. Fork the repository and create a branch from `main`.
2. Make focused changes; one logical change per pull request.
3. Run the checks locally (see below).
4. Open the pull request and fill in the template.
5. Reviews:
   - **one** approval for regular changes;
   - **two** approvals for changes to the core, the plugin API, the file
     format or licensing, once the project has two maintainers.
6. A maintainer squash-merges the pull request when CI is green.

The **Performance** check compares every change to the application with
`main` against the budgets of
[ADR-0014](docs/adr/0014-modest-hardware-budgets.md). When a change costs
more than it allows and a maintainer decides the cost is worth it, they add
the `performance-accepted` label, which keeps the measures in the report
and records the decision.

Large changes (plugin API, file format, new official modules, governance)
need an accepted [RFC](docs/rfcs/) first. Significant technical decisions
are recorded as [ADRs](docs/adr/).

## Running the checks locally

The CI runs the same checks on every pull request:

```sh
# Markdown style (requires Node.js)
npx markdownlint-cli2

# Licensing metadata (requires Python; https://reuse.software/)
pipx run reuse lint
```
