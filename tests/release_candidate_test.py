"""Offline release-promotion checks. No credentials or live releases needed."""

import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
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
                    patch.object(candidate, "check_remote", return_value={"id": 1, "draft": True}), \
                    patch.object(candidate, "check_tag", return_value="c" * 40), \
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
                    write.assert_not_called()


class CandidateProtocolTests(unittest.TestCase):
    """Run the real CLI entry point with real Git and a local fake forge."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.checkout = self.root / "checkout"
        self.checkout.mkdir()
        self.remote = self.root / "origin.git"
        subprocess.run(["git", "init", "--bare", "-q", str(self.remote)], check=True)
        self.git("init", "--initial-branch=main", "-q")
        self.git("config", "user.name", "Test")
        self.git("config", "user.email", "test@example.invalid")
        self.git("remote", "add", "origin", str(self.remote))
        (self.checkout / "Cargo.toml").write_text('[package]\nversion = "1.5.1"\n')
        (self.checkout / "source.rs").write_text("build input")
        self.commit("base")
        self.git("push", "-q", "origin", "main")
        self.git("checkout", "-qb", "release-plz")
        (self.checkout / "Cargo.toml").write_text('[package]\nversion = "1.6.0"\n')
        self.source = self.commit("release candidate")
        self.git("push", "-q", "origin", "release-plz")
        self.dist = self.root / "candidate-files"
        self.dist.mkdir()
        for name in candidate.names("1.6.0"):
            (self.dist / name).write_bytes(f"original {name}".encode())
        bin_dir = self.root / "bin"
        bin_dir.mkdir()
        fixture = Path(__file__).parent / "fixtures/release-candidate/gh.py"
        shutil.copyfile(fixture, bin_dir / "gh")
        (bin_dir / "gh").chmod(0o755)
        self.state_file = self.root / "forge.json"
        self.state_file.write_text(json.dumps({"calls": [], "release": None, "assets": {},
                                               "latest": "v1.5.1", "run": self.build_run(self.source)}))
        self.environment = os.environ | {
            "PATH": str(bin_dir) + os.pathsep + os.environ["PATH"],
            "FAKE_GH_STATE": str(self.state_file), "GITHUB_REPOSITORY": candidate.REPO,
            "GITHUB_REF": "refs/heads/release-plz", "GITHUB_SHA": self.source,
            "GITHUB_RUN_ID": "42", "GITHUB_ENV": str(self.root / "github-env"),
            "GITHUB_OUTPUT": str(self.root / "github-output"),
        }

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.checkout, text=True).strip()

    def commit(self, message):
        self.git("add", ".")
        self.git("commit", "-qm", message)
        return self.git("rev-parse", "HEAD")

    def build_run(self, source):
        return {"head_sha": source, "event": "push", "head_branch": "release-plz", "conclusion": "success",
                "head_repository": {"full_name": candidate.REPO}, "workflow_id": 3}

    def state(self):
        return json.loads(self.state_file.read_text())

    def write_state(self, data):
        self.state_file.write_text(json.dumps(data))

    def cli(self, command, *args, success=True):
        result = subprocess.run([sys.executable, str(SPEC.origin), command, *args], cwd=self.checkout,
                                env=self.environment, text=True, capture_output=True)
        self.assertEqual(result.returncode == 0, success, result.stdout + result.stderr)
        return result

    def stage(self):
        self.cli("stage", "--directory", str(self.dist))
        self.commit("release digest map")
        self.git("push", "-q", "origin", "release-plz")

    def merge(self):
        self.git("checkout", "-q", "main")
        self.git("merge", "--squash", "release-plz")
        final = self.commit("chore: release v1.6.0")
        self.git("tag", "v1.6.0")
        self.git("push", "-q", "origin", "main", "v1.6.0")
        self.environment["GITHUB_REF"] = "refs/tags/v1.6.0"
        self.environment["GITHUB_SHA"] = final
        return final

    def test_stage_merge_promote_keeps_exact_bytes_and_candidate_source(self):
        self.assertEqual(self.cli("mode").stdout.strip(), "build")
        self.stage()
        self.assertFalse(self.git("ls-remote", "origin", "refs/tags/v1.6.0"))
        self.assertEqual(self.cli("mode").stdout.strip(), "ready")
        self.cli("check", "--remote")
        final = self.merge()
        self.assertNotEqual(final, self.source)
        # Squash merge preserves the tree proof without pretending B == T.
        ancestry = subprocess.run(["git", "merge-base", "--is-ancestor", self.source, final], cwd=self.checkout)
        self.assertNotEqual(ancestry.returncode, 0)
        self.cli("gate")
        target = self.root / "downloaded"
        self.cli("download", "--directory", str(target))
        self.assertEqual({file.name: file.read_bytes() for file in target.iterdir()},
                         {file.name: file.read_bytes() for file in self.dist.iterdir()})
        state = self.state()
        self.assertTrue(state["release"]["draft"])
        # GitHub ignores target_commitish once the tag exists. Promotion
        # relies on the real tag/source proof even if this field still says B.
        self.assertEqual(state["release"]["target_commitish"], self.source)
        self.assertIn(self.source, (self.root / "github-env").read_text())
        self.assertEqual(len([call for call in state["calls"] if call[:2] == ["attestation", "verify"]]), 18)
        self.assertEqual(self.git("rev-list", "-n1", "v1.6.0"), final)

    def test_input_refresh_invalidates_then_restages_same_version(self):
        self.stage()
        (self.checkout / "source.rs").write_text("new build input")
        self.commit("changed input")
        self.assertEqual(self.cli("mode").stdout.strip(), "build")
        self.cli("check", success=False)
        (self.checkout / str(candidate.LOCK)).unlink()
        new_source = self.commit("refresh without stale map")
        self.git("push", "-q", "origin", "release-plz")
        self.environment["GITHUB_SHA"] = new_source
        data = self.state()
        data["run"] = self.build_run(new_source)
        self.write_state(data)
        for file in self.dist.iterdir():
            file.write_bytes(f"refreshed {file.name}".encode())
        self.stage()
        self.cli("check", "--remote")
        metadata = json.loads((self.checkout / str(candidate.LOCK)).read_text())
        self.assertEqual(metadata["source_commit"], new_source)
        self.assertEqual(self.state()["release"]["target_commitish"], new_source)

    def test_asset_replacement_during_download_fails_before_attestation(self):
        self.stage()
        self.merge()
        data = self.state()
        # Forge metadata still advertises the signed hash, but download bytes
        # differ: the local digest must catch this before any verifier/execution.
        data["assets"]["install.sh"]["content"] = b"tampered download".hex()
        before = data["release"]["target_commitish"]
        self.write_state(data)
        self.cli("download", "--directory", str(self.root / "downloaded"), success=False)
        data = self.state()
        self.assertEqual(data["release"]["target_commitish"], before)
        self.assertFalse([call for call in data["calls"] if call[:2] == ["attestation", "verify"]])

    def test_failure_after_publication_resumes_without_public_asset_writes(self):
        self.stage()
        final = self.merge()
        target = self.root / "downloaded"
        self.cli("download", "--directory", str(target))
        data = self.state()
        data["release"]["draft"] = False
        data["latest"] = "v1.6.0"
        data["fail_tag_update"] = True
        self.write_state(data)
        original_assets = data["assets"]
        self.cli("finish", "--directory", str(target), success=False)
        self.assertIsNone(self.state().get("major"))
        previous_calls = len(self.state()["calls"])
        retry = self.root / "retry-download"
        self.cli("download", "--directory", str(retry))
        self.assertIn("CANDIDATE_PUBLISHED=true", (self.root / "github-env").read_text())
        self.cli("finish", "--directory", str(retry))
        self.cli("finish", "--directory", str(retry))
        data = self.state()
        self.assertEqual(data["major"]["sha"], final)
        self.assertIn("current-major=true", (self.root / "github-output").read_text())
        self.assertIn("current-latest=true", (self.root / "github-output").read_text())
        self.assertEqual(data["assets"], original_assets)
        self.assertFalse(data["release"]["draft"])
        for call in data["calls"][previous_calls:]:
            self.assertNotEqual(call[:2], ["release", "upload"])
            self.assertFalse(call[0] == "api" and "/releases/1" in call and "PATCH" in call)
        # A much later retry of an old version must not downgrade v1.
        data["major"] = {"sha": "f" * 40, "version": "1.7.0"}
        data["latest"] = "v1.7.0"
        self.write_state(data)
        self.cli("finish", "--directory", str(retry))
        self.assertEqual(self.state()["major"]["sha"], "f" * 40)
        self.assertTrue((self.root / "github-output").read_text().endswith("current-major=false\ncurrent-latest=false\n"))

    def test_failed_downstream_only_retry_rechecks_after_newer_publication(self):
        self.stage()
        self.merge()
        data = self.state()
        data["release"]["draft"] = False
        data["latest"] = "v1.6.0"
        self.write_state(data)
        self.cli("freshness", "--tag", "v1.6.0")
        self.assertTrue((self.root / "github-output").read_text().endswith("current-major=true\ncurrent-latest=true\n"))
        # No rerun of release/finish: failed downstream jobs retain old needs
        # outputs. Even a stale GitHub latest flag must not enable aliases.
        data = self.state()
        data["published"] = ["v1.5.1", "v1.7.0", "v2.0.0"]
        self.write_state(data)
        self.cli("freshness", "--tag", "v1.6.0")
        self.assertTrue((self.root / "github-output").read_text().endswith("current-major=false\ncurrent-latest=false\n"))

    def test_old_draft_promotion_never_resets_latest(self):
        self.stage()
        self.merge()
        self.cli("download", "--directory", str(self.root / "downloaded"))
        data = self.state()
        data["published"] = ["v1.5.1", "v1.7.0"]
        data["latest"] = "v1.7.0"
        self.write_state(data)
        self.cli("publish")
        data = self.state()
        self.assertFalse(data["release"]["draft"])
        self.assertEqual(data["latest"], "v1.7.0")
        self.assertTrue(any("make_latest=false" in call for call in data["calls"]))

    def test_current_draft_promotion_explicitly_sets_latest(self):
        self.stage()
        self.merge()
        self.cli("publish")
        self.assertEqual(self.state()["latest"], "v1.6.0")
        self.assertTrue(any("make_latest=true" in call for call in self.state()["calls"]))

    def test_existing_tag_gate_disables_publishing_on_later_main_push(self):
        self.stage()
        self.merge()
        (self.checkout / "source.rs").write_text("unreleased change while draft remains")
        self.commit("next main input")
        self.environment["GITHUB_REF"] = "refs/heads/main"
        self.cli("gate")
        self.assertTrue(self.state()["release"]["draft"])
        self.assertTrue((self.root / "github-output").read_text().endswith("publish=false\n"))

    def test_published_release_replaced_bytes_or_changed_tag_cannot_resume(self):
        self.stage()
        final = self.merge()
        self.cli("download", "--directory", str(self.root / "first"))
        data = self.state()
        data["release"]["draft"] = False
        self.write_state(data)
        self.git("tag", "-f", "v1.6.0", self.source)
        self.git("push", "-q", "--force", "origin", "v1.6.0")
        self.cli("download", "--directory", str(self.root / "wrong-tag"), success=False)
        self.git("tag", "-f", "v1.6.0", final)
        self.git("push", "-q", "--force", "origin", "v1.6.0")
        data["assets"]["install.sh"]["content"] = b"tampered".hex()
        self.write_state(data)
        self.cli("download", "--directory", str(self.root / "tampered"), success=False)
        self.assertIsNone(self.state().get("major"))

    def test_queued_old_candidate_cannot_stage_over_refreshed_remote(self):
        (self.checkout / "source.rs").write_text("newer queued input")
        self.commit("newer candidate")
        self.git("push", "-q", "origin", "release-plz")
        self.git("checkout", "-q", self.source)
        self.cli("stage", "--directory", str(self.dist), success=False)
        self.assertIsNone(self.state()["release"])
        self.assertFalse(self.state()["assets"])

    def test_ready_check_needs_no_draft_listing_or_write_access(self):
        self.stage()
        before = len(self.state()["calls"])
        self.environment["FAKE_GH_READ_ONLY"] = "1"
        self.cli("check", "--build-run")
        self.assertFalse([call for call in self.state()["calls"][before:] if any("/releases" in arg for arg in call)])

    def test_image_downloads_require_original_candidate_bytes_and_provenance(self):
        self.stage()
        self.merge()
        target = self.root / "image-files"
        target.mkdir()
        for arch in ("x64", "arm64"):
            name = f"packslip-v1.6.0-linux-{arch}"
            shutil.copyfile(self.dist / name, target / name)
        self.cli("verify-image", "--directory", str(target))
        before = len(self.state()["calls"])
        (target / "packslip-v1.6.0-linux-x64").write_bytes(b"replaced image input")
        self.cli("verify-image", "--directory", str(target), success=False)
        self.assertFalse([call for call in self.state()["calls"][before:] if call[:2] == ["attestation", "verify"]])


if __name__ == "__main__":
    unittest.main()
