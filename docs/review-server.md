<!--
SPDX-FileCopyrightText: 2026 OpenCabinet contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# Running a Review session server

OpenCabinet Review hosts live review sessions from the desktop of whoever
starts them, which works when everyone is on the same network. For people
in different offices, a **session server** holds the session instead
(RFC-0004 §7).

The server is `opencabinet-review-server`, licensed under the
**GNU Affero General Public License, version 3 or later**: if you run it
for other people, you must offer them its source code, including your
changes.

## Get it

Each pull request that touches the server builds it for Linux and
Windows: in the Actions tab, the **Review server** run keeps it as an
artifact, with this guide, its license and, for Linux, a systemd service.
There is no release of it yet. You can also build it from this
repository:

```sh
cargo build --release -p opencabinet-review-server
```

## Start it

Run it on a machine people can reach, telling it the name or IPv4
address they reach it at:

```sh
opencabinet-review-server --address review.example.org --port 47100 --data /var/lib/opencabinet-review
```

| Option | Meaning | Default |
| --- | --- | --- |
| `--address` | Name or IPv4 address people reach the server at | (needed) |
| `--port` | TCP port to listen on | 47100 |
| `--data` | Folder for the server's certificate and key | `./review-server` |

`opencabinet-review-server --version` says which version it is. It
prints the **server address**, such as
`ocs1:review.example.org:47100:3f2a…`. Give that address to the people
who start sessions. Open the port in the machine's firewall.

## Run it as a service on Linux

`packaging/linux/opencabinet-review-server.service` runs it with systemd
under a user of its own, its certificate kept in
`/var/lib/opencabinet-review-server`, and little else it may touch:

```sh
sudo cp opencabinet-review-server /usr/local/bin/
sudo cp opencabinet-review-server.service /etc/systemd/system/
sudo nano /etc/systemd/system/opencabinet-review-server.service   # your server's name
sudo systemctl daemon-reload
sudo systemctl enable --now opencabinet-review-server
journalctl -u opencabinet-review-server   # the server address
```

## Keep the data folder private

The data folder holds the server's certificate and private key. They are
made the first time and kept, so the server address stays the same
between runs. Whoever has the key can pretend to be the server: keep the
folder readable only by the account running the server. Deleting the
folder makes a new certificate, and a new server address.

## How a session on a server works

1. Someone opens a drawing in Review, opens the Session panel, pastes
   the server address and presses **Start**.
2. Review sends the server the drawing and the session. The server
   makes a room and answers with its secret; Review shows the
   invitation.
3. Everyone else joins with that invitation, as with a desktop host.

The connection is encrypted with TLS. Review checks that the server's
certificate is the one the address names, so no certificate authority is
needed. Only who has the invitation's secret gets into a room.

The server keeps rooms in memory only. A room nobody has been in for an
hour is dropped; restarting the server drops every room. Each person
keeps their own copy of the markups, and saving in Review writes them to
their PDF as usual.
