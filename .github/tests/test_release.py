"""Exercise release guards without publishing, credentials or network access."""

from __future__ import annotations

import os
from pathlib import Path
import subprocess
import tempfile
import unittest


SCRIPTS = Path(__file__).resolve().parents[1] / "scripts"
TAG = "document-graphics-mermaid-v1.2.3"


class ReleaseChecks(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory(prefix="mermaid-release-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.env = os.environ | {"TAG": TAG}

    def run_command(self, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            args, cwd=self.root, env=self.env, capture_output=True, text=True,
            check=True, timeout=30,
        )

    def guard(self, name: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["bash", str(SCRIPTS / name)], cwd=self.root, env=self.env,
            capture_output=True, text=True, timeout=30,
        )

    def repository(self, package: str = "document-graphics-mermaid") -> None:
        (self.root / "Cargo.toml").write_text(
            f'[package]\nname = "{package}"\nversion = "1.2.3"\n'
            'edition = "2024"\n[lib]\npath = "lib.rs"\n', encoding="utf-8",
        )
        (self.root / "lib.rs").write_text("", encoding="utf-8")
        self.run_command("cargo", "generate-lockfile", "--offline")
        self.run_command("git", "-c", "init.templateDir=", "init", "-q", "-b", "main")
        self.run_command("git", "config", "user.name", "Release test")
        self.run_command("git", "config", "user.email", "release@example.invalid")
        self.run_command("git", "config", "commit.gpgsign", "false")
        self.run_command("git", "config", "core.hooksPath", "/dev/null")
        self.run_command("git", "add", ".")
        self.run_command("git", "commit", "-qm", "Release fixture")
        self.run_command("git", "update-ref", "refs/remotes/origin/main", "HEAD")

    def test_matching_tag_on_main(self) -> None:
        self.repository()
        result = self.guard("check-release.sh")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_rejects_wrong_version_and_upstream_tags(self) -> None:
        self.repository()
        for tag in ("document-graphics-mermaid-v1.2.4", "v1.2.3", "main"):
            with self.subTest(tag=tag):
                self.env["TAG"] = tag
                result = self.guard("check-release.sh")
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("Tag does not match", result.stderr)

    def test_rejects_upstream_package(self) -> None:
        self.repository("mermaid-rs-renderer")
        result = self.guard("check-release.sh")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("expected the fork package", result.stderr)

    def test_rejects_commit_not_on_remote_main(self) -> None:
        self.repository()
        self.run_command("git", "commit", "--allow-empty", "-qm", "Unpushed change")
        result = self.guard("check-release.sh")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("must be on origin/main", result.stderr)

    def test_accepts_older_commit_on_main(self) -> None:
        self.repository()
        self.run_command("git", "commit", "--allow-empty", "-qm", "Later change")
        self.run_command("git", "update-ref", "refs/remotes/origin/main", "HEAD")
        self.run_command("git", "checkout", "--detach", "HEAD~1")
        result = self.guard("check-release.sh")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_rejects_missing_main(self) -> None:
        self.repository()
        self.run_command("git", "update-ref", "-d", "refs/remotes/origin/main")
        self.assertNotEqual(self.guard("check-release.sh").returncode, 0)

    def test_registry_status_and_transport_errors(self) -> None:
        # Only the network call is replaced; run the actual shell decision logic.
        stub = self.root / "curl"
        stub.write_text(
            '#!/usr/bin/env bash\nprintf "%s" "$TEST_STATUS"\nexit "$TEST_EXIT"\n',
            encoding="utf-8",
        )
        stub.chmod(0o755)
        self.env["PATH"] = str(self.root) + os.pathsep + self.env["PATH"]
        cases = [
            ("200", "0", "published=true\n"),
            ("404", "0", "published=false\n"),
            ("403", "0", None), ("429", "0", None), ("500", "0", None),
            ("000", "6", None), ("404", "28", None),
        ]
        for status, exit_code, expected in cases:
            with self.subTest(status=status, exit_code=exit_code):
                self.env.update(TEST_STATUS=status, TEST_EXIT=exit_code)
                result = self.guard("check-published.sh")
                if expected is None:
                    self.assertNotEqual(result.returncode, 0)
                    self.assertEqual(result.stdout, "")
                else:
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(result.stdout, expected)

    def test_requires_tag(self) -> None:
        del self.env["TAG"]
        for script in ("check-release.sh", "check-published.sh"):
            with self.subTest(script=script):
                self.assertNotEqual(self.guard(script).returncode, 0)


if __name__ == "__main__":
    unittest.main()
