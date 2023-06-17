use anyhow::Ok;
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::Walk;
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

#[derive(Serialize, Deserialize, Debug, Copy, Clone)]
#[serde(rename_all = "snake_case")]
enum Severity {
    Warning,
    Error,
}

#[derive(Serialize, Deserialize, Debug)]
struct RuleConfig {
    id: String,
    message: String,
    regex: String,
    severity: Severity,
    includes: Option<Vec<String>>,
    excludes: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Config {
    rules: Vec<RuleConfig>,
}

struct Rule {
    id: String,
    regex: Regex,
    severity: Severity,
    includes: GlobSet,
    excludes: GlobSet,
}

#[derive(Debug)]
struct Violation {
    rule_id: String,
    severity: Severity,
    file: OsString,
    lines: Vec<usize>,
}

fn main() -> anyhow::Result<()> {
    let file = File::open(Path::new(".lintyconfig.json"))?;
    let reader = BufReader::new(file);

    let config: Config = serde_json::from_reader(reader)?;

    let mut rules: Vec<Rule> = Vec::new();

    for rule_config in &config.rules {
        let mut include_globs = GlobSetBuilder::new();
        let mut exclude_globs = GlobSetBuilder::new();

        for include in rule_config.includes.to_owned().unwrap_or(Vec::new()) {
            include_globs.add(Glob::new(include.as_str())?);
        }

        for exclude in rule_config.excludes.to_owned().unwrap_or(Vec::new()) {
            exclude_globs.add(Glob::new(exclude.as_str())?);
        }

        let regex = RegexBuilder::new(&rule_config.regex);

        rules.push(Rule {
            id: rule_config.id.to_owned(),
            regex: regex.build()?,
            severity: rule_config.severity,
            includes: include_globs.build()?,
            excludes: exclude_globs.build()?,
        });
    }

    let mut violations: Vec<Violation> = Vec::new();

    for result in Walk::new("./") {
        match result {
            Err(err) => eprintln!("Error: {err}"),
            Result::Ok(entry) => {
                if entry.metadata()?.is_dir() {
                    continue;
                }

                let mut file_contents = String::new();
                for rule in &rules {
                    if (!rule.includes.is_empty() && !rule.includes.is_match(entry.path()))
                        || rule.excludes.is_match(entry.path())
                    {
                        continue;
                    }

                    if file_contents.is_empty() {
                        File::open(entry.path())?.read_to_string(&mut file_contents)?;
                    }

                    let mut lines = Vec::new();
                    for regex_match in rule.regex.find_iter(&file_contents) {
                        let offending_line = file_contents[..regex_match.start()]
                            .chars()
                            .filter(|&c| c == '\n')
                            .count()
                            + 1;
                        lines.push(offending_line);
                    }
                    if !lines.is_empty() {
                        violations.push(Violation {
                            rule_id: rule.id.to_owned(),
                            severity: rule.severity,
                            file: entry.file_name().to_owned(),
                            lines,
                        })
                    }
                }
            }
        }
    }

    let (warnings, errors): (Vec<Violation>, Vec<Violation>) =
        violations
            .into_iter()
            .partition(|violation| match &violation.severity {
                Severity::Warning => true,
                Severity::Error => false,
            });

    for warning in &warnings {
        let message = &config
            .rules
            .iter()
            .find(|rule| rule.id == warning.rule_id)
            .unwrap()
            .message;
        println!("Found warning {}: {}", warning.rule_id, message);
        println!(
            "Warning present in file: {}, lines: {}",
            warning.file.to_str().unwrap(),
            warning
                .lines
                .iter()
                .map(|line| line.to_string())
                .collect::<Vec<String>>()
                .join(", ")
        );
    }

    for error in &errors {
        let message = &config
            .rules
            .iter()
            .find(|rule| rule.id == error.rule_id)
            .unwrap()
            .message;
        println!("Found error {}: {}", error.rule_id, message);
        println!(
            "Error present in file: {}, lines: {}",
            error.file.to_str().unwrap(),
            error
                .lines
                .iter()
                .map(|line| line.to_string())
                .collect::<Vec<String>>()
                .join(", ")
        );
    }

    if !&errors.is_empty() {
        eprintln!("Found errors!");
        std::process::exit(1);
    }

    Ok(())
}
