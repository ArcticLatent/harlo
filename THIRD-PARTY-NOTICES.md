# Third-party notices

harlo is MIT licensed (see [LICENSE](LICENSE)). It includes the following
work by others, under their own licenses. The packages install these
notices under `/usr/share/licenses/harlo` (the AppImage under
`usr/share/licenses/harlo`).

| What | From | License | Notice |
|---|---|---|---|
| Colour themes (all but harlo and Midnight) | [OpenChamber](https://github.com/openchamber/openchamber) | MIT | [`crates/harlo-app/themes/LICENSE`](crates/harlo-app/themes/LICENSE) |
| Provider logos | [OpenChamber](https://github.com/openchamber/openchamber) | MIT | [`crates/harlo-app/icons/providers/OPENCHAMBER-LICENSE`](crates/harlo-app/icons/providers/OPENCHAMBER-LICENSE) |
| Conversation starters' instructions (`crates/harlo-core/src/starters.rs`) and the streaming-text reveal (`committed_prefix` in `crates/harlo-core/src/markdown.rs`), adapted | [OpenChamber](https://github.com/openchamber/openchamber) | MIT | as above |
| File-type icons | [Material Icon Theme](https://github.com/material-extensions/vscode-material-icon-theme) | MIT | [`crates/harlo-app/icons/file-types/LICENSE`](crates/harlo-app/icons/file-types/LICENSE) |
| Outline icons | [Remix Icon](https://github.com/Remix-Design/RemixIcon) | Remix Icon License | [`crates/harlo-app/icons/REMIXICON-LICENSE`](crates/harlo-app/icons/REMIXICON-LICENSE) |
| Agent marks (Claude Code, OpenCode, pi, Oh My Pi) | [Paseo](https://github.com/getpaseo/paseo) | Apache-2.0 | [`crates/harlo-app/icons/PASEO-NOTICE`](crates/harlo-app/icons/PASEO-NOTICE) |
| Geist | [Vercel](https://github.com/vercel/geist-font) | SIL OFL 1.1 | [`crates/harlo-app/fonts/Geist-OFL.txt`](crates/harlo-app/fonts/Geist-OFL.txt) |
| JetBrains Mono | [JetBrains](https://github.com/JetBrains/JetBrainsMono) | SIL OFL 1.1 | [`crates/harlo-app/fonts/OFL.txt`](crates/harlo-app/fonts/OFL.txt) |
| TweetNaCl-js (the phone page's encryption) | [TweetNaCl-js](https://github.com/dchest/tweetnacl-js) | Public domain (Unlicense) | [`crates/harlo-app/phone/TWEETNACL-LICENSE`](crates/harlo-app/phone/TWEETNACL-LICENSE) |
| Qt 6 (in the release packages) | [The Qt Company](https://www.qt.io) | LGPL-3.0 | installed with the packages (`qt/`) |
| whisper.cpp (dictation) | [ggml-org](https://github.com/ggml-org/whisper.cpp) | MIT | built from the `whisper-rs` crate |

The agents' names and marks belong to their projects. The Rust crates harlo
builds with keep their own licenses (`cargo tree` lists them).
