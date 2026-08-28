#!/usr/bin/env python3
"""Enumerate every place the comment rule has something to say about.

Emits `path:line<TAB>CATEGORY<TAB>detail`, one finding per line, for the
categories in dotfiles/claude-md/hobby.CLAUDE.md:

  BARE-COMMENT      a comment where the rule allows none (body, banner, stray)
  PRIV-DOC          a doc comment on a private element
  PUB-UNDOC         a public element with no doc comment
  MOD-UNDOC         a module with no header comment
  TEST-UNJUSTIFIED  a test with no line saying why it exists

Judging the *content* of a comment that is in a legal place is not done here --
that needs the code, and it is the skill's job.

Usage: comment-candidates.py [--diff [BASE] | PATH...]
"""
import re
import subprocess
import sys
from pathlib import Path

SKIP_DIRS = {"target", ".git", "node_modules"}

RUST_ITEM = re.compile(
    r"^\s*(?P<vis>pub(\s*\(\s*[^)]*\s*\))?\s+)?"
    r"(async\s+|unsafe\s+|const\s+|extern\s+(\"[^\"]*\"\s+)?)*"
    r"(?P<kind>fn|struct|enum|trait|type|const|static|mod|union)\s+"
    r"(?P<name>[A-Za-z_][A-Za-z0-9_]*)"
)
TEST_ATTR = re.compile(r"^\s*#\[\w*(::)?\w*test\b")


def findings_rust(path, lines):
    out = []
    has_mod_doc = any(l.lstrip().startswith("//!") for l in lines[:60])
    if not has_mod_doc:
        out.append((1, "MOD-UNDOC", f"no //! header in {path.name}"))

    # A bare `//` is legal in exactly one place: directly above a test.
    test_anchor = set()
    for i, l in enumerate(lines):
        if TEST_ATTR.match(l):
            j = i - 1
            while j >= 0 and (lines[j].lstrip().startswith("#[") or not lines[j].strip()):
                j -= 1
            while j >= 0 and lines[j].lstrip().startswith("//"):
                test_anchor.add(j)
                j -= 1

    for i, l in enumerate(lines):
        s = l.strip()
        if s.startswith("//") and not s.startswith("///") and not s.startswith("//!"):
            if i in test_anchor:
                continue
            if s.startswith("// SAFETY:"):   # required by clippy::undocumented_unsafe_blocks
                continue
            out.append((i + 1, "BARE-COMMENT", s[:90]))

    for i, l in enumerate(lines):
        m = RUST_ITEM.match(l)
        if not m:
            continue
        j = i - 1
        while j >= 0 and (lines[j].lstrip().startswith("#[") or lines[j].lstrip().startswith("//")):
            j -= 1
        documented = any(lines[k].lstrip().startswith("///") for k in range(j + 1, i))
        name, kind = m.group("name"), m.group("kind")
        if m.group("vis"):
            if not documented:
                out.append((i + 1, "PUB-UNDOC", f"pub {kind} {name}"))
        else:
            if documented:
                out.append((i + 1, "PRIV-DOC", f"private {kind} {name} carries a ///"))

    for i, l in enumerate(lines):
        if not TEST_ATTR.match(l):
            continue
        j = i - 1
        while j >= 0 and (lines[j].lstrip().startswith("#[") or not lines[j].strip()):
            j -= 1
        if j < 0 or not lines[j].lstrip().startswith("//"):
            fn = next((lines[k].strip() for k in range(i, min(i + 4, len(lines)))
                       if " fn " in lines[k]), "")
            out.append((i + 1, "TEST-UNJUSTIFIED", fn[:90]))
    return out


def _hash_blocks(lines, body_pred, name_of, public_of, kindword):
    """Shared shape for #-commented languages."""
    out = []
    first_code = next((i for i, l in enumerate(lines)
                       if l.strip() and not l.startswith("#!") and not l.lstrip().startswith("#")), 0)
    header = any(l.lstrip().startswith("#") and l.strip() != "#"
                 for l in lines[:first_code] if not l.startswith("#!"))
    if not header:
        out.append((1, "MOD-UNDOC", "no header comment block"))

    for i, l in enumerate(lines):
        nm = name_of(l)
        if nm is None:
            continue
        j = i - 1
        while j >= 0 and not lines[j].strip():
            j -= 1
        documented = j >= 0 and lines[j].lstrip().startswith("#")
        if public_of(l, nm):
            if not documented:
                out.append((i + 1, "PUB-UNDOC", f"{kindword} {nm}"))
        elif documented:
            out.append((i + 1, "PRIV-DOC", f"private {kindword} {nm} carries a comment"))

    for i, l in enumerate(lines):
        if not body_pred(lines, i):
            continue
        if l.lstrip().startswith("#!"):   # a just recipe's shebang, not a comment
            continue
        if l.lstrip().startswith("#"):
            out.append((i + 1, "BARE-COMMENT", l.strip()[:90]))
    return out


