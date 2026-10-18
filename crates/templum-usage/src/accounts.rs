//! Accounts of an agent: the agent run with another folder, named by one variable of its
//! environment, whose items are its own or links to another folder's. Never its login: see
//! docs/specs/accounts.md.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// One file or folder of an agent's folder that an account may share.
#[derive(Debug, PartialEq)]
pub struct Item {
    /// Its key in `share` (`skills`).
    pub key: &'static str,
    /// What the dialog calls it.
    pub label: &'static str,
    /// Relative to the agent's state folder; a folder ends in `/`. Shared together.
    pub paths: &'static [&'static str],
}

/// Where an agent keeps its login outside its folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keychain {
    Never,
    /// The macOS Keychain, by the folder's path (Claude Code).
    MacOs,
    /// The system's keychain on every OS (Copilot CLI).
    Always,
}

/// One of Templum's agents that has accounts: what moves its folder, what it shares and its login.
#[derive(Debug, PartialEq)]
pub struct Kind {
    /// Its id among Templum's agents.
    pub agent: &'static str,
    /// The variable that names its folder.
    pub var: &'static str,
    /// Its folder when the variable is unset, under the home folder (`""`: the home itself).
    pub own: &'static str,
    /// The subfolder of the variable's folder that holds its files (Gemini CLI's `.gemini`).
    pub state: Option<&'static str>,
    pub items: &'static [Item],
    /// Its login: never made, linked, moved, read or deleted. A folder ends in `/`.
    pub login: &'static [&'static str],
    /// Typed in a terminal of the account to log in.
    pub login_command: &'static str,
    /// What to type in the agent then (`/auth`).
    pub login_hint: Option<&'static str>,
    /// Variables that win over every account's login.
    pub overrides: &'static [&'static str],
    pub keychain: Keychain,
    /// The dialog's line under the items.
    pub note: Option<&'static str>,
}

const fn item(key: &'static str, label: &'static str, paths: &'static [&'static str]) -> Item {
    Item { key, label, paths }
}

