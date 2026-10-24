//! Automations, the person's cron (docs/specs/automations.md): rules that run at a time and do
//! one thing. This is their pure layer: the saved rules and their file here, the times in
//! `clock`. They run in `workspace/automations.rs` and are drawn by `automations_view`.

use chrono::{NaiveDateTime, NaiveTime};
use serde::{Deserialize, Serialize};

pub(crate) mod clock;

/// When an automation runs: a time, on the computer's clock.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum When {
    Once {
        at: NaiveDateTime,
    },
    /// 1 ..= `clock::EVERY_MOST`, real time from the rule's `since`.
    Every {
        minutes: u32,
    },
    Daily {
        at: NaiveTime,
    },
    /// 1 Monday ..= 7 Sunday.
    Weekly {
        days: Vec<u8>,
        at: NaiveTime,
    },
    /// 1 ..= 31.
    Monthly {
        days: Vec<u8>,
        at: NaiveTime,
    },
    Cron {
        expr: String,
    },
}

use std::io;
use std::path::{Path, PathBuf};

use crate::session::{self, Loaded};

/// The file format's version: raised only when an older Templum couldn't read the new files.
pub(crate) const VERSION: u32 = 1;
/// The most characters of a name.
pub(crate) const NAME_MAX: usize = 60;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Automations {
    pub version: u32,
    /// The last id given out: never reused.
    #[serde(default)]
    pub next_id: u64,
    #[serde(default)]
    pub automations: Vec<Automation>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Automation {
    pub id: u64,
    #[serde(default)]
    pub name: String,
    /// Off keeps it and runs nothing.
    #[serde(default = "on")]
    pub on: bool,
    /// The id of the workspace it is for; None: every workspace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    pub when: When,
    #[serde(rename = "do")]
    pub act: Do,
    /// Stops (turns off) after this many runs that went.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_runs: Option<u32>,
    #[serde(default)]
    pub runs: u32,
    /// When its When started counting: Unix seconds.
    #[serde(default)]
    pub since: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<LastRun>,
    /// The uid of the terminal its command last opened.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opened: Option<String>,
}

fn on() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Do {
    /// Type `text` into the agent in the block `block` (a uid).
    Prompt {
        block: String,
        text: String,
    },
    /// Run `text` in the terminal `terminal` (a uid), or in a new one in `folder`.
    Command {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        terminal: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        folder: Option<String>,
        text: String,
    },
    /// Open the agent `agent` (an id of Settings ▸ Agents).
    Agent {
        agent: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        folder: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        effort: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prompt: Option<String>,
    },
    Notify {
        text: String,
    },
}

/// How an automation's last run went.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct LastRun {
    /// Unix seconds.
    pub at: i64,
    pub went: Went,
    pub about: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Went {
    Ran,
    Skipped,
    Failed,
    /// Its time came while Templum was closed.
    Missed,
}

impl Default for Automations {
    fn default() -> Self {
        Automations {
            version: VERSION,
            next_id: 0,
            automations: Vec::new(),
        }
    }
}

impl Automations {
    /// Gives `a` the next id and puts it after the others; returns the id.
    pub fn add(&mut self, mut a: Automation) -> u64 {
        self.next_id += 1;
        a.id = self.next_id;
        self.automations.push(a);
        self.next_id
    }

    pub fn get(&self, id: u64) -> Option<&Automation> {
        self.automations.iter().find(|a| a.id == id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut Automation> {
        self.automations.iter_mut().find(|a| a.id == id)
    }

    pub fn remove(&mut self, id: u64) {
        self.automations.retain(|a| a.id != id);
    }

    /// A copy after the last: "<name> (copy)" (an empty name stays empty), off, with no runs, last
    /// run nor kept terminal. Returns its id.
    pub fn duplicate(&mut self, id: u64) -> Option<u64> {
        let mut copy = self.get(id)?.clone();
        if !copy.name.is_empty() {
            // Cut first: the copy's name is no longer than a name may be.
            const COPY: &str = " (copy)";
            let kept: String = copy.name.chars().take(NAME_MAX - COPY.len()).collect();
            copy.name = format!("{kept}{COPY}");
        }
        (copy.on, copy.runs, copy.last, copy.opened) = (false, 0, None, None);
        Some(self.add(copy))
    }
}

pub(crate) fn path(dir: &Path) -> PathBuf {
    dir.join("automations.json")
}

pub(crate) fn load(dir: &Path) -> Loaded<Automations> {
    match session::read::<Automations>(&path(dir)) {
        Loaded::Saved(mut saved) => {
            let ids = saved.automations.iter_mut().map(|a| &mut a.id);
            session::unique_ids(ids, &mut saved.next_id);
            Loaded::Saved(saved)
        }
        other => other,
    }
}

/// Writes the automations; like canvases, a write older than the last one done is dropped.
pub(crate) fn save(dir: &Path, automations: &Automations, generation: u64) -> io::Result<()> {
    let text = serde_json::to_string_pretty(automations).map_err(io::Error::other)?;
    session::write_newest(&path(dir), &text, generation)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The example of docs/specs/automations.md#where-they-are-saved.
    const EXAMPLE: &str = r#"{ "version": 1, "next_id": 2, "automations": [
        { "id": 1, "name": "Morning review", "on": true, "workspace": "k2v8q",
          "when": { "weekly": { "days": [1, 2, 3, 4, 5], "at": "09:00:00" } },
          "do": { "prompt": { "block": "abc12", "text": "Review the open pull requests." } },
          "runs": 3, "since": 1761300000,
          "last": { "at": 1761555600, "went": "ran", "about": "Typed into abc12 as task t4." } },
        { "id": 2, "name": "", "on": true,
          "when": { "every": { "minutes": 10 } },
          "do": { "command": { "folder": "~/api", "text": "curl -fsS localhost:8080/health" } },
          "max_runs": 6, "runs": 0, "since": 1761558000, "opened": "q3x9z" } ] }"#;

    fn notify(name: &str) -> Automation {
        Automation {
            id: 0,
            name: name.into(),
            on: true,
            workspace: None,
            when: When::Daily {
                at: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            },
            act: Do::Notify { text: "Hi".into() },
            max_runs: None,
            runs: 0,
            since: 0,
            last: None,
            opened: None,
        }
    }

    #[test]
    fn the_specs_example_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(path(dir.path()), EXAMPLE).unwrap();
        let Loaded::Saved(loaded) = load(dir.path()) else {
            panic!("the example loads");
        };
        let (first, second) = (&loaded.automations[0], &loaded.automations[1]);
        let nine = NaiveTime::from_hms_opt(9, 0, 0).unwrap();
        assert_eq!(
            first.when,
            When::Weekly {
                days: vec![1, 2, 3, 4, 5],
                at: nine
            }
        );
        assert_eq!(
            first.act,
            Do::Prompt {
                block: "abc12".into(),
                text: "Review the open pull requests.".into()
            }
        );
        assert_eq!(first.last.as_ref().unwrap().went, Went::Ran);
        assert_eq!(second.max_runs, Some(6));
        assert_eq!(second.opened.as_deref(), Some("q3x9z"));
        assert_eq!(
            second.act,
            Do::Command {
                terminal: None,
                folder: Some("~/api".into()),
                text: "curl -fsS localhost:8080/health".into()
            }
        );
        save(dir.path(), &loaded, 1).unwrap();
        let Loaded::Saved(again) = load(dir.path()) else {
            panic!("the saved file loads");
        };
        assert_eq!(again, loaded);
    }

    #[test]
    fn ids_are_never_reused_and_a_copy_is_off() {
        let mut list = Automations::default();
        let first = list.add(notify("A"));
        let second = list.add(notify("B"));
        list.remove(second);
        let third = list.add(notify("C"));
        assert!(third > second && second > first);

        let a = list.get_mut(first).unwrap();
        a.runs = 3;
        a.opened = Some("q3x9z".into());
        a.last = Some(LastRun {
            at: 1,
            went: Went::Ran,
            about: "Notified.".into(),
        });
        let copy = list.duplicate(first).unwrap();
        let copy = list.get(copy).unwrap();
        assert_eq!(copy.name, "A (copy)");
        assert!(!copy.on);
        assert_eq!((copy.runs, &copy.last, &copy.opened), (0, &None, &None));
        assert_eq!(list.automations.last().unwrap().id, copy.id, "placed last");
        // An empty name stays empty.
        let unnamed = list.add(notify(""));
        let copy = list.duplicate(unnamed).unwrap();
        assert_eq!(list.get(copy).unwrap().name, "");
    }

    #[test]
    fn a_copy_of_a_long_name_is_no_longer_than_a_name_may_be() {
        let mut list = Automations::default();
        let id = list.add(notify(&"n".repeat(NAME_MAX)));
        let copy = list.duplicate(id).unwrap();
        let name = &list.get(copy).unwrap().name;
        assert_eq!(name.chars().count(), NAME_MAX);
        assert!(name.ends_with(" (copy)"));
    }

    #[test]
    fn a_repeated_or_missing_id_is_given_a_new_one() {
        let dir = tempfile::tempdir().unwrap();
        let mut list = Automations::default();
        list.add(notify("A"));
        list.add(notify("B"));
        list.automations[1].id = 1;
        save(dir.path(), &list, 1).unwrap();
        let Loaded::Saved(loaded) = load(dir.path()) else {
            panic!("it loads");
        };
        let ids: Vec<u64> = loaded.automations.iter().map(|a| a.id).collect();
        assert_eq!(ids, [1, 3]);
    }
}
