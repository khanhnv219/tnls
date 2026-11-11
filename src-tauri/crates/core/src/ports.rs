//! Local port conflict detection before a tunnel starts.
//!
//! Two layers, cheapest first:
//! 1. another managed tunnel already holds the same bind:port
//!    ([`binds_overlap`], checked by the manager);
//! 2. the OS: a try-bind probe, then a best-effort lookup of the listening
//!    process using tools the OS already ships (`netstat` + `tasklist` on
//!    Windows, `lsof` or `ss` on macOS/Linux when present). No new system
//!    dependencies; when nothing can be identified the owner is unknown.

use std::net::{SocketAddr, TcpListener};
use std::process::Command;

use serde::{Deserialize, Serialize};

/// Who holds a port, as far as the OS will tell us.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Listener {
    pub pid: Option<u32>,
    pub process: Option<String>,
}

/// What occupies a tunnel's local endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ConflictOwner {
    /// Another profile managed by tnls.
    Tunnel {
        id: String,
        name: String,
        tags: Vec<String>,
        pid: Option<u32>,
    },
    /// Something outside tnls; fields are `None` when the OS would not say.
    Process {
        pid: Option<u32>,
        process: Option<String>,
    },
}

/// A start blocked because `bind:port` is taken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortConflict {
    pub bind: String,
    pub port: u16,
    pub owner: ConflictOwner,
}

impl std::fmt::Display for PortConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{} is already in use by ", self.bind, self.port)?;
        match &self.owner {
            ConflictOwner::Tunnel { name, pid, .. } => {
                write!(f, "tunnel \"{name}\"")?;
                if let Some(pid) = pid {
                    write!(f, " (pid {pid})")?;
                }
                Ok(())
            }
            ConflictOwner::Process {
                pid: Some(pid),
                process: Some(name),
            } => write!(f, "{name} (pid {pid})"),
            ConflictOwner::Process {
                pid: Some(pid),
                process: None,
            } => write!(f, "process {pid}"),
            ConflictOwner::Process { .. } => write!(f, "an unknown process"),
        }
    }
}

/// OS-side port checks, injectable so manager tests stay deterministic.
pub trait PortProbe: Send {
    fn is_bindable(&mut self, bind: &str, port: u16) -> bool;
    fn find_listener(&mut self, port: u16) -> Option<Listener>;
}

/// Production probe: real bind + OS tools.
pub struct OsPortProbe;

impl PortProbe for OsPortProbe {
    fn is_bindable(&mut self, bind: &str, port: u16) -> bool {
        is_bindable(bind, port)
    }
    fn find_listener(&mut self, port: u16) -> Option<Listener> {
        find_listener(port)
    }
}

/// `bind:port` as a socket address; `None` when the bind is not an IP
/// (`localhost` is accepted and means 127.0.0.1).
pub fn socket_addr(bind: &str, port: u16) -> Option<SocketAddr> {
    let host = normalize_bind(bind);
    let text = if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    text.parse().ok()
}

fn normalize_bind(bind: &str) -> String {
    let b = bind.trim().trim_start_matches('[').trim_end_matches(']');
    match b.to_ascii_lowercase().as_str() {
        "localhost" => "127.0.0.1".to_owned(),
        "*" | "" => "0.0.0.0".to_owned(),
        other => other.to_owned(),
    }
}

fn is_wildcard(bind: &str) -> bool {
    matches!(bind, "0.0.0.0" | "::")
}

/// Whether two `bind` addresses would collide on the same port: equal
/// addresses, or either one is a wildcard that listens on every interface.
pub fn binds_overlap(a: &str, b: &str) -> bool {
    let (a, b) = (normalize_bind(a), normalize_bind(b));
    a == b || is_wildcard(&a) || is_wildcard(&b)
}

/// Can `bind:port` be bound right now? (Bind-then-drop probe.)
pub fn is_bindable(bind: &str, port: u16) -> bool {
    socket_addr(bind, port).is_some_and(|addr| TcpListener::bind(addr).is_ok())
}

