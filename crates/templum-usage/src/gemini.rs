//! Gemini CLI: the extension that gives it Templum's MCP server and hooks.
//!
//! Gemini CLI has no flag for either, and ignores a system settings file root doesn't own, so
//! Templum writes an extension to its data folder that the person links once, agreeing:
//! `gemini extensions link <data>/gemini --consent`. It names `<data>/bin/templum`, the link
//! Templum points at itself at each start, so it outlives upgrades.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::claude::write_changed;

/// The extension's name, and its folder's in `~/.gemini/extensions`.
pub const NAME: &str = "templum";
/// What `gemini` is given to remove the extension.
pub const UNLINK: [&str; 3] = ["extensions", "uninstall", NAME];
/// The Gemini CLI hooks Templum follows a session's turns with.
const HOOKED: [&str; 6] = [
    "SessionStart",
    "BeforeAgent",
    "AfterTool",
    "AfterAgent",
    "Notification",
    "SessionEnd",
];

/// Writes the extension to `<data>/gemini`, for Windows when `windows`, and returns its folder.
/// None when Templum's program can't go in a hook's command, or a file can't be written.
pub fn extension(data: &Path, windows: bool) -> Option<PathBuf> {
    let bin = crate::bin(data, windows);
    let bin = bin.to_string_lossy();
    let command = crate::hook_command(&bin, windows, "gemini")?;
    let hook = json!([{ "hooks": [{ "type": "command", "command": command, "timeout": 5000 }] }]);
    let hooks: serde_json::Map<String, serde_json::Value> = HOOKED
        .iter()
        .map(|event| (event.to_string(), hook.clone()))
        .collect();
    let folder = data.join("gemini");
    // Gemini CLI gives up on a tool after 10 minutes unless told: a question may wait days.
    let server = json!({ "command": bin, "args": ["mcp"], "timeout": crate::MCP_TIMEOUT_MS });
    write_changed(
        &folder.join("gemini-extension.json"),
        &json!({ "name": NAME, "version": "1.0.0", "mcpServers": { "templum": server } }),
    )?;
    write_changed(
        &folder.join("hooks").join("hooks.json"),
        &json!({ "hooks": hooks }),
    )?;
    Some(folder)
}

/// What `gemini` is given to install the extension in `folder`, the person agreeing: a link,
/// so the files Templum rewrites are the ones Gemini CLI reads.
pub fn link_args(folder: &Path) -> Vec<OsString> {
    vec![
        "extensions".into(),
        "link".into(),
        folder.into(),
        "--consent".into(),
    ]
}

/// Gemini CLI, whose folder is `gemini_dir` (`~/.gemini`), has the extension.
pub fn installed(gemini_dir: &Path) -> bool {
    gemini_dir.join("extensions").join(NAME).exists()
}

/// Writes the extension again when Gemini CLI, whose folder is `gemini_dir`, has it: one
/// installed by an older Templum gets this one's (a question's seven-day timeout).
pub fn refresh(data: &Path, gemini_dir: &Path, windows: bool) {
    if installed(gemini_dir) {
        extension(data, windows);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    fn read(path: &Path) -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn the_extension_names_templums_server_and_hooks() {
        let dir = tempfile::tempdir().unwrap();
        let folder = extension(dir.path(), false).unwrap();
        assert_eq!(folder, dir.path().join("gemini"));
        let bin = dir.path().join("bin").join("templum");
        let bin = bin.to_string_lossy();
        assert_eq!(
            read(&folder.join("gemini-extension.json")),
            json!({
                "name": "templum",
                "version": "1.0.0",
                "mcpServers": {
                    "templum": { "command": bin, "args": ["mcp"], "timeout": 604_800_000 }
                },
            })
        );
        let hook = json!([{
            "hooks": [{ "type": "command", "command": format!("'{bin}' hook gemini"), "timeout": 5000 }]
        }]);
        assert_eq!(
            read(&folder.join("hooks").join("hooks.json")),
            json!({ "hooks": {
                "SessionStart": hook,
                "BeforeAgent": hook,
                "AfterTool": hook,
                "AfterAgent": hook,
                "Notification": hook,
                "SessionEnd": hook,
            }})
        );
    }

    #[test]
    fn an_installed_extension_is_written_again() {
        let data = tempfile::tempdir().unwrap();
        let gemini = tempfile::tempdir().unwrap();
        let manifest = data.path().join("gemini").join("gemini-extension.json");
        let old = json!({ "name": NAME, "mcpServers": { "templum": { "timeout": 3_600_000 } } });
        write_changed(&manifest, &old).unwrap();
        refresh(data.path(), gemini.path(), false);
        assert_eq!(read(&manifest), old, "not installed: left alone");
        std::fs::create_dir_all(gemini.path().join("extensions").join(NAME)).unwrap();
        refresh(data.path(), gemini.path(), false);
        assert_eq!(
            read(&manifest)["mcpServers"]["templum"]["timeout"],
            604_800_000
        );
    }

    #[test]
    fn an_unchanged_extension_isnt_written_again() {
        let dir = tempfile::tempdir().unwrap();
        let folder = extension(dir.path(), false).unwrap();
        let files = [
            folder.join("gemini-extension.json"),
            folder.join("hooks").join("hooks.json"),
        ];
        let long_ago = UNIX_EPOCH + Duration::from_secs(1_000_000);
        for file in &files {
            let open = std::fs::File::options().write(true).open(file).unwrap();
            open.set_modified(long_ago).unwrap();
        }
        extension(dir.path(), false).unwrap();
        for file in &files {
            let modified = std::fs::metadata(file).unwrap().modified().unwrap();
            assert_eq!(modified, long_ago, "{file:?}");
        }
    }

    #[test]
    fn a_windows_extension_runs_templum_through_powershell() {
        let dir = tempfile::tempdir().unwrap();
        let folder = extension(dir.path(), true).unwrap();
        let bin = dir.path().join("bin").join("templum.exe");
        assert_eq!(
            read(&folder.join("hooks").join("hooks.json"))["hooks"]["AfterAgent"][0]["hooks"][0]["command"],
            format!("& '{}' hook gemini", bin.display())
        );
        assert_eq!(
            read(&folder.join("gemini-extension.json"))["mcpServers"]["templum"]["command"],
            bin.display().to_string()
        );
        let quoted = dir.path().join("o'neil");
        assert_eq!(extension(&quoted, false), None);
        assert!(!quoted.exists(), "nothing written");
    }

    #[test]
    fn gemini_has_the_extension_once_linked() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!installed(dir.path()));
        std::fs::create_dir_all(dir.path().join("extensions").join("templum")).unwrap();
        assert!(installed(dir.path()));
        assert_eq!(
            link_args(Path::new("/data/gemini")),
            ["extensions", "link", "/data/gemini", "--consent"].map(OsString::from)
        );
        assert_eq!(UNLINK, ["extensions", "uninstall", "templum"]);
    }
}
