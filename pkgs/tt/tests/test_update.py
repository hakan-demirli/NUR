from __future__ import annotations

from dataclasses import replace
from datetime import UTC, datetime, timedelta

from tasktui.task.model import (
    AllProjects,
    Description,
    Done,
    NoProject,
    Pending,
    ProjectFilter,
    ProjectPath,
    Running,
    Snapshot,
    Task,
    TaskId,
    UnderProject,
    summary_scopes,
)
from tasktui.term.keys import Char, Control, Key, KeyPress
from tasktui.ui.action import Keymap, TaskAction, build, default_keymap
from tasktui.ui.state import (
    Adding,
    AddTask,
    AgendaScreen,
    Command,
    Complete,
    Did,
    Editing,
    ForgetProject,
    Jumping,
    ListScreen,
    MakeProject,
    Moved,
    Naming,
    Normal,
    Note,
    Problem,
    Quit,
    RenameProject,
    Renaming,
    Reopen,
    Reproject,
    Reprojecting,
    Retitle,
    StartClock,
    State,
    StopClock,
    SummaryScreen,
    SwapPlaces,
    SwapProjectPlaces,
    Tab,
)
from tasktui.ui.update import (
    agenda_rows,
    describe,
    focus,
    initial_state,
    list_rows,
    reanchor,
    update,
)

KEYMAP = default_keymap()

EPOCH = datetime(2026, 1, 1, tzinfo=UTC)
NOW = EPOCH + timedelta(hours=12)
EVERYTHING: ProjectFilter = AllProjects()


def task(identifier: int, description: str, project: str | None = None) -> Task:
    return Task(
        id=TaskId(identifier),
        description=Description(description),
        project=None if project is None else ProjectPath.parse(project),
        state=Pending(),
        created_at=EPOCH,
    )


def finished(identifier: int, description: str, at: datetime) -> Task:
    return replace(task(identifier, description), state=Done(completed_at=at))


THREE = Snapshot(
    tasks=(
        task(1, "read the docs", "home"),
        task(2, "write the code", "home.garden"),
        task(3, "ssh into the box"),
    ),
)
EMPTY = Snapshot(tasks=())
LATER = EPOCH + timedelta(days=1)
WITH_A_FINISHED_ONE = Snapshot(
    tasks=(task(1, "still open"), finished(2, "finished", EPOCH)),
)
EVERY_TASK = ListScreen(EVERYTHING, None, Normal())


def press(
    keys: str | list[KeyPress],
    state: State,
    snapshot: Snapshot = THREE,
    keymap: Keymap = KEYMAP,
    page: int = 10,
) -> tuple[State, list[Command]]:
    """Feed a run of key presses, collecting every command issued."""
    sequence: list[KeyPress] = (
        [Char(character) for character in keys] if isinstance(keys, str) else keys
    )
    issued: list[Command] = []
    for key in sequence:
        state, commands = update(key, state, snapshot, keymap, NOW, page)
        issued.extend(commands)
    return state, issued


def a_list(scope: ProjectFilter = EVERYTHING, cursor: int | None = 1) -> State:
    return State(
        agenda=AgendaScreen(None, Normal()),
        projects=ListScreen(
            scope=scope,
            cursor=None if cursor is None else TaskId(cursor),
            mode=Normal(),
        ),
        tab=Tab.PROJECTS,
    )


def a_summary(cursor: ProjectFilter = EVERYTHING) -> State:
    return State(
        agenda=AgendaScreen(None, Normal()),
        projects=SummaryScreen(cursor=cursor, mode=Normal()),
        tab=Tab.PROJECTS,
    )


def an_agenda(cursor: int | None = 1) -> State:
    return State(
        agenda=AgendaScreen(None if cursor is None else TaskId(cursor), Normal()),
        projects=SummaryScreen(EVERYTHING, Normal()),
        tab=Tab.AGENDA,
    )


def test_the_agenda_is_what_opens_first() -> None:
    opened = initial_state(None)
    assert opened.tab == Tab.AGENDA
    assert isinstance(opened.screen, AgendaScreen)


def test_a_scope_opens_straight_into_its_task_list() -> None:
    scope = UnderProject(ProjectPath.parse("home"))
    opened = initial_state(scope)
    assert opened.tab == Tab.PROJECTS
    screen = opened.screen
    assert isinstance(screen, ListScreen)
    assert screen.scope == scope


