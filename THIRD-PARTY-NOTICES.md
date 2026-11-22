# Third-party notices

Harlo bundles the following assets. Rust crates are listed in `Cargo.lock`, and each carries its own licence.

## Geist (fonts)

`crates/harlo/ui/fonts/Geist-*.ttf`. Copyright 2024 The Geist Project Authors. These fonts are licensed under the SIL Open Font License 1.1; the full text is in `crates/harlo/ui/fonts/Geist-OFL.txt`.

## JetBrains Mono (fonts)

`crates/harlo/ui/fonts/JetBrainsMono-*.ttf`. Copyright 2020 The JetBrains Mono Project Authors. These fonts are licensed under the SIL Open Font License 1.1; the full text is in `crates/harlo/ui/fonts/JetBrainsMono-OFL.txt`.

## Harlo File Icons (font)

`crates/harlo/ui/fonts/FileIcons.ttf`: file-type glyphs cut by `tools/file-icons.py` from the symbols font of [Nerd Fonts](https://github.com/ryanoasis/nerd-fonts) 3.4.0, which gathers them from Seti-UI, Devicons, Material Design Icons, Codicons, Octicons, Font Awesome and Font Logos. The font is licensed under the SIL Open Font License 1.1 and Nerd Fonts' own work under the MIT License; the glyphs are under MIT, Apache 2.0, CC BY 4.0 or the Unlicense. The details and full texts are in `crates/harlo/ui/fonts/FileIcons-LICENSE.txt`.

## Slint (UI toolkit)

The window is drawn with [Slint](https://slint.dev), used under the Slint Royalty-free Desktop, Mobile, and Web Applications License 2.0 rather than an MIT/Apache licence. The app credits Slint in its About.

## SQLite

The layout database is built into the app through `rusqlite`'s bundled SQLite, which is in the public domain.

## Lucide (icon font)

The icon font is embedded through the `lucide-icons` crate. It is licensed under the ISC License:

> Copyright (c) for portions of Lucide are held by Cole Bemis 2013-2022 as part of Feather (MIT). All other copyright (c) for Lucide are held by Lucide Contributors 2022.
>
> Permission to use, copy, modify, and/or distribute this software for any purpose with or without fee is hereby granted, provided that the above copyright notice and this permission notice appear in all copies.
>
> THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.

## Model provider logos

`crates/harlo/ui/images/providers/*.svg`. These are copied from OpenChamber and licensed under the MIT License; the full text is in `crates/harlo/ui/images/providers/OPENCHAMBER-LICENSE`. The logos are trademarks of their owners and are used only to show which provider a model comes from.

## OpenCode themes (test fixtures)

`crates/harlo-core/tests/fixtures/*.json`. These are copied from OpenCode to check that Harlo reads its theme format. They are licensed under the MIT License; the full text is in `crates/harlo-core/tests/fixtures/OPENCODE-LICENSE`.
