//! The Git block's layout: its size, the left column's fold, Filter files.

use super::*;

#[gpui_kit::test]
fn a_new_git_block_opens_1200_by_760_and_shrinks_to_640_by_420(cx: &mut TestAppContext) {
    let repo = git_repo();
    let (handle, ws) = open(cx);
    let id = cx
        .update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.add_git_at(repo.path(), Point::new(0.0, 0.0), window, cx)
                    .unwrap()
            })
        })
        .unwrap();
    ws.read_with(cx, |ws, _| {
        let r = ws.space.canvas.get(id).unwrap().rect;
        assert_eq!((r.w, r.h), (1200.0, 760.0));
        let min = ws.space.blocks[&id].min_size(ws.cell.size);
        assert_eq!((min.w, min.h), (640.0, 420.0));
    });
}

#[gpui_kit::test]
fn the_lists_fold_away_and_the_block_remembers(cx: &mut TestAppContext) {
    let repo = git_repo();
    std::fs::write(repo.path().join("a.txt"), "x\n").unwrap();
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    // The lists' rows (a uniform list's own id isn't observable).
    let a = crate::git_block::row_id(id, "unstaged", 0);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(a.clone()).is_some(), "the lists are drawn");
        window.click(("git-fold-lists", n), cx);
        window.render_frame(cx);
        assert!(
            window.try_find(a.clone()).is_none(),
            "the diff takes the width"
        );
    })
    .unwrap();
    let saved = ws.read_with(cx, |ws, cx| {
        ws.space
            .git(id)
            .unwrap()
            .item(Rect::new(0.0, 0.0, 1.0, 1.0), cx)
    });
    assert_eq!(saved.lists, "folded");
    cx.update_window(handle, |_, window, cx| {
        window.click(("git-unfold-lists", n), cx);
        window.render_frame(cx);
        assert!(window.try_find(a).is_some());
    })
    .unwrap();
}

#[gpui_kit::test]
fn a_folded_block_comes_back_folded_and_a_small_one_grows_to_the_minimum(cx: &mut TestAppContext) {
    let repo = git_repo();
    let data = tempfile::tempdir().unwrap();
    let json = repo.path().display().to_string().replace('\\', "\\\\");
    write_canvas(
        data.path(),
        &format!(
            r#"{{"version":1,"camera":{{"x":0,"y":0,"zoom":1}},"items":[
                  {{"kind":"git","rect":{{"x":0,"y":0,"w":500,"h":350}},"folder":"{json}","lists":"folded"}}]}}"#
        ),
    );
    let (handle, ws) = open_in(cx, None, Some(data.path().to_path_buf()));
    settle(handle, cx);
    let id = first_id(&ws, cx);
    git_of(&ws, cx, id, |g| assert!(g.lists_folded));
    ws.read_with(cx, |ws, _| {
        let r = ws.space.canvas.get(id).unwrap().rect;
        assert_eq!((r.w, r.h), (640.0, 420.0));
    });
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
    let id = open_git(handle, &ws, cx, repo.path());
    let filter = git_of(&ws, cx, id, |g| {
        assert!(g.filtering(), "16 files");
        g.files_filter.clone()
    });
    cx.update_window(handle, |_, window, cx| {
        filter.update(cx, |f, cx| f.set_value("B.T", window, cx))
    })
    .unwrap();
    let listed = ws.read_with(cx, |ws, cx| ws.space.git(id).unwrap().listed(cx));
    assert_eq!(
        listed,
        [
            (List::Staged, None),
            (List::Unstaged, None),
            (List::Unstaged, Some(15)),
        ],
        "b.txt, by its index among all 16, without case"
    );
    // At 15 files no field shows, and a filter left from before narrows nothing.
    std::fs::remove_file(repo.path().join("b.txt")).unwrap();
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.refresh_git(id, window, cx))
    })
    .unwrap();
    settle(handle, cx);
    ws.read_with(cx, |ws, cx| {
        let g = ws.space.git(id).unwrap();
        assert!(!g.filtering());
        assert_eq!(g.listed(cx).len(), 17, "two headers, fifteen rows");
    });
}

#[gpui_kit::test]
fn discard_all_with_a_filter_on_takes_only_the_rows_shown(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    no_motion(cx);
    let repo = many_files(15);
    let (handle, ws) = open(cx);
    let id = open_git(handle, &ws, cx, repo.path());
    let n = id.0 as usize;
    let filter = git_of(&ws, cx, id, |g| g.files_filter.clone());
    cx.update_window(handle, |_, window, cx| {
        filter.update(cx, |f, cx| f.set_value("b", window, cx));
        window.render_frame(cx);
        window.click(("git-discard-all", n), cx);
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
