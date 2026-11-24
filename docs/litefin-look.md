# Litefin look spec (household configuration)

Implementation reference for making JellyRust look like Litefin as the household runs it. Every
value is measured from Litefin's own source (`MoazSalem/litefin`, branch `development`, fetched
2026-11-20) unless marked "(from screenshot)". Litefin is MPL-2.0: this file copies measurements
and design decisions, not code.

Citation format: `file:selector` relative to Litefin's `src/` (for example
`styles/home.css:.media-card`). `LM` means `src/ui/LayoutManager.js`.

Reference images: `docs/litefin-ref/` (described at the end). They are older builds with other
themes and layouts; where they disagree with this file, this file (the current source plus the
household settings) wins.

## 0. What the household actually sees

Household localStorage: an export of Litefin's settings kept outside this repo. Keys it lacks are Litefin defaults.
Effective configuration on both TVs:

| Setting | Value in effect | Source |
|---|---|---|
| Theme mode | `black` (OLED, pure #000000), default; `litefin:themeMode` unset | `LM` constructor and `init()` (`initialMode = THEME_MODES.BLACK`) |
| Accent | `#6b6ede` ("Litefin Purple Blue"), default; `litefin:themeColor` unset | `LM` `DEFAULT_THEME_COLOR` |
| Signature accent | off | `LM` `_signatureAccent = false` |
| Sidebar layout | `modern-collapsed`, default; `pref:sidebarLayoutMode` unset | `LM init()` `defaultSidebarLayout` |
| Sidebar mode | `shown` (rail visible on every page, including details) | `components/Sidebar.js:_updateTransparentCollapsed` |
| Collapsed sidebar colour | `transparent` | same, `pref:collapsedSidebarColor` default |
| Sidebar items align | `center` (vertically centred) | `Sidebar.js:_updateSidebarItemsAlign` |
| Media rows layout | `classic` (household key `pref:mediaRowsLayout`) | settings.json |
| Card size scale (classic) | 1.0 | `pages/HomePage.js` `pref:classicCardSizeScale` |
| Login page layout | `modern` (household key) | settings.json |
| Hero carousel | on, style `immersive-performance`, NOT compact (`pref:heroCarouselCompact=false`), Ken Burns zoom ON (`pref:heroCarouselZoom=true`), dots indicator, 8 s interval, 5 items | `ui/HeroCarousel.js` constructor, `HomePage.js:_loadHeroCarousel` |
| Rounded corners | on (radius 12 / 16) | `LM init()` `litefin:roundedCorners !== 'false'` |
| UI font | `default` = Poppins | `styles/base.css::root --jf-font-family` |
| Text scale | 1.0 (1rem = 16px) | `styles/base.css:html font-size`, `LM setTextScale` |
| Card labels | style `default` (title + subtitle), align `start` (left) | `LM init()` `pref:cardLabelStyle`, `pref:cardLabelAlign` |
| Badge style | `auto` resolves to `tinted` (accent fill) | `LM setBadgeStyle` (`this._layout` stays `classic`) |
| Button style | `theme-default` | `LM init()` |
| Focus border style | `hidden` (no border on focused primary buttons) | `LM init()`, `styles/base.css:html[data-focus-border-style="hidden"]` |
| Hover border style | `white` (pointer only, irrelevant on remote) | `LM init()` |
| Sidebar icon colours | unselected `grey`, selected `white`, logo `icon-colors` | `LM init()` |
| OSD buttons | style follow-global, shape `circle`, unfocused `icon-only`, seek thumb white, progress accent | `LM init()` |
| Player clock / ends-at | 24 h (`player:timeFormat=24h`) | settings.json, `utils/i18n.js:formatLocalTime` |
| Quality badges | off (`pref:showQualityBadges=false`) | settings.json |
| Details dates | added + aired shown (`pref:showAddedDate`, `pref:showDateAired`) | settings.json, `pages/DetailsPage.js` showDates resolution |
| Live TV in My Media | hidden (`pref:hideLiveTvInMyMedia=true`) | settings.json |
| Language | Polish (`app_language=pl`); strings from Litefin `locales/pl.json` | settings.json |
| Layout tier | `modern` (C1 and C5 both run Chromium >= 57) | `utils/PlatformInfo.js` layoutTier |
| SyncPlay, Seerr Discover, MDBList | not present (plugins default off, no Seerr) | `plugins/installed/*/index.js defaultEnabled: false`, `Sidebar.js` discover hidden until probed |

## 1. Pixel ratio: CSS px = screen px

- Litefin's `appinfo.json` has no `resolution` key. LG's appinfo reference: `resolution` defaults to
  `"1920x1080"` and web apps never get UHD. So `window.innerWidth` is 1920 on both TVs.
- `LM _applyDisplayScale()`: on a TV (`platformInfo.isTv`) with `pref:displayScale` unset (`auto`)
  the multiplier is 1.0 and no CSS `zoom` is applied ("1:1 unscaled framebuffer").
- Result: **1 CSS px = 1 screen px at 1920x1080**, 1rem = 16px. Every number below is a 1080p screen
  pixel and can be used directly in JellyRust.
- The six reference PNGs are 3840x2160 (2x device-pixel captures): halve their coordinates. Spot
  checks after halving match the CSS exactly (classic landscape card image 388px wide at x=178,
  details poster 420x630, genre grid pitch 272px).

## 2. Tokens

### 2.1 Colours (black theme, accent #6b6ede)

Accent variants are derived at runtime by `utils/ThemeUtils.js:getAccentVariants` and
`getContrastColor`; the values below are computed with Litefin's own formulas for #6b6ede
(luminance 0.196, so it is a "dark" accent and gets light text).

| Role | Value | Source |
|---|---|---|
| Page background | `#000000` | `LM _applyDynamicTheme` black branch `--jf-background` |
| Background alt / sidebar bg / navbar bg | `#000000` | same, `--jf-background-alt`, `--sidebar-bg`, `--jf-navbar-bg` |
| Surface (card-image placeholder) | `#0a0a0a` | same, `--jf-surface` |
| Card bg | `#080808`; hover `#121212` | same, `--jf-card-bg`, `--jf-card-bg-hover` |
| Divider | `rgba(255,255,255,0.05)` | same, `--jf-divider` |
| Border / border light | `#222222` / `#2a2a2a` | `themes/black.css` |
| Accent | `#6b6ede` (rgb 107,110,222) | `LM DEFAULT_THEME_COLOR` |
| Accent hover (lightness +10) | `#9597e7` | `ThemeUtils.getAccentVariants` |
| Accent active (lightness -10) | `#4145d5` | same |
| Accent light | `rgba(107,110,222,0.2)` | same, `accentLight` |
| Accent dark rgb (tinted panels) | `17,18,34` | same, `accentDarkRgb` |
| Text on accent (`--jf-primary-btn-color`, `--jf-accent-content-color`) | `#d4d5f7` (light lavender) | `ThemeUtils.getContrastColor` (hsl 238.4, s 69.9, l 90) |
| Focus border for generic `.btn`, inputs, episode cards (`--jf-button-border-focus`) | `#d4d5f7` | `LM _applyDynamicTheme` `focusBorderColor` |
| Card focus ring (`--jf-focus-border-color`) | `#6b6ede` | same |
| Primary button focus border | transparent | `styles/base.css:html[data-focus-border-style="hidden"]` |
| Text primary | `#ffffff` | `LM` text palette default |
| Text secondary | `rgba(255,255,255,0.8)` | same |
| Text tertiary | `#666666` | same |
| Text disabled | `#444444` | `themes/black.css` |
| Action button bg / border / text | `rgba(255,255,255,0.10)` / `rgba(255,255,255,0.20)` / `#ffffff` | `themes/black.css --jf-action-btn-*` |
| Button bg / hover | `#222222` / `#2a2a2a` | `LM` black `buttonBg`, `buttonHover` |
| Input bg / border | `#181818` / `#222222` | `LM` black `inputBg`, `inputBorder` |
| Switch track (off) | `rgba(255,255,255,0.15)` | `themes/black.css --jf-switch-bg` |
| Error / success / warning / info | `#f44336` / `#4caf50` / `#ff9800` / `#2196f3` | `themes/black.css` |
| Overlay bg | `rgba(0,0,0,0.95)` | `themes/black.css --jf-overlay-bg` |
| Star rating | `#f5c518` | `styles/details.css:.meta-star`, `ui/HeroCarousel.js` inline |
| Critic (tomato) | `#fa320a` | `styles/details.css:.meta-tomato` |
| Favorite heart | `#ff3b30` | `styles/base.css:.favorite-badge` |
| Logo gradient | `#9765d2` (top-left) to `#436bf1` (bottom-right), diagonal | (from banner.png pixel sample) |
| Sidebar unselected icon | `#bcbebf` | `styles/sidebar.css:html[data-sidebar-unselected-color="grey"] .sidebar-item:not(...)` (beats the 0.45-white modern rule on specificity) |
| Sidebar selected icon | `#ffffff` | `styles/modern/sidebar.css` active icon rules |
| White overlay ramp used everywhere | 0.05 chip/surface, 0.08 hover, 0.10 action bg, 0.14 selected squircle, 0.15 badge, 0.20 border, 0.25 focused-squircle border, 0.28 hero dot, 0.30 seek track | per-section citations below |

There is no blur, glass or gradient surface anywhere in the black theme. Surfaces are flat fills on
#000000.

### 2.2 Typography

- Family: **Poppins**, one file only, `assets/fonts/Poppins.woff2`: Poppins Regular, usWeightClass
  400, no fvar (checked with fontTools). `styles/fonts.css` declares it at weight 400 only.
  Consequence: every `font-weight` of 600/700/800 in Litefin is Chromium's *synthetic bold* of the
  Regular outlines (all the same thickness); 500 renders as Regular. To match, draw "bold" text as
  Poppins Regular emboldened by one synthetic stroke, not Poppins SemiBold/Bold/ExtraBold, which are
  noticeably heavier and narrower-counter than what the household sees.
- Metrics: unitsPerEm 1000, typo ascender 1050, descender -350, line gap 100, USE_TYPO_METRICS set.
  Litefin body `line-height: 1.5` (`styles/base.css:body:not(.player-active)`).
- 1rem = 16px (`styles/base.css:html`).

| Text role | Size px | Weight | Colour / extra | Source |
|---|---|---|---|---|
| Home row title | 32 | 600 | #fff, letter-spacing -0.5px | `styles/home.css:.row-title` |
| Card title | 19.2 | 600 | #fff, 1 line, ellipsis, left aligned | `styles/home.css:.card-title`, `styles/base.css:html[data-card-label-align="start"]` |
| Card subtitle | 16 | 400 | rgba(255,255,255,0.8), 1 line, 6px below title | `styles/home.css:.card-subtitle` |
| Hero text title (only when no logo) | 51.2 | 800 | #fff, letter-spacing -2px, line-height 1, shadow 0 4 12 rgba(0,0,0,0.5) | `styles/hero-carousel.css:.immersive-performance .hero-item-title` |
| Hero meta line | 20.8 | 500 | rgba(255,255,255,0.9) | `styles/hero-carousel.css:.hero-meta-row` |
| Hero rating badge | 17.6 | 700 | on rgba(255,255,255,0.2), radius 4, padding 2x8 | `.hero-meta-badge` |
| Hero synopsis | 20 | 400 | rgba(255,255,255,0.8), line-height 1.5, max 3 lines | `.immersive-performance .hero-description` |
| Global clock (every page) | 22.4 | 500 | #fff at opacity 0.9, letter-spacing 0.05em, shadow 0 2 8 rgba(0,0,0,0.6) | `styles/layout.css:.global-clock` |
| Sidebar focus tooltip | 16.8 | 600 | #d4d5f7 on accent, letter-spacing 0.25px | `styles/modern/sidebar.css` tooltip rule |
| Details title | 56 | 700 | #fff, line-height 1.1, max-width 60% | `styles/details.css:.details-title` |
| Details original title | 28 | 500 | secondary | `.details-original-title` |
| Details meta row | 22.4 | 400 | #fff; ends-at and dates in secondary | `.details-meta-row`, `.meta-ends-at`, `.meta-item-dates` |
| Details tagline | 22.4 | 300 (renders 400) | secondary, italic, opacity 0.9 | `.details-tagline` |
| Details overview | 24 | 400 | secondary, line-height 1.5, clamp 6 lines | `.overview-text`, `utils/Utils.js:getOverviewClampClass` |
| Details action label (floating) | 18.4 | 600 | #fff, letter-spacing 0.02em | `.action-btn-tooltip-text` |
| Tech pill | 20 | 600 | letter-spacing 0.02em | `.tech-pill` |
| Rich-meta label / chip | 19.2 / 17.6 | 500 / 500 | secondary / #fff | `.meta-label`, `.meta-chip` |
| Library page title | 32 | 700 | | `styles/library.css:.library-title` |
| Library tab pill | 20 | 600 | | `styles/library.css:.tab-btn` |
| Library control button | 18 | 600 | | `.control-btn` |
| Alphabet letter | 18 | 600 | secondary | `.alpha-btn` |
| Library genre row header | 30 | 800 | | `styles/library_rows.css:.header-focusable` |
| Settings page title | 40 | 300 (renders 400) | letter-spacing -0.5px | `styles/settings.css:.content-title` |
| Settings section title | 24 | 600 | secondary, uppercase, letter-spacing 1.5px | `.setting-section-title` |
| Settings item name / description | 22.4 / 16 | 500 / 400 | #fff / secondary, line-height 1.4 | `.setting-name`, `.setting-description` |
| Settings left menu | 18.88 | 500 (active 600) | secondary | `.settings-menu-btn` |
| OSD title | 28.8 | bold | shadow 0 2 4 rgba(0,0,0,0.8), line-height 1.5, max 60vw | `styles/player-osd.css:.osd-title` |
| OSD clock | 22.4 | 500 | opacity 0.9, letter-spacing 0.05em | `.osd-clock` |
| OSD times / ends-at | 19.2 | bold | tabular figures; ends-at opacity 0.9 | `.osd-time`, `.osd-ends-at` |
| Track menu title / option | 24.96 / 19.2 | bold | | `.track-menu-title`, `.track-option` |
| Login brand / section title | 80 / 51.2 | 700 | letter-spacing -2px on brand | `styles/modern/core.css:.login-logo`, `.section-title` |
| Quick Connect code | 96 | 700 | accent, letter-spacing 0.18em | `styles/login.css:.quick-connect-code` |

### 2.3 Radii

| Use | px | Source |
|---|---|---|
| `--jf-border-radius` (cards, setting rows, chips small) | 12 | `styles/base.css:html[data-rounded-corners="true"]` |
| `--jf-border-radius-lg` (buttons, pills, badges, menus) | 16 | same |
| Card image inner (image clip) | 9 (12 - 3px ring) | `styles/home.css:.media-card .card-image img` |
| Sidebar squircle | 18 (56x56) | `styles/modern/sidebar.css:.item-icon::before` |
| Sidebar tooltip pill | 12 | modern sidebar tooltip rule |
| Libraries popover / its rows | 18 / 12 | `styles/modern/sidebar.css` section 9 |
| Alphabet bar | 36 | `styles/library.css:.alpha-picker` |
| Toggle | 34 (full pill) | `styles/settings.css:.toggle-switch` |
| Login buttons / inputs | 14 | `styles/modern/users.css`, `manual.css` |
| Hero meta badge | 4 | `.hero-meta-badge` |
| Round controls (details icon buttons, OSD buttons, avatars) | 50% | `styles/details.css:.btn-icon`, `styles/base.css:html[data-osd-button-shape="circle"]` |

`styles/details.css` re-declares `:root` radius 8/12 but loses to the rounded-corners rule on
specificity; 12/16 stays in effect.

### 2.4 Spacing scale

`--jf-spacing-xs..xl` = 4, 8, 16, 24, 32 (`themes/black.css`). Recurring layout constants:

| Constant | px | Source |
|---|---|---|
| Sidebar rail width (visible) | 100 | `styles/sidebar.css::root --sidebar-width-collapsed` |
| Page left edge (content starts) | 100 | `styles/layout.css:#page-container left` |
| Row side padding (title and track) | 60 | `styles/home.css:.row-title`, `components/VirtualCardRow.js` `sidePadding` |
| Classic card margin left/right | 12 + 12 (24 gap) | `styles/home.css:.media-card` |
| Row items padding | 24 top, 38 bottom | `styles/layout.css:.row-items` |
| Row title to items | 14 | `.row-title margin-bottom` |
| Row to row | 28 | `styles/home.css:.media-row` |
| Library page padding | 40/50 header, 20 50 40 50 content | `styles/library.css:.library-header`, `.library-content` |
| Library grid gap | 20 | `styles/library.css:.library-grid.mode-dynamic` |
| Details split padding | 60 (top 5% of width = 91) | `styles/details.css:.details-main-split` |
| Vertical scroll focus offset | row snaps to 40 below the top | `ui/ScrollController.js DEFAULT_SCROLL_OFFSET_TOP` |

### 2.5 Motion

| What | Duration and curve | Source |
|---|---|---|
| Spring overshoot (buttons, pills, OSD, tooltips) | 180 ms `cubic-bezier(0.34,1.56,0.64,1)` | `styles/components.css:.btn`, many |
| Card focus scale (home.css value) | 180 ms same overshoot | `styles/home.css:.media-card` |
| Card focus scale (effective after chunk prewarm, see 2.6) | 200 ms `cubic-bezier(0.25,0.1,0.25,1)` (CSS `ease`) | `styles/details.css:.media-card` |
| Sidebar squircle / tooltip pop | 250 ms overshoot; colour 200-250 ms `cubic-bezier(0.2,0,0,1)` | `styles/modern/sidebar.css` |
| Row horizontal scroll | spring physics (default), about 160 ms on TV; CSS fallback 150 ms `cubic-bezier(0.25,0.46,0.45,0.94)` | `ui/SpringPhysics.js getHorizontalDurationMs`, `styles/layout.css:.row-items-track` |
| Vertical scroll | 240 ms on TV (`smooth`) | `ui/ScrollController.js getBaseVerticalDuration` |
| Image fade in | 150 ms ease-out (cards), 300 ms (details poster), 1 s (details backdrop) | `home.css`, `details.css` |
| Hero crossfade | 1000 ms `cubic-bezier(0.4,0,0.2,1)` | `styles/hero-carousel.css:.hero-item` |
| Hero text entry | translateY 20 to 0 over 800 ms overshoot, opacity 800 ms ease | `.hero-content` |
| Hero Ken Burns | backdrop scale 1.00 to 1.08, 12 s linear, forwards, active slide only | `.with-zoom .hero-item.active .hero-backdrop` |
| Hero auto advance | 8 s | `ui/HeroCarousel.js` `_autoScrollInterval` |
| OSD fade / auto hide | 300 ms; hides after 3.5 s idle | `styles/player-osd.css:.osd-main`, `player/osd/OSDController.js autoHideDelay` |
| Card title marquee on focus | alternate loop, 4 s, holds 20% at each end, only when text overflows | `styles/components.css` card-text-marquee |

### 2.6 Focus treatment

There is no glow and no drop shadow on focus anywhere. Focus = scale + accent fill (controls) or
scale + accent ring (artwork).

- **Media cards** (`styles/home.css:.media-card.focused`, `.media-card.focused .card-image`):
  - The `.card-image` carries a permanent 3px transparent border; on focus it becomes 3px `#6b6ede`.
    The image is inset by that ring (inner radius 9).
  - Scale: home.css says 1.05 with the overshoot curve. But `styles/library.css` has an unscoped
    `.media-card.focused { scale(1.02) }` and `styles/details.css` an unscoped `.media-card`
    transition; both files are lazy chunks that `core/App.js` prewarms after 4 s of idle on Home, and
    style-loader appends them after home.css, so they win on equal specificity. Effective on the TV
    after the first idle: **scale 1.02, 200 ms ease**. (Cascade analysis, not verified on device.)
    Implement 1.02 everywhere; 1.05 is only what a cold Home shows in its first seconds.
  - Card origin is the card centre (no transform-origin set).
- **Primary buttons** (details Play, focused tabs, controls, chips, settings menu, OSD): fill becomes
  accent `#6b6ede`, text/icon becomes `#d4d5f7`, border transparent (focus border style hidden),
  scale per control: details icon buttons 1.15, library tabs/controls 1.08, alphabet letters 1.2,
  meta chips 1.1, settings menu 1.05, OSD buttons 1.08 (icon inside a further 1.12), sidebar
  squircle 1.08. Sources in each section.
- **Generic `.btn`, text inputs, episode cards**: border becomes `#d4d5f7`
  (`--jf-button-border-focus`), width 3px (`.btn`, `.text-input`) or 4px (`.episode-card`).
- **Rows of settings**: focused row gets `rgba(107,110,222,0.15)` fill, no scale
  (`styles/settings.css:.setting-item.focused-row`).
- **Hero**: when focused, an accent band fades in over the hero: vertical gradient transparent 0-5%,
  accent at 20%, transparent at 100% (bottom to top), whole overlay at opacity 0.15, 300 ms
  (`styles/hero-carousel.css:.hero-carousel-container::after`).

## 3. Sidebar (modern-collapsed)

Geometry (`styles/sidebar.css:.sidebar`, `styles/modern/sidebar.css` section 8):

- The element is 350px wide, translated -250px and locked there in every state, so only a **100px
  rail** at x 0..100 is visible. It never expands; labels appear as a tooltip instead.
- Background: transparent (`pref:collapsedSidebarColor` default `transparent`), so the black page
  and the hero artwork show through. No border, no shadow.
- Padding 20 top/bottom. Order top to bottom:
  1. Logo header: 80 tall + 20 margin below; the logo is the colour PNG `assets/icon-130.png` drawn
     at 40x40, centred at x=50, y=60 (`.sidebar-header`, `.logo-icon`, `.sidebar-logo-img`).
  2. Nav items, each 72 tall, icon column 100 wide, icon 28x28 centred at x=50
     (`.sidebar-item`, `.item-icon`, `.item-icon svg`). The block is vertically centred in the space
     between header and footer (`.sidebar.align-center .sidebar-content`).
  3. Footer pinned to the bottom with 20 padding: the user avatar item (72 tall), avatar a 40x40
     circle centred at x=50; on focus a 2px `#d4d5f7` ring (`.sidebar-avatar`).
- Items for the household, in order (`utils/SidebarLayoutManager.js MODERN_STATIC_ITEMS`,
  `components/Sidebar.js:render`): **Home, Favorites, Search, Random, Libraries, Settings**.
  Discover appears only with Seerr, SyncPlay only with its plugin, Live TV is hidden by default.
  With 6 items the block is 432 tall; centred between y=120 and y=968 it starts at y=328 (computed).
- Icons: Remix Icon set (Apache-2.0), outline variant when idle, filled variant when active or
  focused (`utils/Icons.js` `sidebarIcons`, `styles/sidebar.css:.item-icon .icon-outline/.icon-filled`).
  Home = house, Favorites = heart, Search = magnifier, Random = square die with pips, Libraries =
  books, Settings = gear (from the SVG paths; exact Remix glyph names not confirmed).
- States (all on a 56x56 squircle, radius 18, centred behind the icon, `.item-icon::before`):
  - Idle: no squircle, icon `#bcbebf`.
  - Active (current page): squircle `rgba(255,255,255,0.14)` with 1px `rgba(255,255,255,0.08)`
    border, icon `#ffffff` filled.
  - Focused: squircle `#6b6ede` with 1px `rgba(255,255,255,0.25)` border, icon `#d4d5f7` filled,
    icon and squircle scale 1.08 (250 ms overshoot).
  - Focused also shows the **label tooltip**: a pill whose left edge is at screen x=92 (sidebar-local
    342 minus the 250 translation), vertically centred on the item, padding 8x18, radius 12, bg
    `#6b6ede`, 1px `rgba(255,255,255,0.25)` border, text 16.8px weight 600 `#d4d5f7`, max width 300,
    one line ellipsis. It animates from scale 0.9 / opacity 0 to 1 (200 ms opacity, 250 ms
    overshoot). Hidden when not focused.
- Libraries item opens a floating popover to the right (`styles/modern/sidebar.css` section 9): left
  edge at screen x=92, width 270, bg `#18191c`, 1px `rgba(255,255,255,0.18)` border, radius 18,
  padding 8; rows 48 tall, radius 12, padding 0 14, 28px icon + 12 gap, text 16.8px weight 500
  `rgba(255,255,255,0.9)`; active row `rgba(255,255,255,0.1)` weight 600; focused row accent fill,
  `#d4d5f7` text, scale 1.02. Opens with scale 0.92 to 1 overshoot.

## 4. Home

Layout trick (`styles/hero-carousel.css` "Expanded Sidebar Overlap"): the home page is pulled left by
100 under the transparent rail, the hero spans the full 1920 width, and the rows get 100 of left
padding back. Rows therefore start at x=100 like every other page.

### 4.1 Hero carousel (immersive-performance, full height, zoom on)

- Height 972, full width, clipped (`.immersive-performance .hero-carousel`, `:root
  --hero-carousel-height-immersive`).
- 5 random Movie/Series items that have a backdrop (`HomePage.js:_loadHeroCarousel`).
- Backdrop: an image box anchored to the right, **65% of the width (x 672..1920)**, full height,
  cover, positioned top (`.immersive-performance .hero-backdrop`, `.hero-backdrop-img`).
  - Horizontal alpha mask, left to right: 0 at 0%, 0.02 at 8%, 0.10 at 18%, 0.26 at 30%, 0.48 at
    44%, 0.70 at 58%, 0.86 at 70%, 0.96 at 82%, 1.0 from 92%.
  - Vertical alpha mask on the whole slide, top to bottom: 1.0 until 35%, 0.96 at 48%, 0.75 at 62%,
    0.38 at 76%, 0.08 at 88%, 0 at 96% (`.immersive-performance:not(.compact) .hero-item`).
  - Masks fade into the black page; there is no separate scrim colour. The left vignette and the
    bottom gradient pseudo-elements of other hero styles are disabled here, so the
    `pref:heroCarouselVignette` level (default 70) has no visible effect for this household.
  - Ken Burns: the backdrop scales 1.00 to 1.08 over 12 s linear while its slide is active.
- Content block (`.hero-content`, `.immersive-performance .hero-item`):
  - Left edge x=180 (100 rail compensation + 80 padding), max width 1600 (1440 inner).
  - Bottom-anchored: the block's bottom is at y=502 (972 - 470 padding).
  - Stack, top to bottom: logo box 130 tall (logo image max 400x130, bottom-aligned, `contain`),
    16 gap; meta line 20.8px (items 16 apart: year, runtime `1h 52m` or `52m`, rating badge, star
    `#f5c518` + one decimal); 16 gap; synopsis 20px, max 3 lines, max width 70% (1008px); 20 gap.
    Without a logo the title text (51.2px weight 800) replaces the logo box with 12 below.
- Dots (`.indicator-dots`): 8px circles, gap 10, idle `rgba(255,255,255,0.28)`, active `#ffffff`
  (no pill, no progress fill), right edge 80 from the screen edge, bottom 430 (so y 534..542).
- Focused hero: accent band at 0.15 opacity (2.6).

### 4.2 Rows (classic)

- Rows start under the hero with a 450px overlap: `.home-rows` top at y=522 (972 - 450), 24 top
  padding, so the first row title starts at y=546 (`#home-hero-placeholder.style-immersive-performance
  + .home-rows`, `.home-rows`).
- Row title: 32px weight 600, x=160 (100 + 60), letter-spacing -0.5, line box 48, 14 below.
- Track: cards absolutely placed at `60 + i * pitch` inside the row, plus the card's 12 margin, so
  the first card box starts at x=172 (`VirtualCardRow.js` leftPos, `home.css:.media-card`).
- Card box sizes (`.media-card`, `.card-image`, `VirtualCardRow.js` classic branch). Each card has a
  3px transparent border on the card and another 3px ring on the image (the focus ring), so the
  visible image is 12 narrower than the card box:

| Card | Box width | Pitch | Visible image | Used for |
|---|---|---|---|---|
| Poster 2:3 | 240 | 264 | 228 x 351 | Latest in movies / TV libraries, cast, similar, seasons |
| Landscape 16:9 | 400 | 424 | 388 x 221.6 | My Media, Continue Watching, Next Up, home videos, music videos |
| Square 1:1 | 240 | 264 | 228 x 234 | Latest in music and Live TV |

  (The image box height is a percentage of the image box's outer width, so the inner image is a
  hair taller than an exact ratio; copy the numbers above.)
- Card image background while loading: `#0a0a0a` (`--jf-surface`). Images fade in 150 ms.
- Labels under the image (`.media-card .card-info`): padding 12 top, 4 sides; title 19.2/600 white;
  subtitle 16/400 at 0.8 white, 6 below; both one line with ellipsis and **left aligned**; overflow
  marquee on focus. Subtitle content: movies show the year; resume/next-up episodes show
  `S01E01 - Episode name` (from screenshot sidebar.png); cast shows the role.
  Library tiles (My Media) show the library's own Jellyfin artwork and only the title below.
- Progress (`styles/components.css:.card-progress-container`, `progress.card-progress`): 6px bar at
  the very bottom inside the image, track `rgba(0,0,0,0.5)`, fill a left-to-right gradient
  `#6b6ede` to `#9597e7`, clipped by the image radius.
- Badges (`styles/base.css`, tinted skin `html[data-badge-style="tinted"]`): all filled `#6b6ede`,
  text/icon `#d4d5f7`, no border, radius 16, 40 tall.
  - Unplayed count: top-right, 12 inset, min width 40, padding 0 8, 17.6px weight 700.
  - Episode code / rank: top-left, 12 inset.
  - Watched check: bottom-right 40x40, 12 inset; favourite heart bottom-left 40x40. With a progress
    bar the bottom badges move up to bottom 18.
- Spacing: row items 24 padding above the cards (room for the focus pop) and 38 below the labels;
  28 between rows.
- Scrolling: horizontally the focused card is centred in the row, clamped at both ends (no scroll
  if the row fits); vertically the focused row snaps to 40 below the top of the screen
  (`ui/ScrollController.js`).
- **Household row order** (`HomePage.js` descriptors, no `pref:homeLayout` saved, no Litefin server
  plugin so resume and next-up stay separate, trending and recommendation rows off by default):
  1. My Media (`HeaderMyMedia`), landscape, Live TV excluded.
  2. Continue Watching (`HeaderContinueWatching`), landscape.
  3. Next Up (`NextUp`), landscape.
  4. One "Latest" row per library in the server's view order (`LatestFromLibrary`, Polish pattern
     is library name, space, em dash, space, "ostatnio dodane"), card type per library type as in
     the table above.
  Up to 12 items per row (`pref:homeRowsLimit` default).