SH_FN = re.compile(r"^(?:function\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*\(\s*\)\s*\{")
NU_DEF = re.compile(r"^\s*(export\s+)?def\s+[\"']?([A-Za-z_][A-Za-z0-9_ -]*)[\"']?")
JUST_RECIPE = re.compile(r"^([A-Za-z_][A-Za-z0-9_-]*)(\s+[^:=]*)?:(?!=)")


def indented_body(lines, i):
    return lines[i].startswith((" ", "\t"))


def findings_sh(path, lines):
    return _hash_blocks(
        lines, indented_body,
        lambda l: (SH_FN.match(l).group(1) if SH_FN.match(l) else None),
        lambda l, nm: not nm.startswith("_"), "function")


def findings_nu(path, lines):
    return _hash_blocks(
        lines, indented_body,
        lambda l: (NU_DEF.match(l).group(2).strip() if NU_DEF.match(l) else None),
        lambda l, nm: l.lstrip().startswith("export "), "def")


def findings_just(path, lines):
    return _hash_blocks(
        lines, indented_body,
        lambda l: (JUST_RECIPE.match(l).group(1) if JUST_RECIPE.match(l) else None),
        lambda l, nm: not nm.startswith("_"), "recipe")


def dispatch(path):
    try:
        lines = path.read_text(errors="replace").splitlines()
    except OSError:
        return []
    if path.suffix == ".rs":
        return findings_rust(path, lines)
    if path.suffix == ".nu":
        return findings_nu(path, lines)
    if path.name == "justfile":
        return findings_just(path, lines)
    if path.suffix == ".sh" or (lines and lines[0].startswith("#!") and "sh" in lines[0]):
        return findings_sh(path, lines)
    return []


def collect(paths):
    for p in paths:
        p = Path(p)
        if p.is_dir():
            for q in sorted(p.rglob("*")):
                if q.is_file() and not (SKIP_DIRS & set(q.parts)):
                    yield q
        elif p.is_file():
            yield p


def main():
    args = sys.argv[1:]
    unknown = [a for a in args if a.startswith("--") and a != "--diff"]
    if unknown:
        # Silently treating a flag as a path scans nothing and reports zero
        # findings, which reads exactly like a clean tree.
        sys.exit(f"unknown flag(s): {' '.join(unknown)}\n{__doc__.rstrip()}")
    if args and args[0] == "--diff":
        base = args[1] if len(args) > 1 else None
        if not base:
            for cand in ("main", "master"):
                r = subprocess.run(["git", "merge-base", "HEAD", cand],
                                   capture_output=True, text=True)
                if r.returncode == 0:
                    base = r.stdout.strip()
                    break
        if not base:
            sys.exit("no base to diff against; pass one explicitly")
        # `git diff <base>` and not `<base>...HEAD`: the pass runs before the work
        # is committed, so uncommitted edits are most of what it has to look at.
        names = subprocess.run(["git", "diff", "--name-only", "--diff-filter=d", base],
                               capture_output=True, text=True, check=True).stdout.split()
        untracked = subprocess.run(["git", "ls-files", "--others", "--exclude-standard"],
                                   capture_output=True, text=True).stdout.split()
        seen, targets = set(), []
        for f in names + untracked:
            if f not in seen and Path(f).is_file():
                seen.add(f)
                targets.append(Path(f))
    else:
        targets = list(collect(args or ["."]))

    n = 0
    for f in targets:
        for line, cat, detail in sorted(dispatch(f)):
            print(f"{f}:{line}\t{cat}\t{detail}")
            n += 1
    sys.stdout.flush()
    print(f"\n{n} finding(s) across {len(targets)} file(s)", file=sys.stderr)


if __name__ == "__main__":
    main()
