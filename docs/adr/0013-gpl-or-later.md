<!--
SPDX-FileCopyrightText: 2026 OpenCabinet contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# 0013 — Keep "or later" in the GPL and AGPL licenses

- **Status:** accepted
- **Date:** 2026-10-25
- **Deciders:** Andrés (@A-newGP)

## Context and problem statement

[ADR-0002](0002-license-per-component.md) licenses the core and the desktop
modules under GPL-3.0-or-later and server services under AGPL-3.0-or-later,
and marks "or later" as provisional until a later ADR confirms it or
switches to "-only". That switch is only practical before external
contributions arrive: the project uses the DCO without a CLA
([ADR-0004](0004-dco-instead-of-cla.md)), so changing the license afterwards
needs every contributor's agreement.

[ADR-0012](0012-desktop-ui-toolkit.md) then chose Slint, which is licensed
as GPL-3.0-only. Should OpenCabinet's own code keep "or later", or follow
Slint to "-only"?

## Decision drivers

- **Future flexibility:** the project may need to adopt a future GPL
  version, or combine with code under one, decades from now.
- **No CLA:** without a CLA, the license version cannot be changed later
  without finding every contributor.
- **Control over the terms:** "or later" lets users choose a license text
  the project has not read yet.
- **Compatibility** with the dependencies already chosen, Slint in
  particular.

## Considered options

1. **GPL-3.0-or-later** (and AGPL-3.0-or-later)
2. **GPL-3.0-only** (and AGPL-3.0-only)

## Decision outcome

Chosen option: **keep "or later"**, because it is the only way the project
can ever move to a future GPL version without a CLA, and Slint does not
require OpenCabinet's own code to change.

- **Slint does not force "-only" on our code.** A GPL-3.0-or-later program
  may link a GPL-3.0-only library; the distributed program is then under
  GPL-3.0, while every OpenCabinet source file stays "or later". If a GPL
  version 4 appears, our code can follow at once, and the distributed
  program can follow when Slint allows it or is replaced.
- **The risk is bounded.** If the Free Software Foundation published a
  version the project disliked, anyone could already use that version for
  our code; but the GPL requires later versions to be "similar in spirit",
  and version 3 stays available to everyone.

Nothing in the repository changes: the SPDX headers, `LICENSE` and
`LICENSES/` already say "or later".

### Consequences

- Good, because the project keeps a path to future license versions without
  a CLA and without finding every contributor.
- Good, because it follows the common practice of GNU projects and of
  KiCad and GIMP, so contributors meet familiar terms.
- Bad, because the project trusts the Free Software Foundation with the
  terms of future versions of its license.
- Neutral, because while OpenCabinet uses Slint, the program it distributes
  is in practice under GPL-3.0 only (ADR-0012).

## Pros and cons of the options

### GPL-3.0-or-later

- Good, because of future flexibility without a CLA (above).
- Good, because future GPL-4.0-only code could be combined with ours.
- Bad, because users may choose license terms that do not exist yet.

### GPL-3.0-only

- Good, because the terms are fixed and fully known.
- Bad, because moving to a later version would need every contributor's
  agreement, which in practice becomes impossible, as the Linux kernel's
  GPL-2.0-only license shows.
- Bad, because it would require changing every SPDX header now, for no
  present benefit.

## Sources

- GNU General Public License version 3, section 14 ("Revised Versions of
  this License").
- GNU Affero General Public License version 3, sections 13 and 14.
- Free Software Foundation, "Frequently Asked Questions about the GNU
  Licenses": the "or any later version" clause.
- Slint licensing terms (<https://slint.dev/terms-and-conditions>).