- Global clock: top 32, right 50, 22.4px weight 500, 24 h `HH:MM` (zero padded), on every page
  including Home and Details (`styles/layout.css:.global-clock`, `ui/GlobalClock.js`).

## 5. Details (layout posterLeft, title style both)

- Backdrop: fixed, full screen 1920x1080 (under the transparent rail), cover, centred, fades in
  over 1 s (`styles/details.css:.details-backdrop`). Gradient over it, top to bottom: black at 0.35
  (0%), 0.60 (45%), 0.90 (75%), solid #000 from 88% (`.backdrop-gradient`).
- Split (`.details-main-split`): padding 60 sides, top 5% of the page width (91), 40 below. Content
  starts at x=160.
- Poster (`.hero-poster`): 420 x 630 (landscape items 420 x 236, square 420 x 420), radius 12, 40
  gap to the info column. Loading fill `#080808`.
- Info column, top to bottom (`pages/DetailsPage.js` hero markup, `details.css`):
  - Logo, top-right corner of the info column, box 280 x 100 (max 40% width), right-aligned,
    `contain`, fades in 1 s (`.details-logo`).
  - Title 56/700, line-height 1.1, max width 60%, 12 below (`.details-title`). Original title (when
    different) 28/500 secondary.
  - Episode pages: series/episode line 28.8px in accent (`.details-episode-info`).
  - Meta row, 22.4px, items 20 apart, 16 below (`.details-meta-row`): year, runtime (`2h 37m`),
    official rating as a badge (rgba(255,255,255,0.15), padding 4x10, radius 12, 19.2/600 `#e0e0e0`),
    star rating `#f5c518`, critic score `#fa320a` with tomato icon, `Ends at HH:MM` (secondary,
    Polish `EndsAtValue`), then added/aired dates (secondary).
  - Tech pills row (shown by default, `.tech-pill`): padding 4x12, radius 12,
    `rgba(255,255,255,0.09)` fill with 1px `rgba(255,255,255,0.16)` border, 20/600, 10 apart; HDR
    and Dolby Vision pills use warm and violet tinted gradients (`.tech-pill-hdr`, `.tech-pill-dovi`).
  - Actions row, 44 below (`.details-actions`): all **72x72 circles**, 16 apart: Play (icon only,
    28px icon), [Resume Playback as a 72-tall pill radius 36, padding 0 22, when there is progress],
    [previous/next episode], [reset], [trailer], [shuffle], Watched, Favorite, Audio, Subtitles,
    More. Idle: `rgba(255,255,255,0.10)` fill, 2px `rgba(255,255,255,0.20)` border, white 28px icon
    (outline variant). Focused: accent fill, `#d4d5f7` icon (filled variant), border transparent,
    scale 1.15 (Play/Resume 1.05). Active toggles (watched, favourite) idle: icon in accent.
    The focused button's name floats 18 below it, centred, 18.4/600 white, no background
    (`.action-btn-tooltip-bar.style-floating`, default `floating` tooltips).
  - Tagline: 22.4 italic secondary, 12 below. Overview: 24px secondary, line-height 1.5, clamp 6
    lines, then a small "Show more" pill (17.6/600, padding 6x12, radius 16, action fill).
