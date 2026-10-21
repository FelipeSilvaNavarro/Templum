//! Starting Templum when the person logs in to the computer: the system's own entry for it, which
//! is the only record (the toggle reads it, so removing the entry with the system's tools shows
//! the toggle off). A `.desktop` file in the XDG autostart folder on Linux, a LaunchAgent on
//! macOS, a value of the `Run` key on Windows. See docs/specs/settings.md, Start at login.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;

const COMMENT: &str = "An infinite canvas of terminals and AI agents";
/// macOS's label, and the start of the file's name; `--dev` adds `.dev`.
const LABEL: &str = "io.github.kingdaswinx.templum";
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
/// Where Windows' Task Manager and Settings keep which startup apps are switched off.
const APPROVED_KEY: &str =
    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

/// What an entry starts.
#[derive(Clone, Debug, PartialEq)]
struct Launch {
    /// The file the person opens: the AppImage, not where it is mounted.
    program: PathBuf,
    /// A build beside the installed Templum: its own entry, started with `--dev`.
    dev: bool,
    /// macOS: run from the disk image or from the copy macOS runs a downloaded app in, which
    /// are gone once the image is ejected or the app is moved.
    temporary: bool,
}

impl Launch {
    /// This Templum.
    fn current(dev: bool) -> Option<Launch> {
        use templum_update::{Install, Why};
        let install = Install::detect();
        let temporary = install == Install::Manual(Why::NotInstalled);
        let program = match install {
            Install::AppImage(file) => file,
            _ => std::env::current_exe().ok()?,
        };
        Some(Launch {
            program,
            dev,
            temporary,
        })
    }

    fn title(&self) -> &'static str {
        if self.dev { "Templum (dev)" } else { "Templum" }
    }

    /// What a `--dev` build adds to its entry's label or file name (`sep` + `dev`), so it never
    /// shares the installed Templum's.
    fn suffix(&self, sep: char) -> String {
        if self.dev {
            format!("{sep}dev")
        } else {
            String::new()
        }
    }
}

/// Where the system keeps an entry.
#[derive(Clone, Debug, PartialEq)]
enum Place {
    /// Linux: a file in `$XDG_CONFIG_HOME/autostart`.
    Desktop(PathBuf),
    /// macOS: a file in `~/Library/LaunchAgents`.
    Agent(PathBuf),
    /// Windows: the value named by the launch's title, in the current user's `Run` key.
    Run,
}

/// The entry that starts one Templum (the installed one, or `--dev`) at login.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Autostart {
    place: Place,
    launch: Launch,
}

/// What the system holds for an entry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Status {
    /// No entry: nothing starts Templum.
    #[default]
    Absent,
    /// An entry the system's tools switched off.
    Off,
    On,
}

/// What the person sees of it: the entry, whether the system starts Templum as of the last
/// reading, and why the last change failed.
#[derive(Default)]
pub(crate) struct State {
    /// None: the system has no place Templum can write one (no home folder).
    pub entry: Option<Autostart>,
    /// Read when the panel or the welcome opens, and set by each change.
    pub status: Status,
    pub error: Option<String>,
    /// A change is running off the UI thread; another click waits for its answer.
    pub changing: bool,
    /// How many changes have started. A reading applies only if none started after it was
    /// fired: it may have read the system before that change wrote it.
    pub changes: u32,
}

impl State {
    /// The system starts Templum at login, as of the last reading.
    pub fn on(&self) -> bool {
        self.status == Status::On
    }

    /// What a first run's welcome offers before anyone chooses: on, when the system has no entry
    /// at all (one its tools switched off stays off) and this Templum could keep one: not a
    /// `--dev` build, whose entry is a build's, nor one run from the macOS disk image.
    pub fn offered(&self) -> bool {
        self.status == Status::Absent
            && self
                .entry
                .as_ref()
                .is_some_and(|e| !e.launch.dev && !e.launch.temporary)
    }
}

impl Autostart {
    /// This Templum's entry in the system's place for it; None without a home folder.
    pub fn locate(dev: bool) -> Option<Autostart> {
        Autostart::at(
            std::env::consts::OS,
            std::env::var_os("XDG_CONFIG_HOME"),
            std::env::home_dir(),
            Launch::current(dev)?,
        )
    }

