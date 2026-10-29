import hashlib
import contextlib
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import release


class ReleaseGuards(unittest.TestCase):
    def test_tag_and_branch_must_match_workspace_version(self):
        self.assertEqual(release.tag_for("1.1.0", "branch", "main"), "v1.1.0")
        self.assertEqual(release.tag_for("1.1.0", "branch", "codex/release-v1.1.0"), "v1.1.0")
        self.assertEqual(release.tag_for("1.1.0", "tag", "v1.1.0"), "v1.1.0")
        for kind, name, requested in [("tag", "v1.0.0", ""), ("branch", "main", "v1.2.0"),
                                       ("branch", "codex/release-v1.2.0", "")]:
            with self.assertRaises(ValueError):
                release.tag_for("1.1.0", kind, name, requested)

    def test_published_releases_cannot_be_retargeted(self):
        with self.assertRaises(RuntimeError):
            release.check_release({"draft": False}, "old", "new")
        release.check_release(None, "old", "new")
        release.check_release({"draft": True}, "old", "new")
        release.check_release({"draft": False}, "same", "same")

    def test_annotated_tag_is_peeled_and_missing_tag_is_allowed(self):
        with patch.object(release, "gh", side_effect=[{"object": {"sha": "annotation", "type": "tag"}},
                                                      {"object": {"sha": "commit", "type": "commit"}}]):
            self.assertEqual(release.tag_state("owner/repo", "v1.1.0"), ("annotation", "commit"))
        with patch.object(release, "gh", return_value=None):
            self.assertEqual(release.tag_state("owner/repo", "v1.1.0"), ("", ""))

    def test_changed_tag_aborts_publication_before_any_git_write(self):
        env = {"GITHUB_REPOSITORY": "owner/repo", "RELEASE_SHA": "new", "RELEASE_TAG": "v1.1.0", "EXPECTED_TAG_SHA": "expected"}
        with patch.dict(release.os.environ, env), patch.object(release, "tag_state", return_value=("changed", "old")), patch.object(release, "get_release", return_value=None), patch.object(release.subprocess, "run") as git:
            with self.assertRaises(RuntimeError):
                release.publish_tag()
            git.assert_not_called()

    def test_existing_draft_is_found_when_the_tag_endpoint_omits_it(self):
        draft = {"tag_name": "v1.1.0", "draft": True, "assets": []}
        with patch.object(release, "gh", side_effect=[None, [draft]]):
            self.assertEqual(release.get_release("owner/repo", "v1.1.0"), draft)

    def test_upload_must_exist_and_match_size_and_checksum(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            file = root / "package.zip"
            file.write_bytes(b"built package")
            asset = {"name": file.name, "state": "uploaded", "size": file.stat().st_size,
                     "digest": "sha256:" + hashlib.sha256(file.read_bytes()).hexdigest()}
            release.verify_uploaded(root, [asset])
            for assets in ([], [dict(asset, size=0)], [dict(asset, digest="sha256:bad")]):
                with self.assertRaises(RuntimeError):
                    release.verify_uploaded(root, assets)

    def test_asset_manifest_and_checksums_cover_all_sixteen_packages(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            for name in ("dist-windows.zip", "dist-windows.msi", "dist-worker-windows.zip", "dist-node-windows.zip"):
                (root / name).write_bytes(name.encode())
            for arch, deb_arch in (("x86_64", "amd64"), ("arm64", "arm64")):
                for name in ("mbl", "mbl-worker", "mbl-node"):
                    for filename in (f"{name}-linux-{arch}.tar.gz", f"{name}_1.1.0-1_{deb_arch}.deb"):
                        (root / filename).write_bytes(filename.encode())
            notes = root.parent / (root.name + "-notes.md")
            try:
                with contextlib.redirect_stdout(io.StringIO()):
                    release.prepare_assets(root, "v1.1.0", notes)
                sums = (root / "SHA256SUMS").read_text().splitlines()
                self.assertEqual(len(sums), 16)
                for line in sums:
                    digest, name = line.split("  ", 1)
                    self.assertEqual(hashlib.sha256((root / name).read_bytes()).hexdigest(), digest)
                self.assertIn("# MBL v1.1.0", notes.read_text())
            finally:
                notes.unlink(missing_ok=True)


if __name__ == "__main__":
    unittest.main()
