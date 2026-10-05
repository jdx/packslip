"""Offline release-promotion checks. No credentials or live releases needed."""

import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    "candidate", Path(__file__).resolve().parents[1] / "scripts/release-candidate.py"
)
candidate = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(candidate)


class CandidateTests(unittest.TestCase):
    def setUp(self):
        self.data = {"schema": 1, "version": "1.6.0", "source_commit": "a" * 40, "run_id": 42,
                     "assets": {name: {"sha256": "b" * 64, "size": 17} for name in candidate.names("1.6.0")}}

    def test_exact_inventory_and_valid_metadata(self):
        self.assertEqual(len(self.data["assets"]), 18)
        self.assertEqual(candidate.validate(self.data, "1.6.0"), self.data)

    def test_bad_version_source_digest_size_and_missing_platform_fail(self):
        changes = ({"version": "1.5.1"}, {"source_commit": "bad"}, {"run_id": True}, {"schema": 2})
        for change in changes:
            with self.subTest(change=change), self.assertRaises(ValueError):
                candidate.validate(self.data | change, "1.6.0")
        for change in ({"sha256": "B" * 64}, {"size": 0}, {"size": True}):
            data = copy.deepcopy(self.data)
            data["assets"][next(iter(data["assets"]))].update(change)
            with self.subTest(change=change), self.assertRaises(ValueError):
                candidate.validate(data, "1.6.0")
        self.data["assets"].pop("packslip-v1.6.0-windows-arm64.zip")
        with self.assertRaises(ValueError):
            candidate.validate(self.data, "1.6.0")

    def test_duplicate_metadata_keys_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            file = Path(directory) / "release.json"
            file.write_text('{"schema":1,"schema":1}')
            with patch.object(candidate, "LOCK", file), patch.object(candidate, "version", return_value="1.6.0"):
                with self.assertRaisesRegex(ValueError, "duplicate"):
                    candidate.load()

    def test_remote_asset_tamper_missing_extra_and_duplicate_fail(self):
        assets = [{"name": name, "digest": "sha256:" + item["sha256"], "size": item["size"]}
                  for name, item in self.data["assets"].items()]
        with patch.object(candidate, "api", return_value=assets):
            candidate.check_assets(self.data, {"id": 1})
        modified = copy.deepcopy(assets)
        modified[0]["digest"] = "sha256:" + "c" * 64
        for broken in (modified, assets[:-1], assets + [assets[0]], [assets[0]] * 18):
            with patch.object(candidate, "api", return_value=broken), self.assertRaises(ValueError):
                candidate.check_assets(self.data, {"id": 1})

    def test_retry_bundle_is_ignored_but_unexpected_assets_are_not(self):
        assets = [{"name": name, "digest": "sha256:" + item["sha256"], "size": item["size"]}
                  for name, item in self.data["assets"].items()]
        with patch.object(candidate, "api", return_value=assets + [{"name": "packslip.sigstore.json"}]):
            candidate.check_assets(self.data, {"id": 1}, allow_bundle=True)
        with patch.object(candidate, "api", return_value=assets + [{"name": "unexpected"}]), self.assertRaises(ValueError):
            candidate.check_assets(self.data, {"id": 1}, allow_bundle=True)

    def test_published_release_cannot_be_promoted(self):
        with patch.object(candidate, "check_run"), patch.object(candidate, "release", return_value={"draft": False}):
            with self.assertRaisesRegex(ValueError, "unpublished draft"):
                candidate.check_remote(self.data)

    def test_stale_branch_and_existing_tag_hold_staging_before_any_release_write(self):
        with tempfile.TemporaryDirectory() as directory:
            for name in self.data["assets"]:
                (Path(directory) / name).write_bytes(b"candidate")
            environment = {"GITHUB_REPOSITORY": candidate.REPO, "GITHUB_REF": "refs/heads/release-plz",
                           "GITHUB_SHA": "a" * 40, "GITHUB_RUN_ID": "42"}
            with patch.dict(os.environ, environment), patch.object(candidate, "version", return_value="1.6.0"):
                with patch.object(candidate, "run", side_effect=["a" * 40, "b" * 40 + " refs/heads/release-plz"]), patch.object(candidate, "release") as query:
                    with self.assertRaisesRegex(ValueError, "branch changed"):
                        candidate.stage(directory)
                    query.assert_not_called()
                with patch.object(candidate, "run", side_effect=["a" * 40, "a" * 40 + " refs/heads/release-plz", "c" * 40]), patch.object(candidate, "release") as query:
                    with self.assertRaisesRegex(ValueError, "version tag already"):
                        candidate.stage(directory)
                    query.assert_not_called()

    def test_failed_wrong_source_wrong_workflow_and_fork_runs_fail(self):
        build = {"head_sha": "a" * 40, "event": "push", "head_branch": "release-plz",
                 "conclusion": "success", "head_repository": {"full_name": candidate.REPO}, "workflow_id": 3}
        workflow = {"path": ".github/workflows/release.yml"}
        with patch.object(candidate, "api", side_effect=[build, workflow]):
            candidate.check_run(self.data)
        for change in ({"head_sha": "c" * 40}, {"event": "pull_request"}, {"conclusion": "failure"},
                       {"head_branch": "main"}, {"head_repository": {"full_name": "fork/packslip"}}):
            with patch.object(candidate, "api", side_effect=[build | change, workflow]), self.assertRaises(ValueError):
                candidate.check_run(self.data)
        with patch.object(candidate, "api", side_effect=[build, {"path": "other.yml"}]), self.assertRaises(ValueError):
            candidate.check_run(self.data)

    def test_actual_git_tree_only_metadata_change_is_allowed(self):
        with tempfile.TemporaryDirectory() as directory:
            previous = os.getcwd()
            os.chdir(directory)
            try:
                def git(*args):
                    return subprocess.check_output(["git", *args], text=True).strip()

                git("init", "-q")
                git("config", "user.name", "Test")
                git("config", "user.email", "test@example.invalid")
                Path("Cargo.toml").write_text('[package]\nversion = "1.6.0"\n')
                Path("source.rs").write_text("original")
                git("add", ".")
                git("commit", "-qm", "candidate")
                self.data["source_commit"] = git("rev-parse", "HEAD")
                candidate.LOCK.parent.mkdir()
                candidate.LOCK.write_text(json.dumps(self.data))
                git("add", ".")
                git("commit", "-qm", "lock")
                candidate.check_tree(self.data)
                Path("source.rs").write_text("new build inputs")
                git("add", ".")
                git("commit", "-qm", "changed source")
                with self.assertRaisesRegex(ValueError, "ONLY"):
                    candidate.check_tree(self.data)
            finally:
                os.chdir(previous)

    def test_local_bytes_are_checked_and_symlinks_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in self.data["assets"]:
                (root / name).write_bytes(b"candidate")
            original = candidate.files(root, "1.6.0")
            (root / "install.sh").write_bytes(b"tampered")
            self.assertNotEqual(original, candidate.files(root, "1.6.0"))
            (root / "install.sh").unlink()
            (root / "install.sh").symlink_to(root / "install.ps1")
            with self.assertRaises(ValueError):
                candidate.files(root, "1.6.0")

    def test_promotion_checks_all_original_attestations_before_retargeting(self):
        self.check_promotion(fail_attestation=False)

    def test_failed_provenance_never_retargets_draft(self):
        self.check_promotion(fail_attestation=True)

    def check_promotion(self, fail_attestation):
        payload = b"candidate bytes"
        for asset in self.data["assets"].values():
            asset.update(sha256=hashlib.sha256(payload).hexdigest(), size=len(payload))
        with tempfile.TemporaryDirectory() as directory:
            dist = Path(directory) / "dist"
            calls = []

            def command(*args):
                calls.append(args)
                if args[:3] == ("gh", "release", "download"):
                    for name in self.data["assets"]:
                        (dist / name).write_bytes(payload)
                if args[:3] == ("gh", "attestation", "verify"):
                    self.assertIn("--source-digest", args)
                    self.assertIn("a" * 40, args)
                    self.assertIn("refs/heads/release-plz", args)
                    self.assertIn(candidate.WORKFLOW, args)
                    self.assertIn("--signer-digest", args)
                    self.assertIn("--deny-self-hosted-runners", args)
                    if fail_attestation:
                        raise subprocess.CalledProcessError(1, args)
                return "c" * 40 if args[0] == "git" else ""

            with patch.object(candidate, "load", return_value=self.data), patch.object(candidate, "check_tree"), \
                    patch.object(candidate, "check_remote", return_value={"id": 1}), \
                    patch.object(candidate, "run", side_effect=command), patch.object(candidate, "api") as write, \
                    patch.dict(os.environ, {"GITHUB_ENV": str(Path(directory) / "env")}):
                if fail_attestation:
                    with self.assertRaises(subprocess.CalledProcessError):
                        candidate.download(dist)
                    write.assert_not_called()
                else:
                    candidate.download(dist)
                    attestations = [call for call in calls if call[:3] == ("gh", "attestation", "verify")]
                    self.assertEqual(len(attestations), 18)
                    write.assert_called_once_with("releases/1", "-X", "PATCH", "-f", "target_commitish=" + "c" * 40)


if __name__ == "__main__":
    unittest.main()