    fn at(
        os: &str,
        xdg: Option<OsString>,
        home: Option<PathBuf>,
        launch: Launch,
    ) -> Option<Autostart> {
        let place = match os {
            "linux" => {
                // An unset or relative XDG_CONFIG_HOME means ~/.config, as the spec says.
                let config = xdg
                    .map(PathBuf::from)
                    .filter(|d| d.is_absolute())
                    .or_else(|| home.map(|h| h.join(".config")))?;
                let file = format!("templum{}.desktop", launch.suffix('-'));
                Place::Desktop(config.join("autostart").join(file))
            }
            "macos" => {
                let file = format!("{LABEL}{}.plist", launch.suffix('.'));
                Place::Agent(home?.join("Library").join("LaunchAgents").join(file))
            }
            "windows" => Place::Run,
            _ => return None,
        };
        Some(Autostart { place, launch })
    }

    /// An entry in `dir` of the kind `os` keeps in a folder, for tests.
    #[cfg(test)]
    pub(crate) fn in_dir(os: &str, dir: &std::path::Path, program: &str, dev: bool) -> Autostart {
        let launch = Launch {
            program: program.into(),
            dev,
            temporary: false,
        };
        let home = dir.to_path_buf();
        // The folders the system names under a home, so a test's files stay inside `dir`.
        let xdg = Some(home.join(".config").into_os_string());
        Autostart::at(os, xdg, Some(home), launch).expect("a folder entry")
    }

    /// What the system holds.
    pub fn status(&self) -> Status {
        match self.read() {
            None => Status::Absent,
            Some(stored) if self.on_given(&stored) => Status::On,
            Some(_) => Status::Off,
        }
    }

    /// The system starts Templum at login.
    #[cfg(test)]
    pub fn is_on(&self) -> bool {
        self.status() == Status::On
    }

    /// `is_on`, given what `read` found.
    fn on_given(&self, stored: &str) -> bool {
        match self.place {
            Place::Desktop(_) => !switched_off(stored),
            Place::Agent(_) => true,
            Place::Run => {
                !reg(&reg_query(APPROVED_KEY, self.launch.title())).is_some_and(|o| startup_off(&o))
            }
        }
    }

    /// Turns the entry on (written for this executable) or off (removed); off when it isn't
    /// there is no error.
    pub fn set(&self, on: bool) -> Result<(), String> {
        if on && self.launch.temporary {
            return Err("Move Templum to Applications first".into());
        }
        match (&self.place, on) {
            (Place::Desktop(path) | Place::Agent(path), true) => {
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                }
                std::fs::write(path, self.contents()).map_err(|e| e.to_string())
            }
            (Place::Desktop(path) | Place::Agent(path), false) => {
                match std::fs::remove_file(path) {
                    Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
                    _ => Ok(()),
                }
            }
            (Place::Run, true) => {
                reg_ok(&reg_add(self.launch.title(), &self.contents()))?;
                // Switched off in Task Manager, an entry stays off however it is written again.
                let _ = reg(&reg_delete(APPROVED_KEY, self.launch.title()));
                Ok(())
            }
            (Place::Run, false) if self.read().is_none() => Ok(()),
            (Place::Run, false) => reg_ok(&reg_delete(RUN_KEY, self.launch.title())),
        }
    }

    /// At startup: an entry that is on but whose executable is gone (Templum was moved or
    /// reinstalled elsewhere, the build it named was deleted) is written again for this one. One
    /// that names a file that still exists is another Templum's (a build beside the installed
    /// one, a portable copy, an AppImage): two never take it from each other. Off, or switched
    /// off in the system, it stays so, and so does an entry whose program can't be told (what a
    /// person wrote by hand).
    pub fn refresh(&self) -> Result<(), String> {
        let Some(stored) = self.read() else {
            return Ok(());
        };
        let gone = self
            .stored_program(&stored)
            .is_some_and(|p| p.is_absolute() && matches!(p.try_exists(), Ok(false)));
        if gone && self.on_given(&stored) {
            self.set(true)
        } else {
            Ok(())
        }
    }

    /// What the system holds: the file's text or the value's data; None: no entry.
    fn read(&self) -> Option<String> {
        match &self.place {
            Place::Desktop(path) | Place::Agent(path) => std::fs::read_to_string(path).ok(),
            Place::Run => run_data(&reg(&reg_query(RUN_KEY, self.launch.title()))?),
        }
    }

    /// The whole entry.
    fn contents(&self) -> String {
        match self.place {
            Place::Desktop(_) => desktop_entry(&self.launch),
            Place::Agent(_) => launch_agent(&self.launch),
            Place::Run => command_line(&self.launch),
        }
    }

    /// The executable a stored entry starts; None when it can't be told.
    fn stored_program(&self, stored: &str) -> Option<PathBuf> {
        match self.place {
            Place::Desktop(_) => exec_program(stored),
            Place::Agent(_) => agent_program(stored),
            Place::Run => run_program(stored),
        }
    }
}

