//! Bytes sent to the shell for key presses and pastes (xterm conventions, and the kitty keyboard
//! protocol for programs that turn it on).

use alacritty_terminal::term::TermMode;

/// A key press, as the UI toolkit reports it (GPUI key names: "enter", "up", "f5", "a"...).
#[derive(Clone, Copy, Debug, Default)]
pub struct KeyInput<'a> {
    pub key: &'a str,
    /// The text the key would type, if any.
    pub key_char: Option<&'a str>,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

/// Bytes for a key press, in the terminal's `mode` (cursor keys, the kitty keyboard's flags).
/// `None` means "not a terminal key": printable text, IME and dead keys go through the UI's
/// text-input path instead.
pub(crate) fn encode_key(k: &KeyInput, mode: TermMode) -> Option<Vec<u8>> {
    // Windows reports AltGr as Ctrl+Alt; when it types a printable char, that char is text.
    if k.ctrl
        && k.alt
        && k.key_char
            .is_some_and(|c| c != k.key && !c.is_empty() && !c.chars().any(char::is_control))
    {
        return None;
    }
    // xterm modifier parameter (the kitty protocol's too): 1 + shift + 2*alt + 4*ctrl.
    let m = 1 + k.shift as u8 + 2 * k.alt as u8 + 4 * k.ctrl as u8;
    if mode.intersects(TermMode::KITTY_KEYBOARD_PROTOCOL)
        && let Some(bytes) = kitty_key(k, m, mode)
    {
        return Some(bytes);
    }
    let app_cursor = mode.contains(TermMode::APP_CURSOR);
    let cursor = |f: char| match (m > 1, app_cursor) {
        (true, _) => format!("\x1b[1;{m}{f}").into_bytes(),
        (false, true) => format!("\x1bO{f}").into_bytes(),
        (false, false) => format!("\x1b[{f}").into_bytes(),
    };
    let ss3 = |f: char| match m > 1 {
        true => format!("\x1b[1;{m}{f}").into_bytes(),
        false => format!("\x1bO{f}").into_bytes(),
    };
    let tilde = |code: u8| match m > 1 {
        true => format!("\x1b[{code};{m}~").into_bytes(),
        false => format!("\x1b[{code}~").into_bytes(),
    };
    let meta = |bytes: &[u8]| match k.alt {
        true => [b"\x1b", bytes].concat(),
        false => bytes.to_vec(),
    };
    Some(match k.key {
        "up" => cursor('A'),
        "down" => cursor('B'),
        "right" => cursor('C'),
        "left" => cursor('D'),
        "home" => cursor('H'),
        "end" => cursor('F'),
        "f1" => ss3('P'),
        "f2" => ss3('Q'),
        "f3" => ss3('R'),
        "f4" => ss3('S'),
        "f5" => tilde(15),
        "f6" => tilde(17),
        "f7" => tilde(18),
        "f8" => tilde(19),
        "f9" => tilde(20),
        "f10" => tilde(21),
        "f11" => tilde(23),
        "f12" => tilde(24),
        "insert" => tilde(2),
        "delete" => tilde(3),
        "pageup" => tilde(5),
        "pagedown" => tilde(6),
        "enter" => meta(b"\r"),
        "backspace" if k.ctrl => meta(b"\x08"),
        "backspace" => meta(b"\x7f"),
        "tab" if k.shift => b"\x1b[Z".to_vec(),
        "tab" => meta(b"\t"),
        "escape" => meta(b"\x1b"),
        "space" if k.ctrl => meta(&[0]),
        key if k.ctrl && key.len() == 1 => {
            let c = key.as_bytes()[0].to_ascii_lowercase();
            let byte = match c {
                b'a'..=b'z' | b'[' | b'\\' | b']' | b'^' | b'_' => c & 0x1f,
                b'@' | b'2' => 0,
                b'3' => 0x1b,
                b'4' => 0x1c,
                b'5' => 0x1d,
                b'6' => 0x1e,
                b'7' | b'/' => 0x1f,
                b'8' => 0x7f,
                _ => return None,
            };
            meta(&[byte])
        }
        _ if k.alt => {
            let text = k.key_char.unwrap_or(k.key);
            if text.chars().count() != 1 {
                return None;
            }
            meta(text.as_bytes())
        }
        _ => return None,
    })
}

