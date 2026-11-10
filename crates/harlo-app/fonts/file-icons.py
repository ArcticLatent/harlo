#!/usr/bin/env python3
"""Makes harlo's file-type icon font and its table.

The icons are glyphs of the Nerd Fonts symbols font (pinned to one release,
since glyphs have moved between major versions), cut down to the ones
harlo uses and renamed so an installed Nerd Font is never picked instead.

    curl -LO https://github.com/ryanoasis/nerd-fonts/releases/download/v3.4.0/NerdFontsSymbolsOnly.zip
    curl -LO https://raw.githubusercontent.com/ryanoasis/nerd-fonts/v3.4.0/glyphnames.json
    unzip NerdFontsSymbolsOnly.zip SymbolsNerdFont-Regular.ttf
    python3 fonts/file-icons.py SymbolsNerdFont-Regular.ttf glyphnames.json

(from crates/harlo-app; needs fontTools). Writes fonts/FileIcons.ttf and
src/file_icon_glyphs.rs. A type is named after the extension it's for
when there's one (src/file_icons.rs maps names and extensions to types).
"""
import json
import sys
from pathlib import Path

from fontTools import subset
from fontTools.ttLib import TTFont

NERD_FONTS = "3.4.0"
FAMILY = "Harlo File Icons"

# type: (Nerd Fonts glyph name, colour)
TYPES = {
    "3d": ("md-cube_outline", "#29b6f6"),
    "ada": ("custom-ada", "#0277bd"),
    "apl": ("dev-apl", "#9575cd"),
    "asciidoc": ("seti-text", "#0097a7"),
    "assembly": ("seti-asm", "#ff6e40"),
    "astro": ("custom-astro", "#7c4dff"),
    "audio": ("seti-audio", "#ef5350"),
    "authors": ("oct-people", "#f44336"),
    "babel": ("seti-babel", "#fdd835"),
    "bazel": ("seti-bazel", "#43a047"),
    "bibliography": ("md-bookshelf", "#8d6e63"),
    "biome": ("md-leaf", "#42a5f5"),
    "blender": ("dev-blender", "#ff9800"),
    "bun": ("dev-bun", "#ffcc80"),
    "c": ("seti-c", "#0288d1"),
    "caddy": ("md-server", "#4fc3f7"),
    "certificate": ("seti-license", "#ff5722"),
    "changelog": ("cod-history", "#8bc34a"),
    "claude": ("md-creation", "#ff7043"),
    "clojure": ("seti-clojure", "#7cb342"),
    "cmake": ("dev-cmake", "#1e88e5"),
    "cobol": ("md-card_text_outline", "#0288d1"),
    "codeowners": ("md-shield_account_outline", "#afb42b"),
    "coffee": ("seti-coffee", "#42a5f5"),
    "conduct": ("md-handshake_outline", "#cddc39"),
    "console": ("seti-shell", "#ff7043"),
    "contributing": ("md-hand_heart_outline", "#ffca28"),
    "cpp": ("seti-cpp", "#0288d1"),
    "crystal": ("seti-crystal", "#cfd8dc"),
    "csharp": ("seti-c_sharp", "#0288d1"),
    "css": ("seti-css", "#7e57c2"),
    "cuda": ("seti-cu", "#7cb342"),
    "d": ("seti-d", "#f44336"),
    "dart": ("seti-dart", "#29b6f6"),
    "database": ("seti-db", "#ffca28"),
    "deno": ("dev-denojs", "#cfd8dc"),
    "diff": ("oct-diff", "#42a5f5"),
    "dll": ("cod-file_binary", "#42a5f5"),
    "docker": ("seti-docker", "#0288d1"),
    "document": ("seti-text", "#42a5f5"),
    "drawio": ("fa-draw_polygon", "#fb8c00"),
    "editorconfig": ("seti-editorconfig", "#cfd8dc"),
    "ejs": ("seti-ejs", "#ffca28"),
    "elixir": ("seti-elixir", "#9575cd"),
    "elm": ("seti-elm", "#00b8d4"),
    "email": ("md-email_outline", "#42a5f5"),
    "epub": ("md-book_open_page_variant_outline", "#8bc34a"),
    "erlang": ("dev-erlang", "#f44336"),
    "eslint": ("seti-eslint", "#7986cb"),
    "excalidraw": ("md-draw", "#5c6bc0"),
    "exe": ("md-application_outline", "#e64a19"),
    "favicon": ("seti-favicon", "#ffd54f"),
    "fennel": ("custom-fennel", "#d7a74a"),
    "font": ("seti-font", "#f44336"),
    "fortran": ("dev-fortran", "#ff7043"),
    "fsharp": ("seti-f_sharp", "#00b8d4"),
    "gemfile": ("fa-gem", "#e53935"),
    "git": ("seti-git", "#e64a19"),
    "gleam": ("md-star_four_points_outline", "#ea80fc"),
    "go": ("seti-go", "#00acc1"),
    "go-mod": ("seti-go", "#ec407a"),
    "godot": ("seti-godot", "#42a5f5"),
    "gradle": ("seti-gradle", "#0097a7"),
    "graphql": ("seti-graphql", "#ec407a"),
    "groovy": ("dev-groovy", "#26c6da"),
    "h": ("seti-c", "#0288d1"),
    "hack": ("seti-hacklang", "#ffa000"),
    "haml": ("seti-haml", "#f4511e"),
    "handlebars": ("seti-mustache", "#ff7043"),
    "haskell": ("seti-haskell", "#ab47bc"),
    "haxe": ("seti-haxe", "#ef6c00"),
    "hcl": ("seti-terraform", "#5c6bc0"),
    "heroku": ("seti-heroku", "#7e57c2"),
    "hpp": ("seti-cpp", "#0288d1"),
    "html": ("seti-html", "#e65100"),
    "http": ("md-web", "#e53935"),
    "image": ("seti-image", "#26a69a"),
    "jar": ("md-package_variant_closed", "#f44336"),
    "java": ("seti-java", "#f44336"),
    "javaclass": ("cod-file_binary", "#1e88e5"),
    "javascript": ("seti-javascript", "#ffca28"),
    "jinja": ("seti-jinja", "#bdbdbd"),
    "jsconfig": ("seti-config", "#ffca28"),
    "json": ("seti-json", "#f9a825"),
    "julia": ("seti-julia", "#ab47bc"),
    "jupyter": ("seti-notebook", "#f57c00"),
    "just": ("seti-makefile", "#ef5350"),
    "kdl": ("seti-config", "#90a4ae"),
    "key": ("md-key_outline", "#26a69a"),
    "knip": ("md-content_cut", "#ef6c00"),
    "kotlin": ("seti-kotlin", "#7f52ff"),
    "less": ("seti-less", "#0277bd"),
    "lib": ("cod-library", "#8bc34a"),
    "liquid": ("seti-liquid", "#29b6f6"),
    "lisp": ("custom-common_lisp", "#ef5350"),
    "lock": ("seti-lock", "#ffd54f"),
    "log": ("oct-log", "#afb42b"),
    "lua": ("seti-lua", "#42a5f5"),
    "luau": ("seti-lua", "#03a9f4"),
    "makefile": ("seti-makefile", "#ef5350"),
    "markdown": ("seti-markdown", "#42a5f5"),
    "matlab": ("dev-matlab", "#ff7043"),
    "mdx": ("seti-markdown", "#ffca28"),
    "mermaid": ("cod-type_hierarchy", "#ff4081"),
    "meson": ("md-hammer_wrench", "#3f51b5"),
    "netlify": ("dev-netlify", "#00bfa5"),
    "nginx": ("dev-nginx", "#43a047"),
    "nim": ("seti-nim", "#ffca28"),
    "nix": ("md-nix", "#64b5f6"),
    "nodejs": ("md-nodejs", "#8bc34a"),
    "npm": ("seti-npm", "#e53935"),
    "nunjucks": ("seti-nunjucks", "#388e3c"),
    "objective-c": ("dev-objectivec", "#ffab40"),
    "objective-cpp": ("dev-objectivec", "#ffab40"),
    "ocaml": ("seti-ocaml", "#ff9800"),
    "odin": ("md-alpha_o_circle_outline", "#448aff"),
    "org": ("custom-orgmode", "#77aa99"),
    "pascal": ("md-alpha_p_circle_outline", "#0288d1"),
    "pdf": ("seti-pdf", "#ef5350"),
    "perl": ("seti-perl", "#ba68c8"),
    "php": ("seti-php", "#1e88e5"),
    "pnpm": ("dev-pnpm", "#ffb300"),
    "poetry": ("dev-poetry", "#5c6bc0"),
    "postcss": ("dev-postcss", "#e53935"),
    "powerpoint": ("md-file_powerpoint_outline", "#e64a19"),
    "powershell": ("seti-powershell", "#03a9f4"),
    "prettier": ("custom-prettier", "#26a69a"),
    "prisma": ("seti-prisma", "#00bfa5"),
    "prolog": ("seti-prolog", "#ef5350"),
    "proto": ("dev-grpc", "#00bfa5"),
    "pug": ("seti-pug", "#a1887f"),
    "puppet": ("seti-puppet", "#fbc02d"),
    "purescript": ("seti-purescript", "#42a5f5"),
    "python": ("seti-python", "#0288d1"),
    "python-misc": ("seti-python", "#fbc02d"),
    "qml": ("dev-qt", "#41cd52"),
    "r": ("seti-r", "#1976d2"),
    "racket": ("custom-scheme", "#e53935"),
    "react": ("seti-react", "#00bcd4"),
    "react_ts": ("seti-react", "#0288d1"),
    "readme": ("oct-book", "#42a5f5"),
    "reason": ("seti-reasonml", "#f44336"),
    "renovate": ("md-autorenew", "#ffb300"),
    "rescript": ("seti-rescript", "#ef5350"),
    "robots": ("md-robot_outline", "#ff5252"),
    "ron": ("seti-config", "#90a4ae"),
    "ruby": ("seti-ruby", "#f44336"),
    "rust": ("seti-rust", "#ff7043"),
    "sass": ("seti-sass", "#ec407a"),
    "sbt": ("seti-sbt", "#0277bd"),
    "scala": ("seti-scala", "#f44336"),
    "scheme": ("custom-scheme", "#f44336"),
    "settings": ("seti-config", "#42a5f5"),
    "shader": ("md-shape_outline", "#ab47bc"),
    "slim": ("seti-slim", "#f57f17"),
    "solidity": ("dev-solidity", "#0288d1"),
    "stylus": ("seti-stylus", "#c0ca33"),
    "subtitles": ("md-subtitles_outline", "#ff9800"),
    "svelte": ("seti-svelte", "#ff5722"),
    "svg": ("seti-svg", "#ffb300"),
    "swift": ("seti-swift", "#ff6e40"),
    "systemd": ("md-cogs", "#00e676"),
    "table": ("seti-csv", "#8bc34a"),
    "tailwindcss": ("dev-tailwindcss", "#4db6ac"),
    "tcl": ("md-feather", "#ef5350"),
    "templ": ("seti-html", "#ffd54f"),
    "terraform": ("seti-terraform", "#5c6bc0"),
    "test-js": ("md-test_tube", "#ffca28"),
    "test-jsx": ("md-test_tube", "#00bcd4"),
    "test-ts": ("md-test_tube", "#0288d1"),
    "tex": ("seti-tex", "#2196f3"),
    "todo": ("seti-todo", "#7cb342"),
    "toml": ("custom-toml", "#cfd8dc"),
    "tsconfig": ("seti-tsconfig", "#0288d1"),
    "tune": ("md-tune", "#fbc02d"),
    "turborepo": ("md-circle_double", "#cfd8dc"),
    "twig": ("seti-twig", "#8bc34a"),
    "typescript": ("seti-typescript", "#0288d1"),
    "typescript-def": ("seti-typescript", "#0288d1"),
    "typst": ("linux-typst", "#0097a7"),
    "uml": ("dev-uml", "#b39ddb"),
    "url": ("cod-link", "#42a5f5"),
    "uv": ("md-lightning_bolt_outline", "#e040fb"),
    "vala": ("seti-vala", "#9575cd"),
    "vercel": ("dev-vercel", "#cfd8dc"),
    "verified": ("cod-verified", "#8bc34a"),
    "verilog": ("cod-chip", "#ff7043"),
    "video": ("seti-video", "#ff9800"),
    "vim": ("custom-vim", "#43a047"),
    "vite": ("dev-vite", "#ffab00"),
    "vitest": ("dev-vitest", "#8bc34a"),
    "vue": ("seti-vue", "#41b883"),
    "webassembly": ("seti-wasm", "#7c4dff"),
    "word": ("seti-word", "#1e88e5"),
    "xml": ("seti-xml", "#8bc34a"),
    "yaml": ("seti-yml", "#ff5252"),
    "yarn": ("seti-yarn", "#0288d1"),
    "zig": ("seti-zig", "#f9a825"),
    "zip": ("seti-zip", "#afb42b"),
}


