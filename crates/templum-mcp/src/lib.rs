//! The bus Templum's blocks talk over (docs/specs/connections.md): JSON-RPC lines on a local
//! socket, the MCP tools, the agents' hook events and the `templum` command line. No UI.

pub mod ask;
pub mod bus;
pub mod cli;
pub mod hooks;
pub mod rpc;
pub mod tools;

pub use bus::{Bus, Hello, Message, Replier};
pub use hooks::AgentEvent;
pub use rpc::Incoming;
pub use tools::{Call, Reply};

/// Codex's hooks Templum gives it, with each one's timeout in seconds: Codex clamps an
/// `Interrupt` hook's to 3, and says so at every start for a longer one.
const CODEX_HOOKS: [(&str, u8); 6] = [
    ("SessionStart", 5),
    ("UserPromptSubmit", 5),
    ("PermissionRequest", 5),
    ("PostToolUse", 5),
    ("Stop", 5),
    ("Interrupt", 3),
];

/// What the Codex Templum opens is given (docs/specs/connections.md): Templum's MCP server, the
/// variables it needs (Codex hands a server only those it is told to), and, when `hooks`
/// (`templum` is on the terminal's PATH), its hooks, as `-c` flags to append to its command.
/// Their command is the bare `templum hook codex`, `<data>/bin` first on PATH: Codex asks its
/// person to trust them once, whatever Templum's path. None when `exe` can't go inside single
/// quotes and a TOML string.
pub fn codex_flags(exe: &str, windows: bool, hooks: bool) -> Option<String> {
    let exe = if windows {
        exe.replace('\\', "/")
    } else {
        exe.to_string()
    };
    if exe.is_empty() || exe.contains(['\'', '"', '\\', '\n']) {
        return None;
    }
    let mut flags = format!(
        " -c 'mcp_servers.templum.command=\"{exe}\"' -c 'mcp_servers.templum.args=[\"mcp\"]' \
         -c 'mcp_servers.templum.env_vars=[\"TEMPLUM_BUS\",\"TEMPLUM_TERMINAL\"]' \
         -c 'mcp_servers.templum.tool_timeout_sec=3600'"
    );
    if hooks {
        for (event, timeout) in CODEX_HOOKS {
            flags.push_str(&format!(
                " -c 'hooks.{event}=[{{hooks=[{{type=\"command\",command=\"templum hook codex\",timeout={timeout}}}]}}]'"
            ));
        }
    }
    Some(flags)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_gets_the_server_and_its_hooks_in_single_quotes() {
        let flags = codex_flags("/opt/Templum App/templum", false, true).unwrap();
        assert!(
            flags.starts_with(" -c 'mcp_servers.templum.command=\"/opt/Templum App/templum\"'"),
            "{flags}"
        );
        assert!(flags.contains("-c 'mcp_servers.templum.args=[\"mcp\"]'"));
        assert!(flags.contains("env_vars=[\"TEMPLUM_BUS\",\"TEMPLUM_TERMINAL\"]"));
        for event in [
            "SessionStart",
            "UserPromptSubmit",
            "PermissionRequest",
            "PostToolUse",
            "Stop",
        ] {
            let hook = format!(
                " -c 'hooks.{event}=[{{hooks=[{{type=\"command\",command=\"templum hook codex\",timeout=5}}]}}]'"
            );
            assert!(flags.contains(&hook), "{event}: {flags}");
        }
        // Codex gives an Interrupt hook 3 s at most, and says so at every start otherwise.
        assert!(
            flags.ends_with(
                " -c 'hooks.Interrupt=[{hooks=[{type=\"command\",command=\"templum hook codex\",timeout=3}]}]'"
            ),
            "{flags}"
        );
        assert!(!flags.contains("notify"), "the person's own notify runs");
        // Without `templum` on the terminal's PATH, the server only.
        let bare = codex_flags("/opt/templum", false, false).unwrap();
        assert!(
            bare.ends_with("tool_timeout_sec=3600'") && !bare.contains("hooks."),
            "{bare}"
        );
        let windows = codex_flags(r"C:\Program Files\Templum\templum.exe", true, true).unwrap();
        assert!(
            windows.contains("command=\"C:/Program Files/Templum/templum.exe\""),
            "{windows}"
        );
        assert_eq!(codex_flags("/home/o'neil/templum", false, true), None);
        assert_eq!(codex_flags("", false, true), None);
    }
}
