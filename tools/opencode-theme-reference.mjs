// Prints, as JSON, the colours OpenCode's desktop theme code gives each theme
// file for the tokens Harlo uses: { id: { light: {token: "#rrggbb"}, dark } }.
// A token whose value isn't a plain colour after following var(--…)
// references is left out.
//
//   node opencode-theme-reference.mjs <resolve.ts> <themes folder>
import { readdirSync, readFileSync } from "node:fs"
import { join } from "node:path"
import { pathToFileURL } from "node:url"

const [resolvePath, folder] = process.argv.slice(2)
const { resolveThemeVariant } = await import(pathToFileURL(resolvePath).href)

const TOKENS = [
  "background-base", "background-stronger", "surface-inset-base", "surface-raised-base",
  "surface-raised-strong", "surface-base-hover", "surface-base-active", "surface-brand-base",
  "border-weaker-base", "border-weak-base", "border-base", "text-strong", "text-base",
  "text-weak", "text-weaker", "text-interactive-base", "icon-base", "icon-weak-base",
  "icon-selected", "icon-success-base", "icon-warning-base", "icon-critical-base",
  "icon-info-base", "syntax-comment", "syntax-keyword", "syntax-string", "syntax-primitive",
  "syntax-property", "syntax-type", "syntax-constant", "syntax-operator",
  "syntax-punctuation", "syntax-variable",
]

function resolved(tokens, name) {
  let value = tokens[name]
  for (let depth = 0; depth < 8 && typeof value === "string"; depth++) {
    const reference = /^var\(--([a-z0-9-]+)\)$/.exec(value.trim())
    if (!reference) break
    value = tokens[reference[1]]
  }
  return typeof value === "string" && /^#[0-9a-fA-F]{6}$/.test(value) ? value.toLowerCase() : undefined
}

const out = {}
for (const file of readdirSync(folder).filter((f) => f.endsWith(".json")).sort()) {
  const theme = JSON.parse(readFileSync(join(folder, file), "utf8"))
  out[theme.id] = {}
  for (const mode of ["light", "dark"]) {
    const tokens = resolveThemeVariant(theme[mode], mode === "dark")
    const kept = {}
    for (const name of TOKENS) {
      const value = resolved(tokens, name)
      if (value) kept[name] = value
    }
    out[theme.id][mode] = kept
  }
}
process.stdout.write(JSON.stringify(out, null, 1) + "\n")
