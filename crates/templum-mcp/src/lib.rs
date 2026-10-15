//! The bus Templum's blocks talk over (docs/specs/connections.md): JSON-RPC lines on a local
//! socket, the MCP tools, the agents' hook events and the `templum` command line. No UI.

pub mod hooks;
pub mod rpc;
pub mod tools;

pub use hooks::AgentEvent;
pub use rpc::Incoming;
pub use tools::{Call, Reply};

/// What the Codex Templum opens is given (docs/specs/connections.md): Templum's MCP server, the
/// variables it needs (Codex hands a server only those it is told to), and its turns' end
/// (`notify`), as `-c` flags to append to its command. None when `exe` can't go inside single
/// quotes and a TOML string.
pub fn codex_flags(exe: &str, windows: bool) -> Option<String> {
    let exe = if windows {
        exe.replace('\\', "/")
    } else {
        exe.to_string()
    };
    if exe.is_empty() || exe.contains(['\'', '"', '\\', '\n']) {
        return None;
    }
    Some(format!(
        " -c 'mcp_servers.templum.command=\"{exe}\"' -c 'mcp_servers.templum.args=[\"mcp\"]' \
         -c 'mcp_servers.templum.env_vars=[\"TEMPLUM_BUS\",\"TEMPLUM_TERMINAL\"]' \
         -c 'mcp_servers.templum.tool_timeout_sec=3600' \
         -c 'notify=[\"{exe}\",\"hook\",\"codex\"]'"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_gets_the_server_and_notify_in_single_quotes() {
        let flags = codex_flags("/opt/Templum App/templum", false).unwrap();
        assert!(
            flags.starts_with(" -c 'mcp_servers.templum.command=\"/opt/Templum App/templum\"'"),
            "{flags}"
        );
        assert!(flags.contains("-c 'mcp_servers.templum.args=[\"mcp\"]'"));
        assert!(flags.contains("env_vars=[\"TEMPLUM_BUS\",\"TEMPLUM_TERMINAL\"]"));
        assert!(flags.ends_with("-c 'notify=[\"/opt/Templum App/templum\",\"hook\",\"codex\"]'"));
        let windows = codex_flags(r"C:\Program Files\Templum\templum.exe", true).unwrap();
        assert!(
            windows.contains("command=\"C:/Program Files/Templum/templum.exe\""),
            "{windows}"
        );
        assert_eq!(codex_flags("/home/o'neil/templum", false), None);
        assert_eq!(codex_flags("", false), None);
    }
}
