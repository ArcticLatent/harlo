# harlo.run

The website: plain HTML and CSS, no build step. Cloudflare serves this folder
as it is (`wrangler.jsonc` at the repository's root: Workers static assets;
`_redirects`, `_headers`; `/download` is `download.html`; this file isn't served).

- Desktop screenshots: `python3 -I tools/site-shots.py` takes them (a scratch
  server with a scripted model, a hidden display; 2880×1800 pictures of a
  1440×900 window, and a 760×940 window with the same scale for phone-sized
  pictures), dark and light, into `target/site-shots`. `python3 -I
  tools/site-shots.py cut` then writes the pictures below; its `CUTS` list
  says which part of which window each one is. The page shows the picture for
  its theme (the top bar's button, else the system's).
  - `assets/desktop-*.webp`: whole windows at 1440×900 (the README's too), and
    `desktop-hero*` at the page column's width, 1102 px, with `-2x` at full
    size for the first screen.
  - `assets/crop-*.webp`: the part of a window a section is about, cut at full
    size so its text stays readable: the diff, the approval under the files
    just changed, Settings → Providers, a plugin's page and Settings → Server.
    `crop-*-narrow` is a smaller cut for phones, from the narrow window.
- iPhone screenshots: the iOS workflow's `screenshots` artifact (the app's
  `-tour` mode), downloaded into `target/ios-shots`; the same `cut` writes
  them 660 px wide as `assets/iphone-*.webp` and `assets/iphone-*-light.webp`.
- The start and idle figures: `tools/startup-time.py`.
- Fonts: Geist and JetBrains Mono from `crates/harlo/ui/fonts`, subset to woff2
  (served from here, no font service).
- The download page asks GitHub's API for the latest release's files.