def test_reanchor_selects_the_first_row_when_the_cursor_is_stale() -> None:
    stale = a_list(cursor=99)
    anchored = reanchor(stale, THREE)
    assert isinstance(anchored.screen, ListScreen)
    assert anchored.screen.cursor == TaskId(1)


def test_reanchor_clears_the_cursor_when_there_are_no_rows() -> None:
    anchored = reanchor(a_list(cursor=1), EMPTY)
    assert isinstance(anchored.screen, ListScreen)
    assert anchored.screen.cursor is None


def test_reanchor_leaves_a_live_cursor_alone() -> None:
    assert reanchor(a_list(cursor=2), THREE).screen == a_list(cursor=2).screen


def test_j_and_k_walk_the_list_and_wrap() -> None:
    state, _ = press("j", a_list(cursor=1))
    assert state.screen.cursor == TaskId(2)
    state, _ = press("jj", a_list(cursor=1))
    assert state.screen.cursor == TaskId(3)
    state, _ = press("jjj", a_list(cursor=1))
    assert state.screen.cursor == TaskId(1)
    state, _ = press("k", a_list(cursor=1))
    assert state.screen.cursor == TaskId(3)


def many(count: int) -> Snapshot:
    return Snapshot(tasks=tuple(task(n, f"task {n}") for n in range(1, count + 1)))


def test_a_page_travels_further_than_a_step() -> None:
    state, _ = press([Control("d")], a_list(cursor=1), many(40), page=8)
    assert state.screen.cursor == TaskId(9)
    state, _ = press([Control("u")], state, many(40), page=8)
    assert state.screen.cursor == TaskId(1)


def test_paging_stops_at_the_ends_rather_than_wrapping() -> None:
    """A step wraps because the ends are obvious; half a screen must not."""
    state, _ = press([Control("u")], a_list(cursor=1), many(40), page=8)
    assert state.screen.cursor == TaskId(1)
    state, _ = press([Control("d")] * 10, a_list(cursor=1), many(40), page=8)
    assert state.screen.cursor == TaskId(40)


def test_the_page_keys_work_in_the_project_tree_too() -> None:
    """The tree leads with the all-tasks row, so five rows on lands at p05."""
    snapshot = Snapshot(
        tasks=tuple(task(n, f"task {n}", f"p{n:02d}") for n in range(1, 20)),
    )
    state, _ = press([Control("d")], a_summary(), snapshot, page=5)
    assert state.screen.cursor == UnderProject(ProjectPath.parse("p05"))


def test_escape_does_not_leave_the_program() -> None:
    """Escape collapses a selection in Helix and is pressed constantly."""
    assert press([Key.ESCAPE], a_list())[1] == []
    assert press([Key.ESCAPE], a_summary())[1] == []


def test_o_adds_a_task() -> None:
    """`o` opens a new line below in Helix, which is what adding is."""
    state, _ = press("o", a_list())
    assert state.screen.mode == Adding("")


def test_nothing_is_added_on_the_agenda() -> None:
    """A task made there has no deadline, so the pane could not show it."""
    state, commands = press("o", an_agenda())
    assert commands == []
    assert state.screen.mode == Normal()
    assert isinstance(state.status, Problem)


def test_the_agenda_still_edits_what_it_shows() -> None:
    """Only making a task is refused; acting on a dated one is the point."""
    dated = Snapshot(tasks=(replace(task(1, "dated"), due=NOW + timedelta(days=1)),))
    for key in ("c", "p", "D"):
        state, _ = press(key, an_agenda(1), dated)
        assert state.screen.mode != Normal(), key
    assert press("d", an_agenda(1), dated)[1] == [Complete(TaskId(1))]
    assert press("s", an_agenda(1), dated)[1] == [StartClock(TaskId(1))]


def test_arrow_keys_move_the_list_too() -> None:
    state, _ = press([Key.DOWN], a_list(cursor=1))
    assert state.screen.cursor == TaskId(2)
    state, _ = press([Key.UP], a_list(cursor=2))
    assert state.screen.cursor == TaskId(1)