- Rich metadata table (`.details-rich-meta`, style genres-studios-writers): margin 10 60 50 60,
  padding 0 20; rows padding 6 0 with a 1px `rgba(255,255,255,0.05)` divider; label column 120
  wide 19.2/500 secondary; chips padding 6x10, radius 16, `rgba(255,255,255,0.05)` fill, 2px
  `rgba(255,255,255,0.1)` border, 17.6/500, 4 apart; focused chip accent fill, `#d4d5f7` text,
  scale 1.1. When the table has focus its outline turns accent and it gets an
  `rgba(17,18,34,0.65)` fill.
- Rows below use the Home row style (32px titles at 60 padding, classic cards): Next Up / More from
  Season, Seasons (poster 240), Cast & Crew (poster 240 with name + role), Special Features,
  Collections, More Like This. 40 below each section (`.seasons-row`, `.similar-row`).
- Season page episodes, list layout (`styles/season.css`, `pref:episodeLayout` default `list`): list
  padding 0 60 80, 20 top; each row 250 tall, 24 apart; card fill `rgba(17,18,34,0.65)` (0.85
  focused), radius 12; thumbnail 390 wide full height on the left; text block padding 18x24:
  index 24/700, title 22.4/600 one line, meta 19.2 secondary (rating `#f5c518`), overview 20.8
  secondary at opacity 0.8, 4 lines. Focused: scale 1.02 (200 ms) and a 3px accent ring.
  Round 64px action buttons at the right (accent on focus, `#d4d5f7` border).

