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
