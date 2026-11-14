//! A wiki's language: a BCP-47 tag, canonical as OpenWiki's runtime makes
//! it (`Intl.getCanonicalLocales`), and recognized when English has a name
//! for its language (`Intl.DisplayNames`). The data is that runtime's
//! (lang_data.rs).

use crate::lang_data::{GRANDFATHERED, LANGUAGES, Language, REGIONS, SIGN};

pub enum Resolved {
    Absent,
    Language(String),
    Unrecognized(String),
}

/// A requested language: none, its canonical tag, or why it isn't one.
pub fn resolve(input: Option<&str>) -> Resolved {
    let trimmed = crate::jsre::trim(input.unwrap_or(""));
    if trimmed.is_empty() {
        return Resolved::Absent;
    }
    if let Some(canonical) = canonicalize(trimmed) {
        if language_of(&canonical).is_some_and(|l| named(&l)) {
            return Resolved::Language(canonical);
        }
    }
    Resolved::Unrecognized(format!("Unrecognized language \"{trimmed}\". Use a BCP-47 code such as ko, zh-CN, or pt-BR rather than a language name."))
}

/// A tag's language subtag as `Intl.Locale` gives it ("en" for no tag;
/// the tag itself when it isn't one; None for "und").
pub fn primary(tag: Option<&str>) -> Option<String> {
    match tag.filter(|t| !t.is_empty()) {
        None => Some("en".into()),
        Some(t) => match canonicalize(t) {
            Some(c) => language_of(&c),
            None => Some(t.to_owned()),
        },
    }
}

fn entry(code: &str) -> Option<&'static Language> {
    LANGUAGES.binary_search_by(|l| l.code.cmp(code)).ok().map(|i| &LANGUAGES[i])
}

/// `new Intl.Locale(canonical).language`.
fn language_of(canonical: &str) -> Option<String> {
    let lang = canonical.split('-').next().unwrap_or(canonical);
    if lang == "und" {
        return None;
    }
    if canonical == lang {
        if let Some(e) = entry(lang) {
            return Some(e.locale.to_owned()).filter(|l| !l.is_empty());
        }
    }
    Some(lang.to_owned())
}

fn named(language: &str) -> bool {
    entry(language).is_some_and(|e| e.named)
}

fn alpha(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_alphabetic())
}

