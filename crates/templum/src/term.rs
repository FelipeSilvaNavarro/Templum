//! Shell-in-a-PTY on top of alacritty_terminal: spawn, write, resize, colors and key encoding.
//! No GPUI in here (it is the future `templum-terminal` crate).

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::tty;
use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};

/// Wakes the UI when any terminal has new output. Coalesced: after one wake, nothing more is sent
/// until the UI renders and calls [`Wake::clear`], so `yes` costs one redraw per frame.
#[derive(Clone)]
pub struct Wake {
    pending: Arc<AtomicBool>,
    tx: UnboundedSender<()>,
}

impl Wake {
    pub fn new() -> (Self, UnboundedReceiver<()>) {
        let (tx, rx) = unbounded();
        let pending = Arc::new(AtomicBool::new(false));
        (Self { pending, tx }, rx)
    }

    fn wake(&self) {
        if !self.pending.swap(true, Ordering::AcqRel) {
            let _ = self.tx.unbounded_send(());
        }
    }

    pub fn clear(&self) {
        self.pending.store(false, Ordering::Release);
    }
}

#[derive(Clone)]
pub struct Listener {
    wake: Wake,
    // Set right after the event loop exists; the Term needs the listener before that.
    pty: Arc<OnceLock<EventLoopSender>>,
}

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        match event {
            // Replies to queries (DSR, DA...) go straight back to the shell, off the UI thread.
            Event::PtyWrite(text) => {
                if let Some(pty) = self.pty.get() {
                    let _ = pty.send(Msg::Input(Cow::Owned(text.into_bytes())));
                }
            }
            Event::Wakeup | Event::ChildExit(_) => self.wake.wake(),
            // ponytail: titles, bell, clipboard, color queries ignored; wire them in the real crate.
            _ => {}
        }
    }
}

pub struct Terminal {
    pub term: Arc<FairMutex<Term<Listener>>>,
    pty: EventLoopSender,
    pub cols: u16,
    pub rows: u16,
}

impl Terminal {
    pub fn spawn(id: u64, cols: u16, rows: u16, cell: (u16, u16), wake: Wake) -> std::io::Result<Self> {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
        let options = tty::Options {
            shell: Some(tty::Shell::new(shell, Vec::new())),
            env: HashMap::from([
                ("TERM".into(), "xterm-256color".into()),
                ("COLORTERM".into(), "truecolor".into()),
            ]),
            ..Default::default()
        };
        let pty = tty::new(&options, window_size(cols, rows, cell), id)?;
        let pty_sender = Arc::new(OnceLock::new());
        let listener = Listener { wake, pty: pty_sender.clone() };
        let size = TermSize::new(cols as usize, rows as usize);
        let term = Arc::new(FairMutex::new(Term::new(Config::default(), &size, listener.clone())));
        let event_loop = EventLoop::new(term.clone(), listener, pty, false, false)?;
        let pty = event_loop.channel();
        let _ = pty_sender.set(pty.clone());
        event_loop.spawn();
        Ok(Self { term, pty, cols, rows })
    }

    pub fn write(&self, bytes: impl Into<Cow<'static, [u8]>>) {
        let _ = self.pty.send(Msg::Input(bytes.into()));
    }

    pub fn resize(&mut self, cols: u16, rows: u16, cell: (u16, u16)) {
        if (cols, rows) == (self.cols, self.rows) {
            return;
        }
        (self.cols, self.rows) = (cols, rows);
        self.term.lock().resize(TermSize::new(cols as usize, rows as usize));
        let _ = self.pty.send(Msg::Resize(window_size(cols, rows, cell)));
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.pty.send(Msg::Shutdown);
    }
}

fn window_size(cols: u16, rows: u16, (cell_width, cell_height): (u16, u16)) -> WindowSize {
    WindowSize { num_lines: rows, num_cols: cols, cell_width, cell_height }
}

// Adapted from Alacritty (Apache-2.0): alacritty/src/config/color.rs default colors.
pub const DEFAULT_FG: Rgb = Rgb { r: 0xd8, g: 0xd8, b: 0xd8 };
pub const DEFAULT_BG: Rgb = Rgb { r: 0x18, g: 0x18, b: 0x18 };
const ANSI: [u32; 16] = [
    0x181818, 0xac4242, 0x90a959, 0xf4bf75, 0x6a9fb5, 0xaa759f, 0x75b5aa, 0xd8d8d8, // normal
    0x6b6b6b, 0xc55555, 0xaac474, 0xfeca88, 0x82b8c8, 0xc28cb8, 0x93d3c3, 0xf8f8f8, // bright
];

fn hex(v: u32) -> Rgb {
    Rgb { r: (v >> 16) as u8, g: (v >> 8) as u8, b: v as u8 }
}

