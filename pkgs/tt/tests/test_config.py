from __future__ import annotations

import textwrap
from enum import Enum
from pathlib import Path

import pytest

from tasktui.config import ConfigError, as_toml, defaults, load
from tasktui.term.keys import Char, Key, UnknownKey, key_name, parse_key, parse_sequence
from tasktui.ui.action import (
    Bindings,
    Keymap,
    ProjectAction,
    TaskAction,
    build,
    default_keymap,
    project_hints,
    task_hints,
)
from tasktui.ui.theme import CURSOR, UnknownStyle, build_theme


def bound[A: Enum](bindings: Bindings[A], text: str) -> A | None:
    """Which action a written run of keys leads to, if any."""
    return bindings.entries.get(parse_sequence(text))


def written(tmp_path: Path, body: str) -> Path:
    path = tmp_path / "config.toml"
    path.write_text(textwrap.dedent(body))
    return path


@pytest.mark.parametrize(
    ("text", "expected"),
    [
        ("j", Char("j")),
        ("G", Char("G")),
        ("?", Char("?")),
        ("ü", Char("ü")),
        ("space", Char(" ")),
        ("down", Key.DOWN),
        ("DOWN", Key.DOWN),
        ("enter", Key.ENTER),
        ("esc", Key.ESCAPE),
        ("ctrl-c", Key.INTERRUPT),
        ("pageup", Key.PAGE_UP),
    ],
)
def test_key_names_parse(text: str, expected: Char | Key) -> None:
    assert parse_key(text) == expected


@pytest.mark.parametrize("text", ["", "jj", "nope", "ctrl-shift-q", " "])
def test_unusable_key_names_are_refused(text: str) -> None:
    with pytest.raises(UnknownKey):
        parse_key(text)


@pytest.mark.parametrize("text", ["j", "G", "space", "down", "esc", "ctrl-u"])
def test_key_names_round_trip(text: str) -> None:
    assert key_name(parse_key(text)) == text


def test_a_missing_file_is_not_an_error(tmp_path: Path) -> None:
    settings = load(tmp_path / "absent.toml")
    assert settings.keymap == default_keymap()


def test_an_empty_file_gives_the_defaults(tmp_path: Path) -> None:
    assert load(written(tmp_path, "")).keymap == default_keymap()


def test_broken_toml_is_reported_with_the_path(tmp_path: Path) -> None:
    path = written(tmp_path, "[keys\n")
    with pytest.raises(ConfigError, match=str(path)):
        load(path)


def test_the_database_can_be_set(tmp_path: Path) -> None:
    path = written(tmp_path, '[database]\npath = "/srv/tasks.duckdb"\n')
    assert load(path).database == Path("/srv/tasks.duckdb")


def test_a_tilde_in_the_database_path_is_expanded(tmp_path: Path) -> None:
    path = written(tmp_path, '[database]\npath = "~/tasks.duckdb"\n')
    assert load(path).database == Path.home() / "tasks.duckdb"


def test_the_database_must_be_text(tmp_path: Path) -> None:
    with pytest.raises(ConfigError, match="should be text"):
        load(written(tmp_path, "[database]\npath = 7\n"))


def test_a_style_can_be_replaced(tmp_path: Path) -> None:
    path = written(tmp_path, '[theme]\ncursor = "white on blue"\n')
    assert load(path).theme.styles[CURSOR].bgcolor is not None


def test_an_unknown_style_is_refused(tmp_path: Path) -> None:
    with pytest.raises(ConfigError, match="unknown style 'sparkle'"):
        load(written(tmp_path, '[theme]\nsparkle = "bold"\n'))


def test_a_nonsense_style_is_refused(tmp_path: Path) -> None:
    with pytest.raises(ConfigError, match="cursor"):
        load(written(tmp_path, '[theme]\ncursor = "not a colour"\n'))


def test_build_theme_refuses_unknown_names() -> None:
    with pytest.raises(UnknownStyle):
        build_theme({"nope": "bold"})


def test_a_binding_can_be_moved(tmp_path: Path) -> None:
    """Naming an action replaces its keys rather than adding to them."""
    path = written(tmp_path, '[keys.tasks]\ncomplete = ["x"]\n')
    tasks = load(path).keymap.tasks
    assert bound(tasks, "x") == TaskAction.COMPLETE
    assert bound(tasks, "d") is None


def test_other_bindings_are_left_alone(tmp_path: Path) -> None:
    path = written(tmp_path, '[keys.tasks]\ncomplete = ["x"]\n')
    tasks = load(path).keymap.tasks
    assert bound(tasks, "o") == TaskAction.ADD
    assert bound(tasks, "down") == TaskAction.MOVE_DOWN


def test_a_single_key_need_not_be_a_list(tmp_path: Path) -> None:
    path = written(tmp_path, '[keys.projects]\nfind = "/"\n')
    assert bound(load(path).keymap.projects, "/") == ProjectAction.FIND


def test_arrow_only_bindings_work(tmp_path: Path) -> None:
    path = written(
        tmp_path,
        """
        [keys.tasks]
        move_down = ["down"]
        move_up = ["up"]
        """,
    )
    tasks = load(path).keymap.tasks
    assert bound(tasks, "j") is None
    assert bound(tasks, "down") == TaskAction.MOVE_DOWN