fn alnum(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// The canonical form of a Unicode locale identifier, None when it isn't one.
pub fn canonicalize(tag: &str) -> Option<String> {
    if tag.is_empty() || !tag.is_ascii() {
        return None;
    }
    let lower = tag.to_ascii_lowercase();
    if let Ok(k) = GRANDFATHERED.binary_search_by(|(c, _)| (*c).cmp(lower.as_str())) {
        return Some(GRANDFATHERED[k].1.to_owned());
    }
    let parts: Vec<String> = tag.split('-').map(|p| p.to_ascii_lowercase()).collect();
    if parts.iter().any(|p| p.is_empty() || p.len() > 8 || !alnum(p)) {
        return None;
    }
    let mut i = 0;
    let first = &parts[0];
    let mut language = if alpha(first) && (first.len() == 2 || first.len() == 3 || (5..=8).contains(&first.len())) {
        i += 1;
        first.clone()
    } else {
        return None;
    };
    let mut script = None;
    if let Some(p) = parts.get(i).filter(|p| p.len() == 4 && alpha(p)) {
        let mut s = p.clone();
        s[..1].make_ascii_uppercase();
        script = Some(s);
        i += 1;
    }
    let mut region = None;
    if let Some(p) = parts.get(i).filter(|p| (p.len() == 2 && alpha(p)) || (p.len() == 3 && p.bytes().all(|b| b.is_ascii_digit()))) {
        region = Some(p.to_ascii_uppercase());
        i += 1;
    }
    let mut variants: Vec<String> = Vec::new();
    while let Some(p) = parts.get(i).filter(|p| ((5..=8).contains(&p.len())) || (p.len() == 4 && p.as_bytes()[0].is_ascii_digit())) {
        if variants.contains(p) {
            return None;
        }
        variants.push(p.clone());
        i += 1;
    }
    // Extensions (a singleton, then subtags of 2–8) and a private use part.
    let mut extensions: Vec<(String, Vec<String>)> = Vec::new();
    let mut private: Vec<String> = Vec::new();
    while i < parts.len() {
        let s = &parts[i];
        if s.len() != 1 {
            return None;
        }
        i += 1;
        if s == "x" {
            private = parts[i..].to_vec();
            if private.is_empty() {
                return None;
            }
            break;
        }
        if extensions.iter().any(|(k, _)| k == s) {
            return None;
        }
        let start = i;
        while i < parts.len() && parts[i].len() > 1 {
            if parts[i].len() < 2 {
                return None;
            }
            i += 1;
        }
        if i == start {
            return None;
        }
        extensions.push((s.clone(), parts[start..i].to_vec()));
    }
    // Aliases: the language (which may bring a script and a region), then
    // the region.
    if let Some(e) = entry(&language) {
        if script.is_none() && region.is_none() && variants.is_empty() && extensions.is_empty() && private.is_empty() {
            let mut r = e.alone.split('-');
            language = r.next().unwrap_or(e.alone).to_owned();
            for p in r {
                if p.len() == 4 { script = Some(p.to_owned()) } else { region = Some(p.to_owned()) }
            }
        } else {
            let hans: Vec<&str> = e.with_script.split('-').collect();
            let us: Vec<&str> = e.with_region.split('-').collect();
            language = hans[0].to_owned();
            if script.is_none() {
                script = us.iter().skip(1).find(|p| p.len() == 4).map(|p| (*p).to_owned());
            }
            if region.is_none() {
                region = hans.iter().skip(2).find(|p| p.len() == 2 || p.len() == 3).map(|p| (*p).to_owned());
            }
        }
    }
    if language == "sgn" {
        if let Some(k) = region.as_ref().and_then(|r| SIGN.binary_search_by(|(c, _)| (*c).cmp(r.as_str())).ok()) {
            language = SIGN[k].1.to_owned();
            region = None;
        }
    }
    if let Some(reg) = &region {
        if let Ok(k) = REGIONS.binary_search_by(|(c, _)| (*c).cmp(reg.as_str())) {
            region = Some(REGIONS[k].1.to_owned());
        }
    }
    // (-u-: its attributes, then its keywords by key, a "true" value left out.)
    for (k, v) in extensions.iter_mut() {
        if k != "u" {
            continue;
        }
        let attrs = v.iter().take_while(|p| p.len() > 2).cloned().collect::<Vec<_>>();
        let mut keys: Vec<Vec<String>> = Vec::new();
        for p in &v[attrs.len()..] {
            if p.len() == 2 {
                keys.push(vec![p.clone()]);
            } else if let Some(last) = keys.last_mut() {
                last.push(p.clone());
            }
        }
        keys.sort_by(|a, b| a[0].cmp(&b[0]));
        let mut out = attrs;
        for mut kv in keys {
            if kv.len() == 2 && kv[1] == "true" {
                kv.pop();
            }
            out.extend(kv);
        }
        *v = out;
    }
    variants.sort();
    extensions.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out = vec![language];
    out.extend(script);
    out.extend(region);
    out.extend(variants);
    for (k, v) in extensions {
        out.push(k);
        out.extend(v);
    }
    if !private.is_empty() {
        out.push("x".into());
        out.extend(private);
    }
    Some(out.join("-"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_tags() {
        let r = |s: &str| match resolve(Some(s)) {
            Resolved::Absent => "absent".to_owned(),
            Resolved::Language(l) => l,
            Resolved::Unrecognized(_) => "?".to_owned(),
        };
        assert_eq!(r("ko"), "ko");
        assert_eq!(r(" zh-cn "), "zh-CN");
        assert_eq!(r("pt-br"), "pt-BR");
        assert_eq!(r("zh-hans-cn"), "zh-Hans-CN");
        assert_eq!(r("iw"), "he");
        assert_eq!(r("sh"), "sr-Latn");
        assert_eq!(r("Korean"), "?");
        assert_eq!(r("klingon"), "?");
        assert_eq!(r("en_US"), "?");
        assert_eq!(r(""), "absent");
        assert_eq!(primary(Some("pt-BR")).as_deref(), Some("pt"));
        assert_eq!(primary(None).as_deref(), Some("en"));
        assert_eq!(primary(Some("not a tag")).as_deref(), Some("not a tag"));
        assert_eq!(primary(Some("und")), None);
    }
}
