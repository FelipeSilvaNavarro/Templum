//! The AI coding agents Templum knows: how to start each one in a folder, how to install it, how
//! to tell from a process that it runs in a terminal, and what brings it back when a canvas is
//! restored. Facts checked on 2026-10-10 against each agent's docs (see docs/specs/agents.md).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// One agent's facts.
#[derive(Debug, PartialEq, Eq)]
pub struct Spec {
    /// As the saved canvas spells it; never changes.
    pub id: &'static str,
    pub title: &'static str,
    /// Typed into the shell to start it in the shell's folder.
    pub command: &'static str,
    /// Resumes the folder's latest session. Only for agents whose resume is limited to the
    /// folder: any other would bring back another project's conversation.
    pub resume: Option<&'static str>,
    /// Resumes one session, by its id (`{id}`): each block its own, when several run in a
    /// folder.
    pub resume_id: Option<&'static str>,
    /// Added to its command, starts a new session with an id Templum picks (`{id}`): the block
    /// then knows its session from the process's arguments.
    pub new_id: Option<&'static str>,
    /// Starts a new session that begins with the conversation of session `{id}`, in the shell's
    /// folder, whose own new id its files name (docs/specs/agent-tools.md, spawn_agent with fork).
    pub fork_id: Option<&'static str>,
    /// How to install it, for someone who hasn't.
    pub install: &'static str,
    /// Its process or script names (the program, a renamed process title, a native child).
    pub names: &'static [&'static str],
    /// Pieces of its npm package's path, in a `node` process's script argument.
    pub packages: &'static [&'static str],
}

const SPECS: [Spec; 20] = [
    Spec {
        id: "claude",
        title: "Claude Code",
        command: "claude",
        resume: Some("claude --continue"),
        resume_id: Some("claude --resume {id}"),
        new_id: None,
        fork_id: Some("claude --resume {id} --fork-session"),
        install: "curl -fsSL https://claude.ai/install.sh | bash",
        names: &["claude"],
        packages: &["@anthropic-ai/claude-code"],
    },
    Spec {
        id: "codex",
        title: "Codex",
        command: "codex",
        resume: Some("codex resume --last"),
        resume_id: Some("codex resume {id}"),
        new_id: None,
        fork_id: Some("codex fork -C . {id}"),
        install: "npm install -g @openai/codex",
        names: &["codex"],
        packages: &["@openai/codex"],
    },
    Spec {
        id: "gemini",
        title: "Gemini CLI",
        command: "gemini",
        resume: Some("gemini --resume"),
        resume_id: Some("gemini --resume {id}"),
        new_id: Some("--session-id {id}"),
        fork_id: None,
        install: "npm install -g @google/gemini-cli",
        names: &["gemini"],
        packages: &["@google/gemini-cli"],
    },
    Spec {
        id: "copilot",
        title: "GitHub Copilot CLI",
        command: "copilot",
        resume: None,
        resume_id: Some("copilot --session-id {id}"),
        new_id: Some("--session-id {id}"),
        fork_id: None,
        install: "curl -fsSL https://gh.io/copilot-install | bash",
        names: &["copilot"],
        packages: &["@github/copilot"],
    },
    Spec {
        id: "cursor",
        title: "Cursor CLI",
        command: "cursor-agent",
        resume: Some("cursor-agent --continue"),
        resume_id: None,
        new_id: None,
        fork_id: None,
        install: "curl https://cursor.com/install -fsS | bash",
        names: &["cursor-agent"],
        packages: &["cursor-agent/"],
    },
    Spec {
        id: "opencode",
        title: "OpenCode",
        command: "opencode",
        resume: Some("opencode --continue"),
        resume_id: None,
        new_id: None,
        fork_id: None,
        install: "curl -fsSL https://opencode.ai/install | bash",
        names: &["opencode"],
        packages: &["node_modules/opencode-ai/"],
    },
    Spec {
        id: "amp",
        title: "Amp",
        command: "amp",
        resume: Some("amp threads continue --last"),
        resume_id: None,
        new_id: None,
        fork_id: None,
        install: "curl -fsSL https://ampcode.com/install.sh | bash",
        names: &["amp"],
        packages: &["@sourcegraph/amp"],
    },
    Spec {
        id: "kiro",
        title: "Kiro CLI",
        command: "kiro-cli chat",
        resume: Some("kiro-cli chat --resume"),
        resume_id: None,
        new_id: None,
        fork_id: None,
        install: "curl -fsSL https://cli.kiro.dev/install | bash",
        names: &["kiro-cli"],
        packages: &[],
    },
    Spec {
        id: "qwen",
        title: "Qwen Code",
        command: "qwen",
        resume: Some("qwen --continue"),
        resume_id: Some("qwen --resume {id}"),
        new_id: None,
        fork_id: None,
        install: "npm install -g @qwen-code/qwen-code@latest",
        names: &["qwen"],
        packages: &["@qwen-code/qwen-code"],
    },
    Spec {
        id: "grok",
        title: "Grok Build",
        command: "grok",
        resume: Some("grok --continue"),
        resume_id: Some("grok --resume {id}"),
        new_id: Some("--session-id {id}"),
        fork_id: None,
        install: "curl -fsSL https://x.ai/cli/install.sh | bash",
        names: &["grok"],
        packages: &["@xai-official/grok"],
    },
    Spec {
        id: "kimi",
        title: "Kimi Code",
        command: "kimi",
        resume: Some("kimi --continue"),
        resume_id: None,
        new_id: None,
        fork_id: None,
        install: "curl -fsSL https://code.kimi.com/kimi-code/install.sh | bash",
        names: &["kimi", "kimi-code"],
        packages: &["@moonshot-ai/kimi-code"],
    },
    Spec {
        id: "vibe",
        title: "Mistral Vibe",
        command: "vibe",
        resume: Some("vibe --continue"),
        resume_id: None,
        new_id: None,
        fork_id: None,
        install: "curl -LsSf https://mistral.ai/vibe/install.sh | bash",
        names: &["vibe"],
        packages: &[],
    },
    Spec {
        id: "aider",
        title: "Aider",
        command: "aider",
        resume: Some("aider --restore-chat-history"),
        resume_id: None,
        new_id: None,
        fork_id: None,
        install: "curl -LsSf https://aider.chat/install.sh | sh",
        names: &["aider"],
        packages: &[],
    },
    Spec {
        id: "goose",
        title: "Goose",
        command: "goose session",
        resume: None,
        resume_id: None,
        new_id: None,
        fork_id: None,
        install: "curl -fsSL https://github.com/aaif-goose/goose/releases/download/stable/download_cli.sh | bash",
        names: &["goose"],
        packages: &[],
    },
    Spec {
        id: "crush",
        title: "Crush",
        command: "crush",
        resume: Some("crush --continue"),
        resume_id: None,
        new_id: None,
        fork_id: None,
        install: "npm install -g @charmland/crush",
        names: &["crush"],
        packages: &["@charmland/crush"],
    },
    Spec {
        id: "droid",
        title: "Droid",
        command: "droid",
        resume: None,
        resume_id: None,
        new_id: None,
        fork_id: None,
        install: "curl -fsSL https://app.factory.ai/cli | sh",
        names: &["droid"],
        packages: &["node_modules/droid/"],
    },
    Spec {
        id: "auggie",
        title: "Auggie",
        command: "auggie",
        resume: None,
        resume_id: None,
        new_id: None,
        fork_id: None,
        install: "npm install -g @augmentcode/auggie",
        names: &["auggie"],
        packages: &["@augmentcode/auggie"],
    },
    Spec {
        id: "kilo",
        title: "Kilo Code",
        command: "kilo",
        resume: Some("kilo --continue"),
        resume_id: None,
        new_id: None,
        fork_id: None,
        install: "npm install -g @kilocode/cli",
        names: &["kilo", "kilocode", ".kilo"],
        packages: &["@kilocode/cli"],
    },
    Spec {
        id: "cline",
        title: "Cline",
        command: "cline",
        resume: None,
        resume_id: None,
        new_id: None,
        fork_id: None,
        install: "npm install -g cline",
        names: &["cline", ".cline"],
        packages: &["node_modules/cline/"],
    },
    Spec {
        id: "pi",
        title: "Pi",
        command: "pi",
        resume: Some("pi --continue"),
        resume_id: Some("pi --session-id {id}"),
        new_id: Some("--session-id {id}"),
        fork_id: None,
        install: "curl -fsSL https://pi.dev/install.sh | sh",
        names: &["pi"],
        packages: &["@earendil-works/pi-coding-agent"],
    },
];

