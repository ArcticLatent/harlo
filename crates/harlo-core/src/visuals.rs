//! What an agent can show in a reply besides prose and code, each in a
//! fenced block harlo draws in the chat, on the desktop and the phone:
//! a Mermaid diagram (```mermaid), a comparison of choices (```options,
//! drawn as cards) and a UI mockup (```mockup: an HTML file the agent
//! wrote, shown as a picture, beside the one before it). The block stays
//! plain text in the transcript, so any agent can write one and any
//! viewer can read it; this module reads what the blocks say.

/// What harlo tells agents about them (with orchestration on).
pub const INSTRUCTIONS: &str = "harlo draws three kinds of block in your replies, on the desktop and on the phone. A Mermaid diagram (```mermaid) as a picture: draw one when a flow, a structure or a sequence of steps is clearer as a picture than in words. A choice between approaches (```options) as cards: each option a `## name` line (add `(recommended)` to the one you'd pick), a line about it, then `+ ` lines for its pros and `- ` lines for its cons. A UI change (```mockup) as a picture: write a self-contained HTML mockup into a file (in mockups/, say), then a block with `path: mockups/x.html`, and `before: …` for one of how it is now, `size: 390x844` for a phone (the default is 1280x800); harlo shows the file as it is, so editing it updates the picture.";

/// The fenced blocks of `lang` in a reply, in order (```lang or ~~~lang).
pub fn blocks(text: &str, lang: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut open: Option<(&str, Vec<&str>)> = None;
    for line in text.lines() {
        let t = line.trim();
        match &mut open {
            None => {
                for fence in ["```", "~~~"] {
                    if let Some(rest) = t.strip_prefix(fence)
                        && rest.trim().eq_ignore_ascii_case(lang)
                    {
                        open = Some((fence, Vec::new()));
                    }
                }
            }
            Some((fence, lines)) => {
                let mark = fence.chars().next().unwrap_or('`');
                if t.starts_with(*fence) && t.trim_start_matches(mark).is_empty() {
                    out.push(lines.join("\n"));
                    open = None;
                } else {
                    lines.push(line);
                }
            }
        }
    }
    out
}

/// One of the choices an ```options block compares.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Choice {
    pub name: String,
    /// The one the agent would pick.
    pub recommended: bool,
    pub about: String,
    pub pros: Vec<String>,
    pub cons: Vec<String>,
}

/// An ```options block's choices: `## name` (with `(recommended)` on the
/// one to pick), lines about it, `+ pro` and `- con` lines.
pub fn options(source: &str) -> Result<Vec<Choice>, String> {
    let mut out: Vec<Choice> = Vec::new();
    for line in source.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if let Some(head) = t.strip_prefix('#') {
            let mut name = head.trim_start_matches('#').trim().to_owned();
            let mut recommended = false;
            for mark in ["(recommended)", "[recommended]", "— recommended", "- recommended", "★"] {
                // (Where the lower case lines up with the name.)
                if let Some(at) = name.to_lowercase().find(mark)
                    && name.get(at..at + mark.len()).is_some_and(|m| m.to_lowercase() == mark)
                {
                    name.replace_range(at..at + mark.len(), "");
                    recommended = true;
                }
            }
            out.push(Choice { name: name.trim().trim_end_matches([':', '-', '—']).trim().to_owned(), recommended, ..Default::default() });
            continue;
        }
        let Some(choice) = out.last_mut() else { continue };
        if let Some(pro) = t.strip_prefix("+ ").or_else(|| t.strip_prefix("✓ ")) {
            choice.pros.push(pro.trim().to_owned());
        } else if let Some(con) = t.strip_prefix("- ").or_else(|| t.strip_prefix("− ")).or_else(|| t.strip_prefix("✗ ")) {
            choice.cons.push(con.trim().to_owned());
        } else {
            if !choice.about.is_empty() {
                choice.about.push(' ');
            }
            choice.about.push_str(t);
        }
    }
    out.retain(|c| !c.name.is_empty());
    match out.len() {
        0 => Err("it has no choices: start each with a `## name` line".into()),
        1 => Err("it has only one choice: compare two or more".into()),
        _ => Ok(out),
    }
}

/// What a ```mockup block shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Mockup {
    /// The HTML file, from the session's folder (or absolute).
    pub path: String,
    /// One of how it is now, to show beside it.
    pub before: Option<String>,
    pub title: String,
    /// The window it's shown in, in pixels.
    pub width: u32,
    pub height: u32,
}

