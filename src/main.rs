use anyhow::Ok;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
enum Severity {
    Warning,
    Error,
}

#[derive(Serialize, Deserialize, Debug)]
struct Rule {
    id: String,
    message: String,
    regex: String,
    severity: Severity,
    includes: Option<Vec<String>>,
    excludes: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Config {
    rules: Vec<Rule>,
}

struct Violation {
    rule: Rule,
}

fn main() -> anyhow::Result<()> {
    let file = File::open(Path::new(".lintyconfig.json"))?;
    let reader = BufReader::new(file);

    let config: Config = serde_json::from_reader(reader)?;
    println!("{:?}", config);

    let violations: Vec<Violation> = config
        .rules
        .into_iter()
        .flat_map(|rule| apply_rule(rule).unwrap())
        .collect();

    let (warnings, errors): (Vec<Violation>, Vec<Violation>) =
        violations
            .into_iter()
            .partition(|violation| match &violation.rule.severity {
                Severity::Warning => true,
                Severity::Error => false,
            });

    Ok(())
}

fn apply_rule(rule: Rule) -> anyhow::Result<Vec<Violation>> {
    Ok(vec![Violation { rule: rule }])
}
