//! GitHub Copilot CLI: the words that give one session Templum's MCP server, its instructions
//! and a plugin of hooks (docs/specs/connections.md, GitHub Copilot CLI).

use std::path::Path;

use serde_json::json;

use crate::claude::{typed_path, write_changed};

/// The Copilot CLI hooks Templum follows a session's turns with. A hook's payload doesn't name
/// its event, so each one's command does.
const HOOKED: [&str; 8] = [
    "sessionStart",
    "userPromptSubmitted",
    "postToolUse",
    "postToolUseFailure",
    "agentStop",
    "notification",
    "errorOccurred",
    "sessionEnd",
];

/// Writes `<data>/copilot/mcp.json` and the plugin `<data>/copilot/plugin`, for Windows when
/// `windows`, and returns the words that give them to one Copilot CLI, unquoted, Windows' `\` as
/// `/`; the last names Templum's folder. None when a path can't be typed or go in a hook's
/// command, or a file can't be written.
pub fn args(data: &Path, windows: bool) -> Option<Vec<String>> {
    let folder = data.join("copilot");
    let (mcp, plugin) = (folder.join("mcp.json"), folder.join("plugin"));
    let words = vec![
        "--allow-all-mcp-server-instructions".to_string(),
        format!("--additional-mcp-config=@{}", crate::slashed(&mcp, windows)),
        format!("--plugin-dir={}", crate::slashed(&plugin, windows)),
    ];
    if words.iter().any(|w| typed_path(w, windows).is_none()) {
        return None;
    }
    let bin = crate::bin(data, windows);
    let bin = bin.to_string_lossy();
    let mut hooks = serde_json::Map::new();
    for event in HOOKED {
        let words = format!("copilot {event}");
        let hook = json!([{
            "type": "command",
            "bash": crate::hook_command(&bin, false, &words)?,
            "powershell": crate::hook_command(&bin, true, &words)?,
            "timeoutSec": 5,
        }]);
        hooks.insert(event.to_string(), hook);
    }
    // Copilot hands a local server only PATH and the variables its `env` names, `${VAR}` expanded.
    let server = json!({
        "type": "local",
        "command": bin,
        "args": ["mcp"],
        "env": { "TEMPLUM_BUS": "${TEMPLUM_BUS}", "TEMPLUM_TERMINAL": "${TEMPLUM_TERMINAL}" },
        "tools": ["*"],
        "timeout": crate::MCP_TIMEOUT_MS,
    });
    write_changed(&mcp, &json!({ "mcpServers": { "templum": server } }))?;
    write_changed(&plugin.join("plugin.json"), &json!({ "name": "templum" }))?;
    write_changed(
        &plugin.join("hooks.json"),
        &json!({ "version": 1, "hooks": hooks }),
    )?;
    Some(words)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{Duration, UNIX_EPOCH};

    fn read(path: &Path) -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn copilot_gets_the_server_and_a_plugin_of_hooks() {
        let parent = tempfile::tempdir().unwrap();
        let data = parent.path().join("Templum Data");
        let windows = cfg!(windows);
        let words = args(&data, windows).unwrap();
        let folder = data.join("copilot");
        // Windows' `\` written `/`, as for Claude Code's words.
        let word = |path: PathBuf| path.to_string_lossy().replace('\\', "/");
        assert_eq!(
            words,
            [
                "--allow-all-mcp-server-instructions".to_string(),
                format!("--additional-mcp-config=@{}", word(folder.join("mcp.json"))),
                format!("--plugin-dir={}", word(folder.join("plugin"))),
            ]
        );
        let bin = crate::bin(&data, windows);
        let bin = bin.to_string_lossy();
        assert_eq!(
            read(&folder.join("mcp.json")),
            json!({ "mcpServers": { "templum": {
                "type": "local",
                "command": bin,
                "args": ["mcp"],
                "env": { "TEMPLUM_BUS": "${TEMPLUM_BUS}", "TEMPLUM_TERMINAL": "${TEMPLUM_TERMINAL}" },
                "tools": ["*"],
                "timeout": 604_800_000,
            } } })
        );
        assert_eq!(
            read(&folder.join("plugin").join("plugin.json")),
            json!({ "name": "templum" })
        );
        let hooks = read(&folder.join("plugin").join("hooks.json"));
        assert_eq!(hooks["version"], 1);
        let events = [
            "sessionStart",
            "userPromptSubmitted",
            "postToolUse",
            "postToolUseFailure",
            "agentStop",
            "notification",
            "errorOccurred",
            "sessionEnd",
        ];
        assert_eq!(hooks["hooks"].as_object().unwrap().len(), events.len());
        for event in events {
            assert_eq!(
                hooks["hooks"][event],
                json!([{
                    "type": "command",
                    "bash": format!("'{bin}' hook copilot {event}"),
                    "powershell": format!("& '{bin}' hook copilot {event}"),
                    "timeoutSec": 5,
                }]),
                "{event}"
            );
        }
        // Written only when they changed.
        let files = [
            folder.join("mcp.json"),
            folder.join("plugin").join("plugin.json"),
            folder.join("plugin").join("hooks.json"),
        ];
        let long_ago = UNIX_EPOCH + Duration::from_secs(1_000_000);
        for file in &files {
            let open = std::fs::File::options().write(true).open(file).unwrap();
            open.set_modified(long_ago).unwrap();
        }
        assert_eq!(args(&data, windows).unwrap(), words);
        for file in &files {
            let modified = std::fs::metadata(file).unwrap().modified().unwrap();
            assert_eq!(modified, long_ago, "{file:?}");
        }
        // A `'` can't go in a hook's command.
        assert_eq!(args(&parent.path().join("o'neil"), windows), None);
    }
}