/// How an agent takes a model and a reasoning effort on its command line (`spawn_agent`): the flag
/// before a model, the one before an effort, and the efforts it takes. Checked on 2026-10-16:
/// Claude Code 2.1.286 (`claude --help`), Codex 0.154.0 (`codex --help`, `codex debug models`:
/// a `-c` value that isn't TOML is a string, so no quotes), Gemini CLI 0.63.0 (`gemini --help`).
/// The others on 2026-10-18: the `--help` of Copilot CLI 1.0.95, Cursor CLI, OpenCode 1.19.0,
/// Grok Build and Pi 0.85.1, installed then; the references of Kiro CLI, Qwen Code, Kimi Code,
/// Aider, Droid, Auggie, Kilo Code and Cline (docs/specs/agents.md, A model and an effort).
struct Launch {
    id: &'static str,
    model: &'static str,
    effort: Option<&'static str>,
    efforts: &'static [&'static str],
}

const LAUNCH: [Launch; 16] = [
    Launch {
        id: "claude",
        model: "--model ",
        effort: Some("--effort "),
        efforts: &["low", "medium", "high", "xhigh", "max"],
    },
    Launch {
        id: "codex",
        model: "-m ",
        effort: Some("-c model_reasoning_effort="),
        efforts: &["low", "medium", "high", "xhigh", "max", "ultra"],
    },
    Launch {
        id: "gemini",
        model: "-m ",
        effort: None,
        efforts: &[],
    },
    Launch {
        id: "copilot",
        model: "--model ",
        effort: Some("--reasoning-effort "),
        efforts: &["none", "minimal", "low", "medium", "high", "xhigh", "max"],
    },
    Launch {
        id: "cursor",
        model: "--model ",
        effort: None,
        efforts: &[],
    },
    Launch {
        id: "opencode",
        model: "--model ",
        effort: None,
        efforts: &[],
    },
    Launch {
        id: "kiro",
        model: "--model ",
        effort: Some("--effort "),
        efforts: &["low", "medium", "high", "xhigh", "max"],
    },
    Launch {
        id: "qwen",
        model: "--model ",
        effort: None,
        efforts: &[],
    },
    Launch {
        id: "grok",
        model: "--model ",
        effort: Some("--reasoning-effort "),
        efforts: &["none", "minimal", "low", "medium", "high", "xhigh", "max"],
    },
    Launch {
        id: "kimi",
        model: "--model ",
        effort: None,
        efforts: &[],
    },
    Launch {
        id: "aider",
        model: "--model ",
        effort: Some("--reasoning-effort "),
        efforts: &["low", "medium", "high"],
    },
    Launch {
        id: "droid",
        model: "--model ",
        effort: None,
        efforts: &[],
    },
    Launch {
        id: "auggie",
        model: "--model ",
        effort: None,
        efforts: &[],
    },
    Launch {
        id: "kilo",
        model: "--model ",
        effort: None,
        efforts: &[],
    },
    Launch {
        id: "cline",
        model: "--model ",
        effort: Some("--thinking "),
        efforts: &["none", "low", "medium", "high", "xhigh"],
    },
    Launch {
        id: "pi",
        model: "--model ",
        effort: Some("--thinking "),
        efforts: &["off", "minimal", "low", "medium", "high", "xhigh", "max"],
    },
];

