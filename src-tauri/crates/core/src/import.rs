//! Phase-2 import helpers:
//! - JSON export/import of profiles (typed `TunnelInput` array).
//! - Best-effort import of `Host` + `LocalForward` entries from `~/.ssh/config`.

use std::path::Path;

use crate::tunnel::{TunnelInput, TunnelMode};

/// Parse a JSON array of `TunnelInput`s. Each entry is validated via
/// `into_config` by the caller before insertion.
pub fn parse_json_inputs(json: &str) -> Result<Vec<TunnelInput>, String> {
    serde_json::from_str(json).map_err(|e| format!("invalid JSON: {e}"))
}

/// Serialize a slice of inputs as pretty JSON (export path).
pub fn format_json_inputs(inputs: &[TunnelInput]) -> String {
    serde_json::to_string_pretty(inputs).unwrap_or_else(|_| "[]".into())
}

/// Minimal parsed host record from `~/.ssh/config`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostImport {
    pub alias: String,
    pub hostname: String,
    pub local_forwards: Vec<LocalForward>,
    pub identity_file: Option<String>,
    pub proxy_jump: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalForward {
    pub bind: String,
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
}

/// Read `path` (or current user's `~/.ssh/config`) and extract hosts that
/// declare `LocalForward`. Host entries with wildcards or duplicates are ignored.
pub fn import_ssh_config(path: Option<&Path>) -> Result<Vec<HostImport>, String> {
    let content = read_config(path)?;
    let hosts = parse_config(&content);
    Ok(hosts)
}

fn read_config(path: Option<&Path>) -> Result<String, String> {
    if let Some(p) = path {
        std::fs::read_to_string(p).map_err(|e| format!("read {p:?}: {e}"))
    } else {
        let home = dirs_home().ok_or_else(|| "cannot resolve home directory".to_owned())?;
        let candidates = [
            home.join(".ssh").join("config"),
            home.join(".ssh").join("config.txt"),
        ];
        for p in &candidates {
            if p.is_file() {
                return std::fs::read_to_string(p)
                    .map_err(|e| format!("read {p:?}: {e}"));
            }
        }
        Err("~/.ssh/config not found".into())
    }
}

fn dirs_home() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
}