/// The Linux file. Name, comment and icon are the apps menu's entry's (`install.sh`).
fn desktop_entry(launch: &Launch) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName={}\nComment={COMMENT}\n{}\nTryExec={}\n\
         Icon=templum\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
        launch.title(),
        exec_line(launch),
        // Unquoted, so the desktop skips the entry (no failed start) when the file is gone.
        launch.program.to_string_lossy().replace('\\', r"\\"),
    )
}

/// `Exec=` for `launch`. The program is quoted whatever it holds; `\`, `"`, `` ` `` and `$` in a
/// quoted argument take a backslash, and a backslash in a file's value is written twice.
fn exec_line(launch: &Launch) -> String {
    let mut quoted = String::new();
    for c in launch.program.to_string_lossy().chars() {
        match c {
            '\\' => quoted.push_str(r"\\\\"),
            '"' | '`' | '$' => {
                quoted.push_str(r"\\");
                quoted.push(c);
            }
            '%' => quoted.push_str("%%"),
            c => quoted.push(c),
        }
    }
    format!(
        "Exec=\"{quoted}\"{}",
        if launch.dev { " --dev" } else { "" }
    )
}

/// The macOS file: started when the person logs in to the desktop, never kept alive.
fn launch_agent(launch: &Launch) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
         \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n<dict>\n\
         \t<key>Label</key>\n\t<string>{LABEL}{}</string>\n\
         {}\
         \t<key>AssociatedBundleIdentifiers</key>\n\t<array>\n\t\t<string>{LABEL}</string>\n\t</array>\n\
         \t<key>RunAtLoad</key>\n\t<true/>\n\
         \t<key>LimitLoadToSessionType</key>\n\t<string>Aqua</string>\n\
         </dict>\n</plist>\n",
        launch.suffix('.'),
        program_arguments(launch),
    )
}

fn program_arguments(launch: &Launch) -> String {
    let mut out = String::from("\t<key>ProgramArguments</key>\n\t<array>\n");
    let program = launch.program.to_string_lossy();
    let args = std::iter::once(&*program).chain(launch.dev.then_some("--dev"));
    for arg in args {
        let arg = arg
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        out.push_str(&format!("\t\t<string>{arg}</string>\n"));
    }
    out.push_str("\t</array>\n");
    out
}

/// The Windows value's data: the executable in quotes, then `--dev`.
fn command_line(launch: &Launch) -> String {
    let program = launch.program.to_string_lossy();
    format!("\"{program}\"{}", if launch.dev { " --dev" } else { "" })
}