def test_an_unknown_action_is_refused(tmp_path: Path) -> None:
    with pytest.raises(ConfigError, match="unknown action 'teleport'"):
        load(written(tmp_path, '[keys.tasks]\nteleport = ["t"]\n'))


def test_an_action_from_the_wrong_pane_is_refused(tmp_path: Path) -> None:
    """`rename` belongs to the project tree, not to a task list."""
    with pytest.raises(ConfigError, match="unknown action 'rename'"):
        load(written(tmp_path, '[keys.tasks]\nrename = ["R"]\n'))


def test_an_unknown_key_is_refused(tmp_path: Path) -> None:
    with pytest.raises(ConfigError, match="unknown key"):
        load(written(tmp_path, '[keys.tasks]\ncomplete = ["ctrl-alt-del"]\n'))


def test_binding_one_key_to_two_actions_is_refused(tmp_path: Path) -> None:
    path = written(
        tmp_path,
        """
        [keys.tasks]
        complete = ["x"]
        add = ["x"]
        """,
    )
    with pytest.raises(ConfigError, match="bound to both"):
        load(path)


def test_a_key_freed_by_a_move_can_be_reused(tmp_path: Path) -> None:
    path = written(
        tmp_path,
        """
        [keys.tasks]
        complete = ["x"]
        add = ["d"]
        """,
    )
    tasks = load(path).keymap.tasks
    assert bound(tasks, "x") == TaskAction.COMPLETE
    assert bound(tasks, "d") == TaskAction.ADD


def test_the_two_panes_are_configured_separately(tmp_path: Path) -> None:
    path = written(
        tmp_path,
        """
        [keys.projects]
        quit = ["Q"]
        [keys.tasks]
        quit = ["x"]
        """,
    )
    keymap = load(path).keymap
    assert bound(keymap.projects, "Q") == ProjectAction.QUIT
    assert bound(keymap.tasks, "x") == TaskAction.QUIT


def test_keys_sections_must_be_tables(tmp_path: Path) -> None:
    with pytest.raises(ConfigError, match="should be a table"):
        load(written(tmp_path, "keys = 3\n"))


def test_a_binding_must_be_a_key_or_a_list(tmp_path: Path) -> None:
    with pytest.raises(ConfigError, match="a key or a list of keys"):
        load(written(tmp_path, "[keys.tasks]\ncomplete = 3\n"))


def test_the_hints_describe_the_default_bindings() -> None:
    keymap = default_keymap()
    assert [hint.text for hint in task_hints(keymap)] == [
        "j/k move",
        "gn tab",
        "h back",
        "o add",
        "J/K order",
        "c edit",
        "p project",
        "D due",
        "d done",
        "s clock",
        "q quit",
    ]
    assert [hint.text for hint in project_hints(keymap)] == [
        "j/k move",
        "gn tab",
        "l open",
        "o new",
        "J/K order",
        "f find",
        "r rename",
        "D due",
        "d forget",
        "q quit",
    ]


def test_a_run_of_keys_is_shown_the_way_it_is_typed() -> None:
    """`g g` is written with a space but pressed as two keys in a row."""
    keymap = default_keymap()
    assert "gn tab" in [hint.text for hint in task_hints(keymap)]
    assert bound(keymap.tasks, "g g") == TaskAction.MOVE_TOP
    assert bound(keymap.tasks, "g n") == TaskAction.NEXT_TAB
    assert bound(keymap.tasks, "g") is None


def test_the_hints_follow_a_rebinding(tmp_path: Path) -> None:
    """The reminder must describe the keys in force, not the ones shipped."""
    path = written(
        tmp_path,
        """
        [keys.tasks]
        complete = ["x"]
        add = ["n"]
        """,
    )
    hints = [hint.text for hint in task_hints(load(path).keymap)]
    assert "x done" in hints
    assert "n add" in hints
    assert "d done" not in hints


def test_an_unbound_action_drops_out_of_the_hints(tmp_path: Path) -> None:
    path = written(tmp_path, "[keys.tasks]\ncomplete = []\n")
    assert not any("done" in h.text for h in task_hints(load(path).keymap))


def test_dumped_settings_load_back_unchanged(tmp_path: Path) -> None:
    original = defaults()
    path = tmp_path / "dumped.toml"
    path.write_text(as_toml(original))
    reloaded = load(path)
    assert reloaded.keymap == original.keymap
    assert reloaded.database == original.database


def test_dumped_settings_survive_a_rebinding(tmp_path: Path) -> None:
    path = written(tmp_path, '[keys.tasks]\ncomplete = ["x", "delete"]\n')
    once = load(path)
    twice_path = tmp_path / "again.toml"
    twice_path.write_text(as_toml(once))
    assert load(twice_path).keymap == once.keymap


def test_an_empty_keymap_is_representable() -> None:
    """A pane with nothing bound simply does nothing."""
    keymap = Keymap(projects=build({}), tasks=build({}))
    assert [hint.text for hint in task_hints(keymap)] == []
