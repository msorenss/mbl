"""Versioned release preparation and publication guards (stdlib + GitHub CLI)."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time
import tomllib


def gh(endpoint, missing=False):
    result = subprocess.run(["gh", "api", endpoint], capture_output=True, text=True)
    if result.returncode:
        if missing and "HTTP 404" in result.stderr:
            return None
        raise RuntimeError(result.stderr.strip())
    return json.loads(result.stdout)


def get_release(repo, tag):
    # The tag endpoint may omit drafts; authenticated release listings include them.
    found = gh(f"repos/{repo}/releases/tags/{tag}", missing=True)
    if found is not None:
        return found
    for page in range(1, 11):
        releases = gh(f"repos/{repo}/releases?per_page=100&page={page}")
        for release in releases:
            if release["tag_name"] == tag:
                return release
        if len(releases) < 100:
            return None
    raise RuntimeError("Release listing is too large to resolve this draft safely")


def tag_for(version, ref_type, ref_name, requested=""):
    tag = requested or (ref_name if ref_type == "tag" else f"v{version}")
    if not re.fullmatch(r"v\d+\.\d+\.\d+", tag) or tag != f"v{version}":
        raise ValueError("Release tag must match the workspace version (vX.Y.Z)")
    if ref_name.startswith("codex/release-") and ref_name != f"codex/release-{tag}":
        raise ValueError("Release branch and workspace version differ")
    return tag


def tag_state(repo, tag):
    ref = gh(f"repos/{repo}/git/ref/tags/{tag}", missing=True)
    if ref is None:
        return "", ""
    value = ref["object"]
    original = value["sha"]
    for _ in range(8):
        if value["type"] == "commit":
            return original, value["sha"]
        if value["type"] != "tag":
            break
        value = gh(f"repos/{repo}/git/tags/{value['sha']}")["object"]
    raise RuntimeError("Release tag does not resolve to a commit")


def check_release(release, target, source):
    if release and not release["draft"] and target != source:
        raise RuntimeError("A published release already uses this version; do not retag it")


def prepare():
    version = tomllib.loads(Path("Cargo.toml").read_text())["workspace"]["package"]["version"]
    tag = tag_for(version, os.environ["GITHUB_REF_TYPE"], os.environ["GITHUB_REF_NAME"],
                  os.environ.get("RELEASE_REQUESTED_TAG", ""))
    repo = os.environ["GITHUB_REPOSITORY"]
    source = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    expected, target = tag_state(repo, tag)
    check_release(get_release(repo, tag), target, source)
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as out:
        for key, value in {"tag": tag, "version": version, "sha": source,
                           "expected_tag_sha": expected}.items():
            print(f"{key}={value}", file=out)
    print(f"Preparing {tag} from {source}")


def verify_ci():
    repo, source = os.environ["GITHUB_REPOSITORY"], os.environ["RELEASE_SHA"]
    deadline = time.monotonic() + 1200
    while time.monotonic() < deadline:
        runs = gh(f"repos/{repo}/actions/runs?head_sha={source}&event=push&per_page=30")
        candidates = [r for r in runs["workflow_runs"] if r["name"] == "CI" and r["head_branch"] == "main"]
        if candidates:
            run = max(candidates, key=lambda r: r["id"])
            if run["status"] == "completed":
                if run["conclusion"] != "success":
                    raise RuntimeError(f"CI must pass before packaging: {run['html_url']}")
                print(f"Verified green CI: {run['html_url']}")
                return
        print("Waiting for CI on the exact release commit...", flush=True)
        time.sleep(20)
    raise RuntimeError("Timed out waiting for CI on the release commit")


def publish_tag():
    repo, source, tag = (os.environ[k] for k in ("GITHUB_REPOSITORY", "RELEASE_SHA", "RELEASE_TAG"))
    current, target = tag_state(repo, tag)
    check_release(get_release(repo, tag), target, source)
    if target == source:
        print(f"{tag} already points to this commit")
        return
    if current != os.environ["EXPECTED_TAG_SHA"]:
        raise RuntimeError("Tag changed during packaging; refusing to overwrite it")
    subprocess.run(["git", "config", "user.name", "github-actions[bot]"], check=True)
    subprocess.run(["git", "config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com"], check=True)
    subprocess.run(["git", "tag", "-fa", tag, "-m", f"Release {tag}", source], check=True)
    subprocess.run(["git", "push", f"--force-with-lease=refs/tags/{tag}:{current}",
                    "origin", f"refs/tags/{tag}:refs/tags/{tag}"], check=True)


def verify_linux():
    for name in ("mbl-server", "mbl-worker", "mbl-node"):
        binary = Path("target/release") / name
        output = subprocess.check_output(["readelf", "--version-info", str(binary)], text=True)
        versions = [tuple(map(int, v.split("."))) for v in re.findall(r"Name: GLIBC_(\d+(?:\.\d+)+)", output)]
        if versions and max(versions) > (2, 36):
            raise RuntimeError(f"{name} requires glibc newer than Debian Bookworm: {max(versions)}")
        print(f"Verified {name}: maximum glibc symbol {max(versions) if versions else 'none'}")


def prepare_assets(directory, tag, notes):
    version = tag.removeprefix("v")
    renames = {"dist-windows.zip": f"mbl-{tag}-windows-x86_64.zip",
               "dist-windows.msi": f"mbl-{version}-windows-x86_64.msi",
               "dist-worker-windows.zip": f"mbl-worker-{tag}-windows-x86_64.zip",
               "dist-node-windows.zip": f"mbl-node-{tag}-windows-x86_64.zip"}
    expected = set(renames.values())
    for arch, deb_arch in (("x86_64", "amd64"), ("arm64", "arm64")):
        for name in ("mbl", "mbl-worker", "mbl-node"):
            renames[f"{name}-linux-{arch}.tar.gz"] = f"{name}-{tag}-linux-{arch}.tar.gz"
            expected.add(f"{name}-{tag}-linux-{arch}.tar.gz")
            expected.add(f"{name}_{version}-1_{deb_arch}.deb")
    for before, after in renames.items():
        if (directory / after).exists():
            raise RuntimeError(f"Duplicate release artifact: {after}")
        (directory / before).rename(directory / after)
    actual = {p.name for p in directory.iterdir()}
    if actual != expected or any(not p.is_file() or p.stat().st_size == 0 for p in directory.iterdir()):
        raise RuntimeError(f"Unexpected/missing release artifacts: {actual ^ expected}")
    checksums = []
    for name in sorted(expected):
        with (directory / name).open("rb") as source:
            digest = hashlib.file_digest(source, "sha256").hexdigest()
        checksums.append(f"{digest}  {name}\n")
    (directory / "SHA256SUMS").write_text("".join(checksums))
    print("".join(checksums), end="")
    text = Path("CHANGELOG.md").read_text()
    section = text.split(f"## [{version}] - ", 1)[1].split("\n## [", 1)[0]
    body = section.split("\n", 1)[1].strip()
    notes.write_text(f"# MBL {tag}\n\n{body}\n")


def verify_uploaded(directory, assets):
    remote = {asset["name"]: asset for asset in assets}
    for file in directory.iterdir():
        asset = remote.get(file.name)
        if not asset or asset["state"] != "uploaded" or asset["size"] != file.stat().st_size:
            raise RuntimeError(f"Release upload is missing or incomplete: {file.name}")
        with file.open("rb") as source:
            digest = "sha256:" + hashlib.file_digest(source, "sha256").hexdigest()
        if asset.get("digest") and asset["digest"] != digest:
            raise RuntimeError(f"Release upload checksum mismatch: {file.name}")


def publish_assets():
    repo, source, tag = (os.environ[k] for k in ("GITHUB_REPOSITORY", "RELEASE_SHA", "RELEASE_TAG"))
    _, target = tag_state(repo, tag)
    if target != source:
        raise RuntimeError("Release tag does not match the packaged source commit")
    existing = get_release(repo, tag)
    check_release(existing, target, source)
    common = [tag, "--repo", repo, "--title", f"MBL {tag}", "--notes-file", os.environ["RELEASE_NOTES"]]
    if existing is None:
        subprocess.run(["gh", "release", "create", *common, "--verify-tag", "--draft"], check=True)
    else:
        subprocess.run(["gh", "release", "edit", *common], check=True)
    directory = Path("release-dist")
    subprocess.run(["gh", "release", "upload", tag, "--repo", repo, "--clobber",
                    *(str(p) for p in sorted(directory.iterdir()))], check=True)
    uploaded = get_release(repo, tag)
    verify_uploaded(directory, uploaded["assets"])
    subprocess.run(["gh", "release", "edit", tag, "--repo", repo,
                    "--draft=false", "--prerelease=false", "--latest"], check=True)
    published = get_release(repo, tag)
    if published["draft"]:
        raise RuntimeError("Release is still a draft after publication")
    print(f"Published complete release: {published['html_url']}")


if __name__ == "__main__":
    commands = {"prepare": prepare, "verify-ci": verify_ci, "publish-tag": publish_tag, "verify-linux": verify_linux, "publish-assets": publish_assets}
    if sys.argv[1] == "assets":
        prepare_assets(Path("release-dist"), os.environ["RELEASE_TAG"], Path(os.environ["RELEASE_NOTES"]))
    else:
        commands[sys.argv[1]]()
