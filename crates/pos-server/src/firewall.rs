//! Can the tills reach this main PC? Windows Defender Firewall blocks inbound connections by
//! default; M6's installer adds the two rules below for the Private profile (PLT-008), and a
//! network Windows classed as Public ignores them. The check asks PowerShell for JSON, which
//! does not depend on the language of Windows (research 05 §9).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;
use std::path::Path;

pub const RULE_TCP: &str = "DZBuild POS LAN";
pub const RULE_MDNS: &str = "DZBuild POS mDNS";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RuleState {
    Present,
    Disabled,
    Missing,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum NetworkCategory {
    Private,
    Public,
    Domain,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NetworkProfile {
    pub alias: String,
    pub category: NetworkCategory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FirewallStatus {
    pub tcp_rule: RuleState,
    pub mdns_rule: RuleState,
    pub networks: Vec<NetworkProfile>,
    pub ok: bool,
}

impl FirewallStatus {
    /// `ok` when both rules are present and enabled for the Private profile and no connected
    /// network is Public: a disabled mDNS rule blocks discovery as a missing one does.
    pub fn from_parts(
        tcp_rule: RuleState,
        mdns_rule: RuleState,
        networks: Vec<NetworkProfile>,
    ) -> FirewallStatus {
        let ok = tcp_rule == RuleState::Present
            && mdns_rule == RuleState::Present
            && !networks
                .iter()
                .any(|n| n.category == NetworkCategory::Public);
        FirewallStatus {
            tcp_rule,
            mdns_rule,
            networks,
            ok,
        }
    }
}

fn items(json: &str) -> Option<Vec<Value>> {
    match serde_json::from_str::<Value>(json).ok()? {
        Value::Array(list) => Some(list),
        object @ Value::Object(_) => Some(vec![object]),
        _ => None,
    }
}

fn enabled(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Number(n)) => n.as_i64() == Some(1),
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => s.eq_ignore_ascii_case("true"),
        _ => false,
    }
}

/// Profile flags: Any = 0, Domain = 1, Private = 2, Public = 4.
fn covers_private(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Number(n)) => n.as_i64().is_some_and(|p| p == 0 || p & 2 == 2),
        Some(Value::String(s)) => s.contains("Private") || s == "Any",
        _ => false,
    }
}

pub fn parse_rule(json: &str) -> RuleState {
    if json.trim().is_empty() {
        return RuleState::Missing;
    }
    let Some(rules) = items(json) else {
        return RuleState::Unknown;
    };
    if rules
        .iter()
        .any(|r| enabled(r.get("Enabled")) && covers_private(r.get("Profile")))
    {
        RuleState::Present
    } else {
        RuleState::Disabled
    }
}

pub fn parse_profiles(json: &str) -> Vec<NetworkProfile> {
    items(json)
        .unwrap_or_default()
        .into_iter()
        .map(|p| NetworkProfile {
            alias: p
                .get("InterfaceAlias")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            category: match p.get("NetworkCategory") {
                Some(Value::Number(n)) => match n.as_i64() {
                    Some(0) => NetworkCategory::Public,
                    Some(1) => NetworkCategory::Private,
                    Some(2) => NetworkCategory::Domain,
                    _ => NetworkCategory::Unknown,
                },
                Some(Value::String(s)) if s == "Public" => NetworkCategory::Public,
                Some(Value::String(s)) if s == "Private" => NetworkCategory::Private,
                Some(Value::String(s)) if s.starts_with("Domain") => NetworkCategory::Domain,
                _ => NetworkCategory::Unknown,
            },
        })
        .collect()
}

/// Windows PowerShell's standard output, in UTF-8 (without a BOM) rather than the console's
/// OEM code page, so an interface alias in Arabic or French reads as written.
#[cfg(windows)]
fn powershell(script: &str) -> Option<String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let script =
        format!("[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false; {script}");
    let output = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(not(windows))]
fn powershell(_: &str) -> Option<String> {
    None
}

/// The three questions in one PowerShell (each start costs seconds on a low-end PC), one
/// `key=json` line each. `Get-NetFirewallRule` fails when no rule has the name, which is an
/// empty answer (missing); any other failure (no firewall service, no NetSecurity module)
/// answers `?` (unknown).
fn check_script(tcp_rule: &str, mdns_rule: &str) -> String {
    format!(
        "function Rule($name) {{ $json = Get-NetFirewallRule -DisplayName $name -ErrorAction SilentlyContinue -ErrorVariable failures | Select-Object Enabled,Profile | ConvertTo-Json -Compress; if ($failures | Where-Object {{ $_.CategoryInfo.Category -ne 'ObjectNotFound' }}) {{ '?' }} else {{ [string]$json }} }}; \
         'tcp=' + (Rule '{tcp_rule}'); \
         'mdns=' + (Rule '{mdns_rule}'); \
         'networks=' + (Get-NetConnectionProfile -ErrorAction SilentlyContinue | Select-Object InterfaceAlias,NetworkCategory | ConvertTo-Json -Compress); \
         exit 0"
    )
}