## 6. Library and genres

- Header (`styles/library.css:.library-header`): padding 40 50 0; title 32/700 at x=150.
- Tabs (`.library-tabs`, `.tab-btn`): centred row, pills padding 12x28, radius 16, 20/600, 20 apart,
  idle action fill + 2px border; **active tab: `#d4d5f7` fill with accent text**; focused: accent
  fill, `#d4d5f7` text, scale 1.08.
- Controls row, centred (`.library-controls`, `.control-btn`, `.count-indicator`): item count 14/500
  secondary; buttons Shuffle, Sort, Filter, View Mode: padding 12x18, radius 16, 18/600, 20 apart,
  icon + label; focus as tabs.
- Alphabet bar at the top (`pref:alphaPickerPosition` default `top`, `.alpha-picker`, `.alpha-btn`):
  one centred pill, `rgba(255,255,255,0.05)`, radius 36, padding 12x24; letters `#` and A..Z, 38x38,
  8 apart, 18/600 secondary; focused letter accent fill, `#d4d5f7`, scale 1.2; the letter currently
  in view is accent-coloured text, bold, scale 1.2.
- Grid (`pages/LibraryPage.js` dynamic grid, `.library-grid.mode-dynamic`): default view `poster`
  with **7 columns**; content padding 20 50 40 50, gap 20. Card box width
  (1720 + 20) / 7 - 20 = 228.6, pitch 248.6, first card at x=150, visible poster about 216.6 x 334.
  Same card anatomy, labels (title + year) and focus as Home rows. Thumb view 4 columns, small
  poster 9, banner 3 (`_getDefaultColumnsForMode`).
