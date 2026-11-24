# JellyRust: rules for this fork

JellyRust is our fork of sostk/native-jelly (Native Jelly, itself a fork of PlxNative),
rebranded and reskinned to look like Litefin as the household runs it. This file holds our rules.
CLAUDE.md, AGENTS.md, `.agents/` and `.claude/` are upstream's: leave them alone beyond the app id
and name they spell, so rebases stay small. Upstream's agent rules still apply to the code: no raw
colour literals, theme::size rungs only, one focus engine, layer gates, and two tests for anything
that animates. Their "owner verdicts" (ALL CAPS marks, no transport buttons, amber marks, nothing
filled by rank) give way where Litefin's look says otherwise.

- Identity: app id `com.macluckylab.jellyrust` (stable), `.debug` and `.nightly` flavours, vendor
  `macluckylab`, title `JellyRust`. The ids live in `ci/install-identities.json` and
  `APPID_STABLE`/`paths::STABLE_APP_ID` (`ci/flavor.py --selftest` checks they agree). The storage
  helper is `<app id>.storage` with DB8 kind `<app id>.storage:1`. DB8 object ids are global on a
  TV, so ours are `jrstate.*`, never upstream's `njstate.*`/`plxstate.*`
  (`build_support/install_identities.rs` refuses those).
- Kept from upstream on purpose, because no user sees them: the `nativejelly` binary and
  `nativejelly-storage` helper names, crate names, `/tmp/nativejelly-*` trigger files, `NJ_*` env
  vars and `tools/nativejelly-*`. `site/` is upstream's nativejelly.com website and is not
  published from here.
- Remotes: `origin` is GitHub `mac-lucky/jellyrust` (public). `upstream` is sostk, fetch only.
  Actions runs upstream's `ci.yml` and `simulators.yml` on every push and PR; `release.yml`,
  `nightly.yml`, `pages.yml` and `ci-metrics.yml` are disabled in the repo settings, not deleted,
  so upstream merges stay small. `release.yml` cannot work here as it stands: it ships with
  `telemetry: production` (which needs upstream's Sentry/PostHog variables) and
  `ci/verify-published.sh` asks `sostk/native-jelly` about the release.
- Branch `main` carries everything and is public: never rewrite or force-push it. Sync upstream with
  `git fetch upstream && git merge upstream/main`, then `make check` and fresh sim screenshots. Expect conflicts where upstream touches the id or name.
- Credits: `CONTRIBUTORS.md` and the About page (`settings.about.body`, all five locales) list the
  maintainer, the upstream authors (from `git shortlog -sn upstream/main`, names only) and the
  Litefin design credit; keep the two in step. The GPL notices of Native Jelly and PlxNative stay.
- Look spec: `docs/litefin-look.md`. Reference shots: `docs/litefin-ref/`, downscaled to
  1920x1080, so the spec's "halve the coordinates" note no longer applies. UI map and slice
  ownership: `docs/litefin-map.md`.
- Telemetry is off in this fork: no endpoint can be compiled in (`telemetry/sender.rs` ignores
  `NJ_SENTRY_DSN`/`NJ_POSTHOG_KEY`), no crash or analytics data is spooled, and plex.tv and Plex's
  metadata provider are never dialled (`catalog/account.rs` loopback guard, `catalog/discover.rs`).
  The only network peer is the household's Jellyfin server.
- Gate: `make check` (machine-wide lock, ~10 min cold). Inner loop: `make test-fast T=<filter>`.
  ARM build: `make`.
- Parallel work runs in lanes, i.e. git worktrees under `~/.cache/nativejelly/lanes/<slice>`, one
  branch per slice; the lead merges them into `main`. Never `make disk-watch` (it installs a
  LaunchAgent).
- Install (debug flavour, its own app beside any other): `make ipk`, then install the ipk with the
  owner's Homebrew Channel installer, which lives outside this repo. Stable,
  once the owner approves: `make FLAVOR=stable RELEASE=1 ipk`. A new id is a new app: it does not
  inherit the sign-in or data of any `com.sostk.nativejelly*` install.
- Never type a password into the app: it logs keys and typed text to `/tmp`. Sign in with Quick
  Connect.
- Sim screenshots against the household server: the owner signs in once through Quick Connect.
  Never press OK on Continue Watching against the live server (it writes watch state). Never read
  or print the `nativejelly-token` file or any stored session.
