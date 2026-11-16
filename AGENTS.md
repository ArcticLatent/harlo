<!-- OPENWIKI:START -->

## OpenWiki

This repository has a generated `openwiki/` evidence index. It is optional just-in-time context, not required startup reading.

- Do not enumerate, preload, or search wikis at task start. Use retrieval when the user asks for it, when unfamiliar architecture or dependency behavior materially affects the task, or when source inspection leaves an important uncertainty. Stop once the question is grounded.
- When those conditions apply and OpenWiki retrieval tools are available, use `openwiki_search` for just-in-time context and `openwiki_read` for the relevant complete sections. If search returns `workspace_required`, ask which listed workspace to use and retry with its ID.
- Use `openwiki_list_workspaces` or `openwiki_list_wikis` when workspace membership itself needs to be discovered.
- If the retrieval tools are unavailable, read `openwiki/quickstart.md` and follow its links to the relevant pages.
- Treat source code and tests as authoritative. A brief's unknowns and review items are verification gaps, not automatic requirements.
- Prefer the narrowest quiet validation that proves the changed behavior. Preserve complete failure output.

Use the repository's documented maintenance process to refresh generated pages after source changes. Check its configuration and completed runs before relying on automatic updates. Do not hand-edit generated OpenWiki pages unless explicitly asked; prefer updating source code/docs and letting OpenWiki regenerate.

<!-- OPENWIKI:END -->

## Language skills

Use these skills for all development in this repository: writing, changing, reviewing or refactoring code. Load the skill before touching the code, and follow the guidelines it loads.

- **Rust** (`.rs` files, `Cargo.toml`): `ms-rust`. Mark a file that fully complies with the comment the skill specifies.
- **QML** (`crates/harlo-app/qml`): the Qt development skills. Use `qt-qml` for any QML change and `qt-qml-review` for reviews. Use `qt-qml-profiler` for performance work and `qt-ui-design` for layout and UX. Check Qt APIs with the Qt documentation tools rather than from memory.
- **Swift** (the iPhone app and HarloKit): `swiftui-pro`.