/// A ```mockup block: `path: x.html` (or the path alone on its line),
/// `before: y.html`, `title: …`, `size: 390x844`.
pub fn mockup(source: &str) -> Result<Mockup, String> {
    let mut m = Mockup { path: String::new(), before: None, title: String::new(), width: 1280, height: 800 };
    for line in source.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let (key, value) = match t.split_once(':') {
            Some((k, v)) if !k.contains(['/', '.', ' ']) => (k.trim().to_lowercase(), v.trim().trim_matches(['"', '\'', '`']).to_owned()),
            _ => ("path".to_owned(), t.trim_matches(['"', '\'', '`']).to_owned()),
        };
        match key.as_str() {
            "path" | "file" | "after" => m.path = value,
            "before" => m.before = Some(value).filter(|v| !v.is_empty()),
            "title" => m.title = value,
            "size" => {
                let (w, h) = value.split_once(['x', 'X', '×']).ok_or_else(|| format!("its size `{value}` isn't width x height"))?;
                let n = |s: &str| s.trim().trim_end_matches("px").parse::<u32>().ok().filter(|n| (200..=4000).contains(n));
                (m.width, m.height) = n(w).zip(n(h)).ok_or_else(|| format!("its size `{value}` isn't width x height, from 200 to 4000"))?;
            }
            _ => {}
        }
    }
    if m.path.is_empty() {
        return Err("it names no file: give `path: mockups/x.html`".into());
    }
    for p in std::iter::once(&m.path).chain(m.before.as_ref()) {
        let lower = p.to_lowercase();
        if !(lower.ends_with(".html") || lower.ends_with(".htm") || lower.ends_with(".svg")) {
            return Err(format!("`{p}` isn't an HTML file"));
        }
    }
    Ok(m)
}

/// A mockup's file in a session's folder (as it names it, or absolute).
pub fn file_in(cwd: &std::path::Path, path: &str) -> std::path::PathBuf {
    let p = std::path::Path::new(path);
    if p.is_absolute() { p.to_path_buf() } else { cwd.join(p) }
}

/// What can't be shown of a reply's options and mockups (its diagrams are
/// checked where they're drawn): "an options block has only one choice",
/// "a mockup names `x.html`, which isn't there". Files are looked for in
/// `cwd` when it's on this computer.
pub fn problems(reply: &str, cwd: Option<&std::path::Path>) -> Vec<String> {
    let mut out = Vec::new();
    for b in blocks(reply, "options") {
        if let Err(e) = options(&b) {
            out.push(format!("an options block can't be drawn: {e}"));
        }
    }
    for b in blocks(reply, "mockup") {
        match mockup(&b) {
            Err(e) => out.push(format!("a mockup can't be shown: {e}")),
            Ok(m) => {
                let Some(cwd) = cwd.filter(|c| !crate::remote::is_remote(c)) else { continue };
                for p in std::iter::once(&m.path).chain(m.before.as_ref()) {
                    if !file_in(cwd, p).is_file() {
                        out.push(format!("a mockup names `{p}`, which isn't there"));
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_blocks() {
        let reply = "a\n```options\n## A\n```\n~~~mockup\npath: x.html\n~~~\n```Options\nx\n```\n";
        assert_eq!(blocks(reply, "options"), ["## A", "x"]);
        assert_eq!(blocks(reply, "mockup"), ["path: x.html"]);
        assert!(blocks("```mermaid\nflowchart TD\n", "mermaid").is_empty());
    }

    #[test]
    fn reads_options() {
        let o = options("## Match rows by key (recommended)\nRows are updated in place.\n+ nothing to restore\n+ no redraw\n- more code\n\n## Remember the offset\nSmaller.\nStill redraws.\n- the list flickers\n").unwrap();
        assert_eq!(o, vec![
            Choice { name: "Match rows by key".into(), recommended: true, about: "Rows are updated in place.".into(), pros: vec!["nothing to restore".into(), "no redraw".into()], cons: vec!["more code".into()] },
            Choice { name: "Remember the offset".into(), recommended: false, about: "Smaller. Still redraws.".into(), pros: vec![], cons: vec!["the list flickers".into()] },
        ]);
        assert!(options("### One ★\n+ x").unwrap_err().contains("only one choice"));
        assert!(options("Just prose.").unwrap_err().contains("no choices"));
    }

    #[test]
    fn reads_mockups() {
        let m = mockup("path: mockups/settings.html\nbefore: \"mockups/settings-now.html\"\ntitle: Settings with sync\nsize: 390x844\n").unwrap();
        assert_eq!(m, Mockup { path: "mockups/settings.html".into(), before: Some("mockups/settings-now.html".into()), title: "Settings with sync".into(), width: 390, height: 844 });
        assert_eq!(mockup("mockups/a.html").unwrap().width, 1280);
        assert!(mockup("title: x").unwrap_err().contains("names no file"));
        assert!(mockup("path: a.png").unwrap_err().contains("isn't an HTML file"));
        assert!(mockup("path: a.html\nsize: big").unwrap_err().contains("width x height"));
        let dir = std::env::temp_dir().join(format!("harlo-visuals-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("mockups")).unwrap();
        std::fs::write(dir.join("mockups/a.html"), "<p>a</p>").unwrap();
        let reply = "```mockup\npath: mockups/a.html\nbefore: mockups/old.html\n```\n```options\n## Only\n```";
        assert_eq!(problems(reply, Some(&dir)), ["an options block can't be drawn: it has only one choice: compare two or more", "a mockup names `mockups/old.html`, which isn't there"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
