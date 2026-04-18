#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# ///
"""Bump clayers version across workspace crates, python binding, and CHANGELOG.

Updates every `version = "OLD"` line (both package versions and internal
path-dep versions on clayers-* crates) in:

  - crates/clayers/Cargo.toml
  - crates/clayers-xml/Cargo.toml
  - crates/clayers-spec/Cargo.toml
  - crates/clayers-repo/Cargo.toml
  - crates/clayers-py/Cargo.toml
  - crates/clayers-py/pyproject.toml

Promotes `## [Unreleased]` in CHANGELOG.md to `## [NEW] - TODAY`, keeps an
empty `## [Unreleased]` section on top, and adds a new compare-link at the
bottom (`[NEW]: .../compare/vOLD...vNEW`).

Then refreshes Cargo.lock (`cargo update -p ...`) and the python binding's
uv.lock (`uv lock`) unless --skip-lock is passed.

After this runs, review the diff and tag v<VERSION> to trigger the
crates.io + PyPI release workflows.

Usage:
    ./bump-version.py 0.2.0
    ./bump-version.py 0.2.0 --dry-run
    ./bump-version.py 0.2.0 --skip-lock
    ./bump-version.py 0.2.0 --no-changelog
"""

import argparse
import datetime
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent

CRATES = [
    "crates/clayers/Cargo.toml",
    "crates/clayers-xml/Cargo.toml",
    "crates/clayers-spec/Cargo.toml",
    "crates/clayers-repo/Cargo.toml",
    "crates/clayers-py/Cargo.toml",
]
PYPROJECT = "crates/clayers-py/pyproject.toml"
VERSION_TARGETS = CRATES + [PYPROJECT]
CHANGELOG = "CHANGELOG.md"

CARGO_PACKAGES = ["clayers", "clayers-xml", "clayers-spec", "clayers-repo", "clayers-py"]

SEMVER_RE = re.compile(r"^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$")


def current_version() -> str:
    text = (ROOT / CRATES[0]).read_text()
    m = re.search(r'^version\s*=\s*"([^"]+)"', text, re.MULTILINE)
    if not m:
        sys.exit(f"could not find package version in {CRATES[0]}")
    return m.group(1)


def bump_version_file(path: Path, old: str, new: str, *, dry_run: bool) -> list[str]:
    """Replace every `version = "OLD"` with `version = "NEW"`.

    Returns list of "path:lineno: content" strings. Writes unless dry_run.
    """
    pattern = re.compile(rf'(\bversion\s*=\s*)"{re.escape(old)}"')
    text = path.read_text()
    changes = [
        f"{path.relative_to(ROOT)}:{i}: {line.strip()}"
        for i, line in enumerate(text.splitlines(), 1)
        if pattern.search(line)
    ]
    if not dry_run and changes:
        path.write_text(pattern.sub(rf'\1"{new}"', text))
    return changes


def bump_changelog(path: Path, old: str, new: str, *, dry_run: bool) -> list[str]:
    """Promote `## [Unreleased]` to `## [NEW] - TODAY` and add compare link."""
    today = datetime.date.today().isoformat()
    text = path.read_text()
    changes: list[str] = []

    header_re = re.compile(r"^## \[Unreleased\]\n", re.MULTILINE)
    if not header_re.search(text):
        sys.exit(
            f"could not find `## [Unreleased]` header in {path.relative_to(ROOT)}"
        )
    text = header_re.sub(
        f"## [Unreleased]\n\n## [{new}] - {today}\n",
        text,
        count=1,
    )
    changes.append(
        f"{path.relative_to(ROOT)}: promote [Unreleased] -> [{new}] - {today}"
    )

    link_re = re.compile(
        rf"^\[Unreleased\]: (\S+)/compare/v{re.escape(old)}\.\.\.HEAD$",
        re.MULTILINE,
    )
    m = link_re.search(text)
    if not m:
        sys.exit(
            f"could not find `[Unreleased]: .../compare/v{old}...HEAD` "
            f"link in {path.relative_to(ROOT)}"
        )
    base = m.group(1)
    text = link_re.sub(
        f"[Unreleased]: {base}/compare/v{new}...HEAD\n"
        f"[{new}]: {base}/compare/v{old}...v{new}",
        text,
        count=1,
    )
    changes.append(f"{path.relative_to(ROOT)}: add compare link for v{new}")

    if not dry_run:
        path.write_text(text)
    return changes


def refresh_cargo_lock() -> None:
    cmd = ["cargo", "update", *(arg for p in CARGO_PACKAGES for arg in ("-p", p))]
    print(f"\n$ {' '.join(cmd)}")
    if subprocess.run(cmd, cwd=ROOT, check=False).returncode != 0:
        sys.exit("cargo update failed")


def refresh_uv_lock() -> None:
    py_dir = ROOT / "crates/clayers-py"
    if not (py_dir / "uv.lock").exists():
        return
    print(f"\n$ uv lock  (cwd={py_dir.relative_to(ROOT)})")
    if subprocess.run(["uv", "lock"], cwd=py_dir, check=False).returncode != 0:
        sys.exit("uv lock failed")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Bump clayers version across workspace crates, python binding, and CHANGELOG.",
    )
    parser.add_argument("new_version", help="target semver (e.g. 0.2.0)")
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="show changes without writing files or refreshing locks",
    )
    parser.add_argument(
        "--skip-lock",
        action="store_true",
        help="skip Cargo.lock and uv.lock refresh",
    )
    parser.add_argument(
        "--no-changelog",
        action="store_true",
        help="skip CHANGELOG.md update",
    )
    args = parser.parse_args()

    new = args.new_version
    if not SEMVER_RE.match(new):
        sys.exit(f"invalid version {new!r} (expected semver, e.g. 0.2.0)")

    old = current_version()
    if old == new:
        sys.exit(f"version is already {new}")

    suffix = " (dry run)" if args.dry_run else ""
    print(f"Bumping {old} -> {new}{suffix}\n")

    all_changes: list[str] = []
    for rel in VERSION_TARGETS:
        all_changes.extend(bump_version_file(ROOT / rel, old, new, dry_run=args.dry_run))

    if not all_changes:
        sys.exit(f'no `version = "{old}"` occurrences found')

    if not args.no_changelog:
        all_changes.extend(
            bump_changelog(ROOT / CHANGELOG, old, new, dry_run=args.dry_run)
        )

    for c in all_changes:
        print(c)
    verb = "Would update" if args.dry_run else "Updated"
    n_files = len(VERSION_TARGETS) + (0 if args.no_changelog else 1)
    print(f"\n{verb} {len(all_changes)} locations across {n_files} files")

    if args.dry_run or args.skip_lock:
        return

    refresh_cargo_lock()
    refresh_uv_lock()

    print("\nDone. Next steps:")
    print("  1. Review the diff (especially CHANGELOG.md)")
    print("  2. git commit -a -m 'Problem: ...'")
    print(f"  3. git tag v{new} && git push origin main v{new}")


if __name__ == "__main__":
    main()