- Genres tab (`styles/library_rows.css`): vertical list of rows 28 apart; each row header is a
  focusable title 30/800, padding 6x16, radius 12; focused header: `rgba(255,255,255,0.15)` fill,
  3px accent border, scale 1.08 from the left. Row cards 240 wide, 20 apart, row padding 15 20 25.
  Genre tiles (when shown as tiles) are artwork with a `rgba(0,0,0,0.35)` tint (0.25 focused) and a
  centred label 35.2/800 with a soft black shadow (`styles/home.css:.card-overlay-label`).
- Genre drill-in grid (genres.png, `.genre-grid-items`): columns of 240, gap 32, pitch 272, title +
  year under each poster.

## 7. Search

- Top bar (`styles/search.css:.search-controls`): padding 48 60 0, 40 below.
- Input (`.search-input`): full width, 72 tall, padding 0 32, 22.4px, action fill
  `rgba(255,255,255,0.10)`, 2px `rgba(255,255,255,0.20)` border, radius 16; focused border accent.
  Typing uses the webOS system keyboard.
- Empty state: centred 80px outline icon and text (`.search-empty`).
- Results: Home-style rows (32px titles, classic cards), 32 between rows (`.search-results
  .media-row`).

## 8. Login (modern layout)

Two panes, full screen (`styles/modern/core.css`):

