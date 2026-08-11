"""Validate the public git-cliff release-note contract."""

from __future__ import annotations

import re
from pathlib import Path
from typing import NoReturn

import tomllib

REPO_ROOT = Path(__file__).resolve().parents[2]
CONFIG_PATH = REPO_ROOT / "cliff.toml"

EXPECTED_GROUPS = {
    "feat": "Features",
    "fix": "Bug Fixes",
    "perf": "Performance",
    "refactor": "Refactor",
    "docs": "Documentation",
    "test": "Testing",
    "build": "Build",
    "ci": "CI",
    "chore": "Miscellaneous",
}
HIDDEN_GROUP = "zz-Internal"
EXPECTED_MERGE_REPLACEMENT = "$2 (#$1)"


def fail(message: str) -> NoReturn:
    raise SystemExit(f"git-cliff release-note contract violation: {message}")


def matching_group(parsers: list[dict[str, object]], raw_message: str) -> str | None:
    for parser in parsers:
        field = parser.get("field")
        pattern = parser.get("pattern")
        group = parser.get("group")
        if field != "raw_message":
            fail(f"commit parser must use raw_message, got {field!r}")
        if not isinstance(pattern, str) or not isinstance(group, str):
            fail("commit parser pattern/group must be strings")
        try:
            matched = re.search(pattern, raw_message.strip())
        except re.error as error:
            fail(f"invalid parser regex {pattern!r}: {error}")
        if matched:
            return group
    return None


def normalize_legacy_merge(preprocessor: dict[str, object], message: str) -> str:
    pattern = preprocessor.get("pattern")
    replacement = preprocessor.get("replace")
    if not isinstance(pattern, str):
        fail("legacy merge preprocessor pattern must be a string")
    if replacement != EXPECTED_MERGE_REPLACEMENT:
        fail(
            "legacy merge preprocessor replacement must preserve the conventional "
            f"title and PR number as {EXPECTED_MERGE_REPLACEMENT!r}"
        )
    try:
        match = re.search(pattern, message)
    except re.error as error:
        fail(f"invalid legacy merge preprocessor regex: {error}")
    if match is None:
        return message
    if match.lastindex is None or match.lastindex < 2:
        fail("legacy merge preprocessor must capture PR number and conventional title")
    return f"{match.group(2)} (#{match.group(1)})"


def main() -> None:
    try:
        config = tomllib.loads(CONFIG_PATH.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as error:
        fail(f"cannot parse {CONFIG_PATH.relative_to(REPO_ROOT)}: {error}")

    git = config.get("git")
    if not isinstance(git, dict):
        fail("missing [git] table")

    changelog = config.get("changelog")
    if not isinstance(changelog, dict):
        fail("missing [changelog] table")
    body = changelog.get("body")
    hidden_group_guard = f'group != "{HIDDEN_GROUP}"'
    if not isinstance(body, str) or hidden_group_guard not in body:
        fail("changelog template must omit the hidden internal commit group")

    expected_flags = {
        "conventional_commits": True,
        "filter_unconventional": False,
        "split_commits": False,
        "filter_commits": False,
    }
    for key, expected in expected_flags.items():
        if git.get(key) is not expected:
            fail(f"{key} must be {expected!r}")

    parsers = git.get("commit_parsers")
    expected_parser_count = len(EXPECTED_GROUPS) + 1
    if not isinstance(parsers, list) or len(parsers) != expected_parser_count:
        fail(f"expected exactly {expected_parser_count} commit parsers")
    public_parsers = parsers[:-1]
    internal_parser = parsers[-1]
    if internal_parser != {"message": ".*", "group": HIDDEN_GROUP}:
        fail("final commit parser must group all non-PR entries into the hidden group")

    groups_by_type: dict[str, str] = {}
    for commit_type in EXPECTED_GROUPS:
        fixture = f"{commit_type}(scope): release-visible change (#123)"
        group = matching_group(public_parsers, fixture)
        if group is None:
            fail(f"PR-level {commit_type!r} fixture is omitted")
        groups_by_type[commit_type] = group
    if groups_by_type != EXPECTED_GROUPS:
        fail(f"unexpected parser groups: {groups_by_type!r}")

    for fixture in (
        "fix(scope): branch-internal change",
        "feat: branch-internal change\n\nimplementation detail",
        "Merge branch 'topic'",
        "plain non-conventional commit",
    ):
        if matching_group(public_parsers, fixture) is not None:
            fail(f"non-PR fixture leaked into public notes: {fixture!r}")

    body_fixture = (
        "fix(release): public gate fix (#46)\n\ninternal implementation detail"
    )
    if matching_group(public_parsers, body_fixture) != "Bug Fixes":
        fail("PR suffix matching must apply to the commit title, even with a body")

    preprocessors = git.get("commit_preprocessors")
    if not isinstance(preprocessors, list) or len(preprocessors) != 1:
        fail("expected exactly one legacy merge-PR preprocessor")
    preprocessor = preprocessors[0]
    if not isinstance(preprocessor, dict):
        fail("legacy merge-PR preprocessor must be a table")

    legacy_fixtures = (
        (
            "Merge pull request #40 from example/fix/gui\n\nfix(gui): complete managed model selection",
            "fix(gui): complete managed model selection (#40)",
            "Bug Fixes",
        ),
        (
            "Merge pull request #37 from example/feat/gui\n\nfeat(gui): browse registry models",
            "feat(gui): browse registry models (#37)",
            "Features",
        ),
        (
            "Merge pull request #35 from example/release/readiness\n\nci(release): complete readiness gate",
            "ci(release): complete readiness gate (#35)",
            "CI",
        ),
    )
    for original, expected, expected_group in legacy_fixtures:
        normalized = normalize_legacy_merge(preprocessor, original)
        if normalized != expected:
            fail(f"legacy merge normalized to {normalized!r}, expected {expected!r}")
        if matching_group(public_parsers, normalized) != expected_group:
            fail(f"normalized legacy merge did not map to {expected_group!r}")

    non_pr_merge = "Merge pull request #99 from example/topic\n\nUpdate generated files"
    if normalize_legacy_merge(preprocessor, non_pr_merge) != non_pr_merge:
        fail("non-conventional legacy merge must not be rewritten into public notes")

    print("git-cliff release-note contract check passed")


if __name__ == "__main__":
    main()