/// Reads [`check_script`]'s lines; a question left without an answer stays unknown.
fn answers(output: &str) -> FirewallStatus {
    let (mut tcp_rule, mut mdns_rule, mut networks) =
        (RuleState::Unknown, RuleState::Unknown, Vec::new());
    for line in output.lines() {
        if let Some(json) = line.strip_prefix("tcp=") {
            tcp_rule = parse_rule(json);
        } else if let Some(json) = line.strip_prefix("mdns=") {
            mdns_rule = parse_rule(json);
        } else if let Some(json) = line.strip_prefix("networks=") {
            networks = parse_profiles(json);
        }
    }
    FirewallStatus::from_parts(tcp_rule, mdns_rule, networks)
}

fn ask(script: &str) -> FirewallStatus {
    answers(&powershell(script).unwrap_or_default())
}

/// Whether the tills can reach this PC. Windows PowerShell takes seconds to answer: call it
/// off the UI and async threads. Off Windows every answer is unknown.
pub fn check() -> FirewallStatus {
    ask(&check_script(RULE_TCP, RULE_MDNS))
}

/// The rules M6's installer adds (NSIS `NSIS_HOOK_POSTINSTALL`), for the Private profile only.
pub fn install_commands(exe: &Path) -> [String; 2] {
    let program = exe.display();
    [
        format!(
            "netsh advfirewall firewall add rule name=\"{RULE_TCP}\" dir=in action=allow protocol=TCP localport={} profile=private program=\"{program}\" enable=yes",
            crate::wire::PORT
        ),
        format!(
            "netsh advfirewall firewall add rule name=\"{RULE_MDNS}\" dir=in action=allow protocol=UDP localport=5353 profile=private program=\"{program}\" enable=yes"
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_are_read_from_either_powershell_json_form() {
        assert_eq!(parse_rule(""), RuleState::Missing);
        assert_eq!(
            parse_rule("{\"Enabled\":1,\"Profile\":2}"),
            RuleState::Present
        );
        assert_eq!(
            parse_rule("{\"Enabled\":\"True\",\"Profile\":\"Private\"}"),
            RuleState::Present
        );
        assert_eq!(
            parse_rule("{\"Enabled\":2,\"Profile\":2}"),
            RuleState::Disabled
        );
        assert_eq!(
            parse_rule("{\"Enabled\":1,\"Profile\":4}"),
            RuleState::Disabled,
            "public only: tills on a private LAN are still blocked"
        );
        assert_eq!(
            parse_rule("[{\"Enabled\":2,\"Profile\":2},{\"Enabled\":1,\"Profile\":0}]"),
            RuleState::Present,
            "any profile counts"
        );
        assert_eq!(parse_rule("garbage"), RuleState::Unknown);
    }

    #[test]
    fn a_public_network_is_flagged() {
        let profiles = parse_profiles(
            "[{\"InterfaceAlias\":\"Ethernet\",\"NetworkCategory\":1},{\"InterfaceAlias\":\"Wi-Fi\",\"NetworkCategory\":0}]",
        );
        assert_eq!(
            profiles,
            [
                NetworkProfile {
                    alias: "Ethernet".into(),
                    category: NetworkCategory::Private
                },
                NetworkProfile {
                    alias: "Wi-Fi".into(),
                    category: NetworkCategory::Public
                },
            ]
        );
        assert_eq!(
            parse_profiles(
                "{\"InterfaceAlias\":\"Ethernet\",\"NetworkCategory\":\"DomainAuthenticated\"}"
            )[0]
            .category,
            NetworkCategory::Domain
        );
        let status = FirewallStatus::from_parts(RuleState::Present, RuleState::Present, profiles);
        assert!(!status.ok, "a public network blocks the tills");
    }

    #[test]
    fn the_installer_rules_are_exact() {
        let [tcp, mdns] = install_commands(std::path::Path::new(
            "C:\\Program Files\\DZBuild POS\\DZBuild POS.exe",
        ));
        assert_eq!(
            tcp,
            "netsh advfirewall firewall add rule name=\"DZBuild POS LAN\" dir=in action=allow protocol=TCP localport=7345 profile=private program=\"C:\\Program Files\\DZBuild POS\\DZBuild POS.exe\" enable=yes"
        );
        assert!(
            mdns.contains("protocol=UDP localport=5353")
                && mdns.contains("name=\"DZBuild POS mDNS\"")
        );
    }
}

#[cfg(test)]
mod check_tests {
    use super::*;

    #[test]
    fn one_powershell_answers_the_three_questions_a_line_each() {
        let status = answers(
            "tcp=\r\nmdns={\"Enabled\":1,\"Profile\":2}\r\nnetworks=[{\"InterfaceAlias\":\"Ethernet\",\"NetworkCategory\":1}]\r\n",
        );
        assert_eq!(
            status,
            FirewallStatus {
                tcp_rule: RuleState::Missing,
                mdns_rule: RuleState::Present,
                networks: vec![NetworkProfile {
                    alias: "Ethernet".into(),
                    category: NetworkCategory::Private
                }],
                ok: false,
            }
        );
        let unanswered = answers("tcp=?\r\nnetworks=\r\n");
        assert_eq!(
            (unanswered.tcp_rule, unanswered.mdns_rule),
            (RuleState::Unknown, RuleState::Unknown),
            "a failed lookup or a missing line is not a missing rule"
        );
        assert!(unanswered.networks.is_empty() && !unanswered.ok);
    }

    #[test]
    fn the_script_names_both_rules() {
        let script = check_script(RULE_TCP, RULE_MDNS);
        assert!(script.contains("'DZBuild POS LAN'") && script.contains("'DZBuild POS mDNS'"));
        assert!(
            !script.contains('"'),
            "no double quote to escape on the command line"
        );
    }
}

/// The contract of `ok`: both rules present and enabled for the Private profile, and no connected
/// network in the Public category. A rule that exists but is disabled (or covers only the Domain
/// or Public profile) blocks the tills exactly as a missing one does.
#[cfg(test)]
mod status_tests {
    use super::*;

    fn private_lan() -> Vec<NetworkProfile> {
        vec![NetworkProfile {
            alias: "Ethernet".into(),
            category: NetworkCategory::Private,
        }]
    }

    #[test]
    fn the_tills_reach_this_pc_only_when_both_rules_are_present() {
        assert!(
            FirewallStatus::from_parts(RuleState::Present, RuleState::Present, private_lan()).ok
        );
        assert!(
            FirewallStatus::from_parts(RuleState::Present, RuleState::Present, vec![]).ok,
            "no connected network is not a public one"
        );
        assert!(
            !FirewallStatus::from_parts(RuleState::Present, RuleState::Disabled, vec![]).ok,
            "a disabled mDNS rule blocks discovery"
        );
        for mdns_rule in [RuleState::Disabled, RuleState::Missing, RuleState::Unknown] {
            assert!(
                !FirewallStatus::from_parts(RuleState::Present, mdns_rule, private_lan()).ok,
                "mDNS rule {mdns_rule:?}"
            );
        }
        for tcp_rule in [RuleState::Disabled, RuleState::Missing, RuleState::Unknown] {
            assert!(
                !FirewallStatus::from_parts(tcp_rule, RuleState::Present, private_lan()).ok,
                "TCP rule {tcp_rule:?}"
            );
        }
    }

    #[test]
    fn an_mdns_rule_for_the_public_profile_only_is_not_ok() {
        let status = answers(
            "tcp={\"Enabled\":1,\"Profile\":2}\r\nmdns={\"Enabled\":1,\"Profile\":4}\r\nnetworks=[{\"InterfaceAlias\":\"Ethernet\",\"NetworkCategory\":1}]\r\n",
        );
        assert_eq!(
            (status.tcp_rule, status.mdns_rule, status.ok),
            (RuleState::Present, RuleState::Disabled, false)
        );
    }
}

/// Real Windows PowerShell on the PC.
#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    /// Windows PowerShell exits 1 when `Get-NetFirewallRule` finds no rule, even with
    /// `-ErrorAction SilentlyContinue`: that must read as missing, not unknown.
    #[test]
    fn rules_that_are_not_there_read_as_missing() {
        let status = ask(&check_script(
            "DZBuild POS test rule that no installer adds",
            "DZBuild POS other test rule that no installer adds",
        ));
        assert_eq!(
            (status.tcp_rule, status.mdns_rule),
            (RuleState::Missing, RuleState::Missing)
        );
        assert!(!status.ok);
    }

    #[test]
    fn powershell_answers_in_utf8() {
        let output = powershell("'Supérette مغازة'").unwrap_or_default();
        assert_eq!(output.trim(), "Supérette مغازة");
    }
}
