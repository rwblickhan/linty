use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Severity {
    Warning,
    Error,
}

#[derive(Serialize, Deserialize)]
struct Rule {
    id: String,
    message: String,
    regex: String,
    severity: Severity,
    includes: Option<Vec<String>>,
    excludes: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize)]
struct Config {
    rules: Vec<Rule>,
}

fn main() -> anyhow::Result<()> {
    println!("Hello, world!");
    let file = File::open(Path::new(".lintyconfig.json"))?;
    let reader = BufReader::new(file);

    let config: Config = serde_json::from_reader(reader)?;
    Ok(())
}