def test_gg_and_ge_jump_to_the_ends() -> None:
    state, _ = press("ge", a_list(cursor=1))
    assert state.screen.cursor == TaskId(3)
    state, _ = press("gg", state)
    assert state.screen.cursor == TaskId(1)


def test_movement_on_an_empty_list_is_harmless() -> None:
    state, commands = press("jk", a_list(cursor=None), EMPTY)
    assert commands == []
    assert state.screen.cursor is None


def test_shift_j_moves_a_task_down_the_list() -> None:
    _, commands = press("J", a_list(cursor=1))
    assert commands == [SwapPlaces(TaskId(1), TaskId(2))]


def test_shift_k_moves_a_task_up_the_list() -> None:
    _, commands = press("K", a_list(cursor=2))
    assert commands == [SwapPlaces(TaskId(2), TaskId(1))]


def test_the_cursor_travels_with_the_task_it_moved() -> None:
    """It names a task, not a row, so the key can just be pressed again."""
    state, _ = press("J", a_list(cursor=1))
    assert state.screen.cursor == TaskId(1)


def test_a_task_at_the_end_of_the_list_will_not_go_further() -> None:
    assert press("J", a_list(cursor=3))[1] == []
    assert press("K", a_list(cursor=1))[1] == []


def test_reordering_needs_a_task_under_the_cursor() -> None:
    assert press("J", a_list(cursor=None))[1] == []
    assert press("K", a_list(cursor=None), EMPTY)[1] == []


def test_the_agenda_is_not_reordered_by_hand() -> None:
    """Its order is the order the deadlines fall, which is the point of it."""
    dated = Snapshot(tasks=(replace(task(1, "one"), due=NOW),))
    state, commands = press("J", an_agenda(1), dated)
    assert commands == []
    assert isinstance(state.status, Problem)


def test_reordering_passes_over_what_the_pane_is_not_showing() -> None:
    """Two rows next to each other here may be far apart in the whole list."""
    snapshot = Snapshot(
        tasks=(
            task(1, "shown", "lab"),
            task(2, "hidden", "shed"),
            task(3, "shown too", "lab"),
        ),
    )
    lab = a_list(UnderProject(ProjectPath.parse("lab")), cursor=1)
    _, commands = press("J", lab, snapshot)
    assert commands == [SwapPlaces(TaskId(1), TaskId(3))]


def test_d_completes_the_selected_task() -> None:
    _, commands = press("d", a_list(cursor=2))
    assert commands == [Complete(task_id=TaskId(2))]


def test_d_leaves_the_cursor_on_the_next_task_not_the_top() -> None:
    """The old interface reset to the top after every change."""
    state, _ = press("d", a_list(cursor=2))
    assert state.screen.cursor == TaskId(3)


def test_d_on_the_last_task_stays_on_it() -> None:
    """Nothing is left below it, and it is now the newest finished thing."""
    state, _ = press("d", a_list(cursor=3))
    assert state.screen.cursor == TaskId(3)


def test_d_ticks_off_several_in_a_row() -> None:
    """The cursor holds its place, so the key can just be pressed again."""
    state, commands = press("ddd", a_list(cursor=1))
    assert commands == [Complete(TaskId(1)), Complete(TaskId(2)), Complete(TaskId(3))]
    assert state.screen.cursor == TaskId(3)


def test_finished_work_cannot_be_changed_until_it_is_reopened() -> None:
    """The store will not have it, so refusing the key beats refusing a line
    of typing that has already been done."""
    for key in ("c", "p", "D", "s"):
        state, commands = press(key, a_list(cursor=2), WITH_A_FINISHED_ONE)
        assert commands == [], key
        assert state.screen.mode == Normal(), key
        assert isinstance(state.status, Problem), key


def test_finished_work_can_still_be_moved_about() -> None:
    """Where a task sits is not a change to the task itself."""
    _, commands = press("K", a_list(cursor=2), WITH_A_FINISHED_ONE)
    assert commands == [SwapPlaces(TaskId(2), TaskId(1))]


def test_d_on_a_finished_task_puts_it_back() -> None:
    _, commands = press("d", a_list(cursor=2), WITH_A_FINISHED_ONE)
    assert commands == [Reopen(TaskId(2))]


def test_s_starts_an_idle_task() -> None:
    _, commands = press("s", a_list(cursor=2))
    assert commands == [StartClock(task_id=TaskId(2))]