fn parse_config(content: &str) -> Vec<HostImport> {
    let mut hosts: Vec<HostImport> = Vec::new();
    let mut current: Option<HostImport> = None;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Tokens split on whitespace/tabs; quotes stripped.
        let tokens = split_tokens(line);
        if tokens.is_empty() {
            continue;
        }
        let directive = tokens[0].to_lowercase();
        let args = &tokens[1..];

        match directive.as_str() {
            "host" => {
                if let Some(h) = current.take() {
                    // Skip wildcard hosts (e.g. Host *).
                    if !h.alias.contains('*') && !h.alias.contains('?') {
                        hosts.push(h);
                    }
                }
                let alias = args.first().cloned().unwrap_or_default().trim().to_owned();
                if alias.is_empty() || alias.starts_with('*') || alias.starts_with('?') {
                    current = None;
                    continue;
                }
                current = Some(HostImport {
                    alias,
                    hostname: String::new(),
                    local_forwards: Vec::new(),
                    identity_file: None,
                    proxy_jump: None,
                });
            }
            "hostname" => {
                if let Some(c) = current.as_mut() {
                    c.hostname = args.first().cloned().unwrap_or_default();
                }
            }
            "localforward" => {
                if let Some(c) = current.as_mut() {
                    if let Some(fwd) = parse_local_forward(args) {
                        c.local_forwards.push(fwd);
                    }
                }
            }
            "identityfile" => {
                if let Some(c) = current.as_mut() {
                    if c.identity_file.is_none() {
                        c.identity_file = args.first().cloned();
                    }
                }
            }
            "proxyjump" => {
                if let Some(c) = current.as_mut() {
                    if c.proxy_jump.is_none() {
                        c.proxy_jump = Some(args.join(" "));
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(h) = current.take() {
        if !h.alias.contains('*') && !h.alias.contains('?') {
            hosts.push(h);
        }
    }
    hosts.into_iter().filter(|h| !h.local_forwards.is_empty()).collect()
}

fn split_tokens(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quote: Option<char> = None;
    for ch in line.chars() {
        if let Some(q) = in_quote {
            if ch == q {
                in_quote = None;
            } else {
                cur.push(ch);
            }
        } else if ch == '"' || ch == '\'' {
            in_quote = Some(ch);
        } else if ch.is_whitespace() {
            if !cur.is_empty() {
                out.push(cur.clone());
                cur.clear();
            }
        } else {
            cur.push(ch);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn parse_local_forward(args: &[String]) -> Option<LocalForward> {
    if args.is_empty() {
        return None;
    }
    // Common shapes:
    //   LocalForward 5433 postgres.internal:5432
    //   LocalForward 127.0.0.1:5433 postgres.internal:5432
    let (bind, port) = parse_bind_port(&args[0])?;
    let rest = if args.len() == 1 { None } else { Some(args[1].as_str()) };
    let (remote_host, remote_port) = match rest {
        Some(r) if r.contains(':') => parse_host_port(r)?,
        Some(_) => return None,
        None => return None,
    };
    Some(LocalForward {
        bind,
        local_port: port,
        remote_host,
        remote_port,
    })
}

fn parse_bind_port(spec: &str) -> Option<(String, u16)> {
    if spec.contains(':') {
        let (host, port_str) = spec.rsplit_once(':')?;
        let port = port_str.parse().ok()?;
        let bind = if host.is_empty() { "127.0.0.1" } else { host }.to_owned();
        Some((bind, port))
    } else {
        let port = spec.parse().ok()?;
        Some(("127.0.0.1".into(), port))
    }
}

fn parse_host_port(spec: &str) -> Option<(String, u16)> {
    let (host, port_str) = spec.rsplit_once(':')?;
    let port = port_str.parse().ok()?;
    if host.is_empty() {
        return None;
    }
    Some((host.to_owned(), port))
}

/// Convert a parsed host into one `TunnelInput` per `LocalForward`.
pub fn host_to_inputs(host: &HostImport) -> Vec<TunnelInput> {
    host.local_forwards.iter().map(|fwd| TunnelInput {
        name: format!("{} — {}:{}", host.alias, fwd.remote_host, fwd.remote_port),
        mode: TunnelMode::Local,
        ssh_target: host.alias.clone(),
        bind_address: Some(fwd.bind.clone()),
        local_port: fwd.local_port,
        remote_host: Some(fwd.remote_host.clone()),
        remote_port: Some(fwd.remote_port),
        identity_file: host.identity_file.clone(),
        proxy_jump: host.proxy_jump.clone(),
        auto_start: Some(false),
        keep_alive_interval: Some(30),
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_two_hosts_with_localforwards() {
        let cfg = r#"
            Host dev-bastion
              HostName bastion.example.com
              IdentityFile ~/.ssh/id_ed25519
              ProxyJump jump-host
              LocalForward 5433 postgres.internal:5432
              LocalForward 127.0.0.1:6380 redis.internal:6379

            Host other
              LocalForward 8080 app.internal:80

            Host *
              ForwardAgent yes
        "#;
        let hosts = parse_config(cfg);
        assert_eq!(hosts.len(), 2);
        assert_eq!(hosts[0].alias, "dev-bastion");
        assert_eq!(hosts[0].local_forwards.len(), 2);
        assert_eq!(hosts[0].identity_file.as_deref(), Some("~/.ssh/id_ed25519"));
        assert_eq!(hosts[0].proxy_jump.as_deref(), Some("jump-host"));
        assert_eq!(hosts[1].alias, "other");
    }

    #[test]
    fn wildcard_hosts_are_skipped() {
        let cfg = "Host *\n  LocalForward 1234 host:80\n";
        assert!(parse_config(cfg).is_empty());
    }

    #[test]
    fn host_to_inputs_shapes_name_and_target() {
        let host = HostImport {
            alias: "dev-bastion".into(),
            hostname: "bastion.example.com".into(),
            local_forwards: vec![LocalForward {
                bind: "127.0.0.1".into(),
                local_port: 5433,
                remote_host: "postgres.internal".into(),
                remote_port: 5432,
            }],
            identity_file: None,
            proxy_jump: Some("jump".into()),
        };
        let inputs = host_to_inputs(&host);
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].name, "dev-bastion — postgres.internal:5432");
        assert_eq!(inputs[0].proxy_jump.as_deref(), Some("jump"));
    }

    #[test]
    fn roundtrip_json_with_proxy_jump() {
        let input = TunnelInput {
            name: "db".into(),
            mode: TunnelMode::Local,
            ssh_target: "bastion".into(),
            bind_address: None,
            local_port: 5433,
            remote_host: Some("db.internal".into()),
            remote_port: Some(5432),
            identity_file: Some("~/.ssh/id".into()),
            proxy_jump: Some("jump".into()),
            auto_start: None,
            keep_alive_interval: None,
        };
        let json = format_json_inputs(&[input.clone()]);
        let parsed = parse_json_inputs(&json).unwrap();
        assert_eq!(parsed[0].proxy_jump.as_deref(), Some("jump"));
        assert_eq!(parsed[0].ssh_target, "bastion");
    }

    #[test]
    fn invalid_json_is_friendly() {
        assert!(parse_json_inputs("not json").is_err());
    }
}
