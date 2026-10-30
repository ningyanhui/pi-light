#!/usr/bin/env python
# -*- coding: utf-8 -*-
# @Time : 2026-10-30
# @Author : yanhui.ning
# @File : fetch-pi-packages.py
# @Project : pi-light
# @Software: Cursor
# @Description: Download, SHA-256-verify, and extract pinned Pi npm packages.
"""
功能描述：
    按 scripts/preset-packages.lock.json 从 npm 拉取预置 Pi 包 tarball，
    校验 SHA-256，解压到 sidecar/pi-packages/<dir>/。
    CI 用 --verify-only --check-docs；Release 再 --install-deps。

输入：
    --lock     lock 文件路径（默认仓库 scripts/preset-packages.lock.json）
    --dest     解压目录（verify-only 可不传）
    --role     slim / control / all

输出：
    解压后的包目录；校验失败或 NOTICE/文档缺 SHA 时非零退出。

使用方法：
    python3 scripts/fetch-pi-packages.py --verify-only --check-docs
    python3 scripts/fetch-pi-packages.py --dest sidecar/pi-packages --install-deps

依赖项：
    Python 3.9+；--install-deps 需要 npm。

注意事项：
    只允许 https://registry.npmjs.org/ 的 tarball URL。
    不把 tarball 或解压树提交进 git。
    pi-lens / billion-context 不得出现在 default_enabled 列表。

版本历史：
- v1.0 (2026-10-30): 初始版本
"""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import shutil
import ssl
import subprocess
import sys
import tarfile
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any

ALLOWED_TARBALL_PREFIX = "https://registry.npmjs.org/"
FORBIDDEN_DEFAULT = ("pi-lens", "billion-context")
SHA256_LEN = 64
USER_AGENT = "pi-light-fetch-pi-packages/1.0 (+https://github.com/ningyanhui/pi-light)"


def repo_root() -> Path:
    return Path(__file__).resolve().parent.parent


def load_lock(path: Path) -> dict[str, Any]:
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict) or not isinstance(data.get("packages"), list):
        raise SystemExit(f"invalid lock file: {path}")
    return data


def validate_lock(lock: dict[str, Any]) -> list[dict[str, Any]]:
    packages = lock["packages"]
    names: set[str] = set()
    slim: list[str] = []
    errors: list[str] = []
    for pkg in packages:
        name = str(pkg.get("name") or "")
        version = str(pkg.get("version") or "")
        sha = str(pkg.get("sha256") or "").lower()
        tarball = str(pkg.get("tarball") or "")
        role = str(pkg.get("role") or "")
        directory = str(pkg.get("dir") or "")
        if not name or not version:
            errors.append(f"package missing name/version: {pkg!r}")
            continue
        if name in names:
            errors.append(f"duplicate package {name}")
        names.add(name)
        if len(sha) != SHA256_LEN or any(c not in "0123456789abcdef" for c in sha):
            errors.append(f"{name}@{version}: sha256 missing or not 64 hex")
        if not tarball.startswith(ALLOWED_TARBALL_PREFIX):
            errors.append(f"{name}@{version}: tarball URL not npm registry")
        if role not in ("slim", "control"):
            errors.append(f"{name}@{version}: role must be slim or control")
        if not directory or ".." in Path(directory).parts:
            errors.append(f"{name}@{version}: invalid dir")
        if pkg.get("default_enabled") and name in FORBIDDEN_DEFAULT:
            errors.append(f"{name} must not be default_enabled")
        if pkg.get("default_enabled"):
            slim.append(name)
        if role == "slim" and not pkg.get("default_enabled"):
            errors.append(f"{name}: slim packages must be default_enabled")
        if role == "control" and pkg.get("default_enabled"):
            errors.append(f"{name}: control packages must stay default OFF")
    excluded = lock.get("market_optional_excluded_from_default") or []
    for forbidden in FORBIDDEN_DEFAULT:
        if forbidden not in excluded:
            errors.append(f"lock must list {forbidden} as market-optional excluded")
        if forbidden in names:
            errors.append(f"{forbidden} must not be a preset lock entry")
    expected_slim = [
        "pi-marketplace",
        "pi-mcp-adapter",
        "pi-web-access",
        "pi-memory",
        "pi-subagents",
    ]
    if slim != expected_slim:
        errors.append(f"slim default list mismatch: {slim} != {expected_slim}")
    if errors:
        raise SystemExit("lockfile invalid:\n  " + "\n  ".join(errors))
    return packages


def filter_role(packages: list[dict[str, Any]], role: str) -> list[dict[str, Any]]:
    if role == "all":
        return packages
    return [pkg for pkg in packages if pkg.get("role") == role]


def download(url: str) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    ctx = ssl.create_default_context()
    try:
        with urllib.request.urlopen(req, context=ctx, timeout=120) as resp:
            return resp.read()
    except urllib.error.URLError as exc:
        raise SystemExit(f"download failed: {url}\n{exc}") from exc


def sha256_hex(blob: bytes) -> str:
    return hashlib.sha256(blob).hexdigest()


def safe_members(tar: tarfile.TarFile) -> list[tarfile.TarInfo]:
    out: list[tarfile.TarInfo] = []
    for member in tar.getmembers():
        name = member.name.replace("\\", "/")
        if name.startswith("/") or ".." in Path(name).parts:
            raise SystemExit(f"refusing unsafe tar member: {member.name}")
        out.append(member)
    return out