/// How to tell an agent from its processes: one of Templum's, or one the user set up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detect {
    /// What the saved canvas records.
    pub id: String,
    /// Process or script names.
    pub names: Vec<String>,
    /// Pieces of an npm package's path, in a `node` process's script argument.
    pub packages: Vec<String>,
}

/// An agent Templum knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Agent(usize);

impl Agent {
    pub const CLAUDE: Agent = Agent(0);
    pub const CODEX: Agent = Agent(1);
    pub const GEMINI: Agent = Agent(2);
    pub const COPILOT: Agent = Agent(3);
    pub const OPENCODE: Agent = Agent(5);
    pub const QWEN: Agent = Agent(8);
    pub const KILO: Agent = Agent(17);
    pub const PI: Agent = Agent(19);

    /// Every agent, in the order menus list them.
    pub fn all() -> impl Iterator<Item = Agent> {
        (0..SPECS.len()).map(Agent)
    }

    pub fn spec(self) -> &'static Spec {
        &SPECS[self.0]
    }

    /// As the saved canvas spells it.
    pub fn name(self) -> &'static str {
        self.spec().id
    }

    pub fn from_name(name: &str) -> Option<Agent> {
        Agent::all().find(|a| a.spec().id == name)
    }

    pub fn title(self) -> &'static str {
        self.spec().title
    }

    pub fn command(self) -> &'static str {
        self.spec().command
    }

    /// The program the command runs, to tell whether it is installed.
    pub fn program(self) -> &'static str {
        self.command().split_whitespace().next().unwrap_or_default()
    }

    pub fn install(self) -> &'static str {
        self.spec().install
    }

    /// What brings it back in its folder: its resume command when that is limited to the
    /// folder, else a new session.
    pub fn resume_command(self) -> &'static str {
        self.spec().resume.unwrap_or(self.command())
    }

    /// What brings back its session `id` (as the agent's files name it), when it can resume one
    /// by id and `id` is one.
    pub fn resume_session(self, id: &str) -> Option<String> {
        let template = self.spec().resume_id?;
        typable(id).then(|| template.replace("{id}", id))
    }

    /// What starts a fork of its session `id`, when it can fork one and `id` is one.
    pub fn fork_session(self, id: &str) -> Option<String> {
        let template = self.spec().fork_id?;
        typable(id).then(|| template.replace("{id}", id))
    }

    /// What its command takes to start a new session with the id `id`, when it can.
    pub fn new_session(self, id: &str) -> Option<String> {
        let template = self.spec().new_id?;
        typable(id).then(|| template.replace("{id}", id))
    }

    /// The id of the session a process of it was started on, from its arguments `args`: a UUID
    /// given to its new-session or its resume flag.
    pub fn session_in(self, args: &[&str]) -> Option<String> {
        let spec = self.spec();
        // A fork's (`--fork-session`) name the session it started from, not its own.
        let fork_flag = spec
            .fork_id
            .and_then(|f| f.split_whitespace().last())
            .filter(|w| *w != "{id}");
        if fork_flag.is_some_and(|flag| args.contains(&flag)) {
            return None;
        }
        // The word before `{id}`: `--session-id`, `--resume`, Codex's `resume`.
        let flags: Vec<&str> = [spec.new_id, spec.resume_id]
            .into_iter()
            .flatten()
            .filter_map(|t| t.split_whitespace().take_while(|w| *w != "{id}").last())
            .collect();
        args.iter().enumerate().find_map(|(i, arg)| {
            let value = flags
                .iter()
                .find_map(|flag| match arg.strip_prefix(flag)? {
                    "" => args.get(i + 1).copied(),
                    rest => rest.strip_prefix('='),
                })?;
            uuid(value).then(|| value.to_string())
        })
    }

    /// The words that start it on `model` and `effort`, to append to its command, each after a
    /// space (`" --model opus --effort high"`); empty for neither. Err: why it can't, for the
    /// agent that asked. They are typed into a shell: a model is letters, digits and `.-_:/`.
    pub fn launch_args(self, model: Option<&str>, effort: Option<&str>) -> Result<String, String> {
        if model.is_none() && effort.is_none() {
            return Ok(String::new());
        }
        let title = self.title();
        let Some(launch) = LAUNCH.iter().find(|l| l.id == self.name()) else {
            return Err(format!(
                "Templum doesn't give {title} a model or an effort."
            ));
        };
        let mut args = String::new();
        if let Some(model) = model {
            if !model_typable(model) {
                return Err(format!(
                    "{model:?} isn't a model Templum types: a model is 1 to 64 letters, digits, ., -, \
                     _, : and /, beginning with a letter or a digit."
                ));
            }
            args += &format!(" {}{model}", launch.model);
        }
        if let Some(effort) = effort {
            let Some(flag) = launch.effort else {
                return Err(format!("{title} takes no effort from Templum."));
            };
            if !launch.efforts.contains(&effort) {
                let (last, rest) = launch.efforts.split_last().expect("an agent's efforts");
                return Err(format!(
                    "{title}'s effort is {} or {last}.",
                    rest.join(", ")
                ));
            }
            args += &format!(" {flag}{effort}");
        }
        Ok(args)
    }

    /// The efforts its command line takes from Templum, lowest first; none when it takes none.
    pub fn efforts(self) -> &'static [&'static str] {
        LAUNCH
            .iter()
            .find(|l| l.id == self.name())
            .map_or(&[], |l| l.efforts)
    }

    /// How to tell it from its processes.
    pub fn detect(self) -> Detect {
        let owned = |list: &[&str]| list.iter().map(|s| s.to_string()).collect();
        Detect {
            id: self.name().to_string(),
            names: owned(self.spec().names),
            packages: owned(self.spec().packages),
        }
    }
}

/// An id typed into a shell: letters, digits and `-` only.
fn typable(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// A model typed into a shell: no option (a leading `-`) nor shell syntax can hide in it.
fn model_typable(model: &str) -> bool {
    (1..=64).contains(&model.len())
        && model.starts_with(|c: char| c.is_ascii_alphanumeric())
        && model
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".-_:/".contains(c))
}

