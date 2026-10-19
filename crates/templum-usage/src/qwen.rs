//! Qwen Code: the words that give one session Templum's MCP server, and the system settings file
//! with Templum's hooks that its variable moves (docs/specs/connections.md, Qwen Code).

use std::path::{Path, PathBuf};

use serde_json::json;

use crate::claude::{typed_path, write_changed};

/// The Qwen Code hooks Templum follows a session's turns with: Claude Code's names.
const HOOKED: [&str; 8] = [
    "SessionStart",
    "UserPromptSubmit",
    "PermissionRequest",
    "PostToolUse",
    "Stop",
    "StopFailure",
    "Notification",
    "SessionEnd",
];

/// Writes `<data>/qwen/mcp.json`, for Windows when `windows`, and returns the word that gives it
/// to one Qwen Code, unquoted: `--mcp-config=<path>`, one word (it takes one). None when the path
/// can't be typed or Templum's program holds a `'`, or the file can't be written.
pub fn args(data: &Path, windows: bool) -> Option<Vec<String>> {
    let mcp = data.join("qwen").join("mcp.json");
    let word = format!("--mcp-config={}", crate::slashed(&mcp, windows));
    let bin = crate::bin(data, windows);
    let bin = bin.to_string_lossy();
    if typed_path(&word, windows).is_none() || bin.contains(['\'', '\n', '\r']) {
        return None;
    }
    // A tool call's timeout is in ms (10 minutes unless set); its stdio servers inherit the
    // environment.
    let server = json!({ "command": bin, "args": ["mcp"], "timeout": crate::MCP_TIMEOUT_MS });
    write_changed(&mcp, &json!({ "mcpServers": { "templum": server } }))?;
    Some(vec![word])
}

/// Writes `<data>/qwen/settings.json`, Templum's hooks only, for `QWEN_CODE_SYSTEM_SETTINGS_PATH`,
/// and returns it. None when Qwen Code's own system settings file `system` exists (Templum's would
/// hide it), when a path can't be typed or go in a hook's command, or the file can't be written.
pub fn settings(data: &Path, windows: bool, system: &Path) -> Option<PathBuf> {
    if system.exists() {
        return None;
    }
    let file = data.join("qwen").join("settings.json");
    typed_path(&file.to_string_lossy(), windows)?;
    let bin = crate::bin(data, windows);
    let command = crate::hook_command(&bin.to_string_lossy(), windows, "qwen")?;
    // Its hooks' timeout is in seconds.
    let hook = json!([{ "hooks": [{ "type": "command", "command": command, "timeout": 5 }] }]);
    let hooks: serde_json::Map<String, serde_json::Value> = HOOKED
        .iter()
        .map(|event| (event.to_string(), hook.clone()))
        .collect();
    write_changed(&file, &json!({ "hooks": hooks }))?;
    Some(file)
}

/// Qwen Code's own system settings file, which `QWEN_CODE_SYSTEM_SETTINGS_PATH` replaces.
pub fn system_file() -> PathBuf {
    PathBuf::from(if cfg!(windows) {
        r"C:\ProgramData\qwen-code\settings.json"
    } else if cfg!(target_os = "macos") {
        "/Library/Application Support/QwenCode/settings.json"
    } else {
        "/etc/qwen-code/settings.json"
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(path: &Path) -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn qwen_gets_the_server_and_its_hooks_in_a_settings_file() {
        let parent = tempfile::tempdir().unwrap();
        let data = parent.path().join("Templum Data");
        let windows = cfg!(windows);
        let folder = data.join("qwen");
        let mcp = folder.join("mcp.json");
        assert_eq!(
            args(&data, windows).unwrap(),
            [format!("--mcp-config={}", crate::slashed(&mcp, windows))]
        );
        let bin = crate::bin(&data, windows);
        let bin = bin.to_string_lossy();
        assert_eq!(
            read(&mcp),
            json!({ "mcpServers": { "templum": {
                "command": bin, "args": ["mcp"], "timeout": 604_800_000
            } } })
        );
        let missing = parent.path().join("no-system-settings.json");
        let file = settings(&data, windows, &missing).unwrap();
        assert_eq!(file, folder.join("settings.json"));
        let command = crate::hook_command(&bin, windows, "qwen").unwrap();
        let hook = json!([{ "hooks": [{ "type": "command", "command": command, "timeout": 5 }] }]);
        assert_eq!(
            read(&file),
            json!({ "hooks": {
                "SessionStart": hook,
                "UserPromptSubmit": hook,
                "PermissionRequest": hook,
                "PostToolUse": hook,
                "Stop": hook,
                "StopFailure": hook,
                "Notification": hook,
                "SessionEnd": hook,
            } })
        );
        // Its own system file would be hidden: the server only.
        std::fs::write(&missing, "{}").unwrap();
        assert_eq!(settings(&data, windows, &missing), None);
        let quoted = parent.path().join("o'neil");
        assert_eq!(args(&quoted, windows), None);
        assert_eq!(
            settings(&quoted, windows, &parent.path().join("none")),
            None
        );
    }

    #[test]
    fn its_system_file_is_where_qwen_code_looks() {
        let file = system_file();
        if cfg!(windows) {
            assert_eq!(file, Path::new(r"C:\ProgramData\qwen-code\settings.json"));
        } else if cfg!(target_os = "macos") {
            assert_eq!(
                file,
                Path::new("/Library/Application Support/QwenCode/settings.json")
            );
        } else {
            assert_eq!(file, Path::new("/etc/qwen-code/settings.json"));
        }
    }
}
