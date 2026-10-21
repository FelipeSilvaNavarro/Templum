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
}

impl Launch {
    /// This Templum.
    fn current(dev: bool) -> Option<Launch> {
        let program = match templum_update::Install::detect() {
            templum_update::Install::AppImage(file) => file,
            _ => std::env::current_exe().ok()?,
        };
        Some(Launch { program, dev })
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

/// What the person sees of it: the entry, whether the system starts Templum as of the last
/// reading, and why the last change failed.
#[derive(Default)]
pub(crate) struct State {
    /// None: the system has no place Templum can write one (no home folder).
    pub entry: Option<Autostart>,
    /// Read when the panel or the welcome opens, and set by each change.
    pub on: bool,
    pub error: Option<String>,
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
        };
        let home = dir.to_path_buf();
        // The folders the system names under a home, so a test's files stay inside `dir`.
        let xdg = Some(home.join(".config").into_os_string());
        Autostart::at(os, xdg, Some(home), launch).expect("a folder entry")
    }

    /// The system starts Templum at login.
    pub fn is_on(&self) -> bool {
        let Some(stored) = self.read() else {
            return false;
        };
        match self.place {
            Place::Desktop(_) => !switched_off(&stored),
            Place::Agent(_) => true,
            Place::Run => {
                !reg(&reg_query(APPROVED_KEY, self.launch.title())).is_some_and(|o| startup_off(&o))
            }
        }
    }

    /// Turns the entry on (written for this executable) or off (removed); off when it isn't
    /// there is no error.
    pub fn set(&self, on: bool) -> Result<(), String> {
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

    /// At startup: an entry that is on but starts another executable (Templum was moved,
    /// reinstalled elsewhere) is written again for this one. Off, or switched off in the
    /// system, it stays so, and what a person added to a file by hand stays too.
    pub fn refresh(&self) -> Result<(), String> {
        match self.read() {
            Some(stored) if !stored.contains(&self.runs()) && self.is_on() => self.set(true),
            _ => Ok(()),
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

    /// The part of it that says what it starts.
    fn runs(&self) -> String {
        match self.place {
            Place::Desktop(_) => exec_line(&self.launch),
            Place::Agent(_) => program_arguments(&self.launch),
            Place::Run => command_line(&self.launch),
        }
    }
}

/// The Linux file. Name, comment and icon are the apps menu's entry's (`install.sh`).
fn desktop_entry(launch: &Launch) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName={}\nComment={COMMENT}\n{}\nIcon=templum\n\
         Terminal=false\nX-GNOME-Autostart-enabled=true\n",
        launch.title(),
        exec_line(launch),
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

fn run_reg(args: &[String]) -> std::io::Result<std::process::Output> {
    let mut command = Command::new("reg");
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
        }
    }

    #[test]
    fn the_desktop_file_names_templum_like_the_apps_menu_does() {
        assert_eq!(
            desktop_entry(&launch("/home/a/.local/bin/templum", false)),
            "[Desktop Entry]\nType=Application\nName=Templum\n\
             Comment=An infinite canvas of terminals and AI agents\n\
             Exec=\"/home/a/.local/bin/templum\"\nIcon=templum\nTerminal=false\n\
             X-GNOME-Autostart-enabled=true\n"
        );
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
    fn startup_writes_an_entry_that_is_on_again_when_templum_moved() {
        let dir = tempfile::tempdir().unwrap();
        for os in ["linux", "macos"] {
            let old = Autostart::in_dir(os, dir.path(), "/old/place/templum", false);
            let moved = Autostart::in_dir(os, dir.path(), "/new place/templum", false);
            old.set(true).unwrap();
            assert_eq!(moved.refresh(), Ok(()));
            let (Place::Desktop(path) | Place::Agent(path)) = &moved.place else {
                unreachable!()
            };
            let text = fs::read_to_string(path).unwrap();
            assert_eq!(text, moved.contents());
            assert!(!text.contains("/old/place"), "{text}");
            moved.set(false).unwrap();
        }
    }

    #[test]
    fn startup_leaves_alone_what_is_off_current_or_edited_by_hand() {
        let dir = tempfile::tempdir().unwrap();
        let old = Autostart::in_dir("linux", dir.path(), "/old/templum", false);
        let now = Autostart::in_dir("linux", dir.path(), "/new/templum", false);
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
    }
}
