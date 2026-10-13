//! Each target's menu: its items and what choosing them does.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::agents::AgentEntry;
use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::prelude::*;
use gpui_kit::{App, ClipboardItem, Context, Entity, SharedString, Window, px};
use templum_canvas::{BlockId, Point};

use super::{Items, SendTo};
use crate::git_block;
use crate::kanban;
use crate::launcher;
use crate::prompt;
use crate::workspace::Workspace;

/// An item whose click updates the workspace.
fn item(
    ws: &Entity<Workspace>,
    label: impl Into<SharedString>,
    icon: Option<IconName>,
    act: impl Fn(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static,
) -> PopupMenuItem {
    let ws = ws.clone();
    PopupMenuItem::new(label)
        .when_some(icon, |item, icon| item.icon(Icon::new(icon)))
        .on_click(move |_, window, cx| ws.update(cx, |ws, cx| act(ws, window, cx)))
}

/// The agents, installed first; the others say how to install them when chosen. `at`: where the
/// agent's block goes.
fn agent_items(
    ws: &Entity<Workspace>,
    menu: PopupMenu,
    agents: Vec<(AgentEntry, bool)>,
    at: Point,
) -> PopupMenu {
    let (found, missing): (Vec<_>, Vec<_>) =
        agents.into_iter().partition(|(_, installed)| *installed);
    let entry = |menu: PopupMenu, (agent, installed): (AgentEntry, bool)| {
        let title = SharedString::from(agent.title.clone());
        menu.item(item(ws, title, None, move |ws, window, cx| {
            launcher::ask_folder(ws, agent.clone(), at, installed, window, cx)
        }))
    };
    // Twenty agents: the list scrolls in a short window.
    let menu = found
        .into_iter()
        .fold(menu.scrollable(true).max_h(px(420.0)), entry);
    if missing.is_empty() {
        return menu;
    }
    missing
        .into_iter()
        .fold(menu.separator().label("Not installed"), entry)
}

/// One item per terminal (`Workspace::terminal_list`); `act` on the chosen one.
fn terminal_items(
    ws: &Entity<Workspace>,
    menu: PopupMenu,
    terminals: &[(BlockId, String)],
    act: impl Fn(&mut Workspace, BlockId, &mut Window, &mut Context<Workspace>) + Clone + 'static,
) -> PopupMenu {
    terminals.iter().fold(menu, |menu, (id, label)| {
        let (id, act) = (*id, act.clone());
        menu.item(item(
            ws,
            label.clone(),
            Some(IconName::SquareTerminal),
            move |ws, window, cx| act(ws, id, window, cx),
        ))
    })
}

/// Send to ▸ the workspace's terminals: the text is typed into the chosen one, never run.
/// Nothing without a terminal or a text.
fn send_to(
    ws: &Entity<Workspace>,
    menu: PopupMenu,
    send: SendTo,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    if send.to.is_empty() || send.text.trim().is_empty() {
        return menu;
    }
    let ws = ws.clone();
    let text: Rc<str> = send.text.into();
    menu.submenu_with_icon(
        Some(Icon::new(IconName::SquareTerminal)),
        "Send to",
        window,
        cx,
        move |menu, _, _| {
            let text = text.clone();
            terminal_items(&ws, menu, &send.to, move |ws, id, window, cx| {
                ws.send_to(id, &text, window, cx)
            })
        },
    )
}

/// What deleting `n` cards along with their board or column says.
fn cards(n: usize) -> String {
    if n == 1 {
        "Its card is deleted too.".to_string()
    } else {
        format!("Its {n} cards are deleted too.")
    }
}

impl Items {
    pub(super) fn build(
        self,
        ws: &Entity<Workspace>,
        menu: PopupMenu,
        window: &mut Window,
        cx: &mut Context<PopupMenu>,
    ) -> PopupMenu {
        match self {
            Items::Canvas { at, agents } => {
                let add = ws.clone();
                let ws = ws.clone();
                menu.submenu_with_icon(
                    Some(Icon::new(IconName::Bot)),
                    "AI agent",
                    window,
                    cx,
                    move |menu, _, _| agent_items(&ws, menu, agents.clone(), at),
                )
                .submenu_with_icon(
                    Some(Icon::new(IconName::Plus)),
                    "Add",
                    window,
                    cx,
                    move |menu, _, _| {
                        let ws = &add;
                        menu.item(item(
                            ws,
                            "Terminal",
                            Some(IconName::SquareTerminal),
                            move |ws, window, cx| ws.add_terminal_at(at, window, cx),
                        ))
                        .item(item(
                            ws,
                            "Kanban board",
                            Some(IconName::LayoutDashboard),
                            move |ws, window, cx| ws.add_kanban_at(at, window, cx),
                        ))
                        .item(item(
                            ws,
                            "Note",
                            Some(IconName::StickyNote),
                            move |ws, window, cx| ws.add_note_at(at, window, cx),
                        ))
                        .item(item(
                            ws,
                            "Editor",
                            Some(IconName::FileCode),
                            move |ws, window, cx| ws.ask_editor_folder(at, window, cx),
                        ))
                        .item(item(
                            ws,
                            "Git",
                            Some(IconName::GitBranch),
                            move |ws, window, cx| ws.ask_git_folder(at, window, cx),
                        ))
                    },
                )
            }
            Items::Terminal { block, selection } => menu
                .item(
                    item(ws, "Copy", Some(IconName::Copy), |ws, _, cx| {
                        ws.copy_selection(cx)
                    })
                    .disabled(!selection),
                )
                .item(item(ws, "Paste", None, |ws, _, cx| ws.paste_clipboard(cx)))
                .separator()
                .item(item(
                    ws,
                    "Close terminal",
                    Some(IconName::Close),
                    move |ws, window, cx| ws.close(block, window, cx),
                )),
            Items::Kanban { block, listing } => menu
                .item(item(
                    ws,
                    if listing {
                        "Back to the board"
                    } else {
                        "Board list"
                    },
                    Some(IconName::ChevronsUpDown),
                    move |ws, _, cx| ws.pick_board(block, cx),
                ))
                .separator()
                .item(item(
                    ws,
                    "Close",
                    Some(IconName::Close),
                    move |ws, window, cx| ws.close(block, window, cx),
                )),
            Items::Agents { at, agents } => agent_items(ws, menu, agents, at),
            Items::Tree {
                block,
                folder,
                path,
                is_dir,
                entries,
            } => {
                // Where a new entry goes: the folder clicked, the file's folder, or the root.
                let dir = match &path {
                    Some(p) if is_dir => p.clone(),
                    Some(p) => p.parent().map_or(folder.clone(), Path::to_path_buf),
                    None => folder.clone(),
                };
                let name = path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                let reveal = |ws: &Entity<Workspace>, p: PathBuf| {
                    item(
                        ws,
                        "Reveal in file manager",
                        Some(IconName::ExternalLink),
                        move |_, _, cx| cx.reveal_path(&p),
                    )
                };
                let mut menu = menu;
                if let Some(p) = path.clone().filter(|_| !is_dir) {
                    let open = p.clone();
                    menu = menu
                        .item(item(ws, "Open", None, move |ws, window, cx| {
                            ws.open_file(block, &open, None, window, cx)
                        }))
                        .item(reveal(ws, p));
                }
                if path.is_none() || is_dir {
                    let (file_dir, folder_dir) = (dir.clone(), dir.clone());
                    menu = menu
                        .item(item(
                            ws,
                            "New file…",
                            Some(IconName::FilePlus),
                            move |_, window, cx| {
                                let ws = cx.entity();
                                let dir = file_dir.clone();
                                prompt::ask_text(
                                    "New file",
                                    "Create",
                                    "",
                                    move |text, window, cx| {
                                        ws.update(cx, |ws, cx| {
                                            ws.new_entry(block, &dir, false, &text, window, cx)
                                        })
                                    },
                                    window,
                                    cx,
                                )
                            },
                        ))
                        .item(item(
                            ws,
                            "New folder…",
                            Some(IconName::FolderPlus),
                            move |_, window, cx| {
                                let ws = cx.entity();
                                let dir = folder_dir.clone();
                                prompt::ask_text(
                                    "New folder",
                                    "Create",
                                    "",
                                    move |text, window, cx| {
                                        ws.update(cx, |ws, cx| {
                                            ws.new_entry(block, &dir, true, &text, window, cx)
                                        })
                                    },
                                    window,
                                    cx,
                                )
                            },
                        ));
                }
                if let Some(p) = path.clone().filter(|_| is_dir) {
                    menu = menu.item(item(
                        ws,
                        "Open a terminal here",
                        Some(IconName::SquareTerminal),
                        move |ws, window, cx| {
                            ws.terminal_beside(block, &p, None, window, cx);
                        },
                    ));
                }
                if let Some(p) = path.clone() {
                    let (rename, rename_name) = (p.clone(), name.clone());
                    let (delete, delete_name) = (p.clone(), name.clone());
                    menu = menu
                        .item(item(ws, "Rename…", None, move |_, window, cx| {
                            let ws = cx.entity();
                            let p = rename.clone();
                            prompt::ask_text(
                                "Rename",
                                "Rename",
                                rename_name.clone(),
                                move |text, window, cx| {
                                    ws.update(cx, |ws, cx| {
                                        ws.rename_entry(block, &p, &text, window, cx)
                                    })
                                },
                                window,
                                cx,
                            )
                        }))
                        .item(item(
                            ws,
                            "Delete…",
                            Some(IconName::Delete),
                            move |_, window, cx| {
                                let ws = cx.entity();
                                let p = delete.clone();
                                let what = if is_dir {
                                    format!("It has {entries} entries. This can't be undone.")
                                } else {
                                    "This can't be undone.".to_string()
                                };
                                prompt::confirm(
                                    format!("Delete {delete_name}?"),
                                    what,
                                    "Delete",
                                    move |window, cx| {
                                        ws.update(cx, |ws, cx| {
                                            ws.delete_entry(block, &p, window, cx)
                                        })
                                    },
                                    window,
                                    cx,
                                )
                            },
                        ));
                    if is_dir {
                        menu = menu.item(reveal(ws, p));
                    }
                }
                if path.is_none() {
                    menu = menu.item(item(
                        ws,
                        "Refresh",
                        Some(IconName::RefreshCw),
                        move |ws, _, cx| ws.refresh_tree(block, cx),
                    ));
                }
                menu
            }
            Items::GitRow { block, list, entry } => {
                use git_block::List;
                let path = entry.path.clone();
                let mut menu = menu;
                match list {
                    List::Conflicts => {
                        let (open, ours, theirs, resolved) =
                            (path.clone(), path.clone(), path.clone(), path.clone());
                        menu = menu
                            .item(item(
                                ws,
                                "Open in editor",
                                Some(IconName::FileCode),
                                move |ws, window, cx| ws.open_from_git(block, &open, window, cx),
                            ))
                            .item(item(ws, "Use ours", None, move |ws, window, cx| {
                                ws.resolve(block, ours.clone(), templum_git::Side::Ours, window, cx)
                            }))
                            .item(item(ws, "Use theirs", None, move |ws, window, cx| {
                                ws.resolve(
                                    block,
                                    theirs.clone(),
                                    templum_git::Side::Theirs,
                                    window,
                                    cx,
                                )
                            }))
                            .item(item(
                                ws,
                                "Mark resolved",
                                Some(IconName::Check),
                                move |ws, window, cx| {
                                    ws.stage(block, vec![resolved.clone()], window, cx)
                                },
                            ));
                    }
                    List::Staged => {
                        let (unstage, open) = (path.clone(), path.clone());
                        menu = menu
                            .item(item(
                                ws,
                                "Unstage",
                                Some(IconName::Minus),
                                move |ws, window, cx| {
                                    ws.unstage(block, vec![unstage.clone()], window, cx)
                                },
                            ))
                            .item(item(
                                ws,
                                "Open",
                                Some(IconName::FileCode),
                                move |ws, window, cx| ws.open_from_git(block, &open, window, cx),
                            ));
                    }
                    List::Unstaged => {
                        let (stage, discard, open) = (path.clone(), path.clone(), path.clone());
                        menu = menu
                            .item(item(
                                ws,
                                "Stage",
                                Some(IconName::Plus),
                                move |ws, window, cx| {
                                    ws.stage(block, vec![stage.clone()], window, cx)
                                },
                            ))
                            .item(item(
                                ws,
                                "Discard changes…",
                                Some(IconName::Undo2),
                                move |ws, window, cx| {
                                    ws.discard(block, vec![discard.clone()], window, cx)
                                },
                            ))
                            .item(item(
                                ws,
                                "Open",
                                Some(IconName::FileCode),
                                move |ws, window, cx| ws.open_from_git(block, &open, window, cx),
                            ));
                    }
                }
                let (copy, ignore) = (path.clone(), path.clone());
                menu.item(item(
                    ws,
                    "Copy path",
                    Some(IconName::Copy),
                    move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(copy.clone())),
                ))
                .item(item(ws, "Ignore", None, move |ws, window, cx| {
                    ws.ignore_path(block, ignore.clone(), window, cx)
                }))
            }
            Items::GitCommit { block, commit } => {
                let (checkout, branch, tag, revert, copy) = (
                    commit.hash.clone(),
                    commit.hash.clone(),
                    commit.hash.clone(),
                    commit.hash.clone(),
                    commit.hash.clone(),
                );
                menu.item(item(ws, "Checkout…", None, move |ws, window, cx| {
                    ws.checkout_commit(block, checkout.clone(), window, cx)
                }))
                .item(item(
                    ws,
                    "New branch from here…",
                    Some(IconName::GitBranchPlus),
                    move |ws, window, cx| ws.branch_from(block, Some(branch.clone()), window, cx),
                ))
                .item(item(
                    ws,
                    "Create tag…",
                    Some(IconName::Tag),
                    move |ws, window, cx| ws.tag_commit(block, tag.clone(), window, cx),
                ))
                .item(item(
                    ws,
                    "Revert…",
                    Some(IconName::Undo2),
                    move |ws, window, cx| ws.revert_commit(block, revert.clone(), window, cx),
                ))
                .item(item(
                    ws,
                    "Copy hash",
                    Some(IconName::Copy),
                    move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(copy.clone())),
                ))
            }
            Items::GitBranch {
                block,
                branch,
                current,
            } => {
                let name = branch.name.clone();
                let into = current.unwrap_or_else(|| "HEAD".to_string());
                let mut menu = menu;
                if !branch.remote {
                    let rename = name.clone();
                    menu = menu.item(item(ws, "Rename…", None, move |ws, window, cx| {
                        ws.rename_branch(block, rename.clone(), window, cx)
                    }));
                }
                if !branch.current {
                    let merge = name.clone();
                    menu = menu.item(item(
                        ws,
                        format!("Merge into {into}…"),
                        Some(IconName::GitMerge),
                        move |ws, window, cx| ws.merge_branch(block, merge.clone(), window, cx),
                    ));
                }
                if !branch.remote && !branch.current {
                    let delete = name.clone();
                    menu = menu.item(item(
                        ws,
                        "Delete…",
                        Some(IconName::Delete),
                        move |ws, window, cx| {
                            ws.delete_branch(block, delete.clone(), false, window, cx)
                        },
                    ));
                }
                if !branch.remote && branch.upstream.is_none() {
                    let publish = name.clone();
                    menu = menu.item(item(
                        ws,
                        "Publish",
                        Some(IconName::Upload),
                        move |ws, window, cx| ws.publish_branch(block, publish.clone(), window, cx),
                    ));
                }
                menu.item(item(
                    ws,
                    "Copy name",
                    Some(IconName::Copy),
                    move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(name.clone())),
                ))
            }
            Items::GitStash { block, index } => menu
                .item(item(ws, "Apply", None, move |ws, window, cx| {
                    ws.stash_apply(block, index, false, window, cx)
                }))
                .item(item(ws, "Pop", None, move |ws, window, cx| {
                    ws.stash_apply(block, index, true, window, cx)
                }))
                .item(item(
                    ws,
                    "Drop…",
                    Some(IconName::Delete),
                    move |ws, window, cx| ws.stash_drop(block, index, window, cx),
                ))
                .item(item(
                    ws,
                    "Show",
                    Some(IconName::FileDiff),
                    move |ws, window, cx| ws.show_stash(block, index, window, cx),
                )),
            Items::Git { block, folder } => menu
                .item(item(
                    ws,
                    "Change folder…",
                    Some(IconName::FolderOpen),
                    move |ws, window, cx| ws.choose_git_folder(block, window, cx),
                ))
                .item(item(
                    ws,
                    "Refresh",
                    Some(IconName::RefreshCw),
                    move |ws, window, cx| ws.refresh_git(block, window, cx),
                ))
                .when_some(folder.filter(|f| f.is_dir()), |menu, folder| {
                    menu.item(item(
                        ws,
                        "Open a terminal here",
                        Some(IconName::SquareTerminal),
                        move |ws, window, cx| {
                            if let Some(id) = ws.terminal_beside(block, &folder, None, window, cx) {
                                let space = ws.space.id.clone();
                                ws.reveal(&space, id, window, cx);
                            }
                        },
                    ))
                })
                .separator()
                .item(item(
                    ws,
                    "Close",
                    Some(IconName::Close),
                    move |ws, window, cx| ws.close(block, window, cx),
                )),
            Items::Editor {
                block,
                tree,
                wrap,
                send,
            } => menu
                .item(item(
                    ws,
                    "Change folder…",
                    Some(IconName::FolderOpen),
                    move |ws, window, cx| ws.choose_editor_folder(block, window, cx),
                ))
                .item(item(
                    ws,
                    if tree {
                        "Hide the file tree"
                    } else {
                        "Show the file tree"
                    },
                    Some(IconName::PanelLeft),
                    move |ws, _, cx| ws.toggle_tree(block, cx),
                ))
                .item(item(
                    ws,
                    if wrap {
                        "Turn soft wrap off"
                    } else {
                        "Turn soft wrap on"
                    },
                    None,
                    move |ws, window, cx| ws.toggle_wrap(block, window, cx),
                ))
                .map(|menu| send_to(ws, menu, send, window, cx))
                .separator()
                .item(item(
                    ws,
                    "Close",
                    Some(IconName::Close),
                    move |ws, window, cx| ws.close(block, window, cx),
                )),
            Items::Reveal { terminals } => {
                terminal_items(ws, menu, &terminals, |ws, id, window, cx| {
                    let space = ws.space.id.clone();
                    ws.reveal(&space, id, window, cx);
                })
            }
            Items::Ink { index } => menu.item(item(
                ws,
                "Delete",
                Some(IconName::Delete),
                move |ws, _, cx| ws.delete_ink(index, cx),
            )),
            Items::Note {
                block,
                preview,
                send,
            } => menu
                .item(item(
                    ws,
                    if preview { "Edit" } else { "Preview" },
                    Some(if preview {
                        IconName::EyeOff
                    } else {
                        IconName::Eye
                    }),
                    move |ws, window, cx| ws.toggle_preview(block, window, cx),
                ))
                .item(item(
                    ws,
                    "Copy all",
                    Some(IconName::Copy),
                    move |ws, _, cx| {
                        if let Some(note) = ws.space.blocks.get(&block).and_then(|b| b.note()) {
                            let text = note.text(cx);
                            cx.write_to_clipboard(ClipboardItem::new_string(text));
                        }
                    },
                ))
                .map(|menu| send_to(ws, menu, send, window, cx))
                .separator()
                .item(item(
                    ws,
                    "Close",
                    Some(IconName::Close),
                    move |ws, window, cx| ws.close(block, window, cx),
                )),
            Items::Board {
                block,
                board,
                name,
                cards: n,
            } => {
                let rename_name = name.clone();
                menu.item(item(ws, "Open", None, move |ws, _, cx| {
                    ws.show_board(block, board, cx)
                }))
                .item(item(ws, "Rename…", None, move |_, window, cx| {
                    let ws = cx.entity();
                    prompt::ask_text(
                        "Rename board",
                        "Rename",
                        rename_name.clone(),
                        move |text, _, cx| rename_board(&ws, board, &text, cx),
                        window,
                        cx,
                    )
                }))
                .item(item(
                    ws,
                    "Delete…",
                    Some(IconName::Delete),
                    move |_, window, cx| {
                        let ws = cx.entity();
                        prompt::confirm(
                            format!("Delete {name}?"),
                            cards(n),
                            "Delete",
                            move |_, cx| ws.update(cx, |ws, cx| ws.delete_board(board, cx)),
                            window,
                            cx,
                        )
                    },
                ))
            }
            Items::Column {
                board,
                column,
                title,
                cards: n,
            } => menu
                .item(item(ws, "Rename…", None, move |_, window, cx| {
                    let ws = cx.entity();
                    prompt::ask_text(
                        "Rename column",
                        "Rename",
                        title.clone(),
                        move |text, _, cx| {
                            let title = text.trim().to_string();
                            if title.is_empty() {
                                return false;
                            }
                            ws.update(cx, |ws, cx| {
                                ws.boards.rename_column(board, column, &title);
                                cx.notify();
                            });
                            true
                        },
                        window,
                        cx,
                    )
                }))
                .item(item(
                    ws,
                    "Add a card",
                    Some(IconName::Plus),
                    move |ws, window, cx| kanban::add_card(ws, board, column, window, cx),
                ))
                .separator()
                .item(item(
                    ws,
                    "Delete",
                    Some(IconName::Delete),
                    move |ws, window, cx| {
                        if n == 0 {
                            ws.boards.remove_column(board, column);
                            cx.notify();
                            return;
                        }
                        let entity = cx.entity();
                        prompt::confirm(
                            "Delete this column?",
                            cards(n),
                            "Delete",
                            move |_, cx| {
                                entity.update(cx, |ws, cx| {
                                    ws.boards.remove_column(board, column);
                                    cx.notify();
                                })
                            },
                            window,
                            cx,
                        );
                    },
                )),
            Items::Card { board, card, send } => menu
                .item(item(ws, "Open", None, move |ws, window, cx| {
                    if let Some(current) = ws.boards.card(board, card).cloned() {
                        kanban::open_card(board, current, false, window, cx);
                    }
                }))
                .map(|menu| send_to(ws, menu, send, window, cx))
                .separator()
                .item(item(
                    ws,
                    "Delete card",
                    Some(IconName::Delete),
                    move |ws, _, cx| {
                        ws.boards.remove_card(board, card);
                        cx.notify();
                    },
                )),
            Items::Space {
                id,
                name,
                only_one,
                unsaved,
            } => {
                let delete_id = id.clone();
                let delete_name = name.clone();
                menu.item(item(ws, "Rename…", None, move |_, window, cx| {
                    let ws = cx.entity();
                    let id = id.clone();
                    prompt::ask_text(
                        "Rename workspace",
                        "Rename",
                        name.clone(),
                        move |text, window, cx| {
                            ws.update(cx, |ws, cx| ws.rename_space(&id, &text, window, cx))
                                .is_ok()
                        },
                        window,
                        cx,
                    )
                }))
                .item(
                    item(
                        ws,
                        "Delete…",
                        Some(IconName::Delete),
                        move |_, window, cx| {
                            let ws = cx.entity();
                            let id = delete_id.clone();
                            if unsaved.is_empty() {
                                prompt::confirm(
                                    format!("Delete {delete_name}?"),
                                    "Its terminals will close.",
                                    "Delete",
                                    move |window, cx| {
                                        ws.update(cx, |ws, cx| ws.delete_space(&id, window, cx))
                                    },
                                    window,
                                    cx,
                                );
                                return;
                            }
                            // Its editors have changed files: save them, or not, first.
                            let (saver, save_id) = (ws.clone(), id.clone());
                            prompt::save_or_discard(
                                format!("Delete {delete_name}?"),
                                format!(
                                    "Its terminals will close. Unsaved: {}",
                                    unsaved.join(", ")
                                ),
                                move |window, cx| {
                                    saver.update(cx, |ws, cx| {
                                        let saved = ws.save_spaces(Some(&save_id), window, cx);
                                        if saved {
                                            ws.delete_space(&save_id, window, cx);
                                        }
                                        saved
                                    })
                                },
                                move |window, cx| {
                                    ws.update(cx, |ws, cx| ws.delete_space(&id, window, cx))
                                },
                                window,
                                cx,
                            );
                        },
                    )
                    .disabled(only_one),
                )
            }
        }
    }
}

fn rename_board(ws: &Entity<Workspace>, board: u64, text: &str, cx: &mut App) -> bool {
    let name = text.trim();
    if name.is_empty() {
        return false;
    }
    ws.update(cx, |ws, cx| {
        ws.boards.rename_board(board, name);
        cx.notify();
    });
    true
}