- Left brand pane: 30vw (576) wide, padding 80, content bottom-aligned, bg `#000000`
  (background-alt), 1px `rgba(255,255,255,0.05)` right border. Logo 70x70 with 20 gap, brand word
  "Litefin" 80/700 letter-spacing -2, tagline 22.4 at opacity 0.5, max 500 wide.
- Right pane: padding 40 80, vertically centred; section title 51.2/700, 8 below; labels 19.2/600.
- User picker (`styles/modern/users.css`): items 215 wide with 20 margin; avatar circle 208 (image
  200) on `#0a0a0a`; focused: 4px accent ring, scale 1.08, 300 ms `cubic-bezier(0.175,0.885,0.32,1.275)`;
  name 25.6/600 at 0.8 white (white when focused), 24 below.
- Buttons 72 tall, radius 14: Quick Connect accent fill with white 25.6/600 text (focused: `#d4d5f7`
  fill, accent text, 4px `#d4d5f7` border, scale 1.02); secondary buttons `#0a0a0a` with secondary text (focused:
  `#121212`, accent 4px border, white text, scale 1.02). Inputs `#0a0a0a`, radius 14, padding
  21 21 21 69 with a 28px icon at 24, 25.6px; focused accent 4px border, scale 1.01
  (`styles/modern/manual.css`).
