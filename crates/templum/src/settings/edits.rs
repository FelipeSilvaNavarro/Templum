//! What the panel's fields write to the config, and what its text fields show.

use gpui_kit::Context;

use crate::config::{Edit, WorkingDirectory};
use crate::workspace::Workspace;

/// `font.family`; "" is Default (the key is removed: the first installed font of the built-in list).
pub(crate) fn set_family(ws: &mut Workspace, family: &str, cx: &mut Context<Workspace>) {
    if family.is_empty() {
        ws.edit(Edit::remove("font", "family"), |c| c.font.family = None, cx);
    } else {
        ws.edit(
            Edit::set("font", "family", family),
            |c| c.font.family = Some(family.to_string()),
            cx,
        );
    }
}

/// Splits on spaces; "…" or '…' keeps spaces (and the other quote) inside one argument.
pub(crate) fn split_args(text: &str) -> Result<Vec<String>, &'static str> {
    let mut args = Vec::new();
    let mut current: Option<String> = None;
    let mut quote: Option<char> = None;
    for c in text.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => current.get_or_insert_default().push(c),
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                current.get_or_insert_default();
            }
            None if c.is_whitespace() => args.extend(current.take()),
            None => current.get_or_insert_default().push(c),
        }
    }
    if quote.is_some() {
        return Err("Unclosed quote");
    }
    args.extend(current);
    Ok(args)
}

