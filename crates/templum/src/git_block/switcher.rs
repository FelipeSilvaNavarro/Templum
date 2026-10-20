//! The branch dialog, one for switching and creating: the header's branch name, the Branches
//! tab's New branch…, the old-version banner and a commit's New branch here… open it. A search
//! over the local branches, then the remote ones; Enter switches (through the question about
//! changes); a name no branch has offers to create it. See docs/specs/git.md, section 1, Header.

use std::time::{SystemTime, UNIX_EPOCH};

use gpui_kit::assets::IconName;
use gpui_kit::component::command::{Command, CommandGroup, CommandItem, CommandState};
use gpui_kit::component::{Disableable as _, WindowExt as _};
use gpui_kit::prelude::*;
use gpui_kit::{App, Entity, Hsla, Window, div, px};
use templum_git::Branch;

use super::branches::branch_lines;
use super::words;
use crate::workspace::{GitAt, Workspace};

/// How many matches the dialog draws: gpui-component measures every row of a palette with custom
/// rows on each frame, which hundreds of remote branches would feel. The rest are one row,
/// "N more", and typing narrows them.
const MAX_ROWS: usize = 50;

/// What opened the dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Opened {
    /// The header's branch name: the matches come first, so Enter switches.
    Header,
    /// New branch… and the old-version banner's Create a branch here: the creation comes first, for
    /// any name typed, so Enter never switches (from a detached HEAD that would leave its commits
    /// behind).
    Create,
    /// A commit's New branch here…: no branch to switch to from there, only the creation, at that
    /// commit (a hash).
    Commit(String),
}

/// What Enter or a click on a row does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Pick {
    Switch(Branch),
    Create(String),
    /// The matches past `MAX_ROWS`, as a count: a row that does nothing.
    More(usize),
}

