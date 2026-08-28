"""Reading the configuration file.

Everything a file can say is turned into a fully formed value here, so the
rest of the program never has to wonder whether a setting made sense.  A file
that cannot be understood stops the program with an explanation rather than
being partly applied.
"""

from __future__ import annotations

import os
import tomllib
from collections.abc import Mapping
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import Any, Final

from rich.theme import Theme

from tasktui.term.keys import UnknownKey, sequence_name
from tasktui.ui.action import (
    DEFAULT_PROJECT_KEYS,
    DEFAULT_TASK_KEYS,
    AmbiguousBinding,
    Bindings,
    Keymap,
    ProjectAction,
    TaskAction,
    build,
    default_keymap,
)
from tasktui.ui.theme import (
    DEFAULT_STYLES,
    GLYPH_NAMES,
    PALETTE_NAMES,
    STYLE_NAMES,
    Glyphs,
    Look,
    UnknownStyle,
    glyphs_from,
    palette_from,
)

CONFIG_VARIABLE: Final = "TT_CONFIG"
DATABASE_VARIABLE: Final = "TT_DB"
APPLICATION: Final = "tt"
CONFIG_NAME: Final = "config.toml"
DATABASE_NAME: Final = "tasks.duckdb"


class ConfigError(Exception):
    """The configuration file could not be used."""


@dataclass(frozen=True, slots=True)
class Settings:
    """Everything the interface needs that a person may choose."""

    database: Path
    keymap: Keymap
    look: Look
    theme: Theme = field(compare=False)

    @property
    def glyphs(self) -> Glyphs:
        return self.look.glyphs


def config_home() -> Path:
    override = os.environ.get("XDG_CONFIG_HOME")
    return Path(override) if override else Path.home() / ".config"


def data_home() -> Path:
    override = os.environ.get("XDG_DATA_HOME")
    return Path(override) if override else Path.home() / ".local" / "share"


def default_config_path() -> Path:
    """Where the configuration file is looked for."""
    override = os.environ.get(CONFIG_VARIABLE)
    return Path(override) if override else config_home() / APPLICATION / CONFIG_NAME


def default_database_path() -> Path:
    """Where tasks live unless a file or the command line says otherwise."""
    override = os.environ.get(DATABASE_VARIABLE)
    return Path(override) if override else data_home() / APPLICATION / DATABASE_NAME


def load(path: Path) -> Settings:
    """Read settings from ``path``, falling back to the defaults it omits.

    A file that is not there is not a problem; a file that is there and wrong
    is.

    Raises:
        ConfigError: if the file cannot be read or makes no sense.
    """
    document = _read(path)
    try:
        look = Look(
            palette=palette_from(_texts(_table(document, "palette"), "palette")),
            glyphs=glyphs_from(_texts(_table(document, "glyphs"), "glyphs")),
        )
        return Settings(
            database=_database(_table(document, "database")),
            keymap=_keymap(_table(document, "keys")),
            look=look,
            theme=look.theme(_texts(_table(document, "theme"), "theme")),
        )
    except (UnknownKey, UnknownStyle, AmbiguousBinding, ConfigError) as error:
        raise ConfigError(f"{path}: {error}") from error


def defaults() -> Settings:
    """Settings as though no configuration file existed."""
    look = Look()
    return Settings(
        database=default_database_path(),
        keymap=default_keymap(),
        look=look,
        theme=look.theme(),
    )


def _read(path: Path) -> dict[str, Any]:
    if not path.is_file():
        return {}
    try:
        with path.open("rb") as handle:
            return tomllib.load(handle)
    except tomllib.TOMLDecodeError as error:
        raise ConfigError(f"{path}: {error}") from error
    except OSError as error:
        raise ConfigError(f"{path}: {error}") from error


def _table(document: Mapping[str, Any], name: str) -> dict[str, Any]:
    value = document.get(name, {})
    if not isinstance(value, dict):
        raise ConfigError(f"[{name}] should be a table")
    return value


def _database(table: Mapping[str, Any]) -> Path:
    location = table.get("path")
    if location is None:
        return default_database_path()
    if not isinstance(location, str):
        raise ConfigError("database.path should be text")
    return Path(location).expanduser()