/// The program of a `.desktop` file's `Exec=` line, undoing `exec_line`: the file's `\\`, then the
/// quoting of the first argument (`\"`, `\$`, ``\` ``, `\\` and `%%`). An unquoted program ends at
/// the first space.
fn exec_program(text: &str) -> Option<PathBuf> {
    let value = text.lines().find_map(|l| l.strip_prefix("Exec="))?;
    let value = value.trim_start().replace(r"\\", r"\");
    let mut chars = value.chars().peekable();
    let quoted = chars.next_if_eq(&'"').is_some();
    let mut program = String::new();
    loop {
        match chars.next() {
            Some('"') if quoted => break,
            None if quoted => return None,
            Some(c) if !quoted && c.is_whitespace() => break,
            None => break,
            Some('\\') if quoted => program.push(chars.next()?),
            Some('%') if chars.peek() == Some(&'%') => {
                chars.next();
                program.push('%');
            }
            Some(c) => program.push(c),
        }
    }
    (!program.is_empty()).then(|| program.into())
}

/// The first string of a LaunchAgent's `ProgramArguments`, without the XML entities.
fn agent_program(plist: &str) -> Option<PathBuf> {
    let (_, after) = plist.split_once("<key>ProgramArguments</key>")?;
    let (_, after) = after.split_once("<array>")?;
    let (_, after) = after.split_once("<string>")?;
    let (program, _) = after.split_once("</string>")?;
    // `&amp;` last, so `&amp;lt;` stays `&lt;`.
    let program = program
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&");
    Some(program.into())
}

/// What is between the first quotes of the Windows value's data (`command_line`).
fn run_program(data: &str) -> Option<PathBuf> {
    let (_, after) = data.split_once('"')?;
    let (program, _) = after.split_once('"')?;
    Some(program.into())
}

fn reg_add(name: &str, data: &str) -> Vec<String> {
    ["add", RUN_KEY, "/v", name, "/t", "REG_SZ", "/d", data, "/f"]
        .map(String::from)
        .to_vec()
}

fn reg_query(key: &str, name: &str) -> Vec<String> {
    ["query", key, "/v", name].map(String::from).to_vec()
}

fn reg_delete(key: &str, name: &str) -> Vec<String> {
    ["delete", key, "/v", name, "/f"].map(String::from).to_vec()
}

/// `reg.exe` with `args`: its output when it succeeded, None when it failed (a value that
/// isn't there is a failure too).
fn reg(args: &[String]) -> Option<String> {
    let output = run_reg(args).ok().filter(|o| o.status.success())?;
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// `reg` for a change, with the reason it failed.
fn reg_ok(args: &[String]) -> Result<(), String> {
    let output = run_reg(args).map_err(|e| format!("couldn't run reg.exe: {e}"))?;
    if output.status.success() {
        return Ok(());
    }
    let said = String::from_utf8_lossy(&output.stderr);
    Err(said
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("reg.exe failed")
        .trim()
        .to_string())
}

/// Windows' own `reg.exe`, given `%SystemRoot%`. A bare `reg` is looked for in the folder of the
/// running executable first, where anything can put one.
fn reg_path(system_root: Option<OsString>) -> PathBuf {
    let root = system_root.unwrap_or_else(|| r"C:\Windows".into());
    PathBuf::from(root).join("System32").join("reg.exe")
}

fn run_reg(args: &[String]) -> std::io::Result<std::process::Output> {
    let mut command = Command::new(reg_path(std::env::var_os("SystemRoot")));
    command.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    command.output()
}

/// A `.desktop` file the desktop is told not to start: GNOME's switch and the standard `Hidden`.
fn switched_off(text: &str) -> bool {
    text.lines().any(|line| {
        matches!(
            line.trim().replace(' ', "").as_str(),
            "Hidden=true" | "X-GNOME-Autostart-enabled=false"
        )
    })
}

/// The data of the `REG_SZ` (or `REG_EXPAND_SZ`) value `reg query` printed:
/// `    Templum    REG_SZ    "C:\…\templum.exe"`.
fn run_data(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let (_, data) = line
            .split_once("REG_EXPAND_SZ")
            .or_else(|| line.split_once("REG_SZ"))?;
        Some(data.trim().to_string())
    })
}