/// The agents with accounts, in the spec's order (docs/specs/accounts.md, Which agents).
const KINDS: &[Kind] = &[
    Kind {
        agent: "claude",
        var: "CLAUDE_CONFIG_DIR",
        own: ".claude",
        state: None,
        items: &[
            item("settings", "Settings", &["settings.json"]),
            item("instructions", "Instructions", &["CLAUDE.md"]),
            item("rules", "Rules", &["rules/"]),
            item("skills", "Skills", &["skills/"]),
            item("agents", "Subagents", &["agents/"]),
            item("commands", "Commands", &["commands/"]),
            item("output-styles", "Output styles", &["output-styles/"]),
            item("hooks", "Hook scripts", &["hooks/"]),
            item("plugins", "Plugins", &["plugins/"]),
            item("keybindings", "Keybindings", &["keybindings.json"]),
            item(
                "sessions",
                "Sessions, for /resume",
                &["projects/", "file-history/", "plans/"],
            ),
            item("history", "Prompt history", &["history.jsonl"]),
        ],
        login: &[
            ".credentials.json",
            ".credentials.lock",
            ".claude.json",
            ".config.json",
            "backups/",
            "remote-settings.json",
            "policy-limits.json",
            "stats-cache.json",
            "mcp-needs-auth-cache.json",
            "jobs/",
            "daemon/",
        ],
        login_command: "claude auth login",
        login_hint: None,
        overrides: &[
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "CLAUDE_CODE_OAUTH_TOKEN",
            "CLAUDE_SECURESTORAGE_CONFIG_DIR",
        ],
        keychain: Keychain::MacOs,
        note: Some(
            "MCP servers you added with `claude mcp add -s user` live with the login: add them \
             again in this account, or keep them in a project's `.mcp.json`. It also asks again \
             to trust each folder.",
        ),
    },
    Kind {
        agent: "codex",
        var: "CODEX_HOME",
        own: ".codex",
        state: None,
        items: &[
            item(
                "config",
                "Settings, MCP servers and profiles",
                &["config.toml"],
            ),
            item("instructions", "Instructions", &["AGENTS.md"]),
            item("hooks", "Hooks", &["hooks.json"]),
            item("rules", "Rules", &["rules/"]),
            item("agents", "Agents", &["agents/"]),
            item("skills", "Skills", &["skills/"]),
            item("plugins", "Plugins", &["plugins/"]),
            item(
                "sessions",
                "Sessions",
                &["sessions/", "session_index.jsonl"],
            ),
            item("history", "Prompt history", &["history.jsonl"]),
        ],
        login: &["auth.json", ".credentials.json"],
        login_command: "codex login",
        login_hint: None,
        overrides: &[],
        keychain: Keychain::Never,
        note: Some(
            "Codex's own list of sessions to resume reads a database of each folder: another \
             account's shared sessions resume by their id. Skills in ~/.agents/skills are every \
             account's.",
        ),
    },
    Kind {
        agent: "gemini",
        var: "GEMINI_CLI_HOME",
        own: "",
        state: Some(".gemini"),
        items: &[
            item("instructions", "Instructions", &["GEMINI.md"]),
            item("commands", "Commands", &["commands/"]),
            item("skills", "Skills", &["skills/"]),
            item("extensions", "Extensions", &["extensions/"]),
        ],
        login: &[
            "oauth_creds.json",
            "google_accounts.json",
            "gemini-credentials.json",
            "mcp-oauth-tokens.json",
            "a2a-oauth-tokens.json",
            ".env",
        ],
        login_command: "gemini",
        login_hint: Some("/auth"),
        overrides: &[],
        keychain: Keychain::Never,
        note: Some(
            "Gemini CLI rewrites its settings.json, so each account has its own. Its login is \
             this folder's when you sign in with Google (the default); an API key goes to the \
             system's keychain, one for every folder.",
        ),
    },
    Kind {
        agent: "copilot",
        var: "COPILOT_HOME",
        own: ".copilot",
        state: None,
        items: &[
            item("settings", "Settings", &["settings.json"]),
            item(
                "instructions",
                "Instructions",
                &["copilot-instructions.md", "instructions/"],
            ),
            item("agents", "Agents", &["agents/"]),
            item("skills", "Skills", &["skills/"]),
            item("hooks", "Hooks", &["hooks/"]),
            item("extensions", "Extensions", &["extensions/"]),
            item("mcp", "MCP servers", &["mcp-config.json"]),
            item("lsp", "Language servers", &["lsp-config.json"]),
        ],
        login: &["config.json", "mcp-oauth-config/", "mcp-secrets/"],
        login_command: "copilot login",
        login_hint: None,
        overrides: &["COPILOT_GITHUB_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"],
        keychain: Keychain::Always,
        note: Some("Copilot CLI not logged in here uses the GitHub CLI's login (gh)."),
    },
    Kind {
        agent: "pi",
        var: "PI_CODING_AGENT_DIR",
        own: ".pi/agent",
        state: None,
        items: &[
            item("settings", "Settings", &["settings.json"]),
            item("keybindings", "Keybindings", &["keybindings.json"]),
            item(
                "system",
                "System prompt",
                &["SYSTEM.md", "APPEND_SYSTEM.md"],
            ),
            item("extensions", "Extensions", &["extensions/"]),
            item("skills", "Skills", &["skills/"]),
            item("prompts", "Prompts", &["prompts/"]),
            item("themes", "Themes", &["themes/"]),
            item("tools", "Tools", &["tools/"]),
            item("sessions", "Sessions", &["sessions/"]),
        ],
        login: &["auth.json"],
        login_command: "pi",
        login_hint: Some("/login"),
        overrides: &[],
        keychain: Keychain::Never,
        note: None,
    },
    Kind {
        agent: "grok",
        var: "GROK_HOME",
        own: ".grok",
        state: None,
        items: &[
            item("config", "Settings", &["config.toml"]),
            item("instructions", "Instructions", &["AGENTS.md"]),
            item("rules", "Rules", &["rules/"]),
            item("skills", "Skills", &["skills/"]),
            item("agents", "Agents", &["agents/"]),
            item("commands", "Commands", &["commands/"]),
            item("plugins", "Plugins", &["plugins/", "installed-plugins/"]),
            item("sessions", "Sessions", &["sessions/"]),
        ],
        login: &[
            "auth.json",
            "mcp_credentials.json",
            "managed_config.toml",
            "requirements.toml",
        ],
        login_command: "grok login",
        login_hint: None,
        overrides: &[],
        keychain: Keychain::Never,
        note: None,
    },
    Kind {
        agent: "kimi",
        var: "KIMI_CODE_HOME",
        own: ".kimi-code",
        state: None,
        items: &[
            item("config", "Settings", &["config.toml"]),
            item("instructions", "Instructions", &["AGENTS.md"]),
            item("mcp", "MCP servers", &["mcp.json"]),
            item("skills", "Skills", &["skills/"]),
            item("plugins", "Plugins", &["plugins/"]),
            item("sessions", "Sessions", &["sessions/"]),
        ],
        login: &["credentials/", "device_id", "region"],
        login_command: "kimi login",
        login_hint: None,
        overrides: &[],
        keychain: Keychain::Never,
        note: None,
    },
    Kind {
        agent: "crush",
        var: "CRUSH_GLOBAL_DATA",
        // ponytail: Linux's default; XDG_DATA_HOME and Windows' %LOCALAPPDATA% move it, which
        // only the "not the agent's own folder" check would miss, as Crush shares nothing.
        own: ".local/share/crush",
        state: None,
        items: &[],
        login: &["crush.json"],
        login_command: "crush login",
        login_hint: None,
        overrides: &[],
        keychain: Keychain::Never,
        note: Some(
            "Crush keeps its login and its settings in one file, crush.json: this account shares \
             nothing. Its sessions are each project's (.crush/), every account's.",
        ),
    },
];

