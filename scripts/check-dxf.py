# SPDX-FileCopyrightText: 2026 OpenCabinet contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Audits every DXF in a folder with ezdxf, an independent reader (ADR-0015).

Usage: python -I check-dxf.py <folder> [layer]. Each file must be DXF 2000
in millimeters, have the layer (OUTLINE for parts, WALL for elevations), and
pass the audit with no errors and nothing to repair. Prints a Markdown
table and exits non-zero on any failure.
"""

import sys
from collections import Counter
from pathlib import Path

import ezdxf

MILLIMETERS = 4

folder = Path(sys.argv[1])
required = sys.argv[2] if len(sys.argv) > 2 else "OUTLINE"
files = sorted(folder.glob("*.dxf"))
failures = []
if not files:
    failures.append(f"no DXF files in {folder}")

print("| File | Version | Units | Entities | Audit errors | Audit fixes |")
print("| --- | --- | --- | --- | --- | --- |")
for path in files:
    doc = ezdxf.readfile(path)
    auditor = doc.audit()
    kinds = Counter(e.dxftype() for e in doc.modelspace())
    entities = ", ".join(f"{n} {k}" for k, n in sorted(kinds.items()))
    print(f"| {path.name} | {doc.dxfversion} | {doc.units} | {entities} "
          f"| {len(auditor.errors)} | {len(auditor.fixes)} |")
    if doc.dxfversion != "AC1015":
        failures.append(f"{path.name}: version {doc.dxfversion}, not DXF 2000")
    if doc.units != MILLIMETERS:
        failures.append(f"{path.name}: units {doc.units}, not millimeters")
    if required not in doc.layers:
        failures.append(f"{path.name}: no {required} layer")
    for entry in auditor.errors:
        failures.append(f"{path.name}: error: {entry.message}")
    for entry in auditor.fixes:
        failures.append(f"{path.name}: repaired: {entry.message}")

for failure in failures:
    print(f"FAIL: {failure}", file=sys.stderr)
sys.exit(1 if failures else 0)