/// The dialog's rows for the text typed, in groups: the local branches whose name holds it, the
/// remote ones, and the creation of a branch named as typed. From the header (`create_first`
/// false) the creation is left out when a branch already has that name (a remote's, `origin/`
/// aside, switches to a tracking branch) and comes last, so that Enter right after typing takes
/// the first match. From a create entry it is always there, first: git makes `login` next to
/// `origin/login`, or refuses a local name taken in its own words, and HEAD stays.
pub(crate) fn groups(
    branches: &[Branch],
    query: &str,
    create_first: bool,
) -> Vec<(&'static str, Vec<Pick>)> {
    let query = query.trim();
    let needle = query.to_lowercase();
    // The branch the text names, a remote's without its `origin/`.
    let named = |b: &Branch| {
        let name = if b.remote {
            words::local_name(&b.name)
        } else {
            &b.name
        };
        !needle.is_empty() && name.to_lowercase() == needle
    };
    let listed = |remote: bool| -> Vec<&Branch> {
        let mut list: Vec<&Branch> = branches
            .iter()
            .filter(|b| b.remote == remote && b.name.to_lowercase().contains(&needle))
            .collect();
        // Git sorts by name, which puts `feature/login` before `login`: the one named goes first.
        list.sort_by_key(|b| !named(b));
        list
    };
    let mut sets = vec![("Local", listed(false)), ("Remote", listed(true))];
    let first_is_named = |list: &[&Branch]| list.first().is_some_and(|b| named(b));
    if !first_is_named(&sets[0].1) && first_is_named(&sets[1].1) {
        sets.reverse();
    }
    let hidden = sets
        .iter()
        .map(|(_, list)| list.len())
        .sum::<usize>()
        .saturating_sub(MAX_ROWS);
    let mut room = MAX_ROWS;
    for (_, list) in &mut sets {
        list.truncate(room);
        room -= list.len();
    }
    let mut groups: Vec<(&'static str, Vec<Pick>)> = sets
        .into_iter()
        .filter(|(_, list)| !list.is_empty())
        .map(|(label, list)| {
            let picks = list.into_iter().map(|b| Pick::Switch(b.clone()));
            (label, picks.collect())
        })
        .collect();
    if let Some((_, picks)) = groups.last_mut().filter(|_| hidden > 0) {
        picks.push(Pick::More(hidden));
    }
    let taken = branches
        .iter()
        .any(|b| b.name == query || (b.remote && words::local_name(&b.name) == query));
    if !query.is_empty() && (create_first || !taken) {
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
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let mut command = Command::new(&palette)
            .filterable(false)
            .placeholder(placeholder)
            .empty(move |_, _, _| div().p(px(12.0)).text_color(dim).child(nothing));
        for (label, picks) in &groups {
            let items = picks.iter().map(|pick| match pick {
                Pick::Switch(b) => {
                    let b = b.clone();
                    CommandItem::new()
                        .label(b.name.clone())
                        .child(move |_, _| branch_lines(&b, None, 1.0, now))
                }
                Pick::Create(name) => CommandItem::new()
                    .label(format!("Create branch “{name}”"))
                    .icon(IconName::GitBranchPlus),
                Pick::More(n) => CommandItem::new()
                    .label(format!("{n} more: keep typing to narrow them"))
                    .disabled(true),
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
                let pick = picks.get(at.section).and_then(|p| p.get(at.row)).cloned();
                let Some(pick @ (Pick::Switch(_) | Pick::Create(_))) = pick else {
                    return;
                };
                window.close_dialog(cx);
                ws.update(cx, |ws, cx| {
                    if !ws.show_git(&here, window, cx) {
                        return;
                    }
                    match pick {
                        Pick::Switch(b) => ws.switch_branch(b.name, b.remote, window, cx),
                        Pick::Create(name) => ws.create_branch(name, from.clone(), window, cx),
                        Pick::More(_) => {}
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
        Pick::Switch(branch(name, remote, current))
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
    fn the_branch_the_text_names_comes_first_even_when_git_sorts_it_later() {
        // Git sorts by name: feature/login before login, and the locals before the remotes.
        let branches = [
            branch("feature/login", false, false),
            branch("login", false, false),
            branch("origin/feature/login", true, false),
            branch("origin/login", true, false),
        ];
        let names = |all: Vec<(&str, Vec<Pick>)>| -> Vec<String> {
            all.into_iter()
                .flat_map(|(_, picks)| picks)
                .filter_map(|p| match p {
                    Pick::Switch(b) => Some(b.name),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(
            names(groups(&branches, "login", false)),
            [
                "login",
                "feature/login",
                "origin/login",
                "origin/feature/login"
            ]
        );
        // No local one is named: the remote group, whose first row is, goes first.
        assert_eq!(
            names(groups(&branches[2..], "login", false))[0],
            "origin/login"
        );
        assert_eq!(
            names(groups(
                &[branches[0].clone(), branches[3].clone()],
                "login",
                false
            )),
            ["origin/login", "feature/login"]
        );
        // Nothing typed names nothing: git's order stays.
        assert_eq!(names(groups(&branches, "", false))[0], "feature/login",);
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
        // Nothing typed, nothing to create.
        assert_eq!(groups(&branches, "", true).len(), 1);
    }

    #[test]
    fn from_a_create_entry_a_name_a_branch_already_has_is_still_the_first_row() {
        // Local, remote, and a local name that only `origin/login` holds: Enter must never switch.
        let branches = [
            branch("main", false, true),
            branch("origin/main", true, false),
            branch("origin/login", true, false),
        ];
        for name in ["main", "origin/login", "login"] {
            assert_eq!(
                groups(&branches, name, true)[0],
                ("", vec![Pick::Create(name.to_string())]),
                "{name}"
            );
        }
    }

    #[test]
    fn from_the_header_a_remote_branch_of_that_name_is_not_offered_as_a_creation() {
        // Switching to origin/main makes the tracking branch main: no second way to make it.
        let branches = [
            branch("dev", false, true),
            branch("origin/login", true, false),
        ];
        assert_eq!(
            groups(&branches, "login", false),
            vec![("Remote", vec![switch("origin/login", true, false)])]
        );
        // Another remote's name that only holds the text is still a new name.
        assert_eq!(
            groups(&branches, "logi", false).last(),
            Some(&("", vec![Pick::Create("logi".to_string())]))
        );
    }

    #[test]
    fn past_the_cap_the_rest_is_one_row_that_counts_them() {
        let many: Vec<Branch> = (0..MAX_ROWS + 7)
            .map(|i| branch(&format!("topic-{i}"), i % 2 == 1, false))
            .collect();
        let all = groups(&many, "topic", false);
        let rows: usize = all.iter().map(|(_, p)| p.len()).sum();
        assert_eq!(rows, MAX_ROWS + 1 + 1, "the cap, the count, the creation");
        let last_group = &all[all.len() - 2].1;
        assert_eq!(last_group.last(), Some(&Pick::More(7)));
        assert_eq!(
            all.iter()
                .flat_map(|(_, p)| p)
                .filter(|p| matches!(p, Pick::Switch(_)))
                .count(),
            MAX_ROWS
        );
        // Typing narrows them: under the cap there is no counting row.
        assert!(
            groups(&many, "topic-1", false)
                .iter()
                .flat_map(|(_, p)| p)
                .all(|p| !matches!(p, Pick::More(_)))
        );
    }
}
