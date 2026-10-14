//! Where an update stands, as the title bar's pill, Settings ▸ Updates and the notifications say
//! it (docs/specs/updates.md).

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, ClickEvent, Context, TestSupportExt, px};
use templum_update::{Ready, Release, Why};

use crate::title_bar;
use crate::workspace::Workspace;

/// Where an update stands.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum Update {
    /// Not checked since Templum started.
    #[default]
    Idle,
    Checking,
    /// The newest release is this one; checked at (seconds since the epoch).
    UpToDate(u64),
    Downloading {
        release: Release,
        got: u64,
        total: u64,
    },
    /// `typed`: Install typed the package manager's command in a terminal.
    Ready {
        release: Release,
        ready: Ready,
        typed: bool,
    },
    /// A newer release Templum doesn't install.
    Manual {
        release: Release,
        blocked: Blocked,
    },
    /// The check failed.
    Failed(String),
}

/// Why Templum doesn't install a newer release.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Blocked {
    Install(Why),
    Failed(String),
}

/// What a click on the pill or a button does next.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Act {
    Restart,
    /// Types the package manager's command in a terminal.
    Install,
    /// Opens a page: the release's.
    Open(String),
    /// Shows Settings ▸ Updates.
    Settings,
}

impl Update {
    /// The pill's next step, None while there is no news.
    pub(crate) fn next(&self) -> Option<Act> {
        match self {
            Update::Downloading { .. } => Some(Act::Settings),
            Update::Ready {
                ready: Ready::Terminal { .. },
                typed: false,
                ..
            } => Some(Act::Install),
            Update::Ready { .. } => Some(Act::Restart),
            Update::Manual { release, .. } => Some(Act::Open(release.page.clone())),
            _ => None,
        }
    }

    /// The pill's text.
    fn label(&self) -> Option<String> {
        Some(match self {
            Update::Downloading {
                release,
                got,
                total,
            } => format!("{} · {}%", release.version, percent(*got, *total)),
            Update::Ready {
                release,
                ready: Ready::Terminal { .. },
                typed: false,
            } => format!("Install {}", release.version),
            Update::Ready { .. } => "Restart to update".to_string(),
            Update::Manual { release, .. } => format!("{} available", release.version),
            _ => return None,
        })
    }

    /// Settings ▸ Updates' line about it; `checked` says when the up-to-date check ran.
    pub(crate) fn status(&self, checked: impl Fn(u64) -> String) -> String {
        match self {
            Update::Idle => "Not checked yet".to_string(),
            Update::Checking => "Checking…".to_string(),
            Update::UpToDate(at) => format!("Up to date · checked {}", checked(*at)),
            Update::Downloading {
                release,
                got,
                total,
            } => format!(
                "Downloading {}… {}%",
                release.version,
                percent(*got, *total)
            ),
            Update::Ready {
                release,
                ready: Ready::Terminal { .. },
                typed,
            } => {
                if *typed {
                    format!("Restart once {} is installed", release.version)
                } else {
                    format!(
                        "{} is downloaded: install it in a terminal (it asks for your password)",
                        release.version
                    )
                }
            }
            Update::Ready { release, .. } => {
                format!("{} is ready: restart to use it", release.version)
            }
            Update::Manual {
                release,
                blocked: Blocked::Install(why),
            } => format!("{} is out. {}", release.version, why_text(*why)),
            Update::Manual {
                release,
                blocked: Blocked::Failed(error),
            } => format!("Couldn't update to {}: {error}", release.version),
            Update::Failed(error) => format!("Couldn't check: {error}"),
        }
    }
}

/// Why Templum can't replace itself, for the person.
pub(crate) fn why_text(why: Why) -> &'static str {
    match why {
        Why::Source => "This Templum was built from source: pull and build it again.",
        Why::Package => "Update it with the package manager that installed it.",
        Why::NotInstalled => "Move Templum to Applications to update it in place.",
    }
}