/// A key in the kitty keyboard protocol (sw.kovidgoyal.net/kitty/keyboard-protocol), as
/// `CSI code[:shifted];modifiers[;text] u`: what legacy bytes can't tell apart (Shift+Enter,
/// Ctrl+Shift+letter, Esc) or, when the program asked for every key, any key. None: the legacy
/// bytes say it (arrows, F-keys, Home... already carry their modifiers; plain text is text).
// ponytail: presses only, no repeat or release events nor lone modifier keys; they need key-up
// events from the UI.
fn kitty_key(k: &KeyInput, m: u8, mode: TermMode) -> Option<Vec<u8>> {
    let every = mode.contains(TermMode::REPORT_ALL_KEYS_AS_ESC);
    let (code, text_key) = match k.key {
        "escape" => (27, false),
        "enter" => (13, false),
        "tab" => (9, false),
        "backspace" => (127, false),
        "space" => (32, true),
        key => {
            let mut chars = key.chars();
            let c = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            (u32::from(c), true)
        }
    };
    // Enter, Tab and Backspace alone keep their bytes, so a shell left in this mode by a crashed
    // program still runs commands; text is text unless Ctrl or Alt is held.
    let plain = if text_key {
        !k.ctrl && !k.alt
    } else {
        m == 1 && k.key != "escape"
    };
    if plain && !every {
        return None;
    }
    let mut seq = format!("\x1b[{code}");
    let typed = k
        .key_char
        .filter(|t| !t.is_empty() && !t.chars().any(char::is_control));
    if mode.contains(TermMode::REPORT_ALTERNATE_KEYS)
        && k.shift
        && let Some(shifted) = typed.filter(|t| *t != k.key && t.chars().count() == 1)
    {
        seq += &format!(":{}", u32::from(shifted.chars().next()?));
    }
    let text = typed.filter(|_| {
        text_key && every && !k.ctrl && !k.alt && mode.contains(TermMode::REPORT_ASSOCIATED_TEXT)
    });
    if m > 1 || text.is_some() {
        seq += &format!(";{m}");
    }
    if let Some(text) = text {
        let points: Vec<String> = text.chars().map(|c| u32::from(c).to_string()).collect();
        seq += &format!(";{}", points.join(":"));
    }
    seq.push('u');
    Some(seq.into_bytes())
}

/// Bytes for pasted text. Bracketed when the app asked for it (and then any ESC is dropped,
/// so the payload can't close the bracket early and run commands); otherwise newlines
/// become carriage returns, like typing Enter.
pub fn paste_bytes(text: &str, bracketed: bool) -> Vec<u8> {
    if bracketed {
        let body: String = text.chars().filter(|&c| c != '\x1b').collect();
        [b"\x1b[200~".as_slice(), body.as_bytes(), b"\x1b[201~"].concat()
    } else {
        text.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
    }
}