def test_s_stops_the_task_that_is_running() -> None:
    running = Snapshot(
        tasks=THREE.tasks,
        running=(Running(task_id=TaskId(2), since=EPOCH),),
    )
    _, commands = press("s", a_list(cursor=2), running)
    assert commands == [StopClock(TaskId(2))]


def test_s_starts_a_different_task_while_one_runs() -> None:
    running = Snapshot(
        tasks=THREE.tasks,
        running=(Running(task_id=TaskId(2), since=EPOCH),),
    )
    _, commands = press("s", a_list(cursor=3), running)
    assert commands == [StartClock(task_id=TaskId(3))]


def test_s_leaves_other_clocks_alone() -> None:
    """Starting one task must not quietly stop another."""
    running = Snapshot(
        tasks=THREE.tasks,
        running=(Running(task_id=TaskId(2), since=EPOCH),),
    )
    _, commands = press("s", a_list(cursor=1), running)
    assert commands == [StartClock(task_id=TaskId(1))]


def test_o_types_and_commits_a_new_task() -> None:
    state, commands = press("o", a_list())
    assert state.screen.mode == Adding("")
    state, commands = press("milk", state)
    assert state.screen.mode == Adding("milk")
    state, commands = press([Key.ENTER], state)
    assert commands == [AddTask(Description("milk"), None)]
    assert state.screen.mode == Normal()


def test_a_new_task_takes_the_pane_project() -> None:
    scope = UnderProject(ProjectPath.parse("home.garden"))
    state, _ = press("o", a_list(scope=scope))
    state, _ = press("weeding", state)
    _, commands = press([Key.ENTER], state)
    assert commands == [
        AddTask(Description("weeding"), ProjectPath.parse("home.garden"))
    ]


def test_backspace_edits_the_buffer() -> None:
    state, _ = press("o", a_list())
    state, _ = press("milk", state)
    state, _ = press([Key.BACKSPACE, Key.BACKSPACE], state)
    assert state.screen.mode == Adding("mi")


def test_escape_abandons_a_new_task() -> None:
    state, _ = press("o", a_list())
    state, commands = press("milk", state)
    state, commands = press([Key.ESCAPE], state)
    assert commands == []
    assert state.screen.mode == Normal()


def test_a_blank_task_is_refused() -> None:
    state, _ = press("o", a_list())
    state, commands = press([Key.ENTER], state)
    assert commands == []
    assert isinstance(state.status, Problem)


def test_c_starts_from_the_existing_description() -> None:
    state, _ = press("c", a_list(cursor=2))
    assert state.screen.mode == Editing(TaskId(2), "write the code")


def test_c_commits_a_new_description() -> None:
    state, _ = press("c", a_list(cursor=2))
    state, _ = press([Key.BACKSPACE] * 4, state)
    state, commands = press([Key.ENTER], state)
    assert commands == [Retitle(TaskId(2), Description("write the"))]


def test_c_edits_the_task_it_started_on_even_if_the_list_shifts() -> None:
    state, _ = press("c", a_list(cursor=2))
    assert state.screen.mode == Editing(TaskId(2), "write the code")
    shifted = Snapshot(tasks=THREE.tasks[1:])
    state, commands = press([Key.ENTER], state, shifted)
    assert commands == [Retitle(TaskId(2), Description("write the code"))]


def test_a_rebound_key_drives_the_action_it_was_given() -> None:
    keymap = Keymap(
        projects=KEYMAP.projects,
        tasks=build({TaskAction.COMPLETE: ("x",)}),
    )
    _, commands = press("x", a_list(cursor=2), keymap=keymap)
    assert commands == [Complete(task_id=TaskId(2))]


def test_the_key_a_rebinding_freed_no_longer_does_anything() -> None:
    keymap = Keymap(projects=build({}), tasks=build({TaskAction.COMPLETE: ("x",)}))
    _, commands = press("d", a_list(cursor=2), keymap=keymap)
    assert commands == []


def test_interrupt_quits_whatever_the_keymap_says() -> None:
    """The terminal's own interrupt is not something a file can take away."""
    empty = Keymap(projects=build({}), tasks=build({}))
    assert press([Key.INTERRUPT], a_list(), keymap=empty)[1] == [Quit()]


