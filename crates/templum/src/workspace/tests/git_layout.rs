//! The Git block's layout: its size, the left column's fold, Filter files.

use super::*;

#[gpui_kit::test]
fn the_lists_fold_away_and_the_block_remembers(cx: &mut TestAppContext) {
    let repo = git_repo();
    std::fs::write(repo.path().join("a.txt"), "x\n").unwrap();
    let (handle, ws) = open(cx);
    open_git(handle, &ws, cx, repo.path());
    // The lists' rows (a uniform list's own id isn't observable).
    let a = crate::git_block::row_id("files", 0);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(a.clone()).is_some(), "the lists are drawn");
        window.click("git-fold-lists", cx);
        window.render_frame(cx);
        assert!(
            window.try_find(a.clone()).is_none(),
            "the diff takes the width"
        );
    })
    .unwrap();
    let saved = ws.read_with(cx, |ws, cx| ws.space.git().unwrap().saved(cx));
    assert_eq!(saved.lists, "folded");
    cx.update_window(handle, |_, window, cx| {
        window.click("git-unfold-lists", cx);
        window.render_frame(cx);
        assert!(window.try_find(a).is_some());
    })
    .unwrap();
}

/// `count` untracked files a00.txt…, then b.txt.
fn many_files(count: usize) -> tempfile::TempDir {
    let repo = git_repo();
    for i in 0..count {
        std::fs::write(repo.path().join(format!("a{i:02}.txt")), "x\n").unwrap();
    }
    std::fs::write(repo.path().join("b.txt"), "x\n").unwrap();
    repo
}

#[gpui_kit::test]
fn past_15_files_a_filter_narrows_the_lists(cx: &mut TestAppContext) {
    use crate::git_block::List;
    let repo = many_files(15);
    let (handle, ws) = open(cx);
    open_git(handle, &ws, cx, repo.path());
    let filter = git_of(&ws, cx, |g| {
        assert!(g.filtering(), "16 files");
        g.files_filter.clone()
    });
    cx.update_window(handle, |_, window, cx| {
        filter.update(cx, |f, cx| f.set_value("B.T", window, cx))
    })
    .unwrap();
    let listed = ws.read_with(cx, |ws, cx| ws.space.git().unwrap().listed(cx));
    assert_eq!(
        listed,
        [(List::Files, None), (List::Files, Some(15)),],
        "b.txt, by its index among all 16, without case"
    );
    // At 15 files no field shows, and a filter left from before narrows nothing.
    std::fs::remove_file(repo.path().join("b.txt")).unwrap();
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.refresh_git(window, cx))
    })
    .unwrap();
    settle(handle, cx);
    ws.read_with(cx, |ws, cx| {
        let g = ws.space.git().unwrap();
        assert!(!g.filtering());
        assert_eq!(g.listed(cx).len(), 16, "one header, fifteen rows");
    });
}

#[gpui_kit::test]
fn discard_all_with_a_filter_on_takes_only_the_rows_shown(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let repo = many_files(15);
    let (handle, ws) = open(cx);
    open_git(handle, &ws, cx, repo.path());
    let filter = git_of(&ws, cx, |g| g.files_filter.clone());
    cx.update_window(handle, |_, window, cx| {
        filter.update(cx, |f, cx| f.set_value("b", window, cx));
        window.render_frame(cx);
        window.click("git-discard-all", cx);
    })
    .unwrap();
    settle(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        assert!(
            window.has_active_dialog(cx),
            "Discard the changes to b.txt?"
        );
        window.within("dialog").click("ok", cx);
        window.render_frame(cx);
    })
    .unwrap();
    settle(handle, cx);
    assert!(!repo.path().join("b.txt").exists());
    assert!(
        repo.path().join("a00.txt").exists(),
        "a filtered-out file stays"
    );
}