def main(font_path, names_path):
    here = Path(__file__).resolve().parent.parent
    names = json.loads(Path(names_path).read_text())
    if names["METADATA"]["version"] != NERD_FONTS:
        sys.exit(f"glyphnames.json is from Nerd Fonts {names['METADATA']['version']}, not {NERD_FONTS}")
    points = {}
    for kind, (glyph, _) in TYPES.items():
        if glyph not in names:
            sys.exit(f"{kind}: no glyph {glyph} in Nerd Fonts {NERD_FONTS}")
        points[kind] = int(names[glyph]["code"], 16)

    font = TTFont(font_path)
    options = subset.Options()
    options.name_IDs = []
    options.notdef_outline = True
    sub = subset.Subsetter(options)
    sub.populate(unicodes=sorted(set(points.values())))
    sub.subset(font)
    cmap = font.getBestCmap()
    missing = [k for k, p in points.items() if p not in cmap]
    if missing:
        sys.exit(f"not in the font: {missing}")
    name = font["name"]
    name.names = []
    for nid, text in [(0, "Glyphs from Nerd Fonts {} (see fonts/FileIcons-LICENSE.txt)".format(NERD_FONTS)),
                      (1, FAMILY), (2, "Regular"), (3, f"{FAMILY} {NERD_FONTS}"), (4, FAMILY),
                      (5, f"Version {NERD_FONTS}"), (6, FAMILY.replace(" ", "") + "-Regular")]:
        name.setName(text, nid, 3, 1, 0x409)
    font.save(here / "fonts/FileIcons.ttf")

    rows = "".join(
        f'    ("{kind}", \'\\u{{{points[kind]:x}}}\', 0x{color[1:]}), // {glyph}\n'
        for kind, (glyph, color) in sorted(TYPES.items())
    )
    (here / "src/file_icon_glyphs.rs").write_text(
        "// Made by fonts/file-icons.py: don't edit by hand.\n"
        "/// File types, sorted: the type, its glyph in the bundled \"" + FAMILY + "\" font\n"
        f"/// (Nerd Fonts {NERD_FONTS}) and its colour.\n"
        "pub const GLYPHS: &[(&str, char, u32)] = &[\n" + rows + "];\n"
    )


if __name__ == "__main__":
    main(*sys.argv[1:])