/// Best-effort: the process listening on TCP `port`, if the OS tools can say.
pub fn find_listener(port: u16) -> Option<Listener> {
    #[cfg(windows)]
    {
        let netstat = run(&["netstat", "-ano", "-p", "TCP"])?;
        let pid = parse_netstat_listener(&netstat, port)?;
        let process = run(&[
            "tasklist",
            "/FI",
            &format!("PID eq {pid}"),
            "/FO",
            "CSV",
            "/NH",
        ])
        .and_then(|out| parse_tasklist_name(&out));
        Some(Listener {
            pid: Some(pid),
            process,
        })
    }
    #[cfg(not(windows))]
    {
        let spec = format!("-iTCP:{port}");
        if let Some(out) = run(&["lsof", "-nP", &spec, "-sTCP:LISTEN", "-Fpc"]) {
            if let Some(found) = parse_lsof_listener(&out) {
                return Some(found);
            }
        }
        let filter = format!("sport = :{port}");
        run(&["ss", "-ltnpH", &filter]).and_then(|out| parse_ss_listener(&out))
    }
}

/// Run a system tool; `None` when it is missing or fails.
fn run(argv: &[&str]) -> Option<String> {
    let mut cmd = Command::new(argv[0]);
    cmd.args(&argv[1..]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        cmd.creation_flags(0x08000000 /* CREATE_NO_WINDOW */);
    }
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `netstat -ano -p TCP` rows: `TCP  127.0.0.1:20129  0.0.0.0:0  LISTENING  16704`.
/// The state column is localized on some Windows installs, so a row counts
/// as a listener when its foreign address is the `0.0.0.0:0` / `[::]:0`
/// placeholder that only listening sockets have.
pub fn parse_netstat_listener(output: &str, port: u16) -> Option<u32> {
    let suffix = format!(":{port}");
    output.lines().find_map(|line| {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 5 || !cols[0].eq_ignore_ascii_case("TCP") {
            return None;
        }
        let listening = cols[2] == "0.0.0.0:0" || cols[2] == "[::]:0";
        if listening && cols[1].ends_with(&suffix) {
            cols[cols.len() - 1].parse().ok()
        } else {
            None
        }
    })
}

/// `tasklist /FO CSV /NH` row: `"node.exe","4321","Console","1","52,344 K"`.
pub fn parse_tasklist_name(output: &str) -> Option<String> {
    let line = output.lines().find(|l| l.starts_with('"'))?;
    let name = line.split("\",\"").next()?.trim_start_matches('"');
    (!name.is_empty()).then(|| name.to_owned())
}

/// `lsof -Fpc` output: `p4321` then `cnode` lines per process.
pub fn parse_lsof_listener(output: &str) -> Option<Listener> {
    let mut pid = None;
    let mut process = None;
    for line in output.lines() {
        if let Some(p) = line.strip_prefix('p') {
            if pid.is_some() {
                break;
            }
            pid = p.parse().ok();
        } else if let Some(c) = line.strip_prefix('c') {
            process.get_or_insert_with(|| c.to_owned());
        }
    }
    pid.map(|pid| Listener {
        pid: Some(pid),
        process,
    })
}

/// `ss -ltnpH` row ending in `users:(("nginx",pid=812,fd=6))`.
pub fn parse_ss_listener(output: &str) -> Option<Listener> {
    let line = output.lines().find(|l| l.contains("users:(("))?;
    let users = &line[line.find("users:((")? + 8..];
    let process = users
        .split('"')
        .nth(1)
        .map(str::to_owned)
        .filter(|s| !s.is_empty());
    let pid = users
        .split("pid=")
        .nth(1)
        .and_then(|rest| rest.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|digits| digits.parse().ok());
    Some(Listener { pid, process })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlap_rules() {
        // Same bind + same port is a conflict (the port is compared by caller).
        assert!(binds_overlap("127.0.0.1", "127.0.0.1"));
        assert!(binds_overlap("localhost", "127.0.0.1"));
        // Wildcards listen everywhere, so they collide with any address.
        assert!(binds_overlap("0.0.0.0", "127.0.0.1"));
        assert!(binds_overlap("127.0.0.1", "*"));
        // Same port, different specific bind: both can listen.
        assert!(!binds_overlap("127.0.0.1", "127.0.0.2"));
        assert!(!binds_overlap("127.0.0.1", "192.168.1.10"));
    }

    #[test]
    fn conflict_messages_name_the_owner() {
        let tunnel = PortConflict {
            bind: "127.0.0.1".into(),
            port: 20129,
            owner: ConflictOwner::Tunnel {
                id: "a".into(),
                name: "9r-homelab".into(),
                tags: vec![],
                pid: Some(16704),
            },
        };
        assert_eq!(
            tunnel.to_string(),
            "127.0.0.1:20129 is already in use by tunnel \"9r-homelab\" (pid 16704)"
        );
        let unknown = PortConflict {
            bind: "127.0.0.1".into(),
            port: 3000,
            owner: ConflictOwner::Process {
                pid: None,
                process: None,
            },
        };
        assert_eq!(
            unknown.to_string(),
            "127.0.0.1:3000 is already in use by an unknown process"
        );
    }

    #[test]
    fn bind_probe_sees_a_held_port() {
        let held = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = held.local_addr().unwrap().port();
        assert!(!is_bindable("127.0.0.1", port));
        drop(held);
        assert!(is_bindable("127.0.0.1", port));
    }

    /// End-to-end on the real OS tools: this test process holds a port, and
    /// the lookup must name this process.
    #[cfg(windows)]
    #[test]
    fn finds_this_process_as_the_listener() {
        let held = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = held.local_addr().unwrap().port();
        let found = find_listener(port).expect("netstat should report the listener");
        assert_eq!(found.pid, Some(std::process::id()));
        let exe = std::env::current_exe().unwrap();
        let exe_name = exe.file_name().unwrap().to_string_lossy();
        assert_eq!(found.process.as_deref(), Some(exe_name.as_ref()));
    }

    #[test]
    fn netstat_finds_listener_pid_only_for_listening_rows() {
        let out = "\
Active Connections

  Proto  Local Address          Foreign Address        State           PID
  TCP    0.0.0.0:135            0.0.0.0:0              LISTENING       1204
  TCP    127.0.0.1:20129        0.0.0.0:0              LISTENING       16704
  TCP    127.0.0.1:20129        127.0.0.1:51734        ESTABLISHED     16704
  TCP    127.0.0.1:51734        127.0.0.1:20129        ESTABLISHED     9001
  TCP    [::]:8080              [::]:0                 ABHÖREN         4321
";
        assert_eq!(parse_netstat_listener(out, 20129), Some(16704));
        // Localized state column still matches via the foreign-address placeholder.
        assert_eq!(parse_netstat_listener(out, 8080), Some(4321));
        // A client socket whose *remote* side is 20129 is not the listener.
        assert_eq!(parse_netstat_listener(out, 51734), None);
        assert_eq!(parse_netstat_listener(out, 1), None);
    }

    #[test]
    fn tasklist_csv_name() {
        let out = "\"node.exe\",\"4321\",\"Console\",\"1\",\"52,344 K\"\r\n";
        assert_eq!(parse_tasklist_name(out).as_deref(), Some("node.exe"));
        assert_eq!(
            parse_tasklist_name("INFO: No tasks are running which match the specified criteria."),
            None
        );
    }

    #[test]
    fn lsof_and_ss_listeners() {
        assert_eq!(
            parse_lsof_listener("p812\ncnginx\nf6\n"),
            Some(Listener {
                pid: Some(812),
                process: Some("nginx".into())
            })
        );
        assert_eq!(parse_lsof_listener(""), None);
        let ss = "LISTEN 0 511 0.0.0.0:80 0.0.0.0:* users:((\"nginx\",pid=812,fd=6),(\"nginx\",pid=813,fd=6))";
        assert_eq!(
            parse_ss_listener(ss),
            Some(Listener {
                pid: Some(812),
                process: Some("nginx".into())
            })
        );
    }
}