/// Task Manager and Settings switch a startup app off by a binary value of its name whose first
/// byte is odd (`03…`), and on by an even one (`02…`); `reg query` prints it as hex.
fn startup_off(output: &str) -> bool {
    output
        .lines()
        .find_map(|l| l.split_once("REG_BINARY"))
        .and_then(|(_, hex)| u8::from_str_radix(hex.trim().get(..2)?, 16).ok())
        .is_some_and(|first| first & 1 == 1)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn launch(program: &str, dev: bool) -> Launch {
        Launch {
            program: program.into(),
            dev,
            temporary: false,
        }
    }

    #[test]
    fn the_desktop_file_names_templum_like_the_apps_menu_does() {
        assert_eq!(
            desktop_entry(&launch("/home/a/.local/bin/templum", false)),
            "[Desktop Entry]\nType=Application\nName=Templum\n\
             Comment=An infinite canvas of terminals and AI agents\n\
             Exec=\"/home/a/.local/bin/templum\"\nTryExec=/home/a/.local/bin/templum\n\
             Icon=templum\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
        );
        // Not quoted, with a backslash written twice, as any value of the file.
        let odd = desktop_entry(&launch(r"/a b\c", false));
        assert!(odd.contains("\nTryExec=/a b\\\\c\n"), "{odd}");
        let dev = desktop_entry(&launch("/src/target/debug/templum", true));
        assert!(dev.contains("Name=Templum (dev)\n"), "{dev}");
        assert!(
            dev.contains("Exec=\"/src/target/debug/templum\" --dev\n"),
            "{dev}"
        );
    }

    #[test]
    fn exec_quotes_what_the_desktop_entry_spec_reserves() {
        let exec = |p| exec_line(&launch(p, false));
        assert_eq!(
            exec("/a b/Templum.AppImage"),
            r#"Exec="/a b/Templum.AppImage""#
        );
        // A quote, a dollar and a backtick take a backslash, written twice in the file.
        assert_eq!(exec(r#"/a"b$c`d"#), r#"Exec="/a\\"b\\$c\\`d""#);
        // A backslash is four in the file; a percent sign is two (it would be a field code).
        assert_eq!(exec(r"/a\b%f"), r#"Exec="/a\\\\b%%f""#);
    }

    #[test]
    fn the_launch_agent_runs_at_load_and_escapes_the_path() {
        let plist = launch_agent(&launch(
            "/Applications/Templum.app/Contents/MacOS/templum",
            false,
        ));
        assert_eq!(
            plist,
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
             \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
             <plist version=\"1.0\">\n<dict>\n\
             \t<key>Label</key>\n\t<string>io.github.kingdaswinx.templum</string>\n\
             \t<key>ProgramArguments</key>\n\t<array>\n\
             \t\t<string>/Applications/Templum.app/Contents/MacOS/templum</string>\n\
             \t</array>\n\
             \t<key>AssociatedBundleIdentifiers</key>\n\t<array>\n\
             \t\t<string>io.github.kingdaswinx.templum</string>\n\t</array>\n\
             \t<key>RunAtLoad</key>\n\t<true/>\n\
             \t<key>LimitLoadToSessionType</key>\n\t<string>Aqua</string>\n\
             </dict>\n</plist>\n"
        );
        let dev = launch_agent(&launch("/a&b/<t>", true));
        assert!(
            dev.contains("<string>io.github.kingdaswinx.templum.dev</string>"),
            "{dev}"
        );
        assert!(dev.contains("<string>/a&amp;b/&lt;t&gt;</string>"), "{dev}");
        assert!(dev.contains("<string>--dev</string>"), "{dev}");
    }

    #[test]
    fn the_registry_value_is_the_quoted_executable() {
        let installed = launch(r"C:\Program Files\Templum\templum.exe", false);
        assert_eq!(
            command_line(&installed),
            r#""C:\Program Files\Templum\templum.exe""#
        );
        assert_eq!(
            command_line(&launch(r"C:\src\templum.exe", true)),
            r#""C:\src\templum.exe" --dev"#
        );
        assert_eq!(
            reg_add("Templum", &command_line(&installed)),
            [
                "add",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "Templum",
                "/t",
                "REG_SZ",
                "/d",
                r#""C:\Program Files\Templum\templum.exe""#,
                "/f"
            ]
        );
        assert_eq!(
            reg_delete(RUN_KEY, "Templum (dev)"),
            ["delete", RUN_KEY, "/v", "Templum (dev)", "/f"]
        );
        assert_eq!(
            reg_query(RUN_KEY, "Templum"),
            ["query", RUN_KEY, "/v", "Templum"]
        );
    }

    #[test]
    fn reg_is_the_one_in_system32_not_the_one_found_on_a_search() {
        let expected = |root: &str| PathBuf::from(root).join("System32").join("reg.exe");
        assert_eq!(reg_path(Some(r"D:\Win".into())), expected(r"D:\Win"));
        assert_eq!(reg_path(None), expected(r"C:\Windows"));
    }

    #[test]
    fn reg_query_output_is_read_for_the_data_and_for_the_switch() {
        let out = "\r\nHKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Run\r\n    \
                   Templum (dev)    REG_SZ    \"C:\\a b\\templum.exe\" --dev\r\n\r\n";
        assert_eq!(
            run_data(out).as_deref(),
            Some(r#""C:\a b\templum.exe" --dev"#)
        );
        assert_eq!(
            run_data("    Templum    REG_EXPAND_SZ    %LOCALAPPDATA%\\t.exe\r\n").as_deref(),
            Some(r"%LOCALAPPDATA%\t.exe")
        );
        assert_eq!(run_data("ERROR: nothing"), None);
        let approved = |hex| {
            format!("\r\nHKEY_CURRENT_USER\\…\\Run\r\n    Templum    REG_BINARY    {hex}\r\n")
        };
        assert!(startup_off(&approved("030000000000000000000000")));
        assert!(!startup_off(&approved("020000000000000000000000")));
        assert!(!startup_off(&approved("06000000")));
        assert!(!startup_off("no value"));
    }

    #[test]
    fn a_desktop_file_the_desktop_is_told_not_to_start_reads_off() {
        assert!(!switched_off(
            "[Desktop Entry]\nX-GNOME-Autostart-enabled=true\n"
        ));
        assert!(switched_off("[Desktop Entry]\nHidden=true\n"));
        assert!(switched_off(
            "[Desktop Entry]\nX-GNOME-Autostart-enabled=false\n"
        ));
        assert!(switched_off(
            "[Desktop Entry]\nX-GNOME-Autostart-enabled = false\r\n"
        ));
    }

    #[test]
    fn each_system_keeps_the_entry_where_it_looks() {
        let at = |os, xdg: Option<&str>, home: Option<&str>, dev| {
            Autostart::at(
                os,
                xdg.map(Into::into),
                home.map(Into::into),
                launch("/t", dev),
            )
            .map(|a| a.place)
        };
        let desktop = |p: &str| Some(Place::Desktop(p.into()));
        // Absolute on the system the test runs on: `/x` has no drive on Windows.
        let cfg = std::env::temp_dir().join("cfg");
        assert_eq!(
            at("linux", cfg.to_str(), Some("/h"), false),
            Some(Place::Desktop(
                cfg.join("autostart").join("templum.desktop")
            ))
        );
        // Unset, empty or relative: ~/.config.
        for xdg in [None, Some(""), Some("rel/cfg")] {
            assert_eq!(
                at("linux", xdg, Some("/h"), false),
                desktop("/h/.config/autostart/templum.desktop")
            );
        }
        assert_eq!(
            at("linux", None, Some("/h"), true),
            desktop("/h/.config/autostart/templum-dev.desktop")
        );
        assert_eq!(
            at("macos", None, Some("/Users/a"), false),
            Some(Place::Agent(
                "/Users/a/Library/LaunchAgents/io.github.kingdaswinx.templum.plist".into()
            ))
        );
        assert_eq!(
            at("macos", None, Some("/Users/a"), true),
            Some(Place::Agent(
                "/Users/a/Library/LaunchAgents/io.github.kingdaswinx.templum.dev.plist".into()
            ))
        );
        assert_eq!(at("windows", None, None, false), Some(Place::Run));
        // No home to write under, and a system with no such entry.
        assert_eq!(at("linux", None, None, false), None);
        assert_eq!(at("macos", None, None, false), None);
        assert_eq!(at("freebsd", None, Some("/h"), false), None);
    }

    /// Both kinds of file, in a temporary folder.
    fn entries(dir: &std::path::Path, dev: bool) -> [Autostart; 2] {
        ["linux", "macos"].map(|os| Autostart::in_dir(os, dir, "/opt/templum", dev))
    }

    #[test]
    fn turning_it_on_and_off_writes_and_removes_the_file() {
        let dir = tempfile::tempdir().unwrap();
        for entry in entries(dir.path(), false) {
            assert!(!entry.is_on());
            // Off when it isn't there is no error.
            assert_eq!(entry.set(false), Ok(()));
            assert_eq!(entry.set(true), Ok(()));
            assert!(entry.is_on());
            let (Place::Desktop(path) | Place::Agent(path)) = &entry.place else {
                unreachable!()
            };
            assert_eq!(fs::read_to_string(path).unwrap(), entry.contents());
            assert_eq!(entry.set(false), Ok(()));
            assert!(!entry.is_on());
            assert!(!path.exists());
        }
    }

    #[test]
    fn removing_the_file_by_hand_shows_it_off() {
        let dir = tempfile::tempdir().unwrap();
        for entry in entries(dir.path(), false) {
            entry.set(true).unwrap();
            let (Place::Desktop(path) | Place::Agent(path)) = &entry.place else {
                unreachable!()
            };
            fs::remove_file(path).unwrap();
            assert!(!entry.is_on());
        }
    }

    #[test]
    fn a_desktop_file_switched_off_by_the_desktop_reads_off_and_turning_it_on_writes_it_again() {
        let dir = tempfile::tempdir().unwrap();
        let entry = Autostart::in_dir("linux", dir.path(), "/opt/templum", false);
        entry.set(true).unwrap();
        let Place::Desktop(path) = &entry.place else {
            unreachable!()
        };
        fs::write(path, format!("{}Hidden=true\n", entry.contents())).unwrap();
        assert!(!entry.is_on());
        entry.set(true).unwrap();
        assert!(entry.is_on());
    }

    #[test]
    fn dev_has_an_entry_of_its_own() {
        let dir = tempfile::tempdir().unwrap();
        for (installed, dev) in entries(dir.path(), false)
            .into_iter()
            .zip(entries(dir.path(), true))
        {
            installed.set(true).unwrap();
            assert!(!dev.is_on());
            dev.set(true).unwrap();
            dev.set(false).unwrap();
            // Turning the dev one off leaves the installed one's entry as it was.
            assert!(installed.is_on());
            assert!(!dev.is_on());
            installed.set(false).unwrap();
        }
    }

    #[test]
    fn a_templum_run_from_the_disk_image_wont_write_an_entry_that_would_vanish() {
        let dir = tempfile::tempdir().unwrap();
        let mut entry = Autostart::in_dir("macos", dir.path(), "/Volumes/T/Templum.app", false);
        entry.launch.temporary = true;
        assert_eq!(
            entry.set(true),
            Err("Move Templum to Applications first".into())
        );
        assert!(!entry.is_on());
        // Taking an entry away is always allowed.
        assert_eq!(entry.set(false), Ok(()));
    }

    #[test]
    fn the_status_tells_no_entry_from_one_the_system_switched_off() {
        let dir = tempfile::tempdir().unwrap();
        let entry = Autostart::in_dir("linux", dir.path(), "/opt/templum", false);
        assert_eq!(entry.status(), Status::Absent);
        entry.set(true).unwrap();
        assert_eq!(entry.status(), Status::On);
        let Place::Desktop(path) = &entry.place else {
            unreachable!()
        };
        fs::write(path, format!("{}Hidden=true\n", entry.contents())).unwrap();
        assert_eq!(entry.status(), Status::Off);
        entry.set(false).unwrap();
        assert_eq!(entry.status(), Status::Absent);
    }

    #[test]
    fn a_first_run_offers_only_what_templum_could_keep_and_nobody_switched_off() {
        let dir = tempfile::tempdir().unwrap();
        let state = |entry: Option<Autostart>, status| State {
            entry,
            status,
            ..Default::default()
        };
        let mut entry = Autostart::in_dir("linux", dir.path(), "/opt/templum", false);
        assert!(state(Some(entry.clone()), Status::Absent).offered());
        // An entry that is there: on, or switched off with the system's tools.
        assert!(!state(Some(entry.clone()), Status::On).offered());
        assert!(!state(Some(entry.clone()), Status::Off).offered());
        // No place to write one.
        assert!(!state(None, Status::Absent).offered());
        // A build beside the installed Templum, and the disk image.
        let dev = Autostart::in_dir("linux", dir.path(), "/src/templum", true);
        assert!(!state(Some(dev), Status::Absent).offered());
        entry.launch.temporary = true;
        assert!(!state(Some(entry), Status::Absent).offered());
    }

    #[test]
    fn a_folder_that_cant_be_written_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        // A file where the autostart folder should be.
        fs::create_dir_all(dir.path().join(".config")).unwrap();
        fs::write(dir.path().join(".config/autostart"), "").unwrap();
        let entry = Autostart::in_dir("linux", dir.path(), "/opt/templum", false);
        assert!(entry.set(true).is_err());
        assert!(!entry.is_on());
    }

    #[test]
    fn the_program_of_a_stored_entry_is_read_back_whatever_it_holds() {
        for program in [
            "/opt/templum",
            "/a b/Templum.AppImage",
            r#"/a"b$c`d"#,
            r"/a\b%f",
            "/ü/ñ/t",
        ] {
            for dev in [false, true] {
                let l = launch(program, dev);
                assert_eq!(exec_program(&desktop_entry(&l)), Some(program.into()));
                assert_eq!(agent_program(&launch_agent(&l)), Some(program.into()));
            }
        }
        for program in [
            r"C:\Program Files\Templum\templum.exe",
            r"C:\src\templum.exe",
        ] {
            for dev in [false, true] {
                let data = command_line(&launch(program, dev));
                assert_eq!(run_program(&data), Some(program.into()));
            }
        }
        let agent =
            |p: &str| format!("<key>ProgramArguments</key><array><string>{p}</string></array>");
        assert_eq!(
            agent_program(&agent("/a&amp;lt;&quot;&apos;")),
            Some("/a&lt;\"'".into())
        );
        // What a person wrote differently can't be told.
        assert_eq!(exec_program("[Desktop Entry]\nName=T\n"), None);
        assert_eq!(exec_program("Exec=\"/unclosed\n"), None);
        assert_eq!(
            exec_program("TryExec=/a/t\nExec=env X=1 /a/t\n"),
            Some("env".into())
        );
        assert_eq!(agent_program("<plist></plist>"), None);
        assert_eq!(run_program(r"C:\t.exe"), None);
    }

    /// A file that is there, another Templum's program.
    fn existing(dir: &std::path::Path, name: &str) -> String {
        let path = dir.join(name);
        fs::write(&path, "").unwrap();
        path.to_str().unwrap().to_string()
    }

    /// A program that is nowhere, in `dir`.
    fn missing(dir: &std::path::Path, name: &str) -> String {
        dir.join(name).to_str().unwrap().to_string()
    }

    #[test]
    fn startup_writes_an_entry_that_is_on_again_when_its_executable_is_gone() {
        let dir = tempfile::tempdir().unwrap();
        let (old, new) = (
            missing(dir.path(), "old/templum"),
            missing(dir.path(), "new place/t"),
        );
        for os in ["linux", "macos"] {
            let old = Autostart::in_dir(os, dir.path(), &old, false);
            let moved = Autostart::in_dir(os, dir.path(), &new, false);
            old.set(true).unwrap();
            assert_eq!(moved.refresh(), Ok(()));
            let (Place::Desktop(path) | Place::Agent(path)) = &moved.place else {
                unreachable!()
            };
            let text = fs::read_to_string(path).unwrap();
            assert_eq!(text, moved.contents());
            assert!(!text.contains("old/templum"), "{text}");
            moved.set(false).unwrap();
        }
    }

    #[test]
    fn startup_leaves_the_entry_of_another_templum_that_still_exists() {
        let dir = tempfile::tempdir().unwrap();
        // A build beside the installed Templum, a portable copy, a test AppImage.
        let other = existing(dir.path(), "other templum");
        let here = missing(dir.path(), "here");
        for os in ["linux", "macos"] {
            let theirs = Autostart::in_dir(os, dir.path(), &other, false);
            let ours = Autostart::in_dir(os, dir.path(), &here, false);
            theirs.set(true).unwrap();
            assert_eq!(ours.refresh(), Ok(()));
            let (Place::Desktop(path) | Place::Agent(path)) = &ours.place else {
                unreachable!()
            };
            assert_eq!(fs::read_to_string(path).unwrap(), theirs.contents());
            ours.set(false).unwrap();
        }
    }

    #[test]
    fn startup_leaves_alone_what_is_off_current_or_edited_by_hand() {
        let dir = tempfile::tempdir().unwrap();
        let (gone, here) = (missing(dir.path(), "gone"), existing(dir.path(), "here"));
        let old = Autostart::in_dir("linux", dir.path(), &gone, false);
        let now = Autostart::in_dir("linux", dir.path(), &here, false);
        let Place::Desktop(path) = &now.place else {
            unreachable!()
        };
        // Off: refreshing never turns it on.
        assert_eq!(now.refresh(), Ok(()));
        assert!(!path.exists());
        // Switched off by the desktop: it stays so, and the file is not written over.
        let off = format!("{}Hidden=true\n", old.contents());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, &off).unwrap();
        assert_eq!(now.refresh(), Ok(()));
        assert_eq!(fs::read_to_string(path).unwrap(), off);
        // Current, with a line a person added: untouched.
        let edited = format!("{}X-GNOME-Autostart-Delay=10\n", now.contents());
        fs::write(path, &edited).unwrap();
        assert_eq!(now.refresh(), Ok(()));
        assert_eq!(fs::read_to_string(path).unwrap(), edited);
        // A program a person wrote without a path (found on PATH) can't be told gone.
        let by_hand = "[Desktop Entry]\nType=Application\nExec=templum --dev\n";
        fs::write(path, by_hand).unwrap();
        assert_eq!(now.refresh(), Ok(()));
        assert_eq!(fs::read_to_string(path).unwrap(), by_hand);
    }
}
