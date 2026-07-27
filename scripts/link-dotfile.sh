#!/usr/bin/env bash
# Symlink one dotfile out of this repo, without ever destroying a real file.
#
# `ln -sf` is enough for a target that is absent or is already a symlink, but it
# refuses a regular file — and a regular file is exactly what a machine installed
# by the old home dotfiles repo has sitting at these paths. So the one case that
# needs care is the one plain `ln -sf` cannot handle: move it aside first, loudly.
#
# Nothing is ever overwritten in place. A displaced file becomes
# <target>.pre-hobby-tooling.<timestamp> next to where it was, and the path is
# printed, because a silent backup is the same as a deletion to anyone not reading
# the output.
#
# Usage: link-dotfile.sh <source-in-repo> <target>
#
# This is also where per-machine variant selection will go (issue #9): pick
# <source>.$HOBBY_MACHINE over <source> here, and every install-* recipe inherits
# it without changing a line.

set -euo pipefail

src=$1
dst=$2

if [ ! -e "$src" ]; then
    echo "  link-dotfile: no source at $src" >&2
    exit 1
fi

mkdir -p "$(dirname "$dst")"

# -h, not -e: a symlink to a missing file is still ours to replace, and -e is
# false for it. Only a real file (or directory) gets moved out of the way.
if [ -e "$dst" ] && [ ! -h "$dst" ]; then
    backup="$dst.pre-hobby-tooling.$(date +%Y%m%d%H%M%S)"
    mv "$dst" "$backup"
    echo "  moved aside $(basename "$dst") -> $(basename "$backup")"
    echo "    (it was a real file, not a link — diff it before deleting)"
fi

ln -sfn "$src" "$dst"
echo "  linked $(basename "$dst")"