fn percent(got: u64, total: u64) -> u64 {
    (got * 100).checked_div(total).unwrap_or(0).min(100)
}

/// The title bar's pill while there is news: the download's progress, then what to do.
pub(crate) fn pill(ws: &Workspace, cx: &mut Context<Workspace>) -> Option<AnyElement> {
    let label = ws.update.label()?;
    let ui = ws.ui;
    let icon = match ws.update {
        Update::Downloading { .. } => IconName::Download,
        Update::Ready { .. } => IconName::RotateCw,
        _ => IconName::CircleArrowUp,
    };
    Some(
        title_bar::button("update-pill", ui)
            .gap(px(6.0))
            .child(Icon::new(icon).size(px(14.0)).text_color(ui.title))
            .child(label)
            .on_click(cx.listener(|ws, _: &ClickEvent, window, cx| {
                if let Some(act) = ws.update.next() {
                    ws.update_act(&act, window, cx);
                }
            }))
            .test_support()
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn release() -> Release {
        Release {
            version: templum_update::Version::new(0, 2, 0),
            page: "https://github.com/KingDasWinx/Templum/releases/tag/v0.2.0".into(),
            assets: Vec::new(),
        }
    }

    fn swap() -> Ready {
        Ready::Swap {
            new: PathBuf::new(),
            target: PathBuf::new(),
            restart: PathBuf::new(),
        }
    }

    fn terminal() -> Ready {
        Ready::Terminal {
            folder: PathBuf::new(),
            command: String::new(),
            restart: PathBuf::new(),
        }
    }

    #[test]
    fn the_pill_says_what_comes_next() {
        let downloading = Update::Downloading {
            release: release(),
            got: 45,
            total: 100,
        };
        assert_eq!(downloading.label().as_deref(), Some("0.2.0 · 45%"));
        assert_eq!(downloading.next(), Some(Act::Settings));
        let ready = Update::Ready {
            release: release(),
            ready: swap(),
            typed: false,
        };
        assert_eq!(ready.label().as_deref(), Some("Restart to update"));
        assert_eq!(ready.next(), Some(Act::Restart));
        let package = Update::Ready {
            release: release(),
            ready: terminal(),
            typed: false,
        };
        assert_eq!(package.label().as_deref(), Some("Install 0.2.0"));
        assert_eq!(package.next(), Some(Act::Install));
        let typed = Update::Ready {
            release: release(),
            ready: terminal(),
            typed: true,
        };
        assert_eq!(typed.next(), Some(Act::Restart));
        let manual = Update::Manual {
            release: release(),
            blocked: Blocked::Install(Why::Source),
        };
        assert_eq!(manual.label().as_deref(), Some("0.2.0 available"));
        assert_eq!(manual.next(), Some(Act::Open(release().page)));
        for quiet in [
            Update::Idle,
            Update::Checking,
            Update::UpToDate(0),
            Update::Failed("offline".into()),
        ] {
            assert_eq!(quiet.label(), None, "{quiet:?}");
            assert_eq!(quiet.next(), None);
        }
    }

    #[test]
    fn settings_say_where_it_stands() {
        let at = |_| "14:02".to_string();
        assert_eq!(Update::UpToDate(0).status(at), "Up to date · checked 14:02");
        let downloading = Update::Downloading {
            release: release(),
            got: 0,
            total: 0,
        };
        assert_eq!(downloading.status(at), "Downloading 0.2.0… 0%");
        let failed = Update::Manual {
            release: release(),
            blocked: Blocked::Failed("disk full".into()),
        };
        assert_eq!(failed.status(at), "Couldn't update to 0.2.0: disk full");
        let package = Update::Manual {
            release: release(),
            blocked: Blocked::Install(Why::Package),
        };
        assert_eq!(
            package.status(at),
            "0.2.0 is out. Update it with the package manager that installed it."
        );
    }
}
