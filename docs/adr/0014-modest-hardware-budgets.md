<!--
SPDX-FileCopyrightText: 2026 OpenCabinet contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# 0014 — Performance budgets for modest computers

- **Status:** accepted
- **Date:** 2026-10-25
- **Deciders:** Andrés (@A-newGP)

## Context and problem statement

Principle 5 of the plan says the core runs on modest computers, and the plan
asks for that to be defined by measurable requirements that CI checks.
Small shops often run design software on an old office computer next to the
saw, without a graphics card. If speed and memory are not measured from the
first release, they degrade one change at a time and nobody notices until a
shop cannot use the program.

What is a "modest computer", which numbers must OpenCabinet Designer meet
on it, and how are they enforced?

## Decision drivers

- **Real shops:** the numbers must describe the computers shops actually
  have, not a developer's workstation.
- **Measurable in CI:** every pull request must be able to show that it did
  not make the program slower or heavier.
- **Honest about CI:** GitHub's runners are not modest computers, and
  their timings vary between runs by about 30% (ADR-0012).
- **Evidence:** the prototypes already measured what the chosen stack
  costs (ADR-0011, ADR-0012, the format and export prototypes).

## Considered options

1. **Budgets enforced in CI, against a defined reference computer**
2. Minimum requirements written in the documentation only
3. No budgets until users complain

## Decision outcome

Chosen option: **budgets enforced in CI** (option 1), because it is the
only one that catches a regression in the pull request that causes it.
Option 2 states a promise nobody checks, and option 3 finds problems after
shops have already given up.

### The reference computer

A computer about ten years old, still common in shops:

| | Reference |
| --- | --- |
| Processor | Two cores, x86-64, from about 2015 (for example a 6th generation Intel Core i3) |
| Memory | 4 GB in total, shared with the operating system and a browser |
| Graphics | Integrated, or none: the program must work with software rendering |
| Storage | A hard disk, not only an SSD |
| Screen | 1366 × 768 at 100% scale |
| System | Windows 10 22H2 or later, or a current Linux distribution |
| Network | None required: the program works offline |

### Budgets for OpenCabinet Designer

| Measure | Budget on the reference computer | Evidence today |
| --- | --- | --- |
| Download of the base installer | 50 MB | Slint prototype executable 12.6 MB |
| Start to the first usable frame | 2 s | Slint prototype 166 ms in CI on Windows |
| Memory with the sample kitchen open | 150 MB | Slint prototype 28 MB in CI on Windows |
| Memory with a 1,000-cabinet project open | 400 MB | Not measured yet |
| Response to an edit (re-evaluate one product, redraw) | 50 ms | Evaluation of a small kitchen in microseconds (ADR-0011) |
| Open or save a 1,000-cabinet project | 1 s | 9 ms in CI on Windows ([format prototype](https://github.com/OpenCabinet/opencabinet/pull/10)) |
| Export a 50-sheet PDF | 5 s | Six pages in under 100 ms, including start-up ([export prototype](https://github.com/OpenCabinet/opencabinet/pull/13)) |

Every budget is about four times or more what the prototypes used, so
features have room to grow, but not without anyone noticing.

### How CI enforces them

From phase 2, a performance job runs on every pull request that touches the
application, on Windows and Linux:

- It builds the release version and measures each budget against fixed
  test projects (the sample kitchen and a generated 1,000-cabinet project).
- CI runners are faster than the reference computer, so **CI checks
  regressions, not the absolute budgets**: a measure fails when it is more
  than 30% worse than on `main`, the noise level seen in the prototypes,
  or when it passes half of its budget. The results are kept as a trend.
- Memory is checked on Windows only; on Linux the virtual display of the
  runner inflates it (ADR-0012).
- Before each release, a maintainer or volunteer runs the same
  measurements on a real reference computer and publishes them with the
  release notes.

3D views and AI features are optional modules with their own budgets, so
they never raise the cost of the base program (plan, principle 5).

### Consequences

- Good, because a shop can check before installing whether its computer is
  enough, and the project can prove it.
- Good, because a pull request that makes the program slower fails in CI
  with a number, instead of in a shop months later.
- Bad, because every new feature must fit the budgets, and some may need
  extra work, such as loading data on demand.
- Bad, because a real reference computer must be available before each
  release; until a volunteer has one, the founder must find one.
- Neutral, because the budgets themselves change only through a new ADR.

## Pros and cons of the options

### Budgets enforced in CI

- Good, because regressions are caught where they are introduced.
- Bad, because performance tests take CI time and need care to be stable.

### Minimum requirements in the documentation only

- Good, because it costs nothing.
- Bad, because nobody checks it until it is no longer true.

### No budgets until users complain

- Good, because it costs nothing now.
- Bad, because the shops that would complain have already stopped using the
  program.

## Sources

- The project plan, principle 5 and the section on distribution and modest
  computers.
- Measurements of ADR-0011 (stack), ADR-0012 (UI toolkit), the format
  prototype
  ([#10](https://github.com/OpenCabinet/opencabinet/pull/10)) and the export
  prototype ([#13](https://github.com/OpenCabinet/opencabinet/pull/13)).
