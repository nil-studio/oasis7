#!/usr/bin/env python3
"""Real-Git regressions for affected-document selection and caller failures."""

from __future__ import annotations

import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile
import unittest


SCRIPTS = Path(__file__).resolve().parent
SCOPE = SCRIPTS / "doc-governance-scope.py"


class GitFixture(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="oasis7-doc-scope-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / "repo"
        self.root.mkdir()
        self.output = Path(self.temp.name) / "selected.bin"
        self.env = os.environ.copy()
        # Fixture history and caller inputs must not inherit a hosted CI range.
        for key in (
            "OASIS7_PRODUCT_DOC_BASE", "OASIS7_PRODUCT_DOC_HEAD",
            "GITHUB_EVENT_NAME", "GITHUB_EVENT_PATH", "GITHUB_ACTIONS",
            "GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "BASH_ENV", "ENV",
        ):
            self.env.pop(key, None)
        self.git("init", "-q", "-b", "main")
        self.git("config", "user.name", "Fixture")
        self.git("config", "user.email", "fixture@example.invalid")
        self.git("config", "commit.gpgsign", "false")
        self.git("config", "core.hooksPath", str(Path(self.temp.name) / "hooks"))
        self.write("doc/README.md", "# Documentation\n[Testing](testing/README.md)\n[Other](other/README.md)\n")
        self.write("doc/testing/README.md", "# Testing\n")
        self.write("doc/other/README.md", "# Other\n")
        self.write("doc/other/topic.md", "# Topic\n\n## Kept\n")
        self.write("doc/testing/guide.md", "# Guide\n[Topic](../other/topic.md#kept)\n")
        self.write("src/example.py", "VALUE = 1\n")
        self.base = self.commit("base")

    def git(self, *args):
        result = subprocess.run(
            ["git", "-C", str(self.root), *args], env=self.env,
            text=True, capture_output=True, check=True,
        )
        return result.stdout.strip()

    def write(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")

    def commit(self, message):
        self.git("add", ".")
        self.git("commit", "-qm", message)
        return self.git("rev-parse", "HEAD")

    def invoke(self, *, base=None, head=None, worktree=True, full=False):
        self.output.unlink(missing_ok=True)
        command = [sys.executable, str(SCOPE), "--repo-root", str(self.root),
                   "--base", self.base if base is None else base,
                   "--head", self.base if head is None else head,
                   "--output", str(self.output)]
        if worktree:
            command.append("--worktree")
        if full:
            command.append("--full-corpus")
        return subprocess.run(command, env=self.env, text=True, capture_output=True)

    def selected(self):
        return {item.decode("utf-8") for item in self.output.read_bytes().split(b"\0") if item}

    def assert_status(self, result, expected):
        self.assertEqual(result.returncode, expected, result.stdout + result.stderr)

    def symlink(self, name, target, *, directory=False):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        try:
            path.symlink_to(target, target_is_directory=directory)
        except (NotImplementedError, OSError) as error:
            self.skipTest(f"symlinks unavailable in this fixture environment: {error}")
        return path


class ScopeCliTests(GitFixture):
    def test_unrelated_implementation_edit_skips_document_checks(self):
        self.write("src/example.py", "VALUE = 2\n")
        result = self.invoke()
        self.assert_status(result, 0)
        self.assertEqual(result.stdout.strip(), "skip")
        self.assertEqual(self.selected(), set())

    def test_new_ordinary_document_and_relative_heading_link_are_selected(self):
        name = "doc/testing/guide with spaces.md"
        self.write(name, "# New guide\n[Kept](../other/topic.md#kept)\n")
        result = self.invoke()
        self.assert_status(result, 0)
        self.assertIn(name, self.selected())

    def test_deleted_markdown_target_selects_unchanged_reverse_reference(self):
        (self.root / "doc/other/topic.md").unlink()
        result = self.invoke()
        self.assert_status(result, 1)
        self.assertIn("doc/testing/guide.md", self.selected())
        self.assertIn("doc/testing/guide.md", result.stderr)
        self.assertIn("../other/topic.md#kept", result.stderr)

    def test_renamed_target_requires_repairing_the_original_reference(self):
        self.git("mv", "doc/other/topic.md", "doc/other/renamed.md")
        self.assert_status(self.invoke(), 1)
        self.write("doc/testing/guide.md", "# Guide\n[Topic](../other/renamed.md#kept)\n")
        self.assert_status(self.invoke(), 0)

    def test_removed_heading_selects_and_rejects_the_unchanged_caller(self):
        self.write("doc/other/topic.md", "# Topic\n\n## Renamed\n")
        result = self.invoke()
        self.assert_status(result, 1)
        self.assertIn("doc/testing/guide.md", self.selected())
        self.assertIn("#kept", result.stderr)

    def test_preexisting_bad_reverse_fragment_stays_advisory_for_other_target_edits(self):
        self.write("doc/testing/guide.md", "# Guide\n[Historical gap](../other/topic.md#historical-gap)\n")
        self.base = self.commit("historical reverse fragment")
        self.write("doc/other/topic.md", "# Topic\n\n## Kept\n\nAn unrelated clarification.\n")
        result = self.invoke()
        self.assert_status(result, 0)
        self.assertIn("doc/testing/guide.md", self.selected())
        self.assertIn("advisory", result.stderr.lower())
        self.assertIn("#historical-gap", result.stderr)

    def test_rendered_repeated_headings_and_explicit_html_anchors_resolve(self):
        self.write("doc/other/topic.md", "# Topic\n\n## Repeated\n\n## Repeated\n\n"
                   "<a id=\"explicit-id\"></a>\n\n## **Bold** and `code`\n")
        self.write("doc/testing/guide.md", "# Guide\n[Repeated](../other/topic.md#repeated-1)\n"
                   "[Explicit](../other/topic.md#explicit-id)\n[Rendered](../other/topic.md#bold-and-code)\n")
        self.assert_status(self.invoke(), 0)
        self.write("doc/testing/guide.md", "# Guide\n[Absent repeat](../other/topic.md#repeated-2)\n")
        self.assert_status(self.invoke(), 1)

    def test_a_heading_inside_a_code_example_is_not_a_real_fragment(self):
        self.write("doc/other/topic.md", "# Topic\n\n```markdown\n## Example only\n```\n")
        self.write("doc/testing/guide.md", "# Guide\n[False anchor](../other/topic.md#example-only)\n")
        result = self.invoke()
        self.assert_status(result, 1)
        self.assertIn("#example-only", result.stderr)

    def test_deleted_non_markdown_source_selects_its_manual(self):
        self.write("doc/testing/guide.md", "# Guide\n[Source](../../src/example.py)\n")
        self.base = self.commit("manual links a source")
        (self.root / "src/example.py").unlink()
        result = self.invoke()
        self.assert_status(result, 1)
        self.assertIn("doc/testing/guide.md", self.selected())
        self.assertIn("../../src/example.py", result.stderr)

    def test_whitespace_and_comment_only_edits_do_not_promote_legacy_findings(self):
        original = "# Guide\n[Historical gap](missing.md)\n"
        self.write("doc/testing/guide.md", original)
        self.base = self.commit("historical gap")
        for text in (original + "\n", original.replace("Guide\n", "Guide  \n"),
                     original + "<!-- maintenance comment -->\n"):
            with self.subTest(text=text):
                self.write("doc/testing/guide.md", text)
                self.assert_status(self.invoke(), 0)
        self.write("doc/testing/guide.md", original.replace("# Guide", " # Guide"))
        self.assert_status(self.invoke(), 1)

    def test_copy_and_rename_are_checked_even_when_content_matches_history(self):
        text = "# Guide\n[Historical gap](missing.md)\n"
        self.write("doc/testing/guide.md", text)
        self.base = self.commit("historical gap")
        copied = self.root / "doc/testing/copied.md"
        copied.write_text(text, encoding="utf-8")
        self.assert_status(self.invoke(), 1)
        copied.unlink()
        self.git("mv", "doc/testing/guide.md", "doc/testing/renamed.md")
        self.assert_status(self.invoke(), 1)

    def test_historical_raw_paths_are_advisory_for_full_and_rule_changes(self):
        self.write("doc/testing/guide.md", "# Guide\nHistorical path: doc/testing/missing.md\n")
        self.base = self.commit("historical raw path")
        for full in (True, False):
            with self.subTest(full=full):
                if not full:
                    self.write("scripts/doc-fixture-policy.py", "# Rule change\n")
                result = self.invoke(full=full)
                self.assert_status(result, 0)
                self.assertIn("advisory", result.stderr.lower())
                self.assertIn("doc/testing/missing.md", result.stderr)
        self.write("doc/testing/guide.md", "# Updated guide\nCurrent path: doc/testing/missing.md\n")
        self.assert_status(self.invoke(full=True), 1)

    def test_pinned_historical_raw_path_requires_an_existing_commit_blob(self):
        historic = self.base
        (self.root / "doc/other/topic.md").unlink()
        self.write("doc/testing/guide.md", f"# Guide\nHistory: eng-cc/oasis7@{historic}:doc/other/topic.md\n")
        self.assert_status(self.invoke(), 0)
        self.write("doc/testing/guide.md", f"# Guide\nHistory: eng-cc/oasis7@{historic}:doc/other/absent.md\n")
        self.assert_status(self.invoke(), 1)

    def test_staged_bad_link_is_not_hidden_by_restored_worktree_content(self):
        original = (self.root / "doc/testing/guide.md").read_text(encoding="utf-8")
        self.write("doc/testing/guide.md", "# Guide\n[Bad staged target](missing.md)\n")
        self.git("add", "doc/testing/guide.md")
        self.write("doc/testing/guide.md", original)
        result = self.invoke()
        self.assert_status(result, 1)
        self.assertIn("missing.md", result.stderr)

    def test_staged_anchor_removal_uses_the_index_for_reverse_references(self):
        original = (self.root / "doc/other/topic.md").read_text(encoding="utf-8")
        self.write("doc/other/topic.md", "# Topic\n\n## Renamed\n")
        self.git("add", "doc/other/topic.md")
        self.write("doc/other/topic.md", original)
        result = self.invoke()
        self.assert_status(result, 1)
        self.assertIn("doc/testing/guide.md", result.stderr)
        self.assertIn("#kept", result.stderr)

    def test_markdown_links_resolve_from_the_source_not_repository_prefixes(self):
        self.write("doc/testing/guide.md", "# Guide\n[Wrong](doc/other/topic.md#kept)\n")
        self.assert_status(self.invoke(), 1)
        self.write("doc/testing/doc/inner.md", "# Inner\n")
        self.write("doc/testing/guide.md", "# Guide\n[Correct relative path](doc/inner.md)\n")
        self.assert_status(self.invoke(), 0)

    def test_dangling_and_external_markdown_symlinks_are_input_errors(self):
        outside = Path(self.temp.name) / "outside.md"
        outside.write_text("# Outside\n", encoding="utf-8")
        for target in (outside, Path(self.temp.name) / "absent.md"):
            with self.subTest(target=target):
                link = self.symlink("doc/testing/link.md", target)
                self.assert_status(self.invoke(full=True), 2)
                link.unlink()

    def test_nested_directory_symlink_is_not_silently_skipped(self):
        outside = Path(self.temp.name) / "outside"
        outside.mkdir()
        (outside / "guide.md").write_text("# Outside\n", encoding="utf-8")
        self.symlink("doc/testing/nested", outside, directory=True)
        self.assert_status(self.invoke(full=True), 2)

    def test_link_escaping_the_repository_is_an_input_error(self):
        outside = Path(self.temp.name) / "outside.md"
        outside.write_text("# Outside\n", encoding="utf-8")
        self.write("doc/testing/guide.md", "# Guide\n[Outside](../../../outside.md)\n")
        self.assert_status(self.invoke(), 2)

    def test_invalid_text_is_not_downgraded_in_full_mode(self):
        (self.root / "doc/testing/invalid.md").write_bytes(b"\xff\xfe")
        self.assert_status(self.invoke(full=True), 2)

    def test_invalid_unknown_and_non_commit_ranges_fail_before_selection(self):
        blob = self.git("rev-parse", "HEAD:doc/testing/guide.md")
        for value in ("", "HEAD", self.base[:12], "f" * 40, blob):
            for key in ("base", "head"):
                with self.subTest(value=value, key=key):
                    result = self.invoke(**{key: value}, full=True)
                    self.assert_status(result, 2)
                    self.assertFalse(self.output.exists())
        orphan = self.git("commit-tree", self.git("rev-parse", "HEAD^{tree}"), "-m", "disconnected")
        self.assert_status(self.invoke(head=orphan), 2)

    def test_target_only_changes_do_not_enter_the_source_range(self):
        self.git("checkout", "-qb", "feature")
        self.write("src/example.py", "VALUE = 2\n")
        source = self.commit("source implementation change")
        self.git("checkout", "-q", "main")
        self.write("doc/testing/main-only.md", "# Main only\n[Historical gap](missing.md)\n")
        target = self.commit("target documentation change")
        self.git("checkout", "-q", "feature")
        result = self.invoke(base=target, head=source, worktree=False)
        self.assert_status(result, 0)
        self.assertEqual(result.stdout.strip(), "skip")
        self.assertEqual(self.selected(), set())


class AggregateFailureTests(GitFixture):
    def setUp(self):
        super().setUp()
        self.bash = shutil.which("bash")
        if not self.bash:
            self.skipTest("bash is required for the aggregate caller contract")
        (self.root / "scripts").mkdir(exist_ok=True)
        for name in ("doc-governance-check.sh", "find-python-with-module.sh", "product_doc_markdown.py"):
            shutil.copy2(SCRIPTS / name, self.root / "scripts" / name)
        # Execute the real selector and its real imports. The other content
        # gates are isolated because their own suites validate their contracts.
        self.write("scripts/doc-governance-scope.py", "import runpy, sys\n"
                   f"sys.path.insert(0, {str(SCRIPTS)!r})\n"
                   f"globals().update(runpy.run_path({str(SCOPE)!r}, run_name=__name__))\n")
        self.gate_log = Path(self.temp.name) / "gates.log"
        for name in ("product-doc-governance-check", "product-doc-content-check",
                     "doc-core-content-check", "system-design-traceability-check"):
            self.write(f"scripts/{name}.py", "from pathlib import Path\n"
                       f"with Path({str(self.gate_log)!r}).open('a') as stream:\n"
                       f"    stream.write({name!r} + '\\n')\n")
        self.base = self.commit("aggregate fixtures")
        self.write("doc/testing/guide.md", "# Guide\n[Topic](../other/topic.md#kept)\nCurrent edit.\n")

    def aggregate(self, *, explicit=True, extra_env=None, full=False):
        env = self.env.copy()
        if explicit:
            env.update(OASIS7_PRODUCT_DOC_BASE=self.base, OASIS7_PRODUCT_DOC_HEAD=self.base)
        if extra_env:
            env.update(extra_env)
        command = [self.bash, "scripts/doc-governance-check.sh"]
        if full:
            command.append("--full-corpus")
        return subprocess.run(command, cwd=self.root, env=env, text=True, capture_output=True)

    def wrapper(self, name, body):
        directory = Path(self.temp.name) / "bin"
        directory.mkdir(exist_ok=True)
        path = directory / name
        path.write_text("#!/usr/bin/env bash\n" + body, encoding="utf-8")
        path.chmod(0o755)
        return {"PATH": str(directory) + os.pathsep + self.env.get("PATH", "")}

    def test_real_navigation_and_no_regex_matches_pass(self):
        env = self.wrapper("rg", "exit 1\n")
        result = self.aggregate(extra_env=env)
        self.assert_status(result, 0)
        self.assertIn("doc-governance-check: OK", result.stdout)
        self.assertTrue(self.gate_log.exists())

    def test_find_and_regex_execution_errors_cannot_report_success(self):
        for command in ("find", "rg"):
            with self.subTest(command=command):
                env = self.wrapper(command, f"echo 'fixture {command} read error' >&2\nexit 2\n")
                result = self.aggregate(extra_env=env)
                self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertIn("scan failed", result.stdout)
                self.assertIn(f"fixture {command} read error", result.stderr)
                (Path(self.temp.name) / "bin" / command).unlink()

    def test_grep_fallback_keeps_an_earlier_error_after_a_later_match(self):
        real_grep = shutil.which("grep")
        if not real_grep:
            self.skipTest("grep is unavailable")
        match_log = Path(self.temp.name) / "grep-match.log"
        env = self.wrapper("grep", "for argument in \"$@\"; do\n"
                           "  if [[ \"$argument\" == doc/README.md ]]; then\n"
                           "    echo 'fixture grep read error' >&2\n    exit 2\n  fi\ndone\n"
                           f"{shlex.quote(real_grep)} \"$@\"\nstatus=$?\n"
                           f"if [[ $status == 0 ]]; then printf 'matched\\n' >> {shlex.quote(str(match_log))}; fi\n"
                           "exit \"$status\"\n")
        bash_env = Path(self.temp.name) / "without-rg.sh"
        bash_env.write_text("command() {\n"
                            "  if [[ ${1:-} == -v && ${2:-} == rg ]]; then return 1; fi\n"
                            "  builtin command \"$@\"\n}\n", encoding="utf-8")
        env["BASH_ENV"] = str(bash_env)
        self.write("doc/testing/guide.md", "# Guide\nCurrent authority: same-name project document.\n"
                   "Literal example: /home/fixture/example\n")
        result = self.aggregate(extra_env=env)
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("fixture grep read error", result.stderr)
        self.assertIn("scan failed", result.stdout)
        self.assertTrue(match_log.exists(), "the later matching file must actually exercise grep")

    def test_ci_event_without_explicit_range_does_not_fall_back_to_local_main(self):
        event = Path(self.temp.name) / "event.json"
        event.write_text("{}", encoding="utf-8")
        env = {"GITHUB_ACTIONS": "true", "GITHUB_EVENT_NAME": "push", "GITHUB_EVENT_PATH": str(event)}
        result = self.aggregate(explicit=False, extra_env=env)
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn("doc-governance-check: OK", result.stdout)
        self.assertFalse(self.gate_log.exists())
        self.assert_status(self.aggregate(extra_env=env), 0)
        self.assert_status(self.aggregate(explicit=False), 0)

    def test_full_caller_keeps_historical_raw_paths_advisory(self):
        self.write("doc/testing/guide.md", "# Guide\nHistorical path: doc/testing/missing.md\n")
        self.base = self.commit("historical raw path")
        result = self.aggregate(full=True)
        self.assert_status(result, 0)
        self.assertIn("advisory", result.stderr.lower())
        self.assertIn("doc/testing/missing.md", result.stderr)

    def test_navigation_does_not_accept_a_repo_root_prefix_as_a_relative_link(self):
        self.write("doc/README.md", "# Documentation\n[Testing](doc/testing/README.md)\n[Other](other/README.md)\n")
        result = self.aggregate()
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.write("doc/README.md", "# Documentation\n[Testing](testing/README.md)\n[Other](other/README.md)\n")
        self.assert_status(self.aggregate(), 0)

    def test_staged_navigation_removal_is_not_hidden_by_restored_disk_content(self):
        original = (self.root / "doc/README.md").read_text(encoding="utf-8")
        self.write("doc/README.md", "# Documentation\n[Other](other/README.md)\n")
        self.git("add", "doc/README.md")
        self.write("doc/README.md", original)
        result = self.aggregate()
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("top-level directory navigation failed", result.stdout)
        self.assertIn("doc/testing/README.md", result.stdout)
        self.git("reset", "-q", "HEAD", "--", "doc/README.md")
        self.assert_status(self.aggregate(), 0)

    def test_unrelated_change_skips_all_content_gate_callers(self):
        self.git("checkout", "--", "doc/testing/guide.md")
        self.write("src/example.py", "VALUE = 2\n")
        result = self.aggregate()
        self.assert_status(result, 0)
        self.assertIn("no documentation or affected reference changes", result.stdout)
        self.assertFalse(self.gate_log.exists())


if __name__ == "__main__":
    unittest.main()