- Quick Connect screen (`styles/login.css`): centred column max 700; instructions 20.8 tertiary,
  48 below; code 96/700 accent, letter-spacing 0.18em; status 17.6 tertiary pulsing opacity 0.5..1
  over 2.5 s.

## 9. Settings

`styles/settings.css`, categories in order (`pages/settings/SettingsPage.js`): Display, Layout, Home,
Details, Sidebar, OSD, Player, Subtitles, Performance, Controls, Plugins, Account, Backup, About,
Debug; admins also get Libraries, inserted before Backup.

- Left category list: 380 wide, bg `#000000`, 1px `rgba(255,255,255,0.05)` borders, padding 40 0.
  Buttons padding 13x20, margin 3x16, radius 12, 18.88/500 secondary, icon outline/filled.
  Active: `#d4d5f7` fill, accent text, 600. Focused: accent fill, `#d4d5f7` text, scale 1.05.
- Content panel: padding 40 60. Title 40/300, letter-spacing -0.5, 20 below; subtitle 15.2
  secondary with a 2px divider. Section titles 24/600 uppercase, letter-spacing 1.5, secondary,
  2px divider, 20 above and below.
- Setting row: padding 24, min height 80, radius 12, 8 apart; name 22.4/500, description 16
  secondary; focused row `rgba(107,110,222,0.15)`.
- Toggle: 60 x 34 pill, 2px `rgba(255,255,255,0.20)` border; off track `rgba(255,255,255,0.15)`
  with a 24px white knob at 3px; on track accent with the knob moved 26 and coloured `#d4d5f7`;
  focused scale 1.05.

## 10. Player OSD

All em values resolve against 16px (`.player-osd` sets no font size). Sources:
`styles/player-osd.css` (wins over `styles/player-page.css` by load order) and
`player/osd/OSDController.js` markup; defaults from `utils/PlayerSettings.js`.

- Shows on key press, fades 300 ms, hides after 3.5 s.
- Header: padding 38.4 38.4 76.8, gradient black (opacity 0.75 of `rgba(0,0,0,1)`) to transparent
  downward. Left: title "Name (Year)" 28.8 bold (+ secondary line for episodes); text title, not
  logo (`osdShowLogo` false). No back button on TV (`osdHideBackButton`). Right: clock 22.4/500,
  24 h.
- Bottom area: padding 160 38.4 48, gradient black 0.75 to transparent upward.
- Controls row (buttons above the seek bar, `osdButtonsLocation` above, `osdLayout` left):
  - Left group, 11.52 apart: previous item, previous chapter, rewind (10 s), **play/pause 86.4 circle
    with 48 icon (11.52 extra margin each side)**, forward (10 s), next chapter, next item. Chapter
    buttons sit disabled at opacity 0.3 when there are no chapters.
  - Then `Koniec o HH:MM` / "Ends at" text, 22.4 after the group, 19.2 bold, opacity 0.9.
  - Spacer, then the right group, 11.52 apart: Subtitles (CC), Audio, Chapters, Queue, Settings.
    Favorite and Info are hidden by default on TV (`osdHideFavorite`, `osdHideInfo`); SyncPlay hidden
    without its plugin.
  - Buttons: 58x58 circles, 33.6px white icons, transparent when idle (icon-only). Focused: accent
    fill, `#d4d5f7` icon, border transparent, scale 1.08 and the icon a further 1.12. An "active"
    toggle shows its icon in accent.
- Seek row, below the controls: current time left, total right, 19.2 bold tabular, padding 0 9.6,
  min width 80, 36 tall; times `MM:SS`, or `H:MM:SS` once the media is an hour or longer. Track 8px,
  `rgba(255,255,255,0.3)`, radius 12, 20 margin to the times; fill accent; thumb 8x8 white, on
  focus a 10x28 white bar. Chapter ticks 4 x 13 in the track colour. Seek tooltip above the thumb
  (14 gap): black, 2px `rgba(255,255,255,0.2)` border, radius 16, 26.88 bold, trickplay thumbnail
  inside when available (radius 6).
