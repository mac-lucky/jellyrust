# Contributing

Issues and pull requests are welcome at <https://github.com/mac-lucky/jellyrust>.

- **Bug reports:** include your TV model, webOS version, JellyRust version and Jellyfin server
  version, and whether the file played directly or was converted by the server. Leave out access
  tokens, server addresses and anything about what is being watched. Report security problems
  privately, as described in [SECURITY.md](SECURITY.md).
- **Pull requests:** run `make check` (host unit tests and lint gates) before you open one, and
  keep unrelated changes out of it. Build and toolchain setup: [docs/building.md](docs/building.md).
- **Licence:** contributions are offered under GPL-3.0-or-later, without copyright assignment.
  Keep existing third-party notices, and identify external code, its source revision, licence and
  modifications. Do not submit code whose redistribution rights you cannot establish.
- **Sign-off:** add a `Signed-off-by: Your Name <email>` trailer to each commit, certifying the
  [Developer Certificate of Origin 1.1](https://developercertificate.org/). This records your right to contribute; it is not a
  transfer of copyright.

A problem that is also present in [Native Jelly](https://github.com/sostk/native-jelly) is worth
reporting there too.