def _texts(table: Mapping[str, Any], section: str) -> dict[str, str]:
    """A table whose every value has to be text."""
    written: dict[str, str] = {}
    for name, value in table.items():
        if not isinstance(value, str):
            raise ConfigError(f"{section}.{name} should be text")
        written[name] = value
    return written


def _keymap(table: Mapping[str, Any]) -> Keymap:
    return Keymap(
        projects=_bindings(
            _table(table, "projects"), ProjectAction, DEFAULT_PROJECT_KEYS, "projects"
        ),
        tasks=_bindings(_table(table, "tasks"), TaskAction, DEFAULT_TASK_KEYS, "tasks"),
    )


def _bindings[A: Enum](
    table: Mapping[str, Any],
    actions: type[A],
    fallback: Mapping[A, tuple[str, ...]],
    pane: str,
) -> Bindings[A]:
    """Merge configured bindings over the defaults, action by action.

    Naming an action replaces every key bound to it, so a binding can be moved
    rather than only added to.
    """
    wanted = dict(fallback)
    for name, keys in table.items():
        action = _action(actions, name, pane)
        wanted[action] = _key_names(keys, pane, name)

    claimed: dict[str, A] = {}
    for action, names in wanted.items():
        for text in names:
            settled = sequence_name(_parsed(text, pane))
            clash = claimed.get(settled)
            if clash is not None and clash != action:
                raise ConfigError(
                    f"keys.{pane}: {settled!r} is bound to both "
                    f"{clash.value!r} and {action.value!r}"
                )
            claimed[settled] = action
    try:
        return build(wanted)
    except AmbiguousBinding as error:
        raise ConfigError(f"keys.{pane}: {error}") from error


def _parsed(text: str, pane: str) -> tuple[Any, ...]:
    from tasktui.term.keys import parse_sequence

    try:
        return parse_sequence(text)
    except UnknownKey as error:
        raise ConfigError(f"keys.{pane}: {error}") from error


def _action[A: Enum](actions: type[A], name: str, pane: str) -> A:
    try:
        return actions(name)
    except ValueError:
        known = ", ".join(sorted(member.value for member in actions))
        raise ConfigError(
            f"keys.{pane}: unknown action {name!r}; known actions are: {known}"
        ) from None


def _key_names(keys: Any, pane: str, action: str) -> tuple[str, ...]:
    if isinstance(keys, str):
        return (keys,)
    if isinstance(keys, list) and all(isinstance(key, str) for key in keys):
        return tuple(keys)
    raise ConfigError(f"keys.{pane}.{action} should be a key or a list of keys")


def as_toml(settings: Settings) -> str:
    """The settings written back out, as a starting point for a new file."""
    palette = settings.look.palette
    glyphs = settings.look.glyphs
    lines = [
        f"# {APPLICATION} configuration",
        f"# Save at {default_config_path()}",
        "",
        "[database]",
        f'path = "{settings.database}"',
        "",
        "# Colours, named for the job each one does. Terminal colour names take",
        "# the scheme already configured; a hex value fixes the appearance.",
        "[palette]",
    ]
    lines += [f'{name} = "{getattr(palette, name)}"' for name in PALETTE_NAMES]
    lines += ["", "# Written in terms of the palette unless set here.", "[theme]"]
    lines += [
        f'{short} = "{DEFAULT_STYLES[full]}"'
        for short, full in sorted(STYLE_NAMES.items())
    ]
    lines += ["", "[glyphs]"]
    lines += [f'{name} = "{getattr(glyphs, name)}"' for name in GLYPH_NAMES]
    for pane, bindings in (
        ("projects", settings.keymap.projects),
        ("tasks", settings.keymap.tasks),
    ):
        lines += ["", f"[keys.{pane}]"]
        grouped: dict[str, list[str]] = {}
        for keys, action in bindings.entries.items():
            grouped.setdefault(action.value, []).append(sequence_name(keys))
        for action_name, written in grouped.items():
            rendered = ", ".join(f'"{text}"' for text in written)
            lines.append(f"{action_name} = [{rendered}]")
    return "\n".join(lines) + "\n"
