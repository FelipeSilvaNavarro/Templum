//! The short sound of a request from an agent (docs/specs/ask-user.md, Telling the person): the
//! platform's own player, in a background thread. One missing or failing is silence, never an
//! error. No audio crate: one would add ALSA's headers to every Linux build, for one chime.

use std::process::{Command, Stdio};
use std::time::Duration;

/// At most one sound this often: parallel calls don't stack.
pub(crate) const GAP: Duration = Duration::from_secs(2);

/// The sound theme's event, through libcanberra (it honors the desktop's switch for event
/// sounds); else PipeWire's or PulseAudio's player with the freedesktop file, when it exists.
#[cfg(all(unix, not(target_os = "macos")))]
fn players() -> Vec<(&'static str, Vec<String>)> {
    const FREEDESKTOP: &str = "/usr/share/sounds/freedesktop/stereo/message-new-instant.oga";
    let args = |args: &[&str]| args.iter().map(|a| a.to_string()).collect();
    let mut players = vec![(
        "canberra-gtk-play",
        args(&["-i", "message-new-instant", "-d", "Templum"]),
    )];
    if std::path::Path::new(FREEDESKTOP).exists() {
        players.push(("pw-play", args(&[FREEDESKTOP])));
        players.push(("paplay", args(&[FREEDESKTOP])));
    }
    players
}

#[cfg(target_os = "macos")]
fn players() -> Vec<(&'static str, Vec<String>)> {
    vec![("afplay", vec!["/System/Library/Sounds/Glass.aiff".into()])]
}

/// The user's sound scheme; the sleep lets the sound finish before PowerShell exits.
#[cfg(windows)]
fn players() -> Vec<(&'static str, Vec<String>)> {
    let command = "[System.Media.SystemSounds]::Asterisk.Play(); Start-Sleep -Milliseconds 1500";
    let args = ["-NoProfile", "-NonInteractive", "-Command", command];
    vec![("powershell", args.iter().map(|a| a.to_string()).collect())]
}

/// Runs each player in turn until one exits well: its index.
fn first_ok(players: &[(&str, Vec<String>)]) -> Option<usize> {
    players.iter().position(|(program, args)| {
        let mut command = Command::new(program);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt as _;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        command.status().is_ok_and(|status| status.success())
    })
}

/// Plays the sound in a background thread; tests hear nothing.
pub(crate) fn play() {
    if cfg!(not(test)) {
        std::thread::spawn(|| first_ok(&players()));
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn the_first_player_that_works_plays() {
        let players = [
            ("false", Vec::new()),
            ("templum-no-such-player", Vec::new()),
            ("true", Vec::new()),
        ];
        assert_eq!(first_ok(&players), Some(2));
        assert_eq!(first_ok(&players[..2]), None, "silence");
    }
}
