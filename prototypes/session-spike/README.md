<!--
SPDX-FileCopyrightText: 2026 OpenCabinet contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# Review sessions spike (RFC-0004)

Throwaway. Deleted when RFC-0004 is accepted.

Questions from RFC-0004 §11: can several people mark up at once through
**Automerge**, with a host that relays, the host handing over when it
leaves, and the activity log read from the history?

```sh
cargo run --release
```

Everything runs on this computer over TCP. Not in the spike: TLS, invite
secrets, and a host that pushes; here, participants pull in rounds.

## What it does

1. Andrés hosts a session with one cloud. María and Juan join with empty
   copies and receive it.
2. Without syncing, María adds a callout, recolors the cloud and moves it;
   Juan adds three lengths, recolors the same cloud and edits its text.
3. After syncing, all three copies are the same: five markups; the cloud
   is moved **and** has Juan's text; for the color, one wins the same way
   everywhere and both values stay available, so the window can show the
   conflict.
4. The activity log comes from the history: author, message and time of
   each change.
5. Andrés leaves. María hosts; Juan deletes a length while disconnected,
   joins her and his delete arrives. Andrés's cloud is still there.

## Results

On the founder's computer, release build, 2026-11-21:

| Measure | Result |
| --- | --- |
| 5000 markups in 500 changes, built | 521 ms |
| Saved size | 31 kB |
| Loaded | 32 ms |
| A new participant joins over TCP | 37 ms |

## Findings

1. **Automerge fits.** Concurrent edits to different fields of one markup
   both survive; to the same field, a deterministic winner with the other
   value kept. Nothing blocks.
2. **The log is free.** Each change carries its author and time; the
   message is ours to write ("restyled and moved cloud-1").
3. **Handover works because every copy is whole.** The new host needs
   nothing from the old one.
4. **Setting a value to what it already is records nothing**, so a
   "change" to the same color leaves no trace and no conflict. The log
   only shows real changes, as it should.
5. **Small and quick.** Thousands of markups stay in tens of kilobytes,
   so sending the whole session to a newcomer is cheap; the PDF itself is
   the large transfer.
6. **Still to decide**: authorship here is a name the participant chose
   (the actor id). RFC-0004 §7 needs the host to assign actors to the
   people it admitted, so no one can write as someone else.
