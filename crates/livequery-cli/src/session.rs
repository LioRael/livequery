use livequery_transport::{Request, Response, local};
use rustyline::{
    Context, Editor, Helper,
    completion::{Completer, Pair},
    error::ReadlineError,
    highlight::Highlighter,
    hint::Hinter,
    validate::Validator,
};
use std::{collections::BTreeSet, path::Path};
#[derive(Debug)]
struct Catalog {
    words: BTreeSet<String>,
}
impl Completer for Catalog {
    type Candidate = Pair;
    fn complete(
        &self,
        line: &str,
        pos: usize,
        _: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let start = line[..pos]
            .char_indices()
            .rfind(|(_, c)| !c.is_alphanumeric() && *c != '_')
            .map_or(0, |(i, c)| i + c.len_utf8());
        let prefix = &line[start..pos];
        Ok((
            start,
            self.words
                .iter()
                .filter(|word| word.to_lowercase().starts_with(&prefix.to_lowercase()))
                .map(|word| Pair {
                    display: word.clone(),
                    replacement: word.clone(),
                })
                .collect(),
        ))
    }
}
impl Hinter for Catalog {
    type Hint = String;
}
impl Highlighter for Catalog {}
impl Validator for Catalog {}
impl Helper for Catalog {}
fn catalog(socket: &Path) -> Result<Catalog, Box<dyn std::error::Error>> {
    let mut words: BTreeSet<String> = [
        "SELECT", "FROM", "WHERE", "ORDER BY", "DESC", "ASC", "LIMIT", "COUNT(*)", "MAX", "schema",
        "status", "watch", "quit",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    match local::request(socket, &Request::Schema)? {
        Response::Ok { data } => {
            if let Some(tables) = data.as_object() {
                for (name, columns) in tables {
                    words.insert(name.clone());
                    if let Some(columns) = columns.as_array() {
                        for column in columns {
                            if let Some(name) = column["name"].as_str() {
                                words.insert(name.to_owned());
                            }
                        }
                    }
                }
            }
        }
        Response::Error { code, message } => return Err(format!("{code}: {message}").into()),
        _ => return Err("unexpected schema response".into()),
    }
    Ok(Catalog { words })
}
pub(super) fn run(socket: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let config = rustyline::Config::builder().max_history_size(100)?.build();
    let mut editor = Editor::<Catalog, rustyline::history::MemHistory>::with_config(config)?;
    editor.set_helper(Some(catalog(socket)?));
    println!(
        "Session target: {}. SELECT, schema, status, watch SELECT (10 polls), quit. Tab completes catalog names; history stays in memory.",
        socket.display()
    );
    loop {
        match editor.readline("livequery> ") {
            Ok(line) => {
                let line = line.trim();
                if matches!(line, "quit" | "exit") {
                    break;
                }
                if line.is_empty() {
                    continue;
                }
                editor.add_history_entry(line)?;
                let mut args: Vec<String> = if line.to_ascii_uppercase().starts_with("SELECT ") {
                    vec!["query".into(), line.into()]
                } else if let Some(sql) = line.strip_prefix("watch ") {
                    vec!["watch".into(), sql.into(), "--count".into(), "10".into()]
                } else {
                    vec![line.into()]
                };
                if !matches!(args[0].as_str(), "query" | "watch" | "schema" | "status") {
                    eprintln!("use SELECT, schema, status, watch SELECT or quit");
                    continue;
                }
                args.extend(["--socket".into(), socket.to_string_lossy().into_owned()]);
                if let Err(error) = super::run_args(args) {
                    eprintln!("query failed: {error}");
                }
                if line == "schema" {
                    match catalog(socket) {
                        Ok(catalog) => editor.set_helper(Some(catalog)),
                        Err(error) => {
                            eprintln!("catalog refresh failed; keeping completion cache: {error}")
                        }
                    }
                }
            }
            Err(ReadlineError::Interrupted) => continue,
            Err(ReadlineError::Eof) => break,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn completion_preserves_unicode_boundaries_and_catalog_names() {
        let helper = Catalog {
            words: ["jobs".into(), "elapsed_ms".into()].into_iter().collect(),
        };
        let history = rustyline::history::MemHistory::new();
        let context = Context::new(&history);
        for line in ["SELECT el", "💡el"] {
            let (start, pairs) = helper.complete(line, line.len(), &context).unwrap();
            assert_eq!(&line[start..], "el");
            assert_eq!(pairs[0].replacement, "elapsed_ms");
        }
        assert_eq!(
            helper.complete("FROM jo", 7, &context).unwrap().1[0].replacement,
            "jobs"
        );
    }
}
