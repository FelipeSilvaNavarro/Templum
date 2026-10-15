# The workspaces screen, stopping and moving: Implementation Plan

**Goal:** the workspaces screen and the edit dialog of [cockpit.md §6](../specs/cockpit.md#6-the-workspaces-screen),
plus stopping a workspace (every process of it ends, its canvas stays, opening it starts it), moving blocks to
another workspace while they run, and a folder per workspace for new terminals.

**Decisions** (made alone, as the maintainer asked): Stop is "quit, for one workspace", and opening starts it; no
Restart (Stop then a click does it). The index's new keys are plain strings checked in `load_index`, so an unknown
value can't send the whole index to `.bak`. Colors reuse `board::Label` plus `accent`. Icons are
`IconName::ALL` by path; the spec's `flask` and `home` don't exist (`flask-conical`, `house`).

**Architecture:**
- `templum-terminal` reads a terminal's process tree before its shell is hung up and kills what is left 2 s later
  (`tree`, `kill_left`, sysinfo's `Process::kill`).
- A stopped `Space` keeps its `Canvas` (so block ids keep counting and a Git job in flight can't land on a
  restarted block) and its canvas in `saved`, which the snapshots skip.
- Moving uses `take_block` (the bookkeeping of `remove_block` without the ghost or the drop) and `with_space`,
  which swaps a hidden space in as `load_spaces` does, so `add_block` joins a grid as for a new block.

## Steps (each with the test that fails without it)

1. `templum-terminal`: `tree`, `kill_left`, `descendants` out of `candidates`.
   Test: `a_tree_is_ended_even_when_it_ignores_hangup`.
2. `session`: `icon`, `color`, `image`, `folder`, `stopped`; `icons_dir`, `copy_icon`, `remove_icon`. Tests:
   `an_index_entry_keeps_its_look_folder_and_state`, `an_index_from_before_loads_and_saves_unchanged`,
   `an_unknown_color_or_icon_keeps_the_rest_of_the_index`, `unsafe_image_names_are_dropped`,
   `copying_an_icon_replaces_the_older_one_and_refuses_big_files`.
3. The edit dialog (`space_dialog.rs`), Rename… becomes Edit…, the look in the sidebar, the title bar and
   notifications; Delete names busy terminals, kills the tree, removes the image; a new workspace opens its dialog.
   Tests: `editing_a_workspace_sets_its_look_and_cancel_keeps_the_old_one`,
   `an_empty_name_is_refused_in_the_edit_dialog`, `the_suggested_icons_all_exist`,
   `deleting_a_workspace_removes_its_image`, `a_new_workspace_opens_its_edit_dialog_with_the_name_selected`.
4. The screen (`spaces_screen.rs`), the `workspaces` command, the sidebar's grid button, the name in the title bar.
   Tests: `the_button_the_name_and_the_command_open_the_screen_and_esc_closes_it`, `the_search_filters_the_cards`,
   `clicking_a_card_switches_and_closes_the_screen`, `down_and_enter_in_the_search_open_the_highlighted_card`,
   `summary_counts_blocks_by_kind_and_running_agents`.
5. Drag and drop of cards and tiles. Tests: `dropping_a_card_on_another_takes_its_place`,
   `dropping_a_sidebar_tile_reorders_and_keeps_the_active_one`.
6. Stop, and starting on open. Tests: `stopping_a_workspace_ends_its_programs_and_keeps_its_canvas`,
   `a_snapshot_in_flight_during_stop_doesnt_empty_the_file`, `a_stopped_workspace_starts_when_opened`,
   `blocks_started_again_get_new_ids`, `a_stopped_workspace_stays_stopped_after_a_restart`,
   `a_stopped_workspace_shows_no_agent_dot`, `stopping_the_active_workspace_shows_the_one_before`,
   `stopping_asks_first_when_a_terminal_is_at_work`.
7. Move to workspace. Tests: `moving_a_terminal_keeps_its_shell_running`, `moving_a_group_keeps_its_arrangement`,
   `a_block_moved_into_a_grid_workspace_joins_its_grid`,
   `a_git_block_moved_while_busy_reads_again_in_its_new_workspace`, `a_stopped_workspace_cant_receive_blocks`.
8. The workspace's folder for new terminals. Test:
   `a_new_terminal_starts_in_the_workspace_folder_when_nothing_focused_has_one`.
9. Docs: README, ARCHITECTURE.md, workspaces.md, context-menus.md, minors.

## Manual checks (Hyprland)

1. Choose an image through the portal: PNG, SVG, WebP; a file over 4 MB is refused.
2. A narrow window shows the screen in 3 columns.
3. Stop a workspace with `claude` and `npm run dev` running: both leave `ps`; opening it shows `claude --continue`.
4. Quit with a stopped workspace and start again: it is still stopped, its shells didn't start.
5. Move a terminal running `claude` to another workspace: it keeps answering.
6. Ctrl+Shift+O, typing, Down, Enter.