/// The agents with accounts, in the spec's order.
pub fn kinds() -> &'static [Kind] {
    KINDS
}

/// `agent`'s accounts, when it has them.
pub fn kind(agent: &str) -> Option<&'static Kind> {
    KINDS.iter().find(|k| k.agent == agent)
}

impl Kind {
    /// The agent's own folder: its variable `var` when set and not empty, else its default under
    /// `home`.
    pub fn own_folder(&self, var: Option<OsString>, home: Option<&Path>) -> Option<PathBuf> {
        match var.filter(|v| !v.is_empty()) {
            Some(v) => Some(PathBuf::from(v)),
            None if self.own.is_empty() => home.map(Path::to_path_buf),
            None => home.map(|h| h.join(self.own)),
        }
    }

    /// Where the agent keeps its files when its variable names `folder`.
    pub fn state_dir(&self, folder: &Path) -> PathBuf {
        match self.state {
            Some(sub) => folder.join(sub),
            None => folder.to_path_buf(),
        }
    }

    pub fn item(&self, key: &str) -> Option<&'static Item> {
        self.items.iter().find(|i| i.key == key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    fn trim(p: &str) -> &str {
        p.trim_end_matches('/')
    }

    #[test]
    fn no_item_is_a_login() {
        for kind in kinds() {
            for item in kind.items {
                for path in item.paths {
                    for login in kind.login {
                        let (p, l) = (trim(path), trim(login));
                        assert_ne!(p, l, "{}: {}", kind.agent, item.key);
                        assert!(
                            !p.starts_with(&format!("{l}/")),
                            "{}: {p} under {l}",
                            kind.agent
                        );
                        assert!(
                            !l.starts_with(&format!("{p}/")),
                            "{}: {p} holds {l}",
                            kind.agent
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn keys_are_unique_and_named() {
        for kind in kinds() {
            assert!(!kind.var.is_empty());
            let mut keys: Vec<&str> = kind.items.iter().map(|i| i.key).collect();
            keys.sort();
            keys.dedup();
            assert_eq!(keys.len(), kind.items.len(), "{}", kind.agent);
            for item in kind.items {
                assert!(!item.label.is_empty() && !item.paths.is_empty());
                for path in item.paths {
                    assert!(
                        !Path::new(path).is_absolute() && !path.contains(".."),
                        "{path}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_agents_are_the_specs() {
        let ids: Vec<&str> = kinds().iter().map(|k| k.agent).collect();
        assert_eq!(
            ids,
            [
                "claude", "codex", "gemini", "copilot", "pi", "grok", "kimi", "crush"
            ]
        );
        let claude = kind("claude").unwrap();
        assert_eq!(claude.var, "CLAUDE_CONFIG_DIR");
        assert!(claude.login.contains(&".credentials.json"));
        assert!(claude.login.contains(&".claude.json"));
        assert_eq!(
            claude.item("sessions").unwrap().paths,
            ["projects/", "file-history/", "plans/"]
        );
        let codex = kind("codex").unwrap();
        assert!(codex.login.contains(&"auth.json"));
        assert!(
            codex
                .items
                .iter()
                .flat_map(|i| i.paths)
                .all(|p| !p.ends_with(".sqlite"))
        );
        assert!(kind("gemini").unwrap().item("settings").is_none());
        assert!(kind("crush").unwrap().items.is_empty());
        assert!(kind("opencode").is_none());
    }

    #[test]
    fn own_folder_follows_the_variable() {
        let home = Path::new("/home/u");
        let claude = kind("claude").unwrap();
        assert_eq!(
            claude.own_folder(Some(OsString::from("/x")), Some(home)),
            Some(PathBuf::from("/x"))
        );
        assert_eq!(
            claude.own_folder(Some(OsString::new()), Some(home)),
            Some(home.join(".claude"))
        );
        assert_eq!(
            claude.own_folder(None, Some(home)),
            Some(home.join(".claude"))
        );
        assert_eq!(claude.own_folder(None, None), None);
        let gemini = kind("gemini").unwrap();
        assert_eq!(
            gemini.own_folder(None, Some(home)),
            Some(home.to_path_buf())
        );
    }

    #[test]
    fn state_dir_is_the_folder_or_its_subfolder() {
        let folder = Path::new("/a");
        assert_eq!(kind("claude").unwrap().state_dir(folder), folder);
        assert_eq!(
            kind("gemini").unwrap().state_dir(folder),
            folder.join(".gemini")
        );
    }
}
