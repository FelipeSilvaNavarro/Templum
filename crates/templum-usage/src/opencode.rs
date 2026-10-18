//! OpenCode and Kilo Code: the JSON that gives one session Templum's MCP server and its plugin,
//! for `OPENCODE_CONFIG_CONTENT` or `KILO_CONFIG_CONTENT` (docs/specs/connections.md, OpenCode
//! and Kilo Code).

use std::path::Path;

use serde_json::json;

use crate::claude::{typed_path, write_atomic};

/// The plugin, run by OpenCode and Kilo Code themselves: plain JavaScript, no dependency.
const PLUGIN: &str = include_str!("opencode.js");

/// The plugin, running `program` for each event.
pub(crate) fn plugin(program: &str) -> String {
    let program = serde_json::to_string(program).expect("JSON");
    PLUGIN.replace("__TEMPLUM_PROGRAM__", &program)
}

/// Writes the plugin to `<data>/opencode/templum.js`, for Windows when `windows`, and returns the
/// one-line JSON that gives one session Templum's server and the plugin. None when a path can't
/// be typed or holds a `'`, or the plugin can't be written.
pub fn config(data: &Path, windows: bool) -> Option<String> {
    let bin = crate::bin(data, windows);
    let bin = bin.to_string_lossy();
    let file = data.join("opencode").join("templum.js");
    let path = file.to_string_lossy();
    if bin.contains(['\'', '\n', '\r']) || typed_path(&path, windows).is_none() {
        return None;
    }
    let text = plugin(&bin);
    if std::fs::read_to_string(&file).ok().as_deref() != Some(&text) {
        write_atomic(&file, text.as_bytes()).ok()?;
    }
    // Its local servers inherit the environment: TEMPLUM_BUS and TEMPLUM_TERMINAL reach `mcp`.
    // The timeout covers the server's start and every call.
    let config = json!({
        "mcp": { "templum": {
            "type": "local", "command": [bin, "mcp"], "timeout": crate::MCP_TIMEOUT_MS
        } },
        "plugin": [file_url(&path, windows)],
    });
    Some(config.to_string())
}

/// `path` as a `file://` URL: `/` between folders, and what isn't a letter, a digit or one of
/// `-._~/:` percent-encoded (a space is `%20`).
fn file_url(path: &str, windows: bool) -> String {
    let path = if windows {
        format!("/{}", path.replace('\\', "/"))
    } else {
        path.to_string()
    };
    let mut url = String::from("file://");
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/:".contains(&byte) {
            url.push(byte as char);
        } else {
            url.push_str(&format!("%{byte:02X}"));
        }
    }
    url
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn opencode_gets_the_server_and_the_plugin() {
        let parent = tempfile::tempdir().unwrap();
        let data = parent.path().join("Templum Data");
        let windows = cfg!(windows);
        let given = config(&data, windows).unwrap();
        assert!(!given.contains('\n'), "one line: {given}");
        let bin = crate::bin(&data, windows);
        let file = data.join("opencode").join("templum.js");
        let path = file
            .to_string_lossy()
            .replace('\\', "/")
            .replace(' ', "%20");
        let url = if windows {
            format!("file:///{path}")
        } else {
            format!("file://{path}")
        };
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&given).unwrap(),
            json!({
                "mcp": { "templum": {
                    "type": "local", "command": [bin, "mcp"], "timeout": 604_800_000
                } },
                "plugin": [url],
            })
        );
        let plugin = std::fs::read_to_string(&file).unwrap();
        let program = serde_json::to_string(&bin).unwrap();
        assert!(
            plugin.contains(&format!("const PROGRAM = {program};")),
            "{plugin}"
        );
        assert!(!plugin.contains("__TEMPLUM_PROGRAM__"));
        assert_eq!(config(&parent.path().join("o'neil"), windows), None);
    }

    /// The plugin run by `node` with a fake program: skipped without `node`.
    #[cfg(unix)]
    #[test]
    fn the_plugin_reports_the_main_sessions_turns() {
        use std::os::unix::fs::PermissionsExt as _;
        if std::process::Command::new("node")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("skipped: node isn't installed");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let heard = dir.path().join("heard");
        let program = dir.path().join("templum");
        std::fs::write(
            &program,
            format!(
                "#!/bin/sh\n[ \"$1 $2\" = 'hook opencode' ] || exit 9\n{{ cat; echo; }} >> '{}'\n",
                heard.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let module = |name: &str, program: &Path| {
            let path = dir.path().join(name);
            std::fs::write(&path, plugin(&program.to_string_lossy())).unwrap();
            path
        };
        let run = |plugin: &Path, hooked: &str| {
            let script = r#"
                const { Templum } = await import(process.env.PLUGIN);
                const messages = [
                    { info: { role: "user" }, parts: [{ type: "text", text: "hi" }] },
                    { info: { role: "assistant" }, parts: [{ type: "text", text: "Done." }] },
                ];
                const client = { session: { messages: async () => ({ data: messages }) } };
                const hooks = await Templum({ client });
                const event = (type, properties) => hooks.event({ event: { type, properties } });
                await hooks["chat.message"]({ sessionID: "main" }, {});
                await event("session.status", { sessionID: "main", status: { type: "busy" } });
                await event("message.part.updated", { sessionID: "main" });
                await event("session.created", { info: { id: "child", parentID: "main" } });
                await hooks["chat.message"]({ sessionID: "child" }, {});
                await event("session.idle", { sessionID: "child" });
                await event("session.idle", { sessionID: "main" });
                console.log("ran");
            "#;
            let out = std::process::Command::new("node")
                .args(["--input-type=module", "-e", script])
                .env("PLUGIN", plugin)
                .env("TEMPLUM_HOOKED", hooked)
                .output()
                .unwrap();
            assert!(
                String::from_utf8_lossy(&out.stdout).contains("ran"),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        let lines = || -> Vec<serde_json::Value> {
            std::fs::read_to_string(&heard)
                .unwrap_or_default()
                .lines()
                .map(|l| serde_json::from_str(l).unwrap())
                .collect()
        };
        run(&module("templum.mjs", &program), "");
        let heard_lines = lines();
        let kinds: Vec<&str> = heard_lines
            .iter()
            .map(|l| l["type"].as_str().unwrap())
            .collect();
        assert_eq!(kinds, ["chat.message", "session.status", "session.idle"]);
        assert_eq!(heard_lines[2]["answer"], "Done.");
        assert_eq!(heard_lines[2]["properties"]["sessionID"], "main");
        // Under another OpenCode that reports (its `opencode run`): nothing at all.
        std::fs::remove_file(&heard).unwrap();
        run(&module("nested.mjs", &program), "1");
        assert!(lines().is_empty());
        // Templum gone: the plugin never throws.
        run(&module("gone.mjs", &dir.path().join("missing")), "");
        assert!(lines().is_empty());
    }
}
