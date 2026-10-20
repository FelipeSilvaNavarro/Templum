//! The branch dialog, one for switching and creating: the header's branch name, the Branches
//! tab's New branch…, the old-version banner and a commit's New branch here… open it. A search
//! over the local branches, then the remote ones; Enter switches (through the question about
//! changes); a name no branch has offers to create it. See docs/specs/git.md, section 1, Header.

use gpui_kit::assets::IconName;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::command::{Command, CommandGroup, CommandItem, CommandState};
use gpui_kit::prelude::*;
use gpui_kit::{App, Entity, Hsla, Window, div, px};
use templum_git::Branch;

use super::words;
use crate::workspace::{GitAt, Workspace};

/// What opened the dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Opened {
    /// The header's branch name: the matches come first, so Enter switches.
    Header,
    /// New branch… and the old-version banner's Create a branch here: the creation comes first, so
    /// Enter creates the name typed, whatever branch holds it (a switch from a detached HEAD would
    /// leave its commits behind).
    Create,
    /// A commit's New branch here…: no branch to switch to from there, only the creation, at that
    /// commit (a hash).
    Commit(String),
}

/// What Enter or a click on a row does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Pick {
    Switch {
        name: String,
        remote: bool,
        current: bool,
    },
    Create(String),
}

/// The dialog's rows for the text typed, in groups: the local branches whose name holds it, the
/// remote ones, and the creation of a branch named as typed unless a branch already has that name
/// (a remote's, `origin/` aside, switches to a tracking branch). The creation comes last, so that
/// Enter right after typing takes the first match, or first with `create_first`.
pub(crate) fn groups(
    branches: &[Branch],
    query: &str,
    create_first: bool,
) -> Vec<(&'static str, Vec<Pick>)> {
    let query = query.trim();
    let needle = query.to_lowercase();
    let listed = |remote: bool| -> Vec<Pick> {
        branches
            .iter()
            .filter(|b| b.remote == remote && b.name.to_lowercase().contains(&needle))
            .map(|b| Pick::Switch {
                name: b.name.clone(),
                remote,
                current: b.current,
            })
            .collect()
    };
    let mut groups: Vec<_> = [("Local", listed(false)), ("Remote", listed(true))]
        .into_iter()
        .filter(|(_, picks)| !picks.is_empty())
        .collect();
    let taken = branches
        .iter()
        .any(|b| b.name == query || (b.remote && words::local_name(&b.name) == query));
    if !query.is_empty() && !taken {
        let create = ("", vec![Pick::Create(query.to_string())]);
        if create_first {
            groups.insert(0, create);
        } else {
            groups.push(create);
        }
    }
    groups
}

/// Opens the dialog for the Git `here` names: its answer acts there, whatever shows then. From a
/// commit (`opened`) the dialog only creates, at that commit; else it lists `branches` and creates
/// at HEAD.
pub(crate) fn open(
    ws: Entity<Workspace>,
    here: GitAt,
    branches: Vec<Branch>,
    opened: Opened,
    dim: Hsla,
    window: &mut Window,
    cx: &mut App,
) {
    let create_first = opened != Opened::Header;
    let from = match opened {
        Opened::Commit(commit) => Some(commit),
        Opened::Header | Opened::Create => None,
    };
    let state = cx.new(|cx| CommandState::new(window, cx));
    let palette = state.clone();
    let (title, placeholder, nothing) = match &from {
        Some(commit) => (
            format!("New branch from {}", &commit[..commit.len().min(7)]),
            "Name of the new branch",
            "Type a name for the new branch",
        ),
        None => (
            "Switch or create branch".to_string(),
            "Search branches, or type a new name",
            "No branches yet: type a name to create one",
        ),
    };
    window.open_dialog(cx, move |dialog, _, cx| {
        // The dialog is drawn on every frame: its rows follow the query.
        let query = palette.read(cx).query(cx);
        let groups = groups(&branches, &query, create_first);
        let mut command = Command::new(&palette)
            .filterable(false)
            .placeholder(placeholder)
            .empty(move |_, _, _| div().p(px(12.0)).text_color(dim).child(nothing));
        for (label, picks) in &groups {
            let items = picks.iter().map(|pick| match pick {
                Pick::Switch { name, current, .. } => {
                    CommandItem::new().label(name.clone()).checked(*current)
                }
                Pick::Create(name) => CommandItem::new()
                    .label(format!("Create branch “{name}”"))
                    .icon(IconName::GitBranchPlus),
            });
            let group = if label.is_empty() {
                CommandGroup::new()
            } else {
                CommandGroup::new().label(*label)
            };
            command = command.group(group.items(items));
        }
        let picks: Vec<Vec<Pick>> = groups.into_iter().map(|(_, picks)| picks).collect();
        let (ws, here, from) = (ws.clone(), here.clone(), from.clone());
        dialog
            .title(title.clone())
            .w(px(420.0))
            .child(command.on_confirm(move |at, window, cx| {
                let Some(pick) = picks.get(at.section).and_then(|p| p.get(at.row)).cloned() else {
                    return;
                };
                window.close_dialog(cx);
                ws.update(cx, |ws, cx| {
                    if !ws.show_git(&here, window, cx) {
                        return;
                    }
                    match pick {
                        Pick::Switch { name, remote, .. } => {
                            ws.switch_branch(name, remote, window, cx)
                        }
                        Pick::Create(name) => ws.create_branch(name, from.clone(), window, cx),
                    }
                });
            }))
    });
    state.update(cx, |state, cx| state.focus(window, cx));
}