/// The text field's form of `args`: quoted where an argument is empty or has a space or a quote.
pub(crate) fn join_args(args: &[String]) -> String {
    args.iter()
        .map(|a| {
            if !a.is_empty() && !a.contains(|c: char| c.is_whitespace() || c == '"' || c == '\'') {
                a.clone()
            } else if !a.contains('"') {
                format!("\"{a}\"")
            } else if !a.contains('\'') {
                format!("'{a}'")
            } else {
                // Both quotes: "…", each " written as "'"'" (adjacent quoted parts join up).
                format!("\"{}\"", a.replace('"', "\"'\"'\""))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The Git program typed in the panel; empty or "git" is the default (the key is removed).
pub(crate) fn set_git_program(ws: &mut Workspace, text: &str, cx: &mut Context<Workspace>) {
    // Kept as typed (the field shows the config's value: trimming here would eat a space typed
    // before the next word, as in "Program Files"); GitConfig::program trims it for use.
    if text.trim().is_empty() || text.trim() == "git" {
        ws.edit(
            Edit::remove("git", "program"),
            |c| c.git.program = "git".to_string(),
            cx,
        );
    } else {
        ws.edit(
            Edit::set("git", "program", text),
            |c| c.git.program = text.to_string(),
            cx,
        );
    }
}

/// `shell.program`, trimmed; empty removes the key (the default shell). The typed
/// text is kept as a draft, so a space typed before the rest of a path stays in the field.
pub(crate) fn set_program(ws: &mut Workspace, text: &str, cx: &mut Context<Workspace>) {
    ws.drafts.insert("program", text.to_string());
    let text = text.trim();
    if text.is_empty() {
        ws.edit(
            Edit::remove("shell", "program"),
            |c| c.shell.program = None,
            cx,
        );
    } else {
        ws.edit(
            Edit::set("shell", "program", text),
            |c| c.shell.program = Some(text.to_string()),
            cx,
        );
    }
}

/// `shell.program` and `shell.args` from the list: a shell found here, or None for the default
/// (both keys removed).
pub(crate) fn set_shell(
    ws: &mut Workspace,
    shell: Option<&templum_terminal::ShellOption>,
    cx: &mut Context<Workspace>,
) {
    // The text fields show what the list chose.
    ws.drafts.remove("program");
    ws.drafts.remove("args");
    match shell {
        None => {
            ws.edit(
                Edit::remove("shell", "program"),
                |c| c.shell.program = None,
                cx,
            );
            ws.edit(Edit::remove("shell", "args"), |c| c.shell.args.clear(), cx);
        }
        Some(shell) => {
            let program = shell.program.clone();
            ws.edit(
                Edit::set("shell", "program", program.as_str()),
                |c| c.shell.program = Some(program.clone()),
                cx,
            );
            let args = shell.args.clone();
            let value: toml_edit::Array = args.iter().map(String::as_str).collect();
            ws.edit(
                Edit::set("shell", "args", value),
                |c| c.shell.args = args,
                cx,
            );
        }
    }
    cx.notify();
}

/// `shell.args` from the text field. An unclosed quote waits (the field says so); the typed text
/// is kept as a draft, so the field shows it rather than the config.
pub(crate) fn set_args(ws: &mut Workspace, text: &str, cx: &mut Context<Workspace>) {
    ws.drafts.insert("args", text.to_string());
    if let Ok(args) = split_args(text) {
        let value: toml_edit::Array = args.iter().map(String::as_str).collect();
        ws.edit(
            Edit::set("shell", "args", value),
            |c| c.shell.args = args,
            cx,
        );
    }
    cx.notify();
}

/// What the arguments field shows: the draft while it is what's in effect (or can't be yet),
/// else the config's.
pub(crate) fn args_text(ws: &Workspace) -> String {
    match ws.drafts.get("args") {
        Some(d) if split_args(d).map_or(true, |a| a == ws.config.shell.args) => d.clone(),
        _ => join_args(&ws.config.shell.args),
    }
}

pub(crate) fn args_error(ws: &Workspace) -> Option<&'static str> {
    ws.drafts.get("args").and_then(|d| split_args(d).err())
}

/// The start folder dropdown: "inherit", "home" or "folder" (the home folder's path, to edit).
pub(crate) fn set_folder_kind(ws: &mut Workspace, kind: &str, cx: &mut Context<Workspace>) {
    // What was typed in the folder field belongs to the choice being left.
    ws.drafts.remove("working_directory");
    let (value, wd) = match kind {
        "home" => ("~".to_string(), WorkingDirectory::Home),
        "folder" => {
            if matches!(ws.config.shell.working_directory, WorkingDirectory::Path(_)) {
                return;
            }
            let Some(home) = std::env::home_dir() else {
                return;
            };
            (home.display().to_string(), WorkingDirectory::Path(home))
        }
        _ => ("inherit".to_string(), WorkingDirectory::Inherit),
    };
    ws.edit(
        Edit::set("shell", "working_directory", value),
        |c| c.shell.working_directory = wd,
        cx,
    );
}

/// The folder field. "~" and "inherit" alone would switch the dropdown to Home or Inherit and
/// hide the field mid-typing: they wait for the rest of the path.
pub(crate) fn set_folder(ws: &mut Workspace, text: &str, cx: &mut Context<Workspace>) {
    ws.drafts.insert("working_directory", text.to_string());
    if matches!(text, "" | "~" | "inherit") {
        cx.notify();
        return;
    }
    let wd = WorkingDirectory::from(text.to_string());
    ws.edit(
        Edit::set("shell", "working_directory", text),
        |c| c.shell.working_directory = wd,
        cx,
    );
}

pub(crate) fn folder_text(ws: &Workspace) -> String {
    let current = &ws.config.shell.working_directory;
    match ws.drafts.get("working_directory") {
        Some(d)
            if matches!(d.as_str(), "" | "~" | "inherit")
                || WorkingDirectory::from(d.clone()) == *current =>
        {
            d.clone()
        }
        _ => match current {
            WorkingDirectory::Path(p) => p.display().to_string(),
            _ => String::new(),
        },
    }
}

/// Why the folder field's text isn't in effect yet (see `set_folder`).
pub(crate) fn folder_hint(ws: &Workspace) -> Option<&'static str> {
    match ws.drafts.get("working_directory")?.as_str() {
        "" => Some("Not applied: type a path"),
        "~" | "inherit" => Some("Not applied: type the rest of the path"),
        _ => None,
    }
}

/// What the program field shows: the draft while it is what's in effect, else the config's.
pub(crate) fn program_text(ws: &Workspace) -> String {
    let current = ws.config.shell.program.as_deref().unwrap_or_default();
    match ws.drafts.get("program") {
        Some(d) if d.trim() == current => d.clone(),
        _ => current.to_string(),
    }
}
