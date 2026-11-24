# JellyRust

A native **Jellyfin client for LG webOS televisions**, written mostly in Rust, with the look of
[Litefin](https://github.com/MoazSalem/litefin).

JellyRust draws its interface directly on the GPU and hands video to the television's own hardware
decoder, so it is not a web page wrapped in an app. It is maintained by
[macluckylab](https://macluckylab.com) for one household's two LG TVs.

> JellyRust is an independent, unofficial project. It is not affiliated with or endorsed by
> Jellyfin, LG Electronics, Native Jelly, PlxNative or Litefin.

Source: https://github.com/mac-lucky/jellyrust

## What it is

- A Jellyfin client: up to twelve users signed in on one TV, libraries, search, detail pages,
  Continue Watching, favourites, Quick Connect sign-in, direct play where the TV can decode the file
  and server transcoding where it cannot, audio and subtitle track choice, resume.
- The Litefin look: pure black theme, the left rail, card and detail-page layouts and the accent
  colour of Litefin as the household runs it. The look is reproduced from measurements of Litefin's
  source and screenshots; no Litefin code is used (`docs/litefin-look.md`).
- No telemetry: no crash or analytics endpoint can be compiled in, and the only network peer is your
  own Jellyfin server.

## Lineage

```
JellyRust (macluckylab)
  <- Native Jelly by sostk          https://github.com/sostk/native-jelly
       <- PlxNative by Gleb Linnik  https://github.com/GLinnik21/plx-native
```

JellyRust is Native Jelly modified by macluckylab in 2026: rebranded, reskinned to the Litefin
look and with telemetry removed. Native Jelly is itself a Jellyfin port of PlxNative. The people
who wrote the code this builds on are listed in [CONTRIBUTORS.md](CONTRIBUTORS.md).

## Building and installing

Build requirements and the full toolchain setup are in [docs/building.md](docs/building.md).

```bash
make ipk                              # debug flavour: pkg/com.macluckylab.jellyrust.debug_<version>_arm.ipk
make FLAVOR=stable RELEASE=1 ipk      # stable: pkg/com.macluckylab.jellyrust_<version>_arm.ipk
```

The debug flavour installs as its own app beside the stable one, with its own tile, sign-in and
data. Install the `.ipk` on a rooted TV through the Homebrew Channel's installer (its `/install`
call takes an http(s) URL, so serve the file over HTTP), which also registers the storage helper
service (`com.macluckylab.jellyrust[.<flavour>].storage`). Sign in with Quick
Connect: the app logs keys and typed text to `/tmp`, so never type a password into it.

## Licence

GPL-3.0-or-later ([LICENSE](LICENSE), [LICENSING.md](LICENSING.md)). The copyright notices of
Native Jelly and PlxNative are kept as the GPL requires. Third-party components keep their own
licences: [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md) and `licenses/`. Names and marks:
[TRADEMARKS.md](TRADEMARKS.md).