/// Bytes for text typed in for the person to send (a note, a card): never an Enter. Bracketed
/// like a paste when the app asked for it; otherwise its lines are joined with spaces and every
/// other control character becomes a space. Trailing newlines are dropped.
pub fn typed_bytes(text: &str, bracketed: bool) -> Vec<u8> {
    let text = text.trim_end_matches(['\r', '\n']);
    if bracketed {
        paste_bytes(text, true)
    } else {
        text.replace("\r\n", "\n")
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect::<String>()
            .into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_text_never_presses_enter() {
        assert_eq!(typed_bytes("ls\r\nrm -rf x\n", false), b"ls rm -rf x");
        assert_eq!(typed_bytes("a\tb\x1b[A", false), b"a b [A");
        assert_eq!(typed_bytes("a\nb\n", true), b"\x1b[200~a\nb\x1b[201~");
    }

    fn key(key: &str) -> KeyInput<'_> {
        KeyInput {
            key,
            ..KeyInput::default()
        }
    }

    fn enc(k: KeyInput) -> Option<Vec<u8>> {
        encode_key(&k, TermMode::empty())
    }

    #[test]
    fn plain_keys_encode_like_xterm() {
        assert_eq!(enc(key("enter")).unwrap(), b"\r");
        assert_eq!(enc(key("backspace")).unwrap(), b"\x7f");
        assert_eq!(enc(key("tab")).unwrap(), b"\t");
        assert_eq!(
            enc(KeyInput {
                shift: true,
                ..key("tab")
            })
            .unwrap(),
            b"\x1b[Z"
        );
        assert_eq!(enc(key("escape")).unwrap(), b"\x1b");
        assert_eq!(enc(key("up")).unwrap(), b"\x1b[A");
        assert_eq!(
            encode_key(&key("up"), TermMode::APP_CURSOR).unwrap(),
            b"\x1bOA"
        );
        assert_eq!(enc(key("home")).unwrap(), b"\x1b[H");
        assert_eq!(enc(key("pageup")).unwrap(), b"\x1b[5~");
        assert_eq!(enc(key("delete")).unwrap(), b"\x1b[3~");
    }

    #[test]
    fn function_keys_and_modifiers() {
        assert_eq!(enc(key("f1")).unwrap(), b"\x1bOP");
        assert_eq!(enc(key("f5")).unwrap(), b"\x1b[15~");
        assert_eq!(enc(key("f12")).unwrap(), b"\x1b[24~");
        assert_eq!(
            enc(KeyInput {
                ctrl: true,
                ..key("right")
            })
            .unwrap(),
            b"\x1b[1;5C"
        );
        assert_eq!(
            encode_key(
                &KeyInput {
                    ctrl: true,
                    ..key("right")
                },
                TermMode::APP_CURSOR
            )
            .unwrap(),
            b"\x1b[1;5C"
        );
        assert_eq!(
            enc(KeyInput {
                shift: true,
                ..key("f1")
            })
            .unwrap(),
            b"\x1b[1;2P"
        );
        assert_eq!(
            enc(KeyInput {
                alt: true,
                ..key("delete")
            })
            .unwrap(),
            b"\x1b[3;3~"
        );
    }

    #[test]
    fn ctrl_and_alt_combinations() {
        assert_eq!(
            enc(KeyInput {
                ctrl: true,
                ..key("c")
            })
            .unwrap(),
            [3]
        );
        assert_eq!(
            enc(KeyInput {
                ctrl: true,
                shift: true,
                ..key("c")
            })
            .unwrap(),
            [3]
        );
        assert_eq!(
            enc(KeyInput {
                ctrl: true,
                ..key("[")
            })
            .unwrap(),
            [0x1b]
        );
        assert_eq!(
            enc(KeyInput {
                ctrl: true,
                ..key("space")
            })
            .unwrap(),
            [0]
        );
        assert_eq!(
            enc(KeyInput {
                ctrl: true,
                alt: true,
                ..key("a")
            })
            .unwrap(),
            [0x1b, 1]
        );
        assert_eq!(
            enc(KeyInput {
                alt: true,
                key_char: Some("x"),
                ..key("x")
            })
            .unwrap(),
            b"\x1bx"
        );
        assert_eq!(
            enc(KeyInput {
                alt: true,
                key_char: Some("ç"),
                ..key("ç")
            })
            .unwrap(),
            "\x1bç".as_bytes()
        );
        assert_eq!(
            enc(KeyInput {
                alt: true,
                ..key("backspace")
            })
            .unwrap(),
            b"\x1b\x7f"
        );
    }

    #[test]
    fn printable_text_goes_to_the_text_input_path() {
        assert_eq!(
            enc(KeyInput {
                key_char: Some("a"),
                ..key("a")
            }),
            None
        );
        assert_eq!(
            enc(KeyInput {
                key_char: Some(" "),
                ..key("space")
            }),
            None
        );
        assert_eq!(
            enc(KeyInput {
                shift: true,
                key_char: Some("A"),
                ..key("a")
            }),
            None
        );
        // Windows reports AltGr as Ctrl+Alt: AltGr+q on ABNT2 types "/", not ESC ^Q.
        assert_eq!(
            enc(KeyInput {
                ctrl: true,
                alt: true,
                key_char: Some("/"),
                ..key("q")
            }),
            None
        );
    }

    fn kitty(k: KeyInput, mode: TermMode) -> String {
        String::from_utf8(encode_key(&k, mode).unwrap()).unwrap()
    }

    #[test]
    fn the_kitty_keyboard_tells_apart_what_legacy_bytes_cant() {
        let mode = TermMode::DISAMBIGUATE_ESC_CODES;
        let shift = |k| KeyInput { shift: true, ..k };
        let ctrl = |k| KeyInput { ctrl: true, ..k };
        assert_eq!(
            kitty(shift(key("enter")), mode),
            "\x1b[13;2u",
            "Shift+Enter"
        );
        assert_eq!(kitty(ctrl(key("enter")), mode), "\x1b[13;5u");
        assert_eq!(kitty(shift(key("tab")), mode), "\x1b[9;2u");
        assert_eq!(kitty(key("escape"), mode), "\x1b[27u");
        assert_eq!(kitty(ctrl(key("c")), mode), "\x1b[99;5u");
        assert_eq!(kitty(shift(ctrl(key("a"))), mode), "\x1b[97;6u");
        assert_eq!(
            kitty(ctrl(key("-")), mode),
            "\x1b[45;5u",
            "Claude Code's undo"
        );
        let alt_x = KeyInput {
            alt: true,
            key_char: Some("x"),
            ..key("x")
        };
        assert_eq!(kitty(alt_x, mode), "\x1b[120;3u");
        // A shell left in this mode still runs commands; text and arrows keep their bytes.
        assert_eq!(kitty(key("enter"), mode), "\r");
        assert_eq!(kitty(key("backspace"), mode), "\x7f");
        assert_eq!(kitty(ctrl(key("up")), mode), "\x1b[1;5A");
        let a = KeyInput {
            key_char: Some("a"),
            ..key("a")
        };
        assert_eq!(encode_key(&a, mode), None);
        let upper = KeyInput {
            key_char: Some("A"),
            ..shift(key("a"))
        };
        assert_eq!(encode_key(&upper, mode), None);
    }

    #[test]
    fn the_kitty_keyboard_can_report_every_key() {
        let every = TermMode::DISAMBIGUATE_ESC_CODES | TermMode::REPORT_ALL_KEYS_AS_ESC;
        let a = KeyInput {
            key_char: Some("a"),
            ..key("a")
        };
        assert_eq!(kitty(a, every), "\x1b[97u");
        assert_eq!(kitty(key("enter"), every), "\x1b[13u");
        let upper = KeyInput {
            shift: true,
            key_char: Some("A"),
            ..key("a")
        };
        let all = every | TermMode::REPORT_ALTERNATE_KEYS | TermMode::REPORT_ASSOCIATED_TEXT;
        assert_eq!(kitty(upper, every), "\x1b[97;2u");
        assert_eq!(kitty(upper, all), "\x1b[97:65;2;65u");
    }

    #[test]
    fn paste_is_bracketed_and_sanitized() {
        assert_eq!(paste_bytes("ls\n", true), b"\x1b[200~ls\n\x1b[201~");
        assert_eq!(
            paste_bytes("a\x1b[201~rm -rf ~\n", true),
            b"\x1b[200~a[201~rm -rf ~\n\x1b[201~"
        );
        assert_eq!(paste_bytes("a\r\nb\nc", false), b"a\rb\rc");
    }
}
