//! Accounts made by name: each one's command in the commands folder (docs/specs/accounts.md,
//! Its name and its command). Every test points the commands folder and the agents' own folders
//! at temporary ones: none reaches the person's `~/.local/bin` or `~/.claude`.

use std::path::{Path, PathBuf};

use super::*;
use crate::agents::{AccountDraft, COMMANDS, OWN_FOLDERS};
use templum_usage::accounts::command;

/// Temporary folders for the commands, the agents' own, the config and the data.
struct Dirs {
    commands: tempfile::TempDir,
    config: tempfile::TempDir,
    data: tempfile::TempDir,
    _own: tempfile::TempDir,
}

impl Dirs {
    fn new() -> Dirs {
        let dirs = Dirs {
            commands: tempfile::tempdir().unwrap(),
            config: tempfile::tempdir().unwrap(),
            data: tempfile::tempdir().unwrap(),
            _own: tempfile::tempdir().unwrap(),
        };
        COMMANDS.set(Some(dirs.commands.path().to_path_buf()));
        OWN_FOLDERS.set(Some(dirs._own.path().to_path_buf()));
        dirs
    }

    fn open(&self, cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Workspace>) {
        open_in(
            cx,
            Some(self.config.path().to_path_buf()),
            Some(self.data.path().to_path_buf()),
        )
    }

    fn command(&self, id: &str) -> PathBuf {
        self.commands.path().join(id)
    }

    fn is_own(&self, id: &str) -> bool {
        command::is_own(&self.command(id), id, self.data.path())
    }
}

fn save(
    handle: AnyWindowHandle,
    ws: &Entity<Workspace>,
    cx: &mut TestAppContext,
    id: &str,
) -> Result<String, String> {
    let draft = AccountDraft {
        of: "claude".into(),
        show: true,
        ..Default::default()
    };
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.save_account(id, &draft, window, cx))
    })
    .unwrap()
}

fn delete(ws: &Entity<Workspace>, cx: &mut TestAppContext, id: &str) -> Result<(), String> {
    ws.update(cx, |ws, cx| ws.delete_account(id, cx))
}

#[gpui_kit::test]
fn creating_an_account_by_name_writes_its_command(cx: &mut TestAppContext) {
    let dirs = Dirs::new();
    let (handle, ws) = dirs.open(cx);
    assert_eq!(save(handle, &ws, cx, "claudinho"), Ok("claudinho".into()));
    ws.update(cx, |ws, _| ws.save());
    let text = std::fs::read_to_string(dirs.config.path().join(crate::config::FILE)).unwrap();
    let account = &crate::config::parse(&text).unwrap().agents["claudinho"];
    assert_eq!(account.of.as_deref(), Some("claude"));
    assert_eq!(account.name, None, "{text}");
    assert!(dirs.is_own("claudinho"));
    let made = ws.read_with(cx, |ws, _| ws.commands["claudinho"].clone());
    assert_eq!(
        made,
        Ok(dirs.commands.path().join(if cfg!(windows) {
            "claudinho.cmd"
        } else {
            "claudinho"
        }))
    );
}

#[gpui_kit::test]
fn a_taken_name_is_refused_on_save(cx: &mut TestAppContext) {
    let dirs = Dirs::new();
    let (handle, ws) = dirs.open(cx);
    assert_eq!(
        save(handle, &ws, cx, "claude"),
        Err("claude is one of Templum's agents".into())
    );
    ws.read_with(cx, |ws, _| {
        let agents = &ws.pending.as_ref().unwrap_or(&ws.config).agents;
        assert!(!agents.contains_key("claude"));
    });
    assert!(!dirs.command("claude").exists());
}

#[gpui_kit::test]
fn deleting_an_account_removes_its_command_only(cx: &mut TestAppContext) {
    let dirs = Dirs::new();
    let (handle, ws) = dirs.open(cx);
    save(handle, &ws, cx, "claudinho").unwrap();
    save(handle, &ws, cx, "claudio").unwrap();
    std::fs::write(dirs.command("other"), "#!/bin/sh\n").unwrap();
    assert!(ws.read_with(cx, |ws, _| ws.command_removed("claudinho")));
    assert_eq!(delete(&ws, cx, "claudinho"), Ok(()));
    assert!(!dirs.command("claudinho").exists());
    assert!(dirs.is_own("claudio"));
    assert!(dirs.command("other").exists());
}

#[gpui_kit::test]
fn an_older_accounts_command_is_made_at_start(cx: &mut TestAppContext) {
    let dirs = Dirs::new();
    std::fs::write(
        dirs.config.path().join(crate::config::FILE),
        format!("{}[agents.test]\nof = \"claude\"\n", super::agents::CLAUDE2),
    )
    .unwrap();
    let (_handle, ws) = dirs.open(cx);
    assert!(dirs.is_own("claude2"));
    assert!(!dirs.command("test").exists());
    ws.read_with(cx, |ws, _| {
        assert!(ws.commands["claude2"].is_ok());
        assert_eq!(
            ws.commands["test"],
            Err("test is a word of your shell".to_string())
        );
    });
}

#[gpui_kit::test]
fn a_file_of_someone_else_is_no_accounts_command(cx: &mut TestAppContext) {
    let dirs = Dirs::new();
    std::fs::write(dirs.command("claudinho"), "#!/bin/sh\necho mine\n").unwrap();
    let (handle, ws) = dirs.open(cx);
    let saved = save(handle, &ws, cx, "claudinho");
    assert!(
        saved
            .as_ref()
            .unwrap_err()
            .starts_with("claudinho is already a command: "),
        "{saved:?}"
    );
    assert_eq!(
        std::fs::read_to_string(dirs.command("claudinho")).unwrap(),
        "#!/bin/sh\necho mine\n"
    );
    // An account of config.toml met later on that file: its command is not made, the file stays.
    ws.update(cx, |ws, _| {
        let config = crate::config::parse("[agents.claudinho]\nof = \"claude\"\n").unwrap();
        ws.agents = crate::agents::effective(&config.agents, &[], ws.data_dir.as_deref());
        ws.sync_commands();
    });
    assert!(ws.read_with(cx, |ws, _| ws.commands["claudinho"].is_err()));
    assert!(!dirs.is_own("claudinho"));
    let _: &Path = dirs.commands.path();
}