/// xterm 256-color palette: 16 ANSI, 6x6x6 cube, 24 grays.
fn indexed(i: u8) -> Rgb {
    match i {
        0..16 => hex(ANSI[i as usize]),
        16..232 => {
            let i = i - 16;
            let level = |v: u8| if v == 0 { 0 } else { v * 40 + 55 };
            Rgb { r: level(i / 36), g: level(i / 6 % 6), b: level(i % 6) }
        }
        _ => {
            let v = 8 + (i - 232) * 10;
            Rgb { r: v, g: v, b: v }
        }
    }
}

fn dim(c: Rgb) -> Rgb {
    let f = |v: u8| (v as f32 * 0.66) as u8;
    Rgb { r: f(c.r), g: f(c.g), b: f(c.b) }
}

/// Resolves a cell color: the terminal's OSC-set palette first, then our defaults.
pub fn resolve(color: Color, colors: &Colors) -> Rgb {
    match color {
        Color::Spec(rgb) => rgb,
        Color::Indexed(i) => colors[i as usize].unwrap_or_else(|| indexed(i)),
        Color::Named(n) => colors[n].unwrap_or_else(|| match n as usize {
            i @ 0..16 => indexed(i as u8),
            i @ 259..267 => dim(indexed((i - 259) as u8)),
            _ if n == NamedColor::Background => DEFAULT_BG,
            _ if n == NamedColor::DimForeground => dim(DEFAULT_FG),
            _ => DEFAULT_FG,
        }),
    }
}

/// Bytes a key press sends to the shell. `None` means "not a terminal key": let it reach the
/// text-input path (printable chars, IME, dead keys).
/// ponytail: no modifier variants for arrows/F-keys (ctrl-left etc.) and no kitty protocol.
pub fn encode_key(key: &str, key_char: Option<&str>, ctrl: bool, alt: bool, shift: bool, app_cursor: bool) -> Option<Vec<u8>> {
    let cursor = |c: char| {
        let prefix = if app_cursor { "\x1bO" } else { "\x1b[" };
        format!("{prefix}{c}").into_bytes()
    };
    let bytes = match key {
        "enter" => b"\r".to_vec(),
        "backspace" if ctrl => b"\x08".to_vec(),
        "backspace" => b"\x7f".to_vec(),
        "tab" if shift => b"\x1b[Z".to_vec(),
        "tab" => b"\t".to_vec(),
        "escape" => b"\x1b".to_vec(),
        "up" => cursor('A'),
        "down" => cursor('B'),
        "right" => cursor('C'),
        "left" => cursor('D'),
        "home" => cursor('H'),
        "end" => cursor('F'),
        "pageup" => b"\x1b[5~".to_vec(),
        "pagedown" => b"\x1b[6~".to_vec(),
        "delete" => b"\x1b[3~".to_vec(),
        "insert" => b"\x1b[2~".to_vec(),
        "space" if ctrl => vec![0],
        _ if ctrl && key.len() == 1 => {
            let c = key.as_bytes()[0];
            match c {
                b'a'..=b'z' => vec![c & 0x1f],
                b'[' | b'\\' | b']' | b'^' | b'_' => vec![c & 0x1f],
                b'@' | b'2' => vec![0],
                _ => return None,
            }
        }
        _ if alt => {
            let text = key_char.unwrap_or(key);
            if text.chars().count() != 1 {
                return None;
            }
            let mut bytes = vec![0x1b];
            bytes.extend_from_slice(text.as_bytes());
            bytes
        }
        _ => return None,
    };
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_encode_like_xterm() {
        let enc = |k, c, ctrl, alt, app| encode_key(k, c, ctrl, alt, false, app);
        assert_eq!(enc("enter", None, false, false, false).unwrap(), b"\r");
        assert_eq!(enc("backspace", None, false, false, false).unwrap(), b"\x7f");
        assert_eq!(enc("c", None, true, false, false).unwrap(), [3]);
        assert_eq!(enc("up", None, false, false, false).unwrap(), b"\x1b[A");
        assert_eq!(enc("up", None, false, false, true).unwrap(), b"\x1bOA");
        assert_eq!(enc("x", Some("x"), false, true, false).unwrap(), b"\x1bx");
        // Printable text is left to the IME/text-input path.
        assert_eq!(enc("a", Some("a"), false, false, false), None);
        assert_eq!(enc("space", Some(" "), false, false, false), None);
    }

    #[test]
    fn palette_matches_xterm() {
        let c = Colors::default();
        assert_eq!(resolve(Color::Indexed(16), &c), Rgb { r: 0, g: 0, b: 0 });
        assert_eq!(resolve(Color::Indexed(196), &c), Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(resolve(Color::Indexed(255), &c), Rgb { r: 238, g: 238, b: 238 });
        assert_eq!(resolve(Color::Named(NamedColor::Red), &c), hex(0xac4242));
        assert_eq!(resolve(Color::Named(NamedColor::Background), &c), DEFAULT_BG);
    }
}
