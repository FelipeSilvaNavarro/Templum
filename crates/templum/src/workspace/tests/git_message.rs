//! Commit messages from an agent: Generate ✨, its ⚙, the agents' commands and Settings ▸ Git.

use super::*;

#[gpui_kit::test]
fn the_chosen_agent_is_ready_and_follows_the_config(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    write_config(
        dir.path(),
        concat!(
            "[agents.mine]\nname = \"Mine\"\ncommand = \"mine\"\n",
            // git is on every CI runner: a program that is installed.
            "generate = \"git {model}\"\nmodels = [\"fast\", \"smart\"]\n",
            "[git.commit_message]\nagent = \"mine\"\n",
        ),
    );
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    ws.read_with(cx, |ws, _| {
        let agent = ws.commit_agent.as_ref().unwrap();
        assert_eq!(
            (agent.id.as_str(), agent.model.as_str(), agent.ready),
            ("mine", "fast", true)
        );
    });
    ws.update(cx, |ws, cx| {
        ws.edit(
            Edit::set("git.commit_message", "model", "smart"),
            |c| c.git.commit_message.model = Some("smart".into()),
            cx,
        )
    });
    frame(handle, cx);
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.commit_agent.as_ref().unwrap().model, "smart")
    });
    write_config(dir.path(), "[git.commit_message]\nagent = \"mine\"\n");
    reload(handle, &ws, cx);
    ws.read_with(cx, |ws, _| {
        assert_eq!(ws.commit_agent, None, "mine is gone: never another agent")
    });
}

#[gpui_kit::test]
fn the_agents_dialog_sets_a_commit_message_command(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_with(cx, Some(dir.path().to_path_buf()));
    cx.update_window(handle, |_, window, cx| window.click("settings-button", cx))
        .unwrap();
    settle_slide(handle, cx);
    cx.update_window(handle, |_, window, cx| {
        window.click("settings-tab-agents", cx);
        window.render_frame(cx);
        window.click("edit-agent-claude", cx);
        window.render_frame(cx);
    })
    .unwrap();
    // The dialog slides in for 0.25 s first: presses meanwhile are lost.
    std::thread::sleep(std::time::Duration::from_millis(300));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.within("dialog").try_find("agent-models").is_some());
        window.within("dialog").click("agent-generate", cx);
        window.press("end", cx);
        window.input(" --verbose", cx);
        window.within("dialog").click("ok", cx);
        window.render_frame(cx);
        window.render_frame(cx);
    })
    .unwrap();
    ws.read_with(cx, |ws, _| {
        let claude = &ws.config.agents["claude"];
        assert_eq!(
            claude.generate.as_deref(),
            Some("claude -p --model {model} --verbose")
        );
        assert_eq!(claude.models, None, "the models, unchanged, stay Templum's");
        assert_eq!(claude.command, None);
    });
    ws.update(cx, |ws, _| ws.save());
    let text = std::fs::read_to_string(dir.path().join(crate::config::FILE)).unwrap();
    assert!(
        text.contains("generate = \"claude -p --model {model} --verbose\""),
        "{text}"
    );
}
