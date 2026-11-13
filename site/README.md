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
    `desktop-hero*` (`site-hero`: Claude Code reviewing the Harlo agent's fix
    while the Harlo agent works in another project, both run by the click
    tests' stand-ins; on the wide run the window is made the page column's
    size, 1102×720, so the page shows it at its own size) with `-2x` at full
    size for the first screen.
  - `assets/crop-*.webp`: the part of a window a section is about, cut at full
    size so its text stays readable: the wiki (`site-wiki`, the chat and the
    Wiki panel), the plan (`site-plan`, the chat column) and the diff
    (`site-diff`, the diff without the sidebar); `crop-*-narrow` is a smaller
    cut for phones, all from the narrow window (text at 125%) and at about the
    same scale and below the tabs: the chat and Changes panel of `site-hero`
    once the review is done (`crop-hero-narrow`), the chat from the question
    with the Wiki panel (`crop-wiki-narrow`, from `site-wiki-top`), the inline
    diff with the sidebar hidden, without the hunk buttons (`crop-diff-narrow`) and the chat column from
    the two option cards (`crop-plan-narrow`, from `site-plan-cards`, with the plan folded so
    both show whole).
- iPhone screenshots: the UI tests' (the iOS workflow's pro-dark and pro-light runs), 660 px wide, as
  `assets/iphone-*.webp` and `assets/iphone-*-light.webp`.
- Fonts: Geist and JetBrains Mono from `crates/harlo-app/fonts`, subset to woff2
  (served from here, no font service).
- The download page asks GitHub's API for the latest release's files.