def test_focus_selects_a_task_in_a_list() -> None:
    assert focus(a_list(cursor=1), TaskId(3)).screen.cursor == TaskId(3)


def test_focus_keeps_the_mode_so_it_can_be_used_mid_flow() -> None:
    typing = State(
        agenda=AgendaScreen(None, Normal()),
        projects=ListScreen(EVERYTHING, TaskId(1), Adding("half typed")),
        tab=Tab.PROJECTS,
    )
    assert focus(typing, TaskId(2)).screen.mode == Adding("half typed")


def test_focus_leaves_the_project_tree_alone() -> None:
    assert focus(a_summary(), TaskId(3)) == a_summary()


def test_the_rename_message_counts_the_tasks_moved() -> None:
    command = RenameProject(ProjectPath.parse("a"), ProjectPath.parse("b"))
    assert describe(command, Moved(1)) == Note("moved 1 task from a to b")
    assert describe(command, Moved(4)) == Note("moved 4 tasks from a to b")


def test_quitting_has_nothing_to_say() -> None:
    assert describe(Quit(), Did()) is None


def test_ctrl_u_clears_the_whole_field() -> None:
    state, _ = press("c", a_list(cursor=2))
    state, _ = press([Control("u")], state)
    assert state.screen.mode == Editing(TaskId(2), "")


def test_ctrl_w_removes_one_word_at_a_time() -> None:
    state, _ = press("o", a_list())
    state, _ = press("mow the lawn", state)
    state, _ = press([Control("w")], state)
    assert state.screen.mode == Adding("mow the")
    state, _ = press([Control("w")], state)
    assert state.screen.mode == Adding("mow")
    state, _ = press([Control("w"), Control("w")], state)
    assert state.screen.mode == Adding("")


def test_p_prefills_with_the_current_project() -> None:
    state, _ = press("p", a_list(cursor=1))
    assert state.screen.mode == Reprojecting(TaskId(1), "home")


def test_p_starts_empty_for_an_unassigned_task() -> None:
    state, _ = press("p", a_list(cursor=3))
    assert state.screen.mode == Reprojecting(TaskId(3), "")


def test_p_moves_a_task_into_a_project_that_does_not_exist_yet() -> None:
    """This is the only way a new project comes into being."""
    state, _ = press("p", a_list(cursor=3))
    state, _ = press("work.email", state)
    _, commands = press([Key.ENTER], state)
    assert commands == [Reproject(TaskId(3), ProjectPath.parse("work.email"))]


def test_p_with_a_blank_field_unassigns_the_task() -> None:
    state, _ = press("p", a_list(cursor=1))
    state, _ = press([Control("u")], state)
    _, commands = press([Key.ENTER], state)
    assert commands == [Reproject(TaskId(1), None)]


def test_p_refuses_a_malformed_project() -> None:
    state, _ = press("p", a_list(cursor=3))
    state, _ = press("a..b", state)
    state, commands = press([Key.ENTER], state)
    assert commands == []
    assert isinstance(state.status, Problem)


def test_escape_abandons_a_move() -> None:
    state, _ = press("p", a_list(cursor=1))
    state, _ = press("elsewhere", state)
    state, commands = press([Key.ESCAPE], state)
    assert commands == []
    assert state.screen.mode == Normal()


def test_typing_a_key_that_is_not_text_leaves_the_buffer_alone() -> None:
    state, _ = press("o", a_list())
    state, _ = press("hi", state)
    state, _ = press([Key.PAGE_UP], state)
    assert state.screen.mode == Adding("hi")


def test_q_and_ctrl_c_quit_from_the_list() -> None:
    assert press("q", a_list())[1] == [Quit()]
    assert press([Key.INTERRUPT], a_list())[1] == [Quit()]


def test_ctrl_c_quits_even_while_typing() -> None:
    state, _ = press("o", a_list())
    assert press([Key.INTERRUPT], state)[1] == [Quit()]


def test_h_returns_to_the_project_tree_on_the_pane_that_was_open() -> None:
    scope = UnderProject(ProjectPath.parse("home"))
    state, _ = press("h", a_list(scope=scope))
    assert state.screen == SummaryScreen(cursor=scope, mode=Normal())


def test_l_opens_the_selected_project() -> None:
    scope = UnderProject(ProjectPath.parse("home"))
    state, _ = press("l", a_summary(cursor=scope))
    assert state.screen == ListScreen(scope=scope, cursor=None, mode=Normal())


