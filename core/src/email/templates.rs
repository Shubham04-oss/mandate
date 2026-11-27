//! The built emails (`emails/dist`, baked into the image): loaded and checked once at start, then
//! filled per recipient. Only the dark `<name>.html` is sent for now (owner's call); the light
//! files stay unused until people can choose.
use serde::Deserialize;
use std::collections::{BTreeSet, HashMap};
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("{0}: {1}")]
    Read(String, #[source] std::io::Error),
    #[error("manifest.json: {0}")]
    Manifest(#[from] serde_json::Error),
    #[error("template {0} is not in manifest.json")]
    Missing(String),
    #[error("{file} uses {{{{{name}}}}}, which manifest.json does not list")]
    Unlisted { file: String, name: String },
    #[error("no value for {{{{{0}}}}}")]
    Unfilled(String),
    #[error("the core does not fill {{{{{name}}}}}, which {template} needs")]
    Unfillable { template: String, name: String },
}

#[derive(Deserialize)]
struct ManifestEntry {
    subject: String,
    placeholders: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Template {
    subject: String,
    html: String,
    text: String,
}

/// What goes to one recipient.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filled {
    pub subject: String,
    pub html: String,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct Templates(HashMap<String, Template>);

impl Templates {
    /// Loads each required template with the placeholders the core fills for it, and refuses
    /// one whose manifest needs another or whose files use an unlisted one: a template change
    /// stops the core at start, not an email at send time.
    pub fn load(dir: &Path, required: &[(&str, &[&str])]) -> Result<Templates, TemplateError> {
        let read = |name: &str| {
            let path = dir.join(name);
            std::fs::read_to_string(&path)
                .map_err(|e| TemplateError::Read(path.display().to_string(), e))
        };
        let manifest: HashMap<String, ManifestEntry> =
            serde_json::from_str(&read("manifest.json")?)?;
        let mut out = HashMap::new();
        for &(name, filled) in required {
            let entry = manifest
                .get(name)
                .ok_or_else(|| TemplateError::Missing(name.to_owned()))?;
            if let Some(p) = entry
                .placeholders
                .iter()
                .find(|p| !filled.contains(&p.as_str()))
            {
                return Err(TemplateError::Unfillable {
                    template: name.to_owned(),
                    name: p.clone(),
                });
            }
            let t = Template {
                subject: entry.subject.clone(),
                html: read(&format!("{name}.html"))?,
                text: read(&format!("{name}.txt"))?,
            };
            let listed: BTreeSet<&str> = entry.placeholders.iter().map(String::as_str).collect();
            for (file, body) in [
                (format!("{name} subject"), &t.subject),
                (format!("{name}.html"), &t.html),
                (format!("{name}.txt"), &t.text),
            ] {
                if let Some(name) = names(body)
                    .into_iter()
                    .find(|n| !listed.contains(n.as_str()))
                {
                    return Err(TemplateError::Unlisted { file, name });
                }
            }
            out.insert(name.to_owned(), t);
        }
        Ok(Templates(out))
    }

    pub fn get(&self, name: &str) -> Option<&Template> {
        self.0.get(name)
    }
}

impl Template {
    /// HTML-escapes every value in the HTML; the subject and text take the raw value on one line.
    pub fn fill(&self, values: &[(&str, &str)]) -> Result<Filled, TemplateError> {
        let get = |name: &str| values.iter().find(|(k, _)| *k == name).map(|(_, v)| *v);
        Ok(Filled {
            subject: substitute(&self.subject, |n| get(n).map(one_line))?,
            html: substitute(&self.html, |n| get(n).map(escape_html))?,
            text: substitute(&self.text, |n| get(n).map(one_line))?,
        })
    }
}

/// `{{name}}` with a lowercase name; anything else stays as written.
fn placeholder_at(s: &str) -> Option<(&str, usize)> {
    let rest = s.strip_prefix("{{")?;
    let end = rest.find("}}")?;
    let name = &rest[..end];
    let ok = !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c == '_');
    ok.then_some((name, end + 4))
}

fn names(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(off) = s[i..].find("{{") {
        i += off;
        match placeholder_at(&s[i..]) {
            Some((name, len)) => {
                out.push(name.to_owned());
                i += len;
            }
            None => i += 2,
        }
    }
    out
}

/// One pass over the template: a value is never read again, so a value that itself contains
/// `{{...}}` stays text.
fn substitute(
    template: &str,
    value: impl Fn(&str) -> Option<String>,
) -> Result<String, TemplateError> {
    let mut out = String::with_capacity(template.len());
    let mut i = 0;
    while let Some(off) = template[i..].find("{{") {
        out.push_str(&template[i..i + off]);
        i += off;
        match placeholder_at(&template[i..]) {
            Some((name, len)) => {
                out.push_str(&value(name).ok_or_else(|| TemplateError::Unfilled(name.to_owned()))?);
                i += len;
            }
            None => {
                out.push_str("{{");
                i += 2;
            }
        }
    }
    out.push_str(&template[i..]);
    Ok(out)
}

pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// CR and LF removed: a name can't add a header to the subject or a line to the text.
fn one_line(s: &str) -> String {
    s.chars().filter(|c| *c != '\r' && *c != '\n').collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn template(subject: &str, html: &str, text: &str) -> Template {
        Template {
            subject: subject.into(),
            html: html.into(),
            text: text.into(),
        }
    }

    #[test]
    fn html_values_are_escaped_and_text_values_are_raw_on_one_line() {
        let t = template("{{agent}} waits", "<p>{{agent}}</p>", "{{agent}}\n");
        let f = t
            .fill(&[("agent", "Raya's <b>\r\nagent</b> & co")])
            .unwrap();
        assert_eq!(f.subject, "Raya's <b>agent</b> & co waits");
        assert_eq!(
            f.html,
            "<p>Raya&#39;s &lt;b&gt;\r\nagent&lt;/b&gt; &amp; co</p>"
        );
        assert_eq!(f.text, "Raya's <b>agent</b> & co\n");
    }

    #[test]
    fn a_value_that_looks_like_a_placeholder_is_never_filled_again() {
        let t = template("{{a}}", "{{a}} {{b}}", "{{a}}");
        let f = t.fill(&[("a", "{{b}}"), ("b", "secret")]).unwrap();
        assert_eq!(f.html, "{{b}} secret");
        assert_eq!(f.subject, "{{b}}");
    }

    #[test]
    fn a_missing_value_is_an_error_not_an_empty_gap() {
        let t = template("s", "{{url}}", "t");
        assert!(matches!(t.fill(&[]), Err(TemplateError::Unfilled(n)) if n == "url"));
    }

    #[test]
    fn braces_that_are_not_placeholders_stay_as_written() {
        let t = template("s", "a {{ b }} {{B}} {{ c", "{{");
        let f = t.fill(&[]).unwrap();
        assert_eq!(f.html, "a {{ b }} {{B}} {{ c");
        assert_eq!(f.text, "{{");
    }

    #[test]
    fn load_refuses_a_placeholder_the_manifest_does_not_list() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        std::fs::write(
            p.join("manifest.json"),
            r#"{"x": {"subject": "Hi {{name}}", "placeholders": ["name"]}}"#,
        )
        .unwrap();
        std::fs::write(p.join("x.html"), "{{name}} {{code}}").unwrap();
        std::fs::write(p.join("x.txt"), "{{name}}").unwrap();
        let err = Templates::load(p, &[("x", &["name"])]).unwrap_err();
        assert!(matches!(err, TemplateError::Unlisted { name, .. } if name == "code"));
        std::fs::write(p.join("x.html"), "{{name}}").unwrap();
        let t = Templates::load(p, &[("x", &["name"])]).unwrap();
        assert_eq!(
            t.get("x")
                .unwrap()
                .fill(&[("name", "Ana")])
                .unwrap()
                .subject,
            "Hi Ana"
        );
        assert!(matches!(
            Templates::load(p, &[("y", &["name"])]),
            Err(TemplateError::Missing(n)) if n == "y"
        ));
        // The manifest needs a value the core doesn't fill: refused at start.
        assert!(matches!(
            Templates::load(p, &[("x", &[])]),
            Err(TemplateError::Unfillable { name, .. }) if name == "name"
        ));
    }
}
