use anyhow::Ok;
use clap::Parser;
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::Walk;
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Treat warnings as errors
    #[arg(short, long)]
    error_on_warning: bool,

    /// Optional path to .lintyconfig.json file
    #[arg(short, long)]
    config_path: Option<String>,
}

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
    let args = Args::parse();
    let file = if let Some(config_path) = args.config_path {
        File::open(Path::new(config_path.as_str()))?
    } else {
        File::open(Path::new(".lintyconfig.json"))?
    };
    let reader = BufReader::new(file);

    let config: Config = serde_json::from_reader(reader)?;

    let rules = generate_rules_from_config(&config)?;

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

    let mut warnings_by_id: HashMap<String, Vec<Violation>> = HashMap::new();
    for warning in warnings {
        warnings_by_id
            .entry(warning.rule_id.to_owned())
            .or_insert(Vec::new())
            .push(warning);
    }

    let mut errors_by_id: HashMap<String, Vec<Violation>> = HashMap::new();
    for error in errors {
        errors_by_id
            .entry(error.rule_id.to_owned())
            .or_insert(Vec::new())
            .push(error);
    }

    for rule_id in warnings_by_id.keys() {
        let message = &config
            .rules
            .iter()
            .find(|rule| &rule.id == rule_id)
            .unwrap()
            .message;
        println!("Found warning {}: {}", rule_id, message);

        for violation in warnings_by_id.get(rule_id).unwrap() {
            println!(
                "Warning present in file: {}, lines: {}",
                violation.file.to_str().unwrap(),
                violation
                    .lines
                    .iter()
                    .map(|line| line.to_string())
                    .collect::<Vec<String>>()
                    .join(", ")
            );
        }
    }

    for rule_id in errors_by_id.keys() {
        let message = &config
            .rules
            .iter()
            .find(|rule| &rule.id == rule_id)
            .unwrap()
            .message;
        println!("Found error {}: {}", rule_id, message);

        for violation in errors_by_id.get(rule_id).unwrap() {
            println!(
                "Error present in file: {}, lines: {}",
                violation.file.to_str().unwrap(),
                violation
                    .lines
                    .iter()
                    .map(|line| line.to_string())
                    .collect::<Vec<String>>()
                    .join(", ")
            );
        }
    }

    if !&errors_by_id.is_empty() || (args.error_on_warning && !&warnings_by_id.is_empty()) {
        eprintln!("Failing due to errors");
        std::process::exit(1);
    }

    Ok(())
}

fn generate_rules_from_config(config: &Config) -> anyhow::Result<Vec<Rule>> {
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
    Ok(rules)
}