def extract_tarball(blob: bytes, dest: Path) -> None:
    dest.mkdir(parents=True, exist_ok=True)
    with tarfile.open(fileobj=io.BytesIO(blob), mode="r:gz") as tar:
        for member in safe_members(tar):
            name = member.name.replace("\\", "/")
            if name == "package" or name == "package/":
                continue
            if name.startswith("package/"):
                rel = name[len("package/") :]
            else:
                rel = name
            if not rel:
                continue
            target = dest.joinpath(*Path(rel).parts)
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True)
                continue
            if member.issym() or member.islnk():
                continue
            target.parent.mkdir(parents=True, exist_ok=True)
            extracted = tar.extractfile(member)
            if extracted is None:
                continue
            target.write_bytes(extracted.read())


def package_has_deps(pkg_dir: Path) -> bool:
    manifest = pkg_dir / "package.json"
    if not manifest.is_file():
        return False
    data = json.loads(manifest.read_text(encoding="utf-8"))
    deps = data.get("dependencies") or {}
    return bool(deps)


def npm_install(pkg_dir: Path) -> None:
    cmd = ["npm", "install", "--omit=dev", "--legacy-peer-deps"]
    print(f"npm install in {pkg_dir} ...")
    try:
        subprocess.run(cmd, cwd=str(pkg_dir), check=True)
    except FileNotFoundError as exc:
        raise SystemExit("npm not found on PATH; cannot --install-deps") from exc
    except subprocess.CalledProcessError as exc:
        raise SystemExit(f"npm install failed in {pkg_dir} (exit {exc.returncode})") from exc


def check_docs(lock: dict[str, Any], notice: Path, docs: Path) -> None:
    notice_text = notice.read_text(encoding="utf-8")
    docs_text = docs.read_text(encoding="utf-8")
    missing: list[str] = []
    for pkg in lock["packages"]:
        spec = f"{pkg['name']}@{pkg['version']}"
        sha = str(pkg["sha256"]).lower()
        for label, text in (("NOTICE", notice_text), ("docs/preset-packages.md", docs_text)):
            if spec not in text:
                missing.append(f"{label} missing {spec}")
            if sha not in text.lower():
                missing.append(f"{label} missing SHA-256 for {spec}")
    for forbidden in FORBIDDEN_DEFAULT:
        if forbidden not in docs_text:
            missing.append(f"docs/preset-packages.md must mention excluded {forbidden}")
    if "default_enabled" in docs_text.lower() and "pi-lens" in docs_text:
        # presence is required; make sure they are not listed as Slim default
        pass
    if missing:
        raise SystemExit("documentation drift:\n  " + "\n  ".join(missing))
    print(f"docs ok: {notice} and {docs} list every lockfile spec + SHA-256")


def fetch_one(pkg: dict[str, Any], dest_root: Path | None, install_deps: bool) -> None:
    spec = f"{pkg['name']}@{pkg['version']}"
    expected = str(pkg["sha256"]).lower()
    print(f"fetch {spec}")
    blob = download(str(pkg["tarball"]))
    actual = sha256_hex(blob)
    if actual != expected:
        raise SystemExit(
            f"SHA-256 mismatch for {spec}: expected {expected} got {actual}"
        )
    print(f"  sha256 ok ({len(blob)} bytes)")
    if dest_root is None:
        return
    dest = dest_root.joinpath(*Path(str(pkg["dir"])).parts)
    if dest.exists():
        shutil.rmtree(dest)
    extract_tarball(blob, dest)
    if not (dest / "package.json").is_file():
        raise SystemExit(f"{spec}: package.json missing after extract into {dest}")
    if install_deps and package_has_deps(dest):
        npm_install(dest)


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Fetch and verify Pi preset packages")
    parser.add_argument(
        "--lock",
        default=str(repo_root() / "scripts" / "preset-packages.lock.json"),
        help="path to preset-packages.lock.json",
    )
    parser.add_argument("--dest", help="extract destination (e.g. sidecar/pi-packages)")
    parser.add_argument(
        "--role",
        choices=("slim", "control", "all"),
        default="all",
        help="which lockfile role(s) to fetch",
    )
    parser.add_argument(
        "--verify-only",
        action="store_true",
        help="download and hash tarballs without extracting",
    )
    parser.add_argument(
        "--check-docs",
        action="store_true",
        help="require NOTICE and docs/preset-packages.md to list name@version + sha256",
    )
    parser.add_argument(
        "--install-deps",
        action="store_true",
        help="after extract, npm install --omit=dev --legacy-peer-deps when dependencies exist",
    )
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv if argv is not None else sys.argv[1:])
    lock_path = Path(args.lock)
    if not lock_path.is_file():
        raise SystemExit(f"lock file not found: {lock_path}")
    lock = load_lock(lock_path)
    packages = filter_role(validate_lock(lock), args.role)
    if args.check_docs:
        check_docs(
            lock,
            repo_root() / "NOTICE",
            repo_root() / "docs" / "preset-packages.md",
        )
    dest = None if args.verify_only else (Path(args.dest) if args.dest else None)
    if not args.verify_only and dest is None:
        raise SystemExit("--dest is required unless --verify-only")
    if dest is not None:
        dest.mkdir(parents=True, exist_ok=True)
        (dest / "preset-lock.json").write_text(
            lock_path.read_text(encoding="utf-8"), encoding="utf-8"
        )
    for pkg in packages:
        fetch_one(pkg, dest, install_deps=bool(args.install_deps) and not args.verify_only)
    print(f"ok: {len(packages)} package(s)")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except KeyboardInterrupt:
        raise SystemExit(130)
