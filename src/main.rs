use clap::Parser;
use core::result::Result::Ok;
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::Path;
use std::process::{exit, Command};

const DEFAULT_CONFIG_PATH_STR: &str = ".lintyconfig.json";

#[derive(Parser, Debug)]
enum Subcommand {
    /// Initialize an empty .lintyconfig
    Init,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Treat warnings as errors
    #[arg(long)]
    error_on_warning: bool,

    /// Optional path to .lintyconfig.json file
    #[arg(short, long)]
    config_path: Option<String>,

    /// Print warnings and continue without confirmation
    #[arg(long)]
    no_confirm: bool,

    /// Include files listed in .gitignore and .ignore files
    #[arg(long)]
    ignored: bool,

    /// Include hidden files
    #[arg(long)]
    hidden: bool,

    /// Limit to files staged for commit
    #[arg(long, group = "input")]
    pre_commit: bool,

    /// Relative paths to files to lint (default: all files in current directory recursively)
    #[arg(group = "input")]
    files: Vec<String>,

    #[command(subcommand)]
    command: Option<Subcommand>,
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

    if let Some(Subcommand::Init) = args.command {
        init_config()?;
        return Ok(());
    }

    let Ok(config) = read_config(args.config_path.as_ref().map(|s| s.as_str())) else {
        eprintln!("Failed to find config file; do you need to create a .lintyconfig file?");
        exit(1);
    };

    let rules = generate_rules_from_config(&config)?;

    let current_dir = std::env::current_dir()?;

    let mut specified_paths: Vec<OsString> = Vec::new();

    if args.pre_commit {
        let git_output = Command::new("git")
            .args(&["diff", "--staged", "--name-only"])
            .output()?;

        if git_output.status.success() {
            let stdout = String::from_utf8(git_output.stdout)?;
            let staged_paths: Vec<&str> = stdout.lines().collect();

            for path in staged_paths {
                specified_paths.push(Path::new(path).canonicalize()?.as_os_str().to_owned());
            }
        } else {
            eprintln!(
                "Error running git: {}",
                String::from_utf8_lossy(&git_output.stderr)
            );
            exit(1);
        }
    } else {
        for file in args.files {
            specified_paths.push(
                current_dir
                    .join(Path::new(&file))
                    .canonicalize()?
                    .as_os_str()
                    .to_owned(),
            );
        }
    }

    let mut violations: Vec<Violation> = Vec::new();

    for result in WalkBuilder::new("./")
        .git_ignore(!args.ignored)
        .ignore(!args.ignored)
        .hidden(!args.hidden)
        .build()
    {
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
                        || (!specified_paths.is_empty()
                            && !specified_paths
                                .contains(&entry.path().canonicalize()?.as_os_str().to_owned()))
                    {
                        continue;
                    }

                    if file_contents.is_empty() {
                        let file = File::open(entry.path());

                        match file {
                            std::io::Result::Ok(mut file) => {
                                if let Err(err) = file.read_to_string(&mut file_contents) {
                                    eprintln!(
                                        "Error: Failed to read {}\nReason: {}",
                                        entry.path().to_str().unwrap(),
                                        err
                                    );
                                    continue;
                                };
                            }
                            Err(err) => {
                                eprintln!(
                                    "Error: Failed to open {}\nReason: {}",
                                    entry.path().to_str().unwrap(),
                                    err
                                );
                                continue;
                            }
                        }
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
        println!("Found warning {rule_id}: {message}");

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

        if args.no_confirm {
            continue;
        }

        loop {
            print!("Ignore warning? y/n ");
            std::io::stdout().flush()?;
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            match input.trim() {
                "y" => break,
                "n" => {
                    eprintln!("Failing due to warnings");
                    exit(1);
                }
                _ => continue,
            }
        }
    }

    for rule_id in errors_by_id.keys() {
        let message = &config
            .rules
            .iter()
            .find(|rule| &rule.id == rule_id)
            .unwrap()
            .message;
        println!("Found error {rule_id}: {message}");

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
        exit(1);
    }

    Ok(())
}

fn read_config(config_path: Option<&str>) -> anyhow::Result<Config> {
    let path = Path::new(config_path.unwrap_or(DEFAULT_CONFIG_PATH_STR));
    let file = File::open(path)?;

    let mut reader = BufReader::new(file);

    match path.extension().and_then(std::ffi::OsStr::to_str) {
        Some("toml") => {
            let mut buf = String::new();
            reader.read_to_string(&mut buf)?;
            Ok(toml::from_str(buf.as_str())?)
        }
        _ => Ok(serde_json::from_reader(reader)?),
    }
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

fn init_config() -> anyhow::Result<()> {
    if let Ok(_) = read_config(Some(DEFAULT_CONFIG_PATH_STR)) {
        println!("Config already exists!");
        return Ok(());
    }

    let config = RuleConfig {
        id: String::from("WarnOnTodos"),
        message: String::from("Are you sure you meant to leave a TODO?"),
        regex: String::from("(TODO|todo)"),
        severity: Severity::Warning,
        includes: None,
        excludes: None,
    };

    let file = File::create(DEFAULT_CONFIG_PATH_STR)?;
    serde_json::to_writer_pretty(file, &config)?;

    println!("Initialized example config at .lintyconfig");
    Ok(())
}