def test_the_summary_walks_its_rows() -> None:
    state, _ = press("j", a_summary())
    assert state.screen.cursor == NoProject()
    state, _ = press("j", state)
    assert state.screen.cursor == UnderProject(ProjectPath.parse("home"))


def test_f_jumps_to_a_project_by_first_letter() -> None:
    state, _ = press("f", a_summary())
    assert state.screen.mode == Jumping()
    state, _ = press("h", state)
    assert state.screen.cursor == UnderProject(ProjectPath.parse("home"))
    assert state.screen.mode == Normal()


def test_f_searches_forward_from_the_cursor_and_wraps() -> None:
    home = UnderProject(ProjectPath.parse("home"))
    garden = UnderProject(ProjectPath.parse("home.garden"))
    state, _ = press("fg", a_summary(cursor=home))
    assert state.screen.cursor == garden
    state, _ = press("fg", state)
    assert state.screen.cursor == garden


def test_f_reports_when_nothing_matches() -> None:
    state, _ = press("fz", a_summary())
    assert isinstance(state.status, Problem)
    assert state.screen.mode == Normal()


def test_q_during_a_jump_selects_rather_than_quitting() -> None:
    _, commands = press("fq", a_summary())
    assert commands == []


def test_o_makes_a_project_from_the_tree() -> None:
    """This is the way a project comes into being without a task first."""
    state, _ = press("o", a_summary())
    assert state.screen.mode == Naming("")
    state, _ = press("greenhouse", state)
    state, commands = press([Key.ENTER], state)
    assert commands == [MakeProject(ProjectPath.parse("greenhouse"))]
    assert state.screen.cursor == UnderProject(ProjectPath.parse("greenhouse"))


def test_a_nested_project_can_be_made_in_one_go() -> None:
    state, _ = press("o", a_summary())
    state, _ = press("home.greenhouse", state)
    _, commands = press([Key.ENTER], state)
    assert commands == [MakeProject(ProjectPath.parse("home.greenhouse"))]


def test_a_malformed_new_project_is_refused() -> None:
    state, _ = press("o", a_summary())
    state, _ = press("a..b", state)
    state, commands = press([Key.ENTER], state)
    assert commands == []
    assert isinstance(state.status, Problem)


def test_escape_abandons_a_new_project() -> None:
    state, _ = press("o", a_summary())
    state, _ = press("never mind", state)
    state, commands = press([Key.ESCAPE], state)
    assert commands == []
    assert state.screen.mode == Normal()


NEIGHBOURS = Snapshot(
    tasks=(task(1, "one", "work"), task(2, "two", "home")),
    projects={
        ProjectPath.parse("work"): 1,
        ProjectPath.parse("home"): 2,
        ProjectPath.parse("home.garden"): 3,
    },
)


def under(name: str) -> UnderProject:
    return UnderProject(ProjectPath.parse(name))


def test_shift_j_moves_a_project_down_the_tree() -> None:
    _, commands = press("J", a_summary(cursor=under("work")), NEIGHBOURS)
    assert commands == [SwapProjectPlaces(under("work").path, under("home").path)]


def test_shift_k_moves_a_project_up_the_tree() -> None:
    _, commands = press("K", a_summary(cursor=under("home")), NEIGHBOURS)
    assert commands == [SwapProjectPlaces(under("home").path, under("work").path)]


def test_a_project_swaps_with_the_one_alongside_not_the_row_below() -> None:
    """The row below `home` is its own garden, and nothing goes inside itself."""
    state = a_summary(cursor=under("home"))
    assert press("J", state, NEIGHBOURS)[1] == []


def test_a_project_at_the_end_of_its_neighbours_will_not_go_further() -> None:
    assert press("K", a_summary(cursor=under("work")), NEIGHBOURS)[1] == []


def test_the_cursor_travels_with_the_project_it_moved() -> None:
    state, _ = press("J", a_summary(cursor=under("work")), NEIGHBOURS)
    assert state.screen.cursor == under("work")


def test_only_a_project_row_can_be_moved() -> None:
    for cursor in (AllProjects(), NoProject()):
        state, commands = press("J", a_summary(cursor=cursor), NEIGHBOURS)
        assert commands == []
        assert isinstance(state.status, Problem)


