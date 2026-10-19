//! Pi: the extension that tells Templum its turns, and gives it Templum's MCP server where Pi has
//! MCP, for one session (`-e`; docs/specs/connections.md, Pi).

use std::path::Path;

use crate::claude::{typed_path, write_atomic};

/// The extension, run by Pi: plain JavaScript, which its TypeScript loader takes.
const EXTENSION: &str = include_str!("pi.ts");

/// The extension, running `program` for each event.
pub(crate) fn extension(program: &str) -> String {
    let program = serde_json::to_string(program).expect("JSON");
    EXTENSION.replace("__TEMPLUM_PROGRAM__", &program)
}

/// Writes the extension to `<data>/pi/templum.ts`, for Windows when `windows`, and returns the
/// words that load it in one Pi, unquoted, Windows' `\` as `/`: `-e` and the file, which only
/// Templum types. None when the path can't be typed, Templum's program holds a `'`, or the file
/// can't be written.
pub fn args(data: &Path, windows: bool) -> Option<Vec<String>> {
    let file = data.join("pi").join("templum.ts");
    let word = crate::slashed(&file, windows);
    let bin = crate::bin(data, windows);
    let bin = bin.to_string_lossy();
    if typed_path(&word, windows).is_none() || bin.contains(['\'', '\n', '\r']) {
        return None;
    }
    let text = extension(&bin);
    if std::fs::read_to_string(&file).ok().as_deref() != Some(&text) {
        write_atomic(&file, text.as_bytes()).ok()?;
    }
    Some(vec!["-e".to_string(), word])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pi_gets_templums_extension() {
        let parent = tempfile::tempdir().unwrap();
        let data = parent.path().join("Templum Data");
        let windows = cfg!(windows);
        let file = data.join("pi").join("templum.ts");
        assert_eq!(
            args(&data, windows).unwrap(),
            ["-e".to_string(), crate::slashed(&file, windows)]
        );
        let extension = std::fs::read_to_string(&file).unwrap();
        let program = serde_json::to_string(&crate::bin(&data, windows)).unwrap();
        assert!(
            extension.contains(&format!("const PROGRAM = {program};")),
            "{extension}"
        );
        assert!(!extension.contains("__TEMPLUM_PROGRAM__"));
        assert_eq!(args(&parent.path().join("o'neil"), windows), None);
    }

    /// The extension run by `node` with a fake Pi and a fake program: skipped without `node`.
    #[cfg(unix)]
    #[test]
    fn the_extension_reports_pis_turns() {
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
                "#!/bin/sh\n[ \"$1 $2\" = 'hook pi' ] || exit 9\n{{ cat; echo; }} >> '{}'\n",
                heard.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let module = |name: &str, program: &Path| {
            let path = dir.path().join(name);
            std::fs::write(&path, extension(&program.to_string_lossy())).unwrap();
            path
        };
        // Prints what the extension registered: `mcp` when Pi has `registerMcpServer`.
        let run = |extension: &Path, hooked: &str, mcp: bool| -> String {
            let script = r#"
                const { default: templum } = await import(process.env.EXTENSION);
                const handlers = {};
                const pi = { on: (name, f) => { handlers[name] = f; } };
                if (process.env.MCP) {
                    pi.registerMcpServer = (name, server) => {
                        console.log("mcp " + JSON.stringify([name, server]));
                    };
                }
                templum(pi);
                const fire = async (name, event) => { if (handlers[name]) await handlers[name](event, {}); };
                await fire("session_start", { type: "session_start", reason: "startup" });
                await fire("agent_start", { type: "agent_start" });
                await fire("message_update", { type: "message_update" });
                const said = { role: "assistant", content: [{ type: "thinking", thinking: "x" }, { type: "text", text: "Done." }] };
                await fire("agent_end", { type: "agent_end", messages: [{ role: "user", content: "hi" }, said] });
                await fire("agent_settled", { type: "agent_settled" });
                console.log("ran");
            "#;
            let mut command = std::process::Command::new("node");
            command
                .args(["--input-type=module", "-e", script])
                .env("EXTENSION", extension)
                .env("TEMPLUM_HOOKED", hooked);
            if mcp {
                command.env("MCP", "1");
            }
            let out = command.output().unwrap();
            let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
            assert!(
                stdout.contains("ran"),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            stdout
        };
        let lines = || -> Vec<serde_json::Value> {
            std::fs::read_to_string(&heard)
                .unwrap_or_default()
                .lines()
                .map(|l| serde_json::from_str(l).unwrap())
                .collect()
        };
        let printed = run(&module("templum.mjs", &program), "", true);
        let heard_lines = lines();
        let kinds: Vec<&str> = heard_lines
            .iter()
            .map(|l| l["type"].as_str().unwrap())
            .collect();
        assert_eq!(kinds, ["session_start", "agent_start", "agent_settled"]);
        assert_eq!(heard_lines[2]["answer"], "Done.");
        let server = serde_json::json!(["templum", {
            "command": program.to_string_lossy(), "args": ["mcp"], "timeout": 604_800
        }]);
        let registered = printed
            .lines()
            .find_map(|l| l.strip_prefix("mcp "))
            .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap());
        assert_eq!(registered, Some(server), "{printed}");
        // A Pi without MCP: its events all the same.
        std::fs::remove_file(&heard).unwrap();
        let printed = run(&module("no-mcp.mjs", &program), "", false);
        assert!(!printed.contains("mcp "), "{printed}");
        assert_eq!(lines().len(), 3);
        // Under another Pi that reports (its `pi -p`): neither events nor a server.
        std::fs::remove_file(&heard).unwrap();
        let printed = run(&module("nested.mjs", &program), "1", true);
        assert!(!printed.contains("mcp "), "{printed}");
        assert!(lines().is_empty());
        // Templum gone: the extension never throws.
        run(&module("gone.mjs", &dir.path().join("missing")), "", false);
        assert!(lines().is_empty());
    }
}