/// `8-4-4-4-12` hex digits: not Gemini's `--resume latest` nor an index into its list.
fn uuid(id: &str) -> bool {
    id.len() == 36
        && id.char_indices().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        })
}

/// The program a command runs, as process names compare: `"~/bin/My-Agent --tui"` is `my-agent`.
pub fn program_name(command: &str) -> String {
    stem(command.split_whitespace().next().unwrap_or_default())
}

/// The agent among `known` a process is, by its id, from its name and arguments (`args[0]` is
/// the program). An agent installed with npm runs as `node` with its script first: the package's
/// file, or the bin link the shell found. One installed with pip or uv runs as its script, by
/// name.
pub fn agent_of(name: &str, args: &[&str], known: &[Detect]) -> Option<String> {
    agent_index(name, args, known).map(|i| known[i].id.clone())
}

fn agent_index(name: &str, args: &[&str], known: &[Detect]) -> Option<usize> {
    let program = args.first().map_or_else(String::new, |a| stem(a));
    let by_name = |n: &str| known.iter().position(|a| a.names.iter().any(|x| x == n));
    // The name, or the first argument: on macOS an npm agent's `process.title` replaces it.
    if let Some(agent) = by_name(&stem(name)).or_else(|| by_name(&program)) {
        return Some(agent);
    }
    // Node 26 on Linux names the process after its main thread; Cursor's bundled Node keeps its
    // own name, so its script is what tells.
    let interpreter = |n: &str| matches!(n, "node" | "node-mainthread" | "bun");
    let script = args.iter().skip(1).find(|a| !a.starts_with('-'))?;
    let script = script.replace('\\', "/");
    if let Some(agent) = known
        .iter()
        .position(|a| a.packages.iter().any(|p| script.contains(p.as_str())))
    {
        return Some(agent);
    }
    if !(interpreter(&stem(name)) || interpreter(&program)) {
        return None;
    }
    // An npm bin link (`…/bin/claude`); a script of the user's has an extension.
    Path::new(&script)
        .extension()
        .is_none()
        .then(|| by_name(&stem(&script)))
        .flatten()
}

/// Whether `program` can be run: found in PATH, or in the folders npm, bun, cargo and the like
/// install into (an app started from a desktop menu may not get the shell's PATH).
pub fn installed(program: &str) -> bool {
    locate(program).is_some()
}

/// Where `program` is, as [`installed`] looks for it.
pub fn locate(program: &str) -> Option<PathBuf> {
    find_program(program, &program_dirs())
}

/// PATH as a shell started now gets it. On Windows an installer writes it to the registry, and
/// a program started before (Templum, or the Explorer that started it) keeps the old one: it is
/// read from there, the system's then the user's, as Windows Terminal does, then the folders only
/// Templum's own PATH has. Elsewhere Templum's own.
#[cfg(windows)]
pub fn path_var() -> Option<OsString> {
    use windows_registry::{CURRENT_USER, Key, LOCAL_MACHINE};
    let read = |key: &Key, path: &str| key.open(path).and_then(|k| k.get_string("Path")).ok();
    let values: Vec<String> = [
        read(
            LOCAL_MACHINE,
            r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
        ),
        read(CURRENT_USER, "Environment"),
    ]
    .into_iter()
    .flatten()
    .collect();
    let own = std::env::var_os("PATH");
    let own: Vec<PathBuf> = own.iter().flat_map(std::env::split_paths).collect();
    let dirs = registry_path(&values, &own, |name| std::env::var(name).ok());
    std::env::join_paths(dirs)
        .ok()
        .or_else(|| std::env::var_os("PATH"))
}

/// PATH as a shell started now gets it: Templum's own (see the Windows one).
#[cfg(not(windows))]
pub fn path_var() -> Option<OsString> {
    std::env::var_os("PATH")
}

/// The folders of the registry's PATH values (`;`-separated, each `%NAME%` that `var` knows
/// expanded), then those of `own` they lack: once each, in any case, with or without a final `\`.
#[cfg(any(windows, test))]
fn registry_path(
    values: &[String],
    own: &[PathBuf],
    var: impl Fn(&str) -> Option<String>,
) -> Vec<PathBuf> {
    let key = |p: &Path| {
        p.to_string_lossy()
            .trim_end_matches(['\\', '/'])
            .to_lowercase()
    };
    let listed = values
        .iter()
        .flat_map(|v| v.split(';'))
        .map(|dir| PathBuf::from(expand(dir.trim(), &var)));
    let mut dirs: Vec<PathBuf> = Vec::new();
    for dir in listed.chain(own.iter().cloned()) {
        if !dir.as_os_str().is_empty() && !dirs.iter().any(|d| key(d) == key(&dir)) {
            dirs.push(dir);
        }
    }
    dirs
}