def test_a_tree_nobody_has_arranged_reads_alphabetically() -> None:
    """No places recorded, so the order is the one it always was."""
    snapshot = Snapshot(tasks=(task(1, "one", "work"), task(2, "two", "admin")))
    assert [
        str(scope.path)
        for scope in summary_scopes(snapshot)
        if isinstance(scope, UnderProject)
    ] == ["admin", "work"]


def test_d_forgets_the_selected_project() -> None:
    home = UnderProject(ProjectPath.parse("home"))
    _, commands = press("d", a_summary(cursor=home))
    assert commands == [ForgetProject(ProjectPath.parse("home"))]


def test_d_refuses_a_row_that_is_not_a_project() -> None:
    for cursor in (AllProjects(), NoProject()):
        state, commands = press("d", a_summary(cursor=cursor))
        assert commands == []
        assert isinstance(state.status, Problem)


def test_r_renames_the_last_segment_and_keeps_the_parent() -> None:
    garden = UnderProject(ProjectPath.parse("home.garden"))
    state, _ = press("r", a_summary(cursor=garden))
    assert state.screen.mode == Renaming(ProjectPath.parse("home.garden"), "garden")
    state, _ = press([Key.BACKSPACE] * 6, state)
    state, commands = press("shed", state)
    state, commands = press([Key.ENTER], state)
    assert commands == [
        RenameProject(
            old=ProjectPath.parse("home.garden"),
            new=ProjectPath.parse("home.shed"),
        )
    ]


def test_a_rename_moves_the_cursor_to_the_new_name() -> None:
    garden = UnderProject(ProjectPath.parse("home.garden"))
    state, _ = press("r", a_summary(cursor=garden))
    state, _ = press([Key.BACKSPACE] * 6, state)
    state, _ = press("shed", state)
    state, _ = press([Key.ENTER], state)
    assert state.screen.cursor == UnderProject(ProjectPath.parse("home.shed"))


def test_renaming_to_the_same_name_does_nothing() -> None:
    garden = UnderProject(ProjectPath.parse("home.garden"))
    state, _ = press("r", a_summary(cursor=garden))
    state, commands = press([Key.ENTER], state)
    assert commands == []


def test_a_blank_rename_is_refused() -> None:
    garden = UnderProject(ProjectPath.parse("home.garden"))
    state, _ = press("r", a_summary(cursor=garden))
    state, _ = press([Key.BACKSPACE] * 6, state)
    state, commands = press([Key.ENTER], state)
    assert commands == []
    assert isinstance(state.status, Problem)


def test_the_all_tasks_row_cannot_be_renamed() -> None:
    state, commands = press("r", a_summary(cursor=AllProjects()))
    assert commands == []
    assert isinstance(state.status, Problem)
    assert state.screen.mode == Normal()


def test_unassigned_tasks_cannot_be_renamed() -> None:
    state, _ = press("r", a_summary(cursor=NoProject()))
    assert isinstance(state.status, Problem)


def test_a_status_message_is_cleared_by_the_next_key() -> None:
    state, _ = press("r", a_summary(cursor=AllProjects()))
    assert state.status is not None
    state, _ = press("j", state)
    assert state.status is None


def test_finished_tasks_stay_on_the_list() -> None:
    """They are the record of what was done, so they are not thrown away."""
    rows = list_rows(WITH_A_FINISHED_ONE, EVERY_TASK)
    assert [row.id for row in rows] == [TaskId(1), TaskId(2)]


def test_finished_tasks_keep_their_place_in_the_list() -> None:
    """An order arranged by hand has to survive ticking something off."""
    snapshot = Snapshot(
        tasks=(
            finished(1, "finished first", EPOCH),
            task(2, "still open"),
            finished(3, "finished last", LATER),
        ),
    )
    rows = list_rows(snapshot, EVERY_TASK)
    assert [row.id for row in rows] == [TaskId(1), TaskId(2), TaskId(3)]


def test_the_agenda_leaves_finished_work_out() -> None:
    """It is a view of what is coming, and nothing is coming for a done task."""
    dated = Snapshot(tasks=(replace(finished(1, "done", EPOCH), due=NOW),))
    assert agenda_rows(dated) == ()