/// A searchable list of `names`; Enter or a click gives the one chosen to `chosen`.
pub(crate) fn pick(
    ws: Entity<Workspace>,
    title: &'static str,
    names: Vec<String>,
    chosen: impl Fn(&mut Workspace, String, &mut Window, &mut gpui_kit::Context<Workspace>) + 'static,
    window: &mut Window,
    cx: &mut App,
) {
    let state = cx.new(|cx| CommandState::new(window, cx));
    let palette = state.clone();
    let chosen = std::rc::Rc::new(chosen);
    window.open_dialog(cx, move |dialog, _, _| {
        let (ws, names, chosen) = (ws.clone(), names.clone(), chosen.clone());
        let items: Vec<CommandItem> = names
            .iter()
            .map(|n| CommandItem::new().label(n.clone()))
            .collect();
        dialog.title(title).w(px(420.0)).child(
            Command::new(&palette)
                .items(items)
                .placeholder("Search branches")
                .on_confirm(move |at, window, cx| {
                    let Some(name) = names.get(at.row).cloned() else {
                        return;
                    };
                    window.close_dialog(cx);
                    let chosen = chosen.clone();
                    ws.update(cx, |ws, cx| chosen(ws, name, window, cx));
                }),
        )
    });
    state.update(cx, |state, cx| state.focus(window, cx));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branch(name: &str, remote: bool, current: bool) -> Branch {
        Branch {
            name: name.to_string(),
            remote,
            current,
            commit: String::new(),
            upstream: None,
            gone: false,
            ahead: 0,
            behind: 0,
            subject: String::new(),
            time: 0,
        }
    }

    fn sample() -> Vec<Branch> {
        vec![
            branch("main", false, true),
            branch("feature/one", false, false),
            branch("origin/main", true, false),
            branch("origin/feature/two", true, false),
        ]
    }

    fn switch(name: &str, remote: bool, current: bool) -> Pick {
        Pick::Switch {
            name: name.to_string(),
            remote,
            current,
        }
    }

    #[test]
    fn no_text_lists_every_branch_and_offers_no_creation() {
        let groups = groups(&sample(), "  ", false);
        assert_eq!(
            groups,
            vec![
                (
                    "Local",
                    vec![
                        switch("main", false, true),
                        switch("feature/one", false, false)
                    ]
                ),
                (
                    "Remote",
                    vec![
                        switch("origin/main", true, false),
                        switch("origin/feature/two", true, false)
                    ]
                ),
            ]
        );
    }

    #[test]
    fn text_filters_both_lists_ignoring_case_and_offers_creation_last() {
        let groups = groups(&sample(), " FEATURE ", false);
        assert_eq!(
            groups,
            vec![
                ("Local", vec![switch("feature/one", false, false)]),
                ("Remote", vec![switch("origin/feature/two", true, false)]),
                ("", vec![Pick::Create("FEATURE".to_string())]),
            ],
            "the matches come first, so Enter switches; the name, trimmed, is offered at the end"
        );
    }

    #[test]
    fn a_name_no_branch_holds_is_only_the_creation() {
        assert_eq!(
            groups(&sample(), "brand-new", false),
            vec![("", vec![Pick::Create("brand-new".to_string())])]
        );
    }

    #[test]
    fn an_existing_name_is_not_offered_again() {
        for name in ["main", "origin/main"] {
            let all = groups(&sample(), name, false);
            assert!(
                all.iter()
                    .flat_map(|(_, p)| p)
                    .all(|p| *p != Pick::Create(name.into())),
                "{name}"
            );
        }
    }

    #[test]
    fn with_no_branches_the_text_is_still_a_creation() {
        assert_eq!(groups(&[], "", false), Vec::new());
        assert_eq!(
            groups(&[], "x", false),
            vec![("", vec![Pick::Create("x".to_string())])]
        );
    }

    #[test]
    fn from_a_create_entry_the_creation_comes_first_so_enter_creates() {
        // "login" is held by feature/login: Enter on the first row must still make "login".
        let branches = [
            branch("main", false, true),
            branch("feature/login", false, false),
        ];
        assert_eq!(
            groups(&branches, "login", true),
            vec![
                ("", vec![Pick::Create("login".to_string())]),
                ("Local", vec![switch("feature/login", false, false)]),
            ]
        );
        // From the header the same text keeps the matches first.
        assert_eq!(
            groups(&branches, "login", false)[0].0,
            "Local",
            "the matches first"
        );
        // Nothing typed, nothing to create; an existing name is never offered again.
        assert_eq!(groups(&branches, "", true).len(), 1);
        assert!(
            groups(&branches, "main", true)
                .iter()
                .flat_map(|(_, p)| p)
                .all(|p| !matches!(p, Pick::Create(_)))
        );
    }

    #[test]
    fn a_remote_branch_of_that_name_is_not_offered_as_a_creation() {
        // Switching to origin/main makes the tracking branch main: no second way to make it.
        let branches = [
            branch("dev", false, true),
            branch("origin/login", true, false),
        ];
        for create_first in [false, true] {
            let all = groups(&branches, "login", create_first);
            assert_eq!(
                all,
                vec![("Remote", vec![switch("origin/login", true, false)])]
            );
        }
        // Another remote's name that only holds the text is still a new name.
        assert_eq!(
            groups(&branches, "logi", false).last(),
            Some(&("", vec![Pick::Create("logi".to_string())]))
        );
    }
}
