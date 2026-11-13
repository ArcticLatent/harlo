# harlo.run

The website: plain HTML and CSS, no build step. Cloudflare serves this folder
as it is (`wrangler.jsonc` at the repository's root: Workers static assets;
`_redirects`, `_headers`; `/download` is `download.html`; this file isn't served).

- Desktop screenshots: `QT_SCALE_FACTOR=2 KEEP=1 SCENARIOS=siteshots
  scripts/click-tests.sh 1440 524` (2880×1800 pictures of a 1440×900 window,
  and the narrowest window, 824 px, for phone-sized pictures).
  Each comes dark and light (`site-main-light.png`): the page shows the one for
  its theme (the top bar's button, else the system's).
  - `assets/desktop-*.webp`: whole windows at 1440×900 (the README's too), and
    `desktop-main*-2x.webp` at full size for the first screen.
  - `assets/crop-*.webp`: the part of a window a section is about, cut at full
    size so its text stays readable: the wiki (`site-wiki`, the chat and the
    Wiki panel), the plan (`site-plan`, the chat column) and the diff
    (`site-diff`, the diff without the sidebar); `crop-*-narrow` is a smaller
    cut for phones: the Wiki panel, the new side of the diff, and from the
    narrow window the chat column (`crop-main-narrow`) and the plan from its
    second option (`crop-plan-narrow`).
  - `assets/theme-*.webp`: `site-theme-*` (the first session in six themes,
    each dark or light), its sidebar and chat up to the Changes panel, 1100 px
    wide.
- iPhone screenshots: the UI tests' (the iOS workflow's pro-dark run), 660 px wide, as `assets/iphone-*.webp`.
- Fonts: Geist and JetBrains Mono from `crates/harlo-app/fonts`, subset to woff2
  (served from here, no font service).
- The download page asks GitHub's API for the latest release's files.
