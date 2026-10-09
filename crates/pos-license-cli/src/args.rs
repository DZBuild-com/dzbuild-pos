//! The command line: a subcommand, its positional values, and `--flag value` (or `--flag=value`)
//! pairs, each subcommand with the flags it accepts. `--dev` is the one flag without a value.

use crate::CliError;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Help,
    Keygen,
    Issue,
    Inspect,
    List,
    Revoke,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    pub command: Command,
    pub positional: Vec<String>,
    pub flags: BTreeMap<String, String>,
}

impl Parsed {
    pub fn flag(&self, name: &str) -> Option<&str> {
        self.flags.get(name).map(String::as_str)
    }
}

const SWITCHES: [&str; 1] = ["dev"];

/// (command, accepted flags, positional values).
fn spec(name: &str) -> Option<(Command, &'static [&'static str], usize)> {
    Some(match name {
        "help" | "--help" | "-h" => (Command::Help, &[], 0),
        "keygen" => (Command::Keygen, &["keys", "id"], 0),
        "issue" => (
            Command::Issue,
            &[
                "keys", "device", "kind", "customer", "phone", "days", "replace", "out", "dev",
            ],
            0,
        ),
        "inspect" => (Command::Inspect, &["keys"], 1),
        "list" => (Command::List, &["keys", "device"], 0),
        "revoke" => (Command::Revoke, &["keys"], 1),
        _ => return None,
    })
}

pub fn parse(argv: &[String]) -> Result<Parsed, CliError> {
    let Some(first) = argv.first() else {
        return Ok(Parsed {
            command: Command::Help,
            positional: Vec::new(),
            flags: BTreeMap::new(),
        });
    };
    let (command, allowed, expected) = spec(first)
        .ok_or_else(|| CliError::new(format!("unknown command `{first}` (dzpos-license help)")))?;
    let mut positional = Vec::new();
    let mut flags = BTreeMap::new();
    let mut rest = argv[1..].iter();
    while let Some(arg) = rest.next() {
        let Some(flag) = arg.strip_prefix("--") else {
            positional.push(arg.clone());
            continue;
        };
        let (name, value) = match flag.split_once('=') {
            Some((name, value)) => (name.to_owned(), value.to_owned()),
            None if SWITCHES.contains(&flag) => (flag.to_owned(), "true".to_owned()),
            None => (
                flag.to_owned(),
                rest.next()
                    .cloned()
                    .ok_or_else(|| CliError::new(format!("--{flag} needs a value")))?,
            ),
        };
        if !allowed.contains(&name.as_str()) {
            return Err(CliError::new(format!(
                "`{first}` has no --{name} (dzpos-license help)"
            )));
        }
        if flags.insert(name.clone(), value).is_some() {
            return Err(CliError::new(format!("--{name} is given twice")));
        }
    }
    if positional.len() != expected {
        return Err(CliError::new(format!(
            "`{first}` takes {expected} value(s) besides its flags (dzpos-license help)"
        )));
    }
    Ok(Parsed {
        command,
        positional,
        flags,
    })
}