/// `text` with each `%NAME%` that `var` knows replaced by its value; the others stay, as Windows
/// leaves them.
#[cfg(any(windows, test))]
fn expand(text: &str, var: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('%') else { break };
        out.push_str(&rest[..start]);
        match var(&after[..end]) {
            Some(value) => {
                out.push_str(&value);
                rest = &after[end + 1..];
            }
            None => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The folders of the user's shell's PATH, once [`read_shell_path`] has read them.
#[cfg(unix)]
static SHELL_PATH: std::sync::OnceLock<Vec<PathBuf>> = std::sync::OnceLock::new();

/// Reads, on a thread of its own, the PATH the user's shell sets up, for [`installed`]: an app
/// started from a desktop menu or the Dock gets the session's, without what .bashrc or .zshrc
/// add. Once, when Templum starts.
#[cfg(unix)]
pub fn read_shell_path() {
    SHELL_PATH_READ_STARTED.store(true, std::sync::atomic::Ordering::SeqCst);
    std::thread::spawn(|| {
        let path = std::env::var("SHELL").ok().and_then(|s| shell_path(&s));
        let _ = SHELL_PATH.set(path.iter().flat_map(std::env::split_paths).collect());
    });
}

/// Set when [`read_shell_path`] starts: what [`shell_path_pending`] goes by, so nothing counts as
/// pending for a read that was never started (the tests, `--bench`).
#[cfg(unix)]
static SHELL_PATH_READ_STARTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Whether the user's shell's PATH is still being read: what [`locate`] finds meanwhile may lack
/// programs only that PATH has (Homebrew's, from the Dock).
#[cfg(unix)]
pub fn shell_path_pending() -> bool {
    SHELL_PATH_READ_STARTED.load(std::sync::atomic::Ordering::SeqCst) && SHELL_PATH.get().is_none()
}

#[cfg(not(unix))]
pub fn shell_path_pending() -> bool {
    false
}

/// Printed before the PATH, to find it among what the shell's startup files print.
#[cfg(unix)]
const PATH_MARK: &str = "__TEMPLUM_PATH__";

/// The PATH of `shell` started as a terminal starts it (login, interactive). None when it
/// fails or takes over 5 seconds (a startup file waiting on a terminal).
#[cfg(unix)]
fn shell_path(shell: &str) -> Option<String> {
    use std::io::BufRead as _;
    use std::process::{Command, Stdio};
    let mut child = Command::new(shell)
        .args(["-l", "-i", "-c"])
        .arg(format!("printf '\\n{PATH_MARK}%s\\n' \"$PATH\""))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let (tx, rx) = std::sync::mpsc::channel();
    // Up to the mark, not to the end: a program the startup files leave running (an agent, a
    // daemon) keeps the output open.
    std::thread::spawn(move || {
        let path = std::io::BufReader::new(stdout)
            .lines()
            .map_while(Result::ok)
            .find_map(|line| line.strip_prefix(PATH_MARK).map(str::to_string));
        let _ = tx.send(path);
    });
    let path = rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .ok()
        .flatten();
    let _ = child.kill();
    let _ = child.wait();
    path
}

/// PATH (as [`path_var`] reads it), the shell's, then the usual per-user install folders.
fn program_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = path_var()
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    #[cfg(unix)]
    dirs.extend(SHELL_PATH.get().into_iter().flatten().cloned());
    if let Some(home) = std::env::home_dir() {
        for sub in [
            ".local/bin",
            ".npm-global/bin",
            ".bun/bin",
            ".cargo/bin",
            ".nix-profile/bin",
            ".deno/bin",
            ".volta/bin",
            "go/bin",
            "AppData/Roaming/npm",
            // Version managers that put shims on the shell's PATH only (mise, asdf).
            ".local/share/mise/shims",
            ".asdf/shims",
        ] {
            dirs.push(home.join(sub));
        }
        // nvm: every installed Node's bin folder.
        // ponytail: one read_dir; fnm and others add theirs when someone needs them.
        if let Ok(versions) = std::fs::read_dir(home.join(".nvm/versions/node")) {
            dirs.extend(versions.flatten().map(|v| v.path().join("bin")));
        }
    }
    // What a desktop launch's PATH lacks: Homebrew's (Apple Silicon, Linuxbrew, Intel), Nix's,
    // MacPorts'.
    dirs.extend(
        [
            "/opt/homebrew/bin",
            "/home/linuxbrew/.linuxbrew/bin",
            "/nix/var/nix/profiles/default/bin",
            "/opt/local/bin",
            "/usr/local/bin",
        ]
        .map(PathBuf::from),
    );
    dirs
}

/// The first `program` among `dirs` that can be run (on Windows, with a program extension).
pub(crate) fn find_program(program: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    let names: Vec<String> = if cfg!(windows) {
        ["exe", "cmd", "bat", "ps1"]
            .iter()
            .map(|ext| format!("{program}.{ext}"))
            .collect()
    } else {
        vec![program.to_string()]
    };
    dirs.iter()
        .flat_map(|dir| names.iter().map(move |n| dir.join(n)))
        .find(|path| runnable(path))
}

#[cfg(unix)]
fn runnable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn runnable(path: &Path) -> bool {
    path.is_file()
}

/// A program's lowercase file name without its extension (`.exe`), from either kind of slash.
fn stem(path: &str) -> String {
    Path::new(&path.replace('\\', "/"))
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(id: &str) -> Option<String> {
        Agent::from_name(id).map(|a| a.name().to_string())
    }

    fn builtins() -> Vec<Detect> {
        Agent::all().map(Agent::detect).collect()
    }

    /// A shell or an agent from a package manager is found when Templum starts from the Dock or a
    /// desktop menu, without waiting for the login shell's PATH.
    #[test]
    fn programs_are_looked_up_in_the_package_managers_folders() {
        let dirs = program_dirs();
        for dir in [
            "/opt/homebrew/bin",
            "/home/linuxbrew/.linuxbrew/bin",
            "/nix/var/nix/profiles/default/bin",
            "/opt/local/bin",
            "/usr/local/bin",
        ] {
            assert!(dirs.contains(&PathBuf::from(dir)), "{dir}");
        }
        if let Some(home) = std::env::home_dir() {
            assert!(dirs.contains(&home.join(".nix-profile/bin")));
        }
    }

    #[test]
    fn agents_are_known_by_name_or_by_the_script_node_runs() {
        let cases: &[(&str, &[&str], Option<String>)] = &[
            ("claude", &["claude"], named("claude")),
            (
                "claude.exe",
                &["C:\\Users\\u\\.local\\bin\\claude.exe"],
                named("claude"),
            ),
            ("codex", &["codex", "resume"], named("codex")),
            ("codex.exe", &["codex.exe"], named("codex")),
            (
                "node",
                &[
                    "node",
                    "/usr/lib/node_modules/@anthropic-ai/claude-code/cli.js",
                ],
                named("claude"),
            ),
            (
                "node.exe",
                &[
                    "node.exe",
                    "C:\\npm\\node_modules\\@openai\\codex\\bin\\codex.js",
                ],
                named("codex"),
            ),
            (
                "node",
                &["node", "/home/u/.npm-global/bin/claude"],
                named("claude"),
            ),
            (
                "node",
                &["node", "--no-warnings", "/usr/bin/codex"],
                named("codex"),
            ),
            // Node 26 on Linux: the main thread names the process.
            (
                "node-MainThread",
                &["node", "/x/node_modules/@openai/codex/bin/codex.js"],
                named("codex"),
            ),
            (
                "node-MainThread",
                &["node", "/usr/bin/claude"],
                named("claude"),
            ),
            // macOS: `process.title = "claude"` overwrites the arguments.
            ("node", &["claude", "TERM=xterm-256color"], named("claude")),
            // The agents added on 2026-10-10, as their processes look.
            (
                "node",
                &[
                    "node",
                    "/usr/lib/node_modules/@google/gemini-cli/bundle/gemini.js",
                ],
                named("gemini"),
            ),
            ("node", &["node", "/usr/local/bin/gemini"], named("gemini")),
            ("copilot", &["copilot"], named("copilot")),
            (
                "node",
                &["node", "/x/node_modules/@github/copilot/npm-loader.js"],
                named("copilot"),
            ),
            // Cursor's bundled Node: argv[0] is `…/agent`, a name too common to trust.
            (
                "node",
                &[
                    "/home/u/.local/bin/agent",
                    "/home/u/.local/share/cursor-agent/versions/2026.10.01/index.js",
                ],
                named("cursor"),
            ),
            ("node", &["/home/u/.local/bin/agent", "x.js"], None),
            ("opencode", &["opencode"], named("opencode")),
            ("amp", &["amp", "threads", "continue"], named("amp")),
            ("kiro-cli", &["kiro-cli", "chat"], named("kiro")),
            (
                "node",
                &["node", "/x/@qwen-code/qwen-code/cli-entry.js"],
                named("qwen"),
            ),
            ("grok", &["grok"], named("grok")),
            // Kimi's npm build renames its process.
            ("node", &["kimi-code"], named("kimi")),
            // A uv or pip script: its own name, run by Python.
            (
                "vibe",
                &["/usr/bin/python3", "/home/u/.local/bin/vibe"],
                named("vibe"),
            ),
            (
                "aider",
                &["python3", "/home/u/.local/bin/aider"],
                named("aider"),
            ),
            ("goose", &["goose", "session"], named("goose")),
            ("crush", &["crush"], named("crush")),
            ("droid", &["droid"], named("droid")),
            (
                "node",
                &["node", "/x/node_modules/@augmentcode/auggie/augment.mjs"],
                named("auggie"),
            ),
            // Kilo and Cline: a node parent, and a native child named with a dot.
            (".kilo", &["/x/.kilo"], named("kilo")),
            ("node", &["node", "/usr/bin/cline"], named("cline")),
            ("node", &["pi"], named("pi")),
            // The user's own scripts and programs.
            ("node", &["node", "claude.js"], None),
            ("node-MainThread", &["node", "tools/codex.mjs"], None),
            ("node", &["node", "server.js", "codex"], None),
            ("node", &["node"], None),
            ("python3", &["python3", "manage.py", "runserver"], None),
            ("vim", &["vim", "claude.md"], None),
            ("zsh", &["-zsh"], None),
        ];
        let known = builtins();
        for (name, args, want) in cases {
            assert_eq!(agent_of(name, args, &known), *want, "{name} {args:?}");
        }
    }

    #[test]
    fn an_agent_the_user_set_up_is_known_by_its_program() {
        let mut known = builtins();
        known.push(Detect {
            id: "mine".into(),
            names: vec![program_name("~/bin/My-Agent --tui")],
            packages: Vec::new(),
        });
        assert_eq!(
            agent_of("my-agent", &["my-agent"], &known),
            Some("mine".into())
        );
        assert_eq!(
            agent_of("node", &["node", "/usr/bin/my-agent"], &known),
            Some("mine".into())
        );
        assert_eq!(agent_of("claude", &["claude"], &known), named("claude"));
        assert_eq!(
            agent_of("my-agent", &["my-agent"], &builtins()),
            None,
            "unknown elsewhere"
        );
    }

    #[test]
    fn every_agent_has_its_facts_and_a_unique_name() {
        let mut ids: Vec<&str> = Agent::all().map(Agent::name).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), SPECS.len(), "ids are unique");
        for (agent, id) in [
            (Agent::CLAUDE, "claude"),
            (Agent::CODEX, "codex"),
            (Agent::GEMINI, "gemini"),
            (Agent::COPILOT, "copilot"),
            (Agent::OPENCODE, "opencode"),
            (Agent::QWEN, "qwen"),
            (Agent::KILO, "kilo"),
            (Agent::PI, "pi"),
        ] {
            assert_eq!(agent.name(), id);
        }
        for agent in Agent::all() {
            let spec = agent.spec();
            assert_eq!(Agent::from_name(spec.id), Some(agent));
            assert!(
                !spec.title.is_empty() && !spec.install.is_empty(),
                "{spec:?}"
            );
            assert!(
                spec.names.contains(&agent.program()),
                "{}: its program is one of its names",
                spec.id
            );
            assert!(
                agent.resume_command().starts_with(agent.program()),
                "{spec:?}"
            );
        }
        assert_eq!(Agent::from_name("rm -rf ~"), None);
    }

    /// Claude Code and Codex fork a session into a new one; the others can't (docs/specs/
    /// agent-tools.md, Every agent).
    #[test]
    fn only_claude_code_and_codex_fork_a_session() {
        let id = "0d5e5f4b-1c2d-4e3f-8a9b-0c1d2e3f4a5b";
        assert_eq!(
            Agent::CLAUDE.fork_session(id),
            Some(format!("claude --resume {id} --fork-session"))
        );
        assert_eq!(
            Agent::CODEX.fork_session(id),
            Some(format!("codex fork -C . {id}"))
        );
        for other in ["gemini", "copilot", "opencode", "pi", "grok", "qwen"] {
            assert_eq!(
                Agent::from_name(other).unwrap().fork_session(id),
                None,
                "{other}"
            );
        }
        assert_eq!(Agent::CLAUDE.fork_session("a;b"), None);
        // A fork's arguments name the session it started from, not its own.
        let args = ["claude", "--resume", id, "--fork-session"];
        assert_eq!(Agent::CLAUDE.session_in(&args), None);
        assert_eq!(Agent::CLAUDE.session_in(&args[..3]), Some(id.to_string()));
    }

    #[test]
    fn only_a_resume_limited_to_the_folder_is_used() {
        assert_eq!(Agent::CLAUDE.resume_command(), "claude --continue");
        assert_eq!(Agent::CODEX.resume_command(), "codex resume --last");
        assert_eq!(
            Agent::CLAUDE.resume_session("0d5e-a1").as_deref(),
            Some("claude --resume 0d5e-a1")
        );
        assert_eq!(
            Agent::CODEX.resume_session("0d5e-a1").as_deref(),
            Some("codex resume 0d5e-a1")
        );
        let by = |id| Agent::from_name(id).unwrap();
        assert_eq!(
            by("pi").new_session("0d5e-a1").as_deref(),
            Some("--session-id 0d5e-a1")
        );
        assert_eq!(Agent::CLAUDE.new_session("0d5e-a1"), None);
        // An id is typed into a shell: anything but letters, digits and `-` isn't one.
        assert_eq!(Agent::CLAUDE.resume_session("x; rm -rf ~"), None);
        assert_eq!(Agent::CLAUDE.resume_session(""), None);
        // Copilot's `--continue` may reopen another folder's session: a new one, or its own by id.
        assert_eq!(by("copilot").resume_command(), "copilot");
        let uuid = "019a1f2e-7b3c-7d4e-8f5a-6b7c8d9e0f1a";
        assert_eq!(
            by("copilot").resume_session(uuid),
            Some(format!("copilot --session-id {uuid}"))
        );
        assert_eq!(by("cursor").resume_command(), "cursor-agent --continue");
        assert_eq!(by("crush").resume_command(), "crush --continue");
        assert_eq!(by("goose").resume_command(), "goose session");
    }

    #[test]
    fn a_model_and_an_effort_become_each_agents_flags() {
        let agent = |id| Agent::from_name(id).unwrap();
        let (claude, codex, gemini) = (agent("claude"), agent("codex"), agent("gemini"));
        let ok = |s: &str| -> Result<String, String> { Ok(s.to_string()) };
        assert_eq!(
            claude.launch_args(Some("opus"), Some("high")),
            ok(" --model opus --effort high")
        );
        assert_eq!(
            codex.launch_args(Some("gpt-5.6-luna"), Some("ultra")),
            ok(" -m gpt-5.6-luna -c model_reasoning_effort=ultra")
        );
        assert_eq!(
            gemini.launch_args(Some("gemini-2.5-pro"), None),
            ok(" -m gemini-2.5-pro")
        );
        assert_eq!(claude.launch_args(None, Some("max")), ok(" --effort max"));
        assert_eq!(
            codex.launch_args(Some("openrouter/qwen-3:free"), None),
            ok(" -m openrouter/qwen-3:free")
        );
        assert_eq!(agent("copilot").launch_args(None, None), ok(""));
        let longest = "a".repeat(64);
        assert!(claude.launch_args(Some(&longest), None).is_ok());
        let too_long = "a".repeat(65);
        for bad in [
            "opus;rm -rf ~",
            "$(x)",
            "a b",
            "'",
            "-rf",
            "",
            "opus[1m]",
            "~/m",
            too_long.as_str(),
        ] {
            let err = claude.launch_args(Some(bad), None).unwrap_err();
            assert!(err.contains("letters, digits"), "{bad:?}: {err}");
        }
        let err = claude.launch_args(None, Some("ultra")).unwrap_err();
        assert!(
            err.contains("low, medium, high, xhigh or max"),
            "Codex's, not Claude Code's: {err}"
        );
        assert!(codex.launch_args(None, Some("HIGH")).is_err());
        let err = gemini.launch_args(None, Some("high")).unwrap_err();
        assert!(err.contains("no effort"), "{err}");
        assert_eq!(
            agent("copilot").launch_args(Some("gpt-5.4"), Some("none")),
            ok(" --model gpt-5.4 --reasoning-effort none")
        );
        assert_eq!(
            agent("grok").launch_args(Some("grok-5"), Some("max")),
            ok(" --model grok-5 --reasoning-effort max")
        );
        assert_eq!(
            agent("pi").launch_args(Some("anthropic/claude-opus-5-5"), Some("off")),
            ok(" --model anthropic/claude-opus-5-5 --thinking off")
        );
        assert_eq!(
            agent("kiro").launch_args(Some("claude-sonnet-5-5"), Some("xhigh")),
            ok(" --model claude-sonnet-5-5 --effort xhigh")
        );
        assert_eq!(
            agent("cline").launch_args(Some("gpt-5.4"), Some("none")),
            ok(" --model gpt-5.4 --thinking none")
        );
        assert_eq!(
            agent("aider").launch_args(Some("o4"), Some("medium")),
            ok(" --model o4 --reasoning-effort medium")
        );
        for id in [
            "opencode", "kilo", "cursor", "qwen", "kimi", "droid", "auggie",
        ] {
            assert_eq!(
                agent(id).launch_args(Some("p/m-1"), None),
                ok(" --model p/m-1"),
                "{id}"
            );
        }
        assert_eq!(
            agent("opencode")
                .launch_args(None, Some("high"))
                .unwrap_err(),
            "OpenCode takes no effort from Templum."
        );
        assert_eq!(
            agent("aider").launch_args(None, Some("xhigh")).unwrap_err(),
            "Aider's effort is low, medium or high."
        );
        assert_eq!(
            agent("amp").launch_args(Some("x"), None).unwrap_err(),
            "Templum doesn't give Amp a model or an effort."
        );
        assert!(
            agent("crush").launch_args(Some("x"), None).is_err()
                && agent("goose").launch_args(Some("x"), None).is_err()
        );
    }

    /// Each installed agent's `--help` still names the flags `LAUNCH` types (run by hand after
    /// changing an agent's words: Copilot's and OpenCode's mise shims rewrite mise's global
    /// config at every run).
    #[test]
    #[ignore = "runs each installed agent's --help"]
    fn installed_agents_take_the_flags_templum_types() {
        let dirs = program_dirs();
        for launch in &LAUNCH {
            let agent = Agent::from_name(launch.id).unwrap();
            let Some(program) = find_program(agent.program(), &dirs) else {
                println!("skipped: {}", agent.program());
                continue;
            };
            let mut command = std::process::Command::new(&program);
            command
                .args(agent.command().split_whitespace().skip(1))
                .arg("--help")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());
            let mut child = command.spawn().expect("run the agent");
            let started = std::time::Instant::now();
            while child.try_wait().unwrap().is_none() {
                assert!(
                    started.elapsed() < std::time::Duration::from_secs(20),
                    "{}: --help took over 20 s",
                    launch.id
                );
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            let out = child.wait_with_output().unwrap();
            let help = String::from_utf8_lossy(&out.stdout) + String::from_utf8_lossy(&out.stderr);
            let word = |flag: &str| {
                flag.split(['=', ' '])
                    .find(|w| !w.is_empty())
                    .unwrap()
                    .to_string()
            };
            let model = word(launch.model);
            assert!(
                help.contains(&model),
                "{}: no {model} in its --help",
                launch.id
            );
            if let Some(effort) = launch.effort.filter(|e| !e.starts_with("-c ")) {
                let effort = word(effort);
                assert!(
                    help.contains(&effort),
                    "{}: no {effort} in its --help",
                    launch.id
                );
            }
            println!("checked: {}", launch.id);
        }
    }

    #[test]
    fn a_session_is_read_from_the_flag_that_started_or_resumed_it() {
        let id = "019a1f2e-7b3c-7d4e-8f5a-6b7c8d9e0f1a";
        let by = |id| Agent::from_name(id).unwrap();
        let cases: &[(&str, &[&str], Option<&str>)] = &[
            (
                "gemini",
                &["node", "/x/gemini.js", "--session-id", id],
                Some(id),
            ),
            (
                "gemini",
                &["node", "/x/gemini.js", "--resume", id],
                Some(id),
            ),
            ("gemini", &["gemini", "--resume", "latest"], None),
            ("gemini", &["gemini", "--resume", "5"], None),
            ("gemini", &["gemini", "--session-id"], None),
            ("grok", &["grok", &format!("--resume={id}")], Some(id)),
            (
                "pi",
                &["node", "/x/pi", "--model", "m", "--session-id", id],
                Some(id),
            ),
            ("pi", &["pi", "--session-id-x", id], None),
            ("codex", &["codex", "resume", id], Some(id)),
            ("codex", &["codex", "resume", "--last"], None),
            ("claude", &["claude", "--resume", id], Some(id)),
            ("claude", &["claude", "--continue"], None),
            ("copilot", &["copilot", "--resume", id], None),
            ("copilot", &["copilot", "--session-id", id], Some(id)),
            (
                "copilot",
                &["copilot", &format!("--session-id={id}")],
                Some(id),
            ),
        ];
        for (agent, args, want) in cases {
            assert_eq!(
                by(agent).session_in(args).as_deref(),
                *want,
                "{agent} {args:?}"
            );
        }
    }

    #[test]
    fn a_program_is_found_only_where_it_can_run() {
        let d = tempfile::tempdir().unwrap();
        let name = if cfg!(windows) {
            "fakeagent.cmd"
        } else {
            "fakeagent"
        };
        let file = d.path().join(name);
        std::fs::write(&file, "").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                find_program("fakeagent", &[d.path().into()]),
                None,
                "not executable"
            );
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let dirs = [d.path().join("missing"), d.path().into()];
        assert_eq!(find_program("fakeagent", &dirs), Some(file));
        assert_eq!(find_program("otheragent", &dirs), None);
    }

    /// A folder an installer just added to the user's PATH in the registry is found, before
    /// Templum's own folders; `%NAME%` is expanded and a folder isn't listed twice.
    #[test]
    fn registry_path_puts_the_registrys_folders_first() {
        let var = |name: &str| (name == "USERPROFILE").then(|| r"C:\Users\Ana".to_string());
        let values = [
            r"C:\Windows\system32;C:\Windows;".to_string(),
            r"%USERPROFILE%\AppData\Local\Microsoft\WindowsApps;%MISSING%\bin;C:\NewTool\bin"
                .to_string(),
        ];
        let own = [r"c:\windows\System32\".into(), r"C:\Launcher\bin".into()];
        let dirs: Vec<String> = registry_path(&values, &own, var)
            .iter()
            .map(|d| d.display().to_string())
            .collect();
        assert_eq!(
            dirs,
            [
                r"C:\Windows\system32",
                r"C:\Windows",
                r"C:\Users\Ana\AppData\Local\Microsoft\WindowsApps",
                r"%MISSING%\bin",
                r"C:\NewTool\bin",
                r"C:\Launcher\bin",
            ]
        );
    }

    /// The PATH the shell's startup files set is read past what they print.
    #[cfg(unix)]
    #[test]
    fn shell_path_reads_past_what_startup_files_print() {
        use std::os::unix::fs::PermissionsExt as _;
        let d = tempfile::tempdir().unwrap();
        let shell = d.path().join("fakesh");
        // Run as `fakesh -l -i -c <command>`, like a login shell reading its .bashrc.
        std::fs::write(
            &shell,
            "#!/bin/sh\necho Welcome back\nPATH=/from/rc:$PATH\nexport PATH\neval \"$4\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = shell_path(&shell.to_string_lossy()).unwrap();
        assert!(path.starts_with("/from/rc:"), "{path}");
        assert_eq!(shell_path("/no/such/shell"), None);
    }
}
