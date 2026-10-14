//! Updates from GitHub Releases (docs/specs/updates.md).

use std::path::PathBuf;

use templum_update::{Ready, Release, Version, Why};

use super::*;
use crate::update::{Blocked, Update};

fn release() -> Release {
    Release {
        version: Version::new(0, 2, 0),
        page: "https://github.com/KingDasWinx/Templum/releases/tag/v0.2.0".into(),
        assets: Vec::new(),
    }
}

fn done(handle: AnyWindowHandle, ws: &Entity<Workspace>, state: Update, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.update_done(state, false, window, cx));
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
}

/// A downloaded version shows Restart in the title bar and is told once, not at every check.
#[gpui_kit::test]
fn a_ready_update_shows_restart_and_is_told_once(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("update-pill").is_none(), "no news");
    })
    .unwrap();
    let ready = Update::Ready {
        release: release(),
        ready: Ready::Swap {
            new: PathBuf::from("new"),
            target: PathBuf::from("target"),
            restart: PathBuf::from("target"),
        },
        typed: false,
    };
    let before = notification_count(handle, cx);
    done(handle, &ws, ready.clone(), cx);
    assert_eq!(notification_count(handle, cx), before + 1);
    cx.update_window(handle, |_, window, _| {
        assert!(window.try_find("update-pill").is_some());
    })
    .unwrap();
    done(handle, &ws, ready, cx);
    assert_eq!(notification_count(handle, cx), before + 1, "told once");
}

/// A release Templum can't install is "available": the pill opens its page.
#[gpui_kit::test]
fn a_release_templum_cant_install_is_available(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let manual = Update::Manual {
        release: release(),
        blocked: Blocked::Install(Why::Source),
    };
    done(handle, &ws, manual, cx);
    cx.update_window(handle, |_, window, _| {
        assert!(window.try_find("update-pill").is_some());
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(
            ws.update.next(),
            Some(crate::update::Act::Open(release().page))
        );
    });
}

/// Install opens a terminal in the download's folder, then the pill says Restart.
#[gpui_kit::test]
fn install_opens_a_terminal_in_the_downloads_folder(cx: &mut TestAppContext) {
    let (handle, ws) = open(cx);
    let folder = tempfile::tempdir().unwrap();
    let ready = Update::Ready {
        release: release(),
        ready: Ready::Terminal {
            folder: folder.path().to_path_buf(),
            command: "sudo apt install ./templum_0.2.0_amd64.deb".into(),
            restart: PathBuf::from("/usr/bin/templum"),
        },
        typed: false,
    };
    done(handle, &ws, ready, cx);
    let terminals = ws.read_with(cx, |ws, _| ws.space.terminals().count());
    cx.update_window(handle, |_, window, cx| {
        window.click("update-pill", cx);
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.space.terminals().count(), terminals + 1);
        assert!(matches!(ws.update, Update::Ready { typed: true, .. }));
        assert_eq!(ws.update.next(), Some(crate::update::Act::Restart));
    });
}

/// Quitting puts a downloaded file in the old one's place, restarted or not.
#[gpui_kit::test]
fn quitting_puts_the_new_version_in_place(cx: &mut TestAppContext) {
    let (_, ws) = open(cx);
    let dir = tempfile::tempdir().unwrap();
    let (new, target) = (
        dir.path().join(".templum.update"),
        dir.path().join("templum"),
    );
    std::fs::write(&new, "0.2.0").unwrap();
    std::fs::write(&target, "0.1.1").unwrap();
    ws.update(cx, |ws, _| {
        ws.update = Update::Ready {
            release: release(),
            ready: Ready::Swap {
                new: new.clone(),
                target: target.clone(),
                restart: target.clone(),
            },
            typed: false,
        };
        ws.finish_update();
    });
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "0.2.0");
    assert!(!new.exists());
}

/// Settings ▸ Updates turns the automatic check off in config.toml.
#[gpui_kit::test]
fn the_automatic_check_is_turned_off_in_settings(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    cx.update_window(handle, |_, window, cx| window.click("settings-button", cx))
        .unwrap();
    settle_slide(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        window.click("settings-tab-updates", cx);
        window.render_frame(cx);
        assert!(window.try_find("update-check").is_some());
        window.click("updates-check-switch", cx);
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .unwrap();
    ws.update(cx, |ws, _| ws.save());
    let text = std::fs::read_to_string(dir.path().join(crate::config::FILE)).unwrap();
    assert!(text.contains("check = false"), "{text}");
}