- Track menus (audio, subtitles, `.track-menu-overlay`): full-screen dim `rgba(0,0,0,0.85)`
  (`osdTrackMenuBgOpacity` 85), list centred, min width 750, max 90vw x 70vh, padding 28.8; title
  24.96 bold centred, 19.2 below; options padding 15.4 x 23, radius 16, 19.2 bold, a 28.8 check
  column (check visible on the selected option); focused option accent fill, `#d4d5f7` text. Badges
  inside options 13.6/700 uppercase, padding 2x14, radius 6, `rgba(255,255,255,0.12)` with 1px 0.2
  border.
- Up Next card (`styles/upnext.css`, style `normal`, scale 1.0, base 20px): bottom 270, right 48,
  max width 600; bg `rgba(12,12,18,0.92)`, 1px `rgba(255,255,255,0.15)`, radius 16; slides up 15
  and fades in (280 ms opacity, 320 ms overshoot). Row padding 28 28 12: thumbnail 150 x 85 radius
  12 with episode badge; label 14/700 uppercase accent, letter-spacing 0.12em; title 20/700 two
  lines; countdown 16.4/500 at 0.55 white. Buttons row padding 16 28 28, right-aligned, 16 apart:
  padding 11.2 x 24, radius 12, 18.4/700, `rgba(255,255,255,0.10)` with a 2.5px border; focused play
  button accent fill, `#d4d5f7` text; focused hide button 0.22 white; both scale 1.05.
- Intro/credits skip: "AskToSkip" button (intro-skipper is present on the server); styled like the
  Up Next button style (`plugins/installed/skip-intro/skip-intro.css`, not measured in detail).

## 11. What differs from the PlxNative-style UI

1. **Navigation is a left rail, not a top bar.** A transparent 100px icon rail on every page,
   content starts at x=100, labels only as an accent tooltip pill next to the focused icon.
2. **No glass.** No blur, no live-backdrop panels, no frosted bars. Pure #000000 canvas, flat
   white-alpha fills (0.05 to 0.15), and the hero fades into black through alpha masks.
3. **Focus is scale plus a flat 3px accent ring (artwork) or an accent fill (controls), never a
   glow or shadow.** Cards pop only 1.02; controls pop 1.05 to 1.2 with a spring overshoot.
4. **One accent, derived.** `#6b6ede` with light-lavender `#d4d5f7` as the text/icon colour on
   accent, and the same lavender as the "active tab" fill. Badges are accent-filled.
5. **Smaller, single-weight type.** Poppins Regular with synthetic bold; card titles 19.2 and
   subtitles 16, below the PlxNative 24px caption floor. Labels are left-aligned under the image,
   one line each, marquee on focus.
6. **Rounded 12/16 everywhere**, 50% circles for every icon button (details actions are 72px icon
   circles with the label floating below the focused one; OSD buttons are icon-only circles).
7. **Hero is a right-aligned 65% backdrop with Ken Burns**, dots bottom-right, and the rows overlap
   the lower 450px of it; no separate scrim layer.
8. **Rows are uniform classic shelves**: 240 poster / 400 landscape boxes on a 264 / 424 pitch, row
   titles 32/600 at x=160, focused card centred horizontally, focused row snapped 40 from the top.
9. **Global clock** top-right on every screen, 24 h.

## 12. Reference images (`docs/litefin-ref/`)

All PNGs are 3840x2160 (halve coordinates); `readme-hero.jpg` is 1920x1080. None shows the exact
household combination (black theme, modern-collapsed rail, classic rows, immersive hero).

- `home.png`: older build, purple tinted theme, classic collapsed rail with logo, SyncPlay, avatar,
  Home (accent), Random, Favorites, Search, Settings; a *banner* hero (rounded, inset, 3px accent
  focus ring) with logo, meta line and MDBList rating pills, 2-line synopsis, dots with an active
  pill; "My Media" row of four 388x222 landscape library tiles with centred labels; "1:06 AM" clock.
- `sidebar.png`: classic sidebar *expanded* (350px) in a blue accent: logo + "Litefin", SyncPlay,
  user, a full-width accent Home row, Random, Favorites, Search, Settings, a "MY MEDIA" header and
  library names; behind it Continue Watching cards with a progress bar and centred labels, accent
  count badges on posters.
- `details.png`: yellow accent, sidebar hidden: full-bleed darkened backdrop, 420x630 poster, 56px
  title, meta row with rating badge and Ends at, MDBList ratings row, an older pill Play button plus
  72px circle icon buttons, italic tagline, overview, Genres/Studios/Tags chip table, "Cast & Crew".
- `library.png`: lime accent, "Movies" title, centred tab pills (active filled), count + Shuffle,
  Sort, Filter, View Mode, alphabet pill bar with S focused, a 5-column poster grid (older default)
  with centred title + year.
- `genres.png`: pink accent, "Action" genre drill-in, 6 columns of 240 cards with gap 32.
- `player-osd.png`: red accent OSD: back arrow (hidden on TV now), "Tenet (2020)", clock; focused
  pause circle filled with accent, transport buttons, "Ends at", right-side icons (heart, CC, audio,
  group, info, settings), thin seek bar with accent fill, chapter ticks, times at both ends.
- `readme-hero.jpg`: closest to today: modern-collapsed transparent rail (logo top, active Home
  squircle, discover, favorites, search, random, libraries, settings, avatar at the bottom) over an
  immersive full-bleed hero; its rows are the modern layout, not the household's classic one.
- `banner.png`: project banner (logo gradient and wordmark), not UI.

## 13. Not determined

- Effective card focus scale: 1.02 is a cascade conclusion (lazy chunk order), not observed on a TV.
- Exact Remix Icon glyph names for Random and Libraries; continue-watching subtitle format is from a
  screenshot.
- How LG's Chromium renders synthetic bold of Poppins at each size (stroke amount) was not measured.
- Hero content top position varies with synopsis line count (bottom-anchored at y=502).
- Login server-picker screen (`styles/modern/server.css`) and the skip-intro button were not
  measured in detail.
