# harlo.run

The website: plain HTML and CSS, no build step. Cloudflare serves this folder
as it is (`wrangler.jsonc` at the repository's root: Workers static assets;
`_redirects`, `_headers`; `/download` is `download.html`; this file isn't served).

- Desktop screenshots: `KEEP=1 SCENARIOS=siteshots scripts/click-tests.sh 1440`,
  then the `site-*.png` shots as `assets/desktop-*.webp` (1440×900).
- iPhone screenshots: the App Store's (dark), 660 px wide, as `assets/iphone-*.webp`.
- Fonts: Geist and JetBrains Mono from `crates/harlo-app/fonts`, subset to woff2
  (served from here, no font service).
- The download page asks GitHub's API for the latest release's files.
