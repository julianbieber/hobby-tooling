# Hobby Worktree Management for Claude Code Development
#
# Ported from the work `wt` module, restructured for single-repo hobby tasks.
#
# Structure:
#   ~/fun_repos/<project>                       - main repositories
#   ~/fun_worktrees/<project>/<topic>/<task>    - the worktree ITSELF
#   .../hobby-mimisbrunnr/notes/<p>/<t>/<task>  - task notes
#
# The task directory is the worktree, not a container of worktrees. That is the
# one structural difference from the work module, and it is why `wt add` has no
# counterpart here: a hobby task cannot gain a second repo.
#
# Usage:
#   hwt new <task> -p <project> -t <topic>  - create a task worktree
#   hwt go <task>                           - cd to a worktree
#   hwt tree                                - project/topic/task overview
#   hwt rm <task>                           - remove worktree (keeps notes)

const WORKTREE_BASE = "~/fun_worktrees" | path expand
const PROJECTS_BASE = "~/fun_repos" | path expand
const NOTES_REPO = "~/fun_repos/hobby-mimisbrunnr" | path expand
const NOTES_BASE = $NOTES_REPO | path join "notes"

# `hwt sessions` opens hcdev sessions, so it needs the session name for a task.
# Imported rather than re-derived: the name is clamped to what zellij's socket
# path allows, and a second copy of that rule would silently stop matching the
# sessions hcdev actually creates.
use hcdev.nu [hcdev-session-name]

# Main hwt command - shows help.
# Named `main` rather than `hwt`: a nushell module cannot export a command with
# the same name as the module, and `use hwt.nu *` exposes `main` as `hwt`.
export def main [] {
    print "Hobby worktree management for task-based development"
    print ""
    print "Usage: hwt <command>"
    print ""
    print "Commands:"
    print "  hwt new <task> -p <proj> -t <topic> [-b base]  Create a task worktree"
    print "  hwt pr <url> [-t <topic>]                      Create a worktree from a GitHub PR URL"
    print "  hwt list [-p <proj>] [-t <topic>]              List all worktrees"
    print "  hwt projects                                   List projects with task counts"
    print "  hwt topics [-p <proj>]                         List topics with task counts"
    print "  hwt tasks                                      List all tasks"
    print "  hwt tree                                       Tree view: project / topic / task"
    print "  hwt go <task> [-p <proj>]                      Navigate to a worktree"
    print "  hwt status                                     Git status of all worktrees"
    print "  hwt sync [-p <proj>]                           Pull repos, rebase+push worktrees, /sync-notes"
    print "  hwt orphans                                    Worktrees with no task.json"
    print "  hwt rm <task> [-p <proj>] [-f]                 Remove worktree (keeps notes)"
    print "  hwt restore <task>                             Re-create a removed worktree"
    print "  hwt forget <task> [-a]                         Purge task metadata (-a: whole notes dir)"
    print "  hwt done [task] / undone [task]                Mark task done / active"
    print "  hwt block [-u <date>] [-r <reason>]            Mark current task blocked"
    print "  hwt unblock                                    Unblock the current task"
    print "  hwt blocked                                    List blocked + ready tasks"
    print "  hwt prune                                      Clean stale worktree references"
    print "  hwt refresh [-n]                               Fetch/pull default branch on all repos"
    print "  hwt claude <task>                              Start Claude (hobby account) in a worktree"
    print "  hwt sessions [-n]                              Open an hcdev session per open task"
    print ""
    print "Structure:"
    print $"  ($PROJECTS_BASE)/<project>                       Main repositories"
    print $"  ($WORKTREE_BASE)/<project>/<topic>/<task>        The worktree itself"
    print $"  ($NOTES_BASE)/<project>/<topic>/<task>           Task notes"
    print ""
    print "A task exists because its notes dir holds a task.json. The worktree is a"
    print "checkout it may or may not currently have."
}

# Ensure the worktree base directory exists
def ensure-worktree-dir [] {
    if not ($WORKTREE_BASE | path exists) {
        mkdir $WORKTREE_BASE
    }
}

# Ensure the notes directory for a task exists, named to match its worktree
# (notes/<project>/<topic>/<task>/). Keeping the notes dir name identical to the
# worktree task name is what lets journals, prs.json, and subtasks.json line up
# with the right worktree. Idempotent; never touches git on the notes repo.
def ensure-notes-dir [project: string, topic: string, task: string] {
    let notes_dir = ($NOTES_BASE | path join $project $topic $task)
    if not ($notes_dir | path exists) {
        mkdir $notes_dir
        print $"Created notes dir: ($notes_dir)"
    }
    $notes_dir
}

# --- task.json metadata -----------------------------------------------------
# A task's metadata lives at notes/<project>/<topic>/<task>/task.json:
#   { done: bool, blocked: {until?, reason?}|null }
# It outlives the worktree, so done/blocked state survives `hwt rm` and lets
# `hwt restore` re-create the checkout. Tracked in the notes repo, not gitignored.
#
# Unlike the work module there is no `repos` field — the project is in the path,
# and a task is exactly one repo. Because the file is tracked and hdash
# writes it too, both writers must emit the same keys: `blocked` is always
# present (null when unblocked), and nothing else is ever added.

# Path to a task's task.json.
def task-meta-path [project: string, topic: string, task: string] {
    $NOTES_BASE | path join $project $topic $task "task.json"
}

# Derive {project, topic, task} from a worktree path
# (~/fun_worktrees/<project>/<topic>/<task>[/...]). Null if the path isn't under
# the worktree base with at least three components.
def task-parts-from-path [task_path: path] {
    let expanded = ($task_path | path expand)
    if not ($expanded | str starts-with $"($WORKTREE_BASE)/") { return null }
    let parts = ($expanded | str replace $"($WORKTREE_BASE)/" "" | split row "/")
    if ($parts | length) < 3 { return null }
    { project: ($parts | get 0), topic: ($parts | get 1), task: ($parts | get 2) }
}

# Read a task's task.json, filling defaults for missing fields. Tolerant of a
# missing or malformed file (returns the all-default record).
def read-task-meta [project: string, topic: string, task: string] {
    let default = { done: false, blocked: null }
    let f = (task-meta-path $project $topic $task)
    if not ($f | path exists) { return $default }
    let parsed = (try { open --raw $f | from json } catch { null })
    if not (($parsed | describe) | str starts-with 'record') { return $default }
    {
        done: ($parsed.done? | default false)
        blocked: ($parsed.blocked? | default null)
    }
}

# Write a task's task.json (creating the notes dir if needed). Never touches git.
def write-task-meta [project: string, topic: string, task: string, meta: record] {
    ensure-notes-dir $project $topic $task | ignore
    $meta | to json --indent 2 | save -f (task-meta-path $project $topic $task)
}

# Set the done flag on a task.
def meta-set-done [project: string, topic: string, task: string, done: bool] {
    let meta = (read-task-meta $project $topic $task)
    write-task-meta $project $topic $task ($meta | upsert done $done)
}

# Set (or clear) the blocked record. Pass null to unblock; pass a (possibly
# empty) record {until?, reason?} to block.
def meta-set-blocked [project: string, topic: string, task: string, blocked: any] {
    let meta = (read-task-meta $project $topic $task)
    write-task-meta $project $topic $task ($meta | upsert blocked $blocked)
}

# --- predicates -------------------------------------------------------------

# Check if a directory is a git repository or worktree (has a .git entry).
# For linked worktrees .git is a file, not a directory — `path exists` covers both.
def is-git-repo [dir: path] {
    ($dir | path join ".git") | path exists
}

# Check whether a git ref (e.g. refs/heads/main) exists in a repo
def ref-exists [repo: path, ref: string] {
    (do -i { ^git -C $repo rev-parse --verify --quiet $ref } | complete).exit_code == 0
}

# Check if a task is marked done.
def is-task-done [task_path: path] {
    let parts = (task-parts-from-path $task_path)
    if $parts == null { return false }
    (read-task-meta $parts.project $parts.topic $parts.task).done
}

# Read a task's block info. Null if not blocked, otherwise
# { until: string|null, reason: string|null, ready: bool }.
# A block is "ready" once its until-date is today or past (the block has lifted).
def task-block-info [task_path: path] {
    let parts = (task-parts-from-path $task_path)
    if $parts == null { return null }
    let blocked = (read-task-meta $parts.project $parts.topic $parts.task).blocked
    if $blocked == null { return null }
    let until = ($blocked.until? | default null)
    let reason = ($blocked.reason? | default null)
    let today = (date now | format date "%Y-%m-%d")
    let ready = (($until | is-not-empty) and ($today >= $until))
    { until: $until, reason: $reason, ready: $ready }
}

# State label for a task: "done" | "ready" | "blocked" | "active".
def task-state [task_path: path] {
    if (is-task-done $task_path) { return "done" }
    let b = (task-block-info $task_path)
    if $b == null { return "active" }
    if $b.ready { "ready" } else { "blocked" }
}

# A group of tasks (a topic, or a whole project) is done iff it holds at least
# one task and every one of them is done. Takes `known-tasks` rows rather than a
# worktree directory: a topic whose last worktree was removed still has tasks.
def tasks-all-done [tasks: list] {
    if ($tasks | is-empty) { return false }
    ($tasks | all {|t| $t.done })
}

# Is a named zellij session currently running (exists and not EXITED)?
def session-running [lines: list, session: string] {
    let m = ($lines | where { $in | str starts-with $"($session) " })
    if ($m | is-empty) { false } else { not (($m | first) | str contains "EXITED") }
}

# --- flag parsing for completions -------------------------------------------

# Pull the value of a flag (e.g. --project/-p) out of a completion context line.
def flag-from-context [context: string, names: list<string>] {
    let parts = ($context | str trim | split row " " | where { $in != "" })
    let hit = ($parts | window 2 | where { ($in | first) in $names })
    if ($hit | is-empty) { return null }
    try { $hit | first | get 1 } catch { null }
}

# --- notes-tree inventory ---------------------------------------------------
# The notes repo — not ~/fun_worktrees — is what makes a task exist. `hwt rm`
# deletes the worktree and keeps the notes (and prunes the now-empty topic and
# project dirs on the worktree side), so the worktree tree is only ever a subset:
# enumerating from it hides every task between `hwt rm` and `hwt forget`, and
# hides a topic entirely once its last task is removed. Every command that asks
# "what work exists" therefore reads the notes tree; the worktree tree answers
# only "is it checked out".
#
# TODO(jb-comment): why hdash and hwt have to agree on this rule, not merely overlap

# Every task directory under the notes tree, as [{project, topic, task}].
# A `task.json` is what marks the directory as a task — the same rule hdash
# applies, and what makes `hwt forget` (which removes exactly that file) do what
# it says. A notes dir without one is notes, not a task.
def notes-task-dirs [] {
    if not ($NOTES_BASE | path exists) { return [] }
    try {
        glob $"($NOTES_BASE)/*/*/*/task.json" --no-dir --no-symlink
        | each {|f|
            let rel = ($f | path dirname | path relative-to $NOTES_BASE | path split)
            { project: ($rel | get 0), topic: ($rel | get 1), task: ($rel | get 2) }
        }
    } catch { [] }
}

# Every topic directory under the notes tree, as [{project, topic}]. Includes
# topics holding no task yet — `misc`, where a plan lives before a worktree does.
def notes-topic-dirs [] {
    if not ($NOTES_BASE | path exists) { return [] }
    try {
        glob $"($NOTES_BASE)/*/*" --no-file --no-symlink
        | each {|d|
            let rel = ($d | path relative-to $NOTES_BASE | path split)
            { project: ($rel | get 0), topic: ($rel | get 1) }
        }
    } catch { [] }
}

# Every task the notes repo records, as
# [{project, topic, task, path, present, done}], where `path` is the worktree
# directory the task would occupy and `present` says whether it is checked out.
#
# The worktree tree is deliberately *not* unioned in: a directory under
# ~/fun_worktrees with no task.json behind it is an orphan (see
# `orphan-worktrees`), not a task.
def known-tasks [] {
    notes-task-dirs
    | each {|t|
        let path = ($WORKTREE_BASE | path join $t.project $t.topic $t.task)
        {
            project: $t.project
            topic: $t.topic
            task: $t.task
            path: $path
            present: ($path | path exists)
            done: (read-task-meta $t.project $t.topic $t.task).done
        }
    }
}

# Every worktree with no task.json in the notes repo, as
# [{project, topic, task, path}]. The inverse of `known-tasks`: what is on disk
# but not recorded.
def orphan-worktrees [] {
    all-tasks
    | where {|t| not ((task-meta-path $t.project $t.topic $t.task) | path exists) }
}

# "2/5 open" / "all done" / "no tasks" for a group of known-tasks rows. Shown in
# completion descriptions so it is visible up front whether filtering a command
# by that project or topic will select anything.
def task-count-summary [tasks: list] {
    let total = ($tasks | length)
    let open = ($tasks | where {|t| not $t.done } | length)
    if $total == 0 { "no tasks" } else if $open == 0 { "all done" } else { $"($open)/($total) open" }
}

# --- completions ------------------------------------------------------------

# Projects: git repos under ~/fun_repos, since a project without a checkout is
# not something any command can act on. Task counts come from the notes tree, so
# a project whose worktrees are all removed no longer reads as "no tasks".
def project-completions [] {
    if not ($PROJECTS_BASE | path exists) { return [] }
    try {
        let tasks = (known-tasks)
        ls $PROJECTS_BASE
        | where type == dir
        | get name
        | where { is-git-repo $in }
        | each {|p|
            let name = ($p | path basename)
            let mine = ($tasks | where project == $name)
            let open = ($mine | where {|t| not $t.done } | length)
            {
                value: $name
                description: $"project — (task-count-summary $mine)"
                rank: (if $open > 0 { 0 } else { 1 })
            }
        }
        | sort-by rank value
        | each {|r| { value: $r.value, description: $r.description } }
    } catch { [] }
}

# Topics from the notes tree (plus any live worktree topic), narrowed to
# --project/-p when that flag is already present. Topics with open tasks sort
# first; the description names the project(s) the topic appears under.
def topic-completions [context?: string] {
    let proj_filter = if $context != null { flag-from-context $context ["--project" "-p"] } else { null }
    let tasks = (known-tasks | where {|t| ($proj_filter == null) or ($t.project == $proj_filter) })

    notes-topic-dirs
    | append ($tasks | each {|t| { project: $t.project, topic: $t.topic } })
    | where {|r| ($proj_filter == null) or ($r.project == $proj_filter) }
    | uniq-by project topic
    | group-by topic
    | items {|topic, rows|
        # One entry per topic name: the value inserted is the same either way, so
        # a topic shared by two projects names both rather than appearing twice.
        let projects = ($rows | get project | uniq | sort | str join ", ")
        let mine = ($tasks | where topic == $topic)
        {
            value: $topic
            description: $"($projects) — (task-count-summary $mine)"
            rank: (if (($mine | where {|t| not $t.done } | length) > 0) { 0 } else { 1 })
        }
    }
    | sort-by rank value
    | each {|r| { value: $r.value, description: $r.description } }
}

# Shared body of the two task completers. Narrowed by --project/-p and
# --topic/-t; open checked-out tasks sort first, then removed ones, then done.
def task-completion-rows [live_only: bool, context?: string] {
    let proj_filter = if $context != null { flag-from-context $context ["--project" "-p"] } else { null }
    let topic_filter = if $context != null { flag-from-context $context ["--topic" "-t"] } else { null }

    known-tasks
    | where {|t| ($proj_filter == null) or ($t.project == $proj_filter) }
    | where {|t| ($topic_filter == null) or ($t.topic == $topic_filter) }
    | where {|t| (not $live_only) or $t.present }
    | each {|t|
        let tag = $"($t.project)/($t.topic)"
        let suffix = if $t.done { " [done]" } else if not $t.present { " [removed]" } else { "" }
        {
            value: $t.task
            description: $"($tag)($suffix)"
            rank: (if $t.done { 2 } else if not $t.present { 1 } else { 0 })
        }
    }
    | sort-by rank value
    | each {|r| { value: $r.value, description: $r.description } }
}

# Every task the notes tree knows, checked out or not. Removed tasks are marked
# [removed] and kept, because `hwt done`/`undone` read and write notes metadata
# and work perfectly well without a checkout.
def task-completions [context?: string] {
    task-completion-rows false $context
}

# Only tasks with a checkout on disk — for `go`, `rm` and `claude`, which act on
# the worktree. A removed task typed out in full still resolves, and `find-task`
# then names `hwt restore` rather than claiming the task does not exist.
def live-task-completions [context?: string] {
    task-completion-rows true $context
}

# Branch names from the project implied by --project/-p or the current directory
def base-branch-completions [context: string] {
    let proj_name = (flag-from-context $context ["--project" "-p"])

    let proj_path = if $proj_name != null {
        $PROJECTS_BASE | path join $proj_name
    } else {
        let current = (pwd)
        if ($current | str starts-with $"($PROJECTS_BASE)/") {
            $PROJECTS_BASE | path join ($current | str replace $"($PROJECTS_BASE)/" "" | split row "/" | first)
        } else if ($current | str starts-with $"($WORKTREE_BASE)/") {
            let rel = ($current | str replace $"($WORKTREE_BASE)/" "" | split row "/")
            if ($rel | length) >= 1 { $PROJECTS_BASE | path join ($rel | first) } else { null }
        } else {
            null
        }
    }

    if $proj_path == null or not ($proj_path | path exists) { return [] }
    try {
        ^git -C $proj_path branch --format '%(refname:short)' | lines | where { $in | is-not-empty }
    } catch { [] }
}

# --- task lookup ------------------------------------------------------------

# Every worktree directory on disk, as [{project, topic, task, path}]. Not the
# task list: a directory here with no task.json behind it is an orphan. Used only
# by `orphan-worktrees`.
def all-tasks [] {
    if not ($WORKTREE_BASE | path exists) { return [] }
    try {
        ls $WORKTREE_BASE | where type == dir | get name | each {|project_path|
            let project = ($project_path | path basename)
            try {
                ls $project_path | where type == dir | get name | each {|topic_path|
                    let topic = ($topic_path | path basename)
                    try {
                        ls $topic_path | where type == dir | get name | each {|task_path|
                            { project: $project, topic: $topic, task: ($task_path | path basename), path: $task_path }
                        }
                    } catch { [] }
                } | flatten
            } catch { [] }
        } | flatten
    } catch { [] }
}

# Pick one task out of a pool by name: an exact match wins over a substring one.
# Returns null when nothing matches, but *raises* on ambiguity (with `hint`
# naming the way to narrow it), so callers report "ambiguous" rather than the
# misleading "not found".
def match-task [pool: list, task_name: string, hint: string] {
    if ($pool | is-empty) { return null }

    let exact = ($pool | where task == $task_name)
    let matches = if ($exact | is-not-empty) { $exact } else {
        $pool | where { $in.task | str contains $task_name }
    }

    if ($matches | is-empty) { return null }
    if ($matches | length) == 1 { return ($matches | first) }

    print "Multiple matching tasks:"
    $matches | each {|m| print $"  ($m.project)/($m.topic)/($m.task)" }
    error make { msg: $"Task '($task_name)' is ambiguous — ($hint)" }
}

# Find a task that is checked out, as {project, topic, task, path, present, done}.
# For the commands that need the worktree itself.
#
# Resolves against the notes tree, not the worktree tree, so a task that exists
# but has been `hwt rm`'d is reported as removed rather than as missing — the
# fix is `hwt restore`, and "not found" sends you looking for the wrong thing.
def find-task [task_name: string, project?: string] {
    let pool = if $project != null { known-tasks | where project == $project } else { known-tasks }
    let found = (match-task $pool $task_name "narrow it with -p <project>, or give the full task name")
    if $found == null { return null }
    if not $found.present {
        error make { msg: $"Task '($found.project)/($found.topic)/($found.task)' is not checked out — run 'hwt restore ($found.task)'" }
    }
    $found
}

# Find a task among everything the notes tree records, checked out or not. For
# the commands that only read and write notes metadata (`done` / `undone`):
# marking a task done is exactly as meaningful after `hwt rm` as before it, and
# the completions offer removed tasks there, so the lookup has to see them too.
def find-known-task [task_name: string, project?: string] {
    let pool = if $project != null { known-tasks | where project == $project } else { known-tasks }
    match-task $pool $task_name "narrow it with -p <project>, or give the full task name"
}

# Find a task by name among notes tasks. `restore` and `forget` take no -p, so
# the only way to narrow an ambiguous name there is to spell it out.
def find-task-in-notes [task_name: string] {
    match-task (known-tasks) $task_name "give the full task name"
}

# Completion over notes tasks (includes removed ones). For restore / forget,
# where a removed task is the normal argument, so those sort first.
def notes-task-completions [] {
    known-tasks | each {|t|
        let done = $t.done
        let suffix = if $t.present { (if $done { " [done]" } else { "" }) } else { " [removed]" }
        {
            value: $t.task
            description: $"($t.project)/($t.topic)($suffix)"
            rank: (if not $t.present { 0 } else if $done { 2 } else { 1 })
        }
    }
    | sort-by rank value
    | each {|r| { value: $r.value, description: $r.description } }
}

# Resolve the project: explicit name, else inferred from the current directory
# (inside ~/fun_repos/<project> or ~/fun_worktrees/<project>/...).
def find-project [name?: string] {
    if $name != null {
        let project_path = ($PROJECTS_BASE | path join $name)
        if not ($project_path | path exists) {
            error make { msg: $"Project '($name)' not found in ($PROJECTS_BASE)" }
        }
        if not (is-git-repo $project_path) {
            error make { msg: $"Project '($name)' is not a git repository" }
        }
        return { path: $project_path, name: $name }
    }

    let current = (pwd)

    if ($current | str starts-with $"($PROJECTS_BASE)/") {
        let proj_name = ($current | str replace $"($PROJECTS_BASE)/" "" | split row "/" | first)
        let proj_path = ($PROJECTS_BASE | path join $proj_name)
        if not (is-git-repo $proj_path) {
            error make { msg: $"'($proj_name)' is not a git repository. Use -p to select a project." }
        }
        return { path: $proj_path, name: $proj_name }
    }

    # Inside a worktree: the project is the first path component.
    if ($current | str starts-with $"($WORKTREE_BASE)/") {
        let parts = ($current | str replace $"($WORKTREE_BASE)/" "" | split row "/")
        if ($parts | length) >= 1 {
            let proj_name = ($parts | first)
            let proj_path = ($PROJECTS_BASE | path join $proj_name)
            if ($proj_path | path exists) and (is-git-repo $proj_path) {
                return { path: $proj_path, name: $proj_name }
            }
        }
    }

    error make { msg: $"Not in a project under ($PROJECTS_BASE) or ($WORKTREE_BASE). Use -p to select one." }
}

# Resolve {project, topic, task, path} from the current directory, or null.
def task-from-cwd [] {
    let parts = (task-parts-from-path (pwd))
    if $parts == null { return null }
    let task_path = ($WORKTREE_BASE | path join $parts.project $parts.topic $parts.task)
    if not ($task_path | path exists) { return null }
    { project: $parts.project, topic: $parts.topic, task: $parts.task, path: $task_path }
}

# Resolve a task either by name or, with no name, from the current directory.
# Notes-inclusive: its callers only touch task.json, which outlives the worktree.
def resolve-task-arg [task?: string, project?: string] {
    if $task != null {
        find-known-task $task $project
    } else {
        let from_cwd = task-from-cwd
        if $from_cwd == null {
            error make { msg: $"No task given and the current directory is not under ($WORKTREE_BASE)/<project>/<topic>/<task>/" }
        }
        $from_cwd
    }
}

def registered-worktrees [main_repo: path] {
    git-out $main_repo ["worktree" "list" "--porcelain"]
    | split row "\n\n"
    | each {|block| $block | lines | where { $in | is-not-empty } }
    | where {|lines| $lines | any { $in | str starts-with "worktree " } }
    | each {|lines|
        {
            path: ($lines | where { $in | str starts-with "worktree " } | first | str substring 9.. | path expand)
            branch: ($lines | where { $in | str starts-with "branch refs/heads/" } | get -o 0 | default "" | str substring 18..)
            prunable: ($lines | any { $in | str starts-with "prunable" })
        }
    }
}

def prune-dead-worktrees [main_repo: path] {
    let stale = (registered-worktrees $main_repo | where prunable)
    if ($stale | is-empty) { return }
    for w in $stale { print $"Clearing stale worktree registration: ($w.path)" }
    ^git -C $main_repo worktree prune
}

def branch-checked-out-at [main_repo: path, branch: string] {
    let hits = (registered-worktrees $main_repo | where branch == $branch)
    if ($hits | is-empty) { null } else { ($hits | first).path }
}

def remote-branch-ref [main_repo: path, branch: string] {
    git-out $main_repo ["remote"]
    | lines
    | where { $in | is-not-empty }
    | each {|r| $"refs/remotes/($r)/($branch)" }
    | where {|r| ref-exists $main_repo $r }
    | get -o 0
}

def create-worktree [main_repo: path, worktree_dir: path, task: string, base?: string] {
    prune-dead-worktrees $main_repo

    if ($worktree_dir | path exists) {
        let target = ($worktree_dir | path expand)
        if (registered-worktrees $main_repo | any {|w| $w.path == $target }) {
            print $"Worktree already present: ($worktree_dir)"
            return
        }
        if (ls -a $worktree_dir | is-empty) {
            rm $worktree_dir
        } else {
            error make { msg: $"($worktree_dir) exists and is not a registered worktree — move it aside, then retry" }
        }
    }

    let has_local = (ref-exists $main_repo $"refs/heads/($task)")
    let add_args = if $has_local {
        let at = (branch-checked-out-at $main_repo $task)
        if $at != null {
            error make { msg: $"Branch '($task)' is already checked out at ($at)" }
        }
        print $"Creating worktree for existing branch '($task)'..."
        ["worktree" "add" ($worktree_dir | into string) $task]
    } else {
        let remote_ref = (remote-branch-ref $main_repo $task)
        let base_ref = if $base != null { $base } else if $remote_ref != null { $remote_ref } else { "HEAD" }
        print $"Creating worktree with new branch '($task)' from ($base_ref)..."
        ["worktree" "add" "-b" $task ($worktree_dir | into string) $base_ref]
    }

    let r = (do -i { ^git -C $main_repo ...$add_args } | complete)
    let out = ([$r.stdout, $r.stderr] | str join | str trim)
    if $r.exit_code != 0 {
        error make { msg: $"git ($add_args | str join ' ') failed: ($out)" }
    }
    if ($out | is-not-empty) { print $out }
}

# --- commands ---------------------------------------------------------------

# Create a new worktree for a task
export def --env "hwt new" [
    task: string@task-completions                # Task/branch name
    --project (-p): string@project-completions   # Project (defaults to the current repo)
    --topic (-t): string@topic-completions       # Topic to group under (required for new tasks)
    --base (-b): string@base-branch-completions  # Base branch (defaults to current HEAD)
    --no-cd                                      # Don't cd to the new worktree
] {
    ensure-worktree-dir

    let proj = find-project $project

    # Topic: explicit flag, else inherited from an existing task of the same name
    # within this project.
    let resolved_topic = if $topic != null {
        $topic
    } else {
        let found = find-known-task $task $proj.name
        if $found != null {
            $found.topic
        } else {
            error make { msg: "Topic is required for new tasks. Use --topic (-t)." }
        }
    }

    let worktree_dir = ($WORKTREE_BASE | path join $proj.name $resolved_topic $task)

    if ($worktree_dir | path exists) {
        ensure-notes-dir $proj.name $resolved_topic $task | ignore
        # An existing worktree with no task.json is an orphan; recording it here is
        # the same write hdash's Adopt makes.
        if not ((task-meta-path $proj.name $resolved_topic $task) | path exists) {
            write-task-meta $proj.name $resolved_topic $task { done: false, blocked: null }
            print $"Recorded task.json for existing worktree: ($proj.name)/($resolved_topic)/($task)"
        }
        if $no_cd {
            print $"Worktree already exists: ($worktree_dir)"
        } else {
            cd $worktree_dir
            print $"Switched to existing worktree: ($worktree_dir)"
        }
        return $worktree_dir
    }

    # git worktree add creates the leaf itself, so only make the parents.
    let topic_dir = ($WORKTREE_BASE | path join $proj.name $resolved_topic)
    if not ($topic_dir | path exists) { mkdir $topic_dir }

    create-worktree $proj.path $worktree_dir $task $base

    print $"Created worktree: ($worktree_dir)"
    ensure-notes-dir $proj.name $resolved_topic $task | ignore
    write-task-meta $proj.name $resolved_topic $task { done: false, blocked: null }

    if not $no_cd { cd $worktree_dir }
    $worktree_dir
}

# Find notes task(s) that reference a given PR URL, by scanning prs.json.
# Returns [{project, topic, task, notes_path}].
def pr-owning-tasks [url: string] {
    if not ($NOTES_BASE | path exists) { return [] }
    let target = ($url | str replace -r '\.git$' '' | str replace -r '/+$' '')
    try {
        glob $"($NOTES_BASE)/*/*/*/prs.json"
        | each {|f|
            let entries = (try { open $f } catch { [] })
            let urls = (try { $entries | get url } catch { [] })
            let hit = ($urls | any {|u|
                ($u | str replace -r '\.git$' '' | str replace -r '/+$' '') == $target
            })
            if $hit {
                let task_dir = ($f | path dirname)
                let rel = ($task_dir | path relative-to $NOTES_BASE | path split)
                { project: ($rel | get 0), topic: ($rel | get 1), task: ($rel | get 2), notes_path: $task_dir }
            }
        }
        | compact
    } catch { [] }
}

# Create a worktree from a GitHub PR URL.
# Looks the PR up via `gh`, finds the matching project under ~/fun_repos/, fetches
# the head branch, and creates a worktree in the standard layout. Without --topic,
# the URL is reverse-looked-up in the notes prs.json files so the worktree lands
# beside existing notes.
export def --env "hwt pr" [
    url: string                              # PR URL (https://github.com/owner/repo/pull/123)
    --topic (-t): string@topic-completions   # Topic (only needed when no notes track this PR)
    --no-cd                                  # Don't cd to the new worktree
] {
    ensure-worktree-dir

    let url_match = ($url | parse --regex 'github\.com[/:](?<owner>[^/]+)/(?<repo>[^/]+?)(?:\.git)?/pull/\d+')
    if ($url_match | is-empty) {
        error make { msg: $"Could not parse PR URL '($url)'. Expected https://github.com/<owner>/<repo>/pull/<n>." }
    }
    let owner = ($url_match | first | get owner)
    let repo_name = ($url_match | first | get repo)

    let info = try {
        ^gh pr view $url --json number,headRefName,isCrossRepository,title | from json
    } catch {
        error make { msg: $"Failed to fetch PR info for '($url)'. Is `gh` installed and authenticated?" }
    }

    let pr_number = $info.number
    let branch = $info.headRefName
    let is_fork = $info.isCrossRepository
    let pr_title = $info.title

    let proj_path = ($PROJECTS_BASE | path join $repo_name)
    if not ($proj_path | path exists) {
        error make { msg: $"Project '($repo_name)' not found in ($PROJECTS_BASE)" }
    }
    if not (is-git-repo $proj_path) {
        error make { msg: $"'($repo_name)' is not a git repository" }
    }

    # Slashes in a branch name would add path levels; flatten them.
    let sanitized_task = ($branch | str replace -a "/" "-")

    # Resolve topic/task: explicit --topic, else the notes task that already
    # records this PR, else an existing same-name task, else error.
    let owners = (pr-owning-tasks $url)
    let resolved = if $topic != null {
        { topic: $topic, task: $sanitized_task }
    } else if ($owners | length) == 1 {
        let o = ($owners | first)
        print $"Found existing notes for this PR: ($o.project)/($o.topic)/($o.task)"
        { topic: $o.topic, task: $o.task }
    } else if ($owners | length) > 1 {
        print "This PR is referenced by multiple notes tasks:"
        $owners | each {|o| print $"  ($o.project)/($o.topic)/($o.task)" }
        error make { msg: "Ambiguous: pass --topic (-t) to choose where the worktree lands." }
    } else {
        let found = find-known-task $sanitized_task $repo_name
        if $found != null {
            { topic: $found.topic, task: $found.task }
        } else {
            error make { msg: "No notes or worktree track this PR. Pass --topic (-t) to create a new task." }
        }
    }

    let worktree_dir = ($WORKTREE_BASE | path join $repo_name $resolved.topic $resolved.task)

    if ($worktree_dir | path exists) {
        if not $no_cd { cd $worktree_dir }
        print $"Worktree already exists: ($worktree_dir)"
        return $worktree_dir
    }

    let topic_dir = ($WORKTREE_BASE | path join $repo_name $resolved.topic)
    if not ($topic_dir | path exists) { mkdir $topic_dir }

    if $is_fork {
        # Cross-repo PR: fetch via pull/N/head — works without fork remotes.
        let local_branch = $"pr-($pr_number)-($resolved.task)"
        print $"Fetching cross-repo PR #($pr_number) into local branch '($local_branch)'..."
        ^git -C $proj_path fetch origin $"pull/($pr_number)/head:($local_branch)"
        ^git -C $proj_path worktree add $worktree_dir $local_branch
    } else {
        print $"Fetching branch '($branch)' from origin..."
        ^git -C $proj_path fetch origin $branch
        let branch_exists_local = (do -i { ^git -C $proj_path rev-parse --verify $branch } | complete).exit_code == 0
        if $branch_exists_local {
            ^git -C $proj_path worktree add $worktree_dir $branch
        } else {
            ^git -C $proj_path worktree add -b $branch $worktree_dir $"origin/($branch)"
        }
    }

    print $"Created worktree: ($worktree_dir)"

    # Ensure the notes dir exists and the PR is recorded in prs.json (deduped by
    # URL), mirroring the track-pr hook's entry shape.
    let notes_dir = (ensure-notes-dir $repo_name $resolved.topic $resolved.task)
    let prs_file = ($notes_dir | path join "prs.json")
    let existing = if ($prs_file | path exists) { (try { open $prs_file } catch { [] }) } else { [] }
    if not ($existing | any {|e| (try { $e.url } catch { "" }) == $url }) {
        let entry = {
            url: $url
            repo: $"($owner)/($repo_name)"
            title: $pr_title
            opened: (date now | format date "%Y-%m-%d")
        }
        ($existing | append $entry) | save -f $prs_file
        print $"Seeded notes: ($prs_file)"
    } else {
        print $"Notes: ($notes_dir)"
    }

    if $is_fork {
        print "NOTE: fork PR — you cannot push back to pull/N/head."
        print "  To take over, push to your own branch and open a PR."
    } else {
        print $"Take-over ready: commits on '($branch)' push straight to origin/($branch)."
    }

    if not $no_cd { cd $worktree_dir }
    $worktree_dir
}

# List all worktrees
export def "hwt list" [
    --project (-p): string@project-completions  # Filter by project
    --topic (-t): string@topic-completions      # Filter by topic
    --task: string@task-completions             # Filter by task name
] {
    ensure-worktree-dir

    # One `where` per filter: a multi-line boolean expression needs an enclosing
    # paren in nushell, and chained filters read better than that anyway.
    known-tasks
    | where {|t| ($project == null) or ($t.project == $project) }
    | where {|t| ($topic == null) or ($t.topic == $topic) }
    | where {|t| ($task == null) or ($t.task == $task) }
    | each {|t|
        # A removed task has no checkout to read, so git is not asked.
        let info = if not $t.present {
            { head: null, branch: null }
        } else {
            try {
                {
                    head: (^git -C $t.path rev-parse --short HEAD | str trim)
                    branch: (try { ^git -C $t.path symbolic-ref --short HEAD | str trim } catch { "(detached)" })
                }
            } catch {
                { head: "???", branch: "???" }
            }
        }
        let block = (task-block-info $t.path)
        {
            project: $t.project
            topic: $t.topic
            task: $t.task
            present: $t.present
            branch: $info.branch
            head: $info.head
            done: $t.done
            blocked: ($block != null)
            ready: (if $block == null { false } else { $block.ready })
            until: (if $block == null { null } else { $block.until })
            path: $t.path
        }
    }
}

# Go to a worktree directory
export def --env "hwt go" [
    task: string@live-task-completions           # Task name
    --project (-p): string@project-completions   # Disambiguate when the name repeats across projects
] {
    let found = find-task $task $project
    if $found == null {
        error make { msg: $"Task '($task)' not found" }
    }
    cd $found.path
}

# Remove a worktree. Notes and task.json are deliberately kept so done/blocked
# state survives and `hwt restore` can re-create the checkout; `hwt forget`
# purges the metadata when a task is truly finished.
export def "hwt rm" [
    task: string@live-task-completions           # Task to remove
    --project (-p): string@project-completions   # Disambiguate across projects
    --force (-f)                                 # Force removal even if dirty
] {
    let found = find-task $task $project
    if $found == null {
        error make { msg: $"Task '($task)' not found" }
    }

    let main_repo = ($PROJECTS_BASE | path join $found.project)
    print $"Removing worktree: ($found.project)/($found.topic)/($found.task)"

    if ($main_repo | path exists) {
        if $force {
            ^git -C $main_repo worktree remove --force $found.path
        } else {
            ^git -C $main_repo worktree remove $found.path
        }
    } else {
        print "  Warning: main repo not found, removing directory directly"
        rm -rf $found.path
    }

    # Clean up now-empty topic and project directories.
    let topic_path = ($WORKTREE_BASE | path join $found.project $found.topic)
    if ($topic_path | path exists) and ((ls $topic_path | length) == 0) {
        rm -r $topic_path
        print $"Removed empty topic directory: ($found.topic)"
    }
    let project_path = ($WORKTREE_BASE | path join $found.project)
    if ($project_path | path exists) and ((ls $project_path | length) == 0) {
        rm -r $project_path
        print $"Removed empty project directory: ($found.project)"
    }

    print $"Kept notes. 'hwt restore ($found.task)' re-creates it; 'hwt forget ($found.task)' purges the metadata."
}

# Re-create a removed task's worktree from its notes directory. The branch is the
# task name — an existing local branch is reused, else the branch's own remote-
# tracking ref, else HEAD. Only the notes path is needed, so a task whose
# task.json was purged by `hwt forget` still restores.
#
# Repairs a checkout left half-removed as far as it can: a stale registration or
# an empty directory at the task path is cleared rather than reported. A directory
# with contents that is not a worktree, and a branch checked out elsewhere, raise.
export def --env "hwt restore" [
    task: string@notes-task-completions   # Task to restore (from the notes tree)
    --no-cd                               # Don't cd into the restored worktree
] {
    let found = find-task-in-notes $task
    if $found == null {
        error make { msg: $"No notes directory found for '($task)' under ($NOTES_BASE)" }
    }
    if $found.topic == "coordinator" {
        error make { msg: $"'($found.task)' is a coordinator — it has no worktree; run 'hcdev coordinator ($found.task) -p ($found.project)'" }
    }
    if $found.present and (is-git-repo $found.path) {
        print $"Already present: ($found.path)"
        if not $no_cd { cd $found.path }
        return $found.path
    }

    let main_repo = ($PROJECTS_BASE | path join $found.project)
    if not ($main_repo | path exists) {
        error make { msg: $"Main repo not found at ($main_repo)" }
    }

    ensure-worktree-dir
    let topic_dir = ($WORKTREE_BASE | path join $found.project $found.topic)
    if not ($topic_dir | path exists) { mkdir $topic_dir }

    create-worktree $main_repo $found.path $found.task
    print $"Restored: ($found.project)/($found.topic)/($found.task)"

    if not $no_cd { cd $found.path }
    $found.path
}

# Purge a task's metadata so it drops out of the overview. Removes task.json only
# (journals and prs.json stay). With --all, removes the whole notes task dir.
export def "hwt forget" [
    task: string@notes-task-completions   # Task to forget
    --all (-a)                            # Also delete the notes task directory
] {
    let found = find-task-in-notes $task
    if $found == null {
        error make { msg: $"No notes directory found for '($task)' under ($NOTES_BASE)" }
    }
    if $found.present {
        print $"Warning: worktree still exists at ($found.path). Run 'hwt rm ($found.task)' first if you meant to remove it."
    }
    let notes_dir = ($NOTES_BASE | path join $found.project $found.topic $found.task)
    if $all {
        if ($notes_dir | path exists) { rm -r $notes_dir }
        # Drop now-empty topic/project dirs too, mirroring what `hwt rm` does for
        # the worktree side — otherwise the notes tree accretes empty shells.
        let topic_dir = ($NOTES_BASE | path join $found.project $found.topic)
        if ($topic_dir | path exists) and ((ls $topic_dir | length) == 0) { rm -r $topic_dir }
        let project_dir = ($NOTES_BASE | path join $found.project)
        if ($project_dir | path exists) and ((ls $project_dir | length) == 0) { rm -r $project_dir }
        print $"Forgot ($found.project)/($found.topic)/($found.task) — removed notes dir"
    } else {
        let meta = (task-meta-path $found.project $found.topic $found.task)
        if ($meta | path exists) { rm $meta }
        print $"Forgot ($found.project)/($found.topic)/($found.task) — removed task.json"
    }
}

# Mark a task done (defaults to the task containing the current directory)
export def "hwt done" [
    task?: string@task-completions                # Task (defaults to cwd's task)
    --project (-p): string@project-completions    # Disambiguate across projects
] {
    let found = resolve-task-arg $task $project
    if $found == null {
        error make { msg: $"Task '($task)' not found" }
    }
    meta-set-done $found.project $found.topic $found.task true
    print $"Marked done: ($found.project)/($found.topic)/($found.task)"
}

# Mark a task active again
export def "hwt undone" [
    task?: string@task-completions                # Task (defaults to cwd's task)
    --project (-p): string@project-completions    # Disambiguate across projects
] {
    let found = resolve-task-arg $task $project
    if $found == null {
        error make { msg: $"Task '($task)' not found" }
    }
    meta-set-done $found.project $found.topic $found.task false
    print $"Marked active: ($found.project)/($found.topic)/($found.task)"
}

# Mark the current task blocked. --until is the date the block lifts, --reason
# what it is waiting on; both optional.
export def "hwt block" [
    --until (-u): string    # YYYY-MM-DD the block lifts
    --reason (-r): string   # Why it's blocked
] {
    let found = task-from-cwd
    if $found == null {
        error make { msg: $"Not in a task directory \(($WORKTREE_BASE)/<project>/<topic>/<task>/...\)" }
    }
    mut blocked = {}
    if ($until | is-not-empty) { $blocked = ($blocked | upsert until $until) }
    if ($reason | is-not-empty) { $blocked = ($blocked | upsert reason $reason) }
    meta-set-blocked $found.project $found.topic $found.task $blocked
    let until_msg = if ($until | is-not-empty) { $" until ($until)" } else { "" }
    print $"Blocked: ($found.project)/($found.topic)/($found.task)($until_msg)"
}

# Unblock the current task
export def "hwt unblock" [] {
    let found = task-from-cwd
    if $found == null {
        error make { msg: $"Not in a task directory \(($WORKTREE_BASE)/<project>/<topic>/<task>/...\)" }
    }
    meta-set-blocked $found.project $found.topic $found.task null
    print $"Unblocked: ($found.project)/($found.topic)/($found.task)"
}

# List blocked + ready (block-lifted) tasks. Ready sorts first — a daily
# "what's waiting on me / what just freed up" view.
export def "hwt blocked" [] {
    ensure-worktree-dir
    known-tasks
    | each {|t|
        if $t.done { return null }
        let block = (task-block-info $t.path)
        if $block == null { return null }
        {
            project: $t.project
            topic: $t.topic
            task: $t.task
            state: (if $block.ready { "ready" } else { "blocked" })
            until: $block.until
            reason: $block.reason
        }
    }
    | compact
    | sort-by { |r| if $r.state == "ready" { 0 } else { 1 } } project topic task
}

# List projects with task counts
export def "hwt projects" [] {
    ensure-worktree-dir
    let all = (known-tasks)
    $all
    | get project
    | uniq
    | each {|project|
        let tasks = ($all | where project == $project)
        {
            project: $project
            topics: ($tasks | get topic | uniq | length)
            tasks: ($tasks | length)
            open: ($tasks | where { not $in.done } | length)
            checked_out: ($tasks | where present | length)
            done: (tasks-all-done $tasks)
            has_repo: (($PROJECTS_BASE | path join $project) | path exists)
        }
    }
    | sort-by done project
}

# List topics with task counts
export def "hwt topics" [
    --project (-p): string@project-completions  # Filter by project
] {
    ensure-worktree-dir
    known-tasks
    | where {|t| ($project == null) or ($t.project == $project) }
    | group-by { |t| $"($t.project)/($t.topic)" }
    | items {|key, rows|
        let first = ($rows | first)
        {
            project: $first.project
            topic: $first.topic
            tasks: ($rows | length)
            open: ($rows | where { not $in.done } | length)
            done: (tasks-all-done $rows)
            task_list: ($rows | get task | str join ", ")
        }
    }
    | sort-by done project topic
}

# List all tasks
export def "hwt tasks" [] {
    ensure-worktree-dir
    known-tasks | each {|t|
        let block = (task-block-info $t.path)
        {
            project: $t.project
            topic: $t.topic
            task: $t.task
            present: $t.present
            state: (task-state $t.path)
            until: (if $block == null { null } else { $block.until })
            reason: (if $block == null { null } else { $block.reason })
        }
    }
}

# Most recent journal date for a task (YYYY-MM-DD.md in its notes dir), or "—".
def last-active [project: string, topic: string, task: string] {
    let notes_dir = ($NOTES_BASE | path join $project $topic $task)
    try {
        ls $notes_dir
        | where type == file
        | get name
        | each { path basename }
        | where { $in =~ '^\d{4}-\d{2}-\d{2}\.md$' }
        | each { str replace '.md' '' }
        | sort -r
        | first
    } catch { "—" }
}

# Tree view: project / topic / task, with last-active date and state
export def "hwt tree" [] {
    ensure-worktree-dir

    let all = (known-tasks)
    let projects = ($all | get project | uniq | sort)

    for project in $projects {
        let project_tasks = ($all | where project == $project)
        let project_done = (tasks-all-done $project_tasks)

        if $project_done {
            print $"(ansi dark_gray)($project)  \(done\)(ansi reset)"
        } else {
            print $"(ansi cyan_bold)($project)(ansi reset)"
        }

        let topics = ($project_tasks | get topic | uniq | sort)
        let last_topic_idx = (($topics | length) - 1)

        for t_idx in 0..$last_topic_idx {
            let topic = ($topics | get $t_idx)
            let topic_tasks = ($project_tasks | where topic == $topic)
            let topic_last = ($t_idx == $last_topic_idx)
            let topic_prefix = if $topic_last { "└── " } else { "├── " }
            let child_indent = if $topic_last { "    " } else { "│   " }
            let topic_done = (tasks-all-done $topic_tasks)

            if $topic_done {
                print $"($topic_prefix)(ansi dark_gray)($topic)  \(done\)(ansi reset)"
            } else {
                print $"($topic_prefix)(ansi blue_bold)($topic)(ansi reset)"
            }

            let tasks = ($topic_tasks
                | sort-by task
                | sort-by { |x|
                    # ready floats up, blocked sinks below active, done last
                    match (task-state $x.path) {
                        "ready" => 0
                        "active" => 1
                        "blocked" => 2
                        _ => 3
                    }
                  })

            let last_task_idx = (($tasks | length) - 1)
            for k_idx in 0..$last_task_idx {
                let row = ($tasks | get $k_idx)
                let task = $row.task
                let prefix = if ($k_idx == $last_task_idx) { "└── " } else { "├── " }
                let done = $row.done
                let block = (task-block-info $row.path)
                let active = (last-active $project $topic $task)

                let color = if $done {
                    (ansi dark_gray)
                } else if ($block != null) and $block.ready {
                    (ansi magenta_bold)
                } else if ($block != null) {
                    (ansi yellow)
                } else if not $row.present {
                    (ansi dark_gray)
                } else {
                    (ansi green)
                }
                let suffix = if $done {
                    "  (done)"
                } else if ($block != null) and $block.ready {
                    let u = if ($block.until | is-not-empty) { $" \(block lifted ($block.until)\)" } else { "" }
                    $"  ⚑ READY($u)"
                } else if ($block != null) {
                    let u = if ($block.until | is-not-empty) { $" until ($block.until)" } else { "" }
                    let r = if ($block.reason | is-not-empty) { $": ($block.reason)" } else { "" }
                    $"  ⏸ blocked($u)($r)"
                } else if not $row.present {
                    "  (removed)"
                } else {
                    ""
                }
                print $"($child_indent)($prefix)($color)($task)(ansi reset)  (ansi dark_gray)($active)($suffix)(ansi reset)"
            }
        }
    }

    let orphans = (orphan-worktrees)
    if ($orphans | is-not-empty) {
        print ""
        print $"(ansi yellow)orphan worktrees \(no task.json\)(ansi reset)"
        for o in $orphans {
            print $"  (ansi yellow)($o.project)/($o.topic)/($o.task)(ansi reset)"
        }
    }
}

# Git status of all worktrees
export def "hwt status" [] {
    hwt list | where present | each {|tree|
        let dirty = try { ^git -C $tree.path status --porcelain | lines | length } catch { 0 }
        # A fresh task branch has no upstream, which is normal and not worth
        # printing. `complete` captures git's stderr instead of letting it leak.
        let up = (do -i { ^git -C $tree.path rev-parse --abbrev-ref '@{upstream}' } | complete)
        let ahead_behind = if $up.exit_code != 0 {
            { behind: 0, ahead: 0 }
        } else {
            let counts = (do -i {
                ^git -C $tree.path rev-list --left-right --count $"($up.stdout | str trim)...HEAD"
            } | complete)
            if $counts.exit_code != 0 {
                { behind: 0, ahead: 0 }
            } else {
                try {
                    $counts.stdout | str trim | split row "\t"
                    | { behind: ($in.0 | into int), ahead: ($in.1 | into int) }
                } catch {
                    { behind: 0, ahead: 0 }
                }
            }
        }
        $tree | merge { dirty: $dirty, ahead: $ahead_behind.ahead, behind: $ahead_behind.behind }
    }
    | select project topic task branch done blocked ready dirty ahead behind path
}

# List worktrees with no task.json in the notes repo. These are what hdash offers
# to Adopt: on disk, and recorded nowhere.
export def "hwt orphans" [] {
    orphan-worktrees | select project topic task path
}

# Prune stale worktree references across all projects
export def "hwt prune" [] {
    let repos = try {
        ls $PROJECTS_BASE | where type == dir | get name | where { is-git-repo $in }
    } catch { [] }

    for repo in $repos {
        print $"Pruning stale worktrees for ($repo | path basename)..."
        ^git -C $repo worktree prune
    }
    print "Done"
}

# Fetch/pull the default branch on all projects
export def "hwt refresh" [
    --dry-run (-n)   # Show what would be done without doing it
] {
    let repos = try {
        ls $PROJECTS_BASE | where type == dir | get name | where { is-git-repo $in }
    } catch { [] }

    mut dirty = []

    for repo in $repos {
        let name = ($repo | path basename)

        # Resolve the default branch by checking which of main/master exists.
        # Prefer the local head, fall back to the remote-tracking ref so a repo
        # whose default branch isn't checked out locally still works.
        let target = (
            ["main" "master"]
            | where {|c|
                (ref-exists $repo $"refs/heads/($c)") or (ref-exists $repo $"refs/remotes/origin/($c)")
            }
            | get -o 0
        )

        if $target == null {
            print $"($name): skipped \(no main/master branch\)"
            continue
        }

        let current_branch = try { ^git -C $repo branch --show-current | str trim } catch { "" }

        if $dry_run {
            print $"($name): would update ($target)"
            continue
        }

        print $"($name): fetching..."
        try {
            do -i { ^git -C $repo fetch origin --prune } | ignore

            if (^git -C $repo status --porcelain | str trim | is-not-empty) {
                $dirty = ($dirty | append $"($name) \(on ($current_branch)\)")
                continue
            }

            if $current_branch != $target {
                do -i { ^git -C $repo checkout $target } | ignore
            }
            do -i { ^git -C $repo pull --ff-only origin $target } | ignore

            if $current_branch != $target {
                print $"  ($name): updated ($target) \(was on ($current_branch)\)"
            } else {
                print $"  ($name): updated ($target)"
            }
        } catch {|e|
            print $"  ($name): error — ($e.msg)"
        }
    }

    if ($dirty | is-not-empty) {
        print "Skipped (dirty working tree):"
        for entry in $dirty { print $"  - ($entry)" }
    }
    print "Done"
}

# Start Claude Code (hobby account) in a worktree
export def --env "hwt claude" [
    task: string@live-task-completions           # Task name
    --project (-p): string@project-completions   # Disambiguate across projects
] {
    let found = find-task $task $project
    if $found == null {
        error make { msg: $"Task '($task)' not found" }
    }
    print $"Starting Claude Code (hobby account) in ($found.path)"
    cd $found.path
    ^chobby
}

# Open an alacritty window with an hcdev session for every open (not-done) task.
# Skips tasks whose session is already running.
export def "hwt sessions" [
    --dry-run (-n)   # Show what would open without opening anything
] {
    ensure-worktree-dir

    # A session needs a checkout to open in, so removed tasks are skipped here even
    # though they are tasks everywhere else.
    let tasks = (known-tasks
        | where present
        | where { not $in.done }
        | each {|t| $t | merge { session: (hcdev-session-name $t.project $t.topic $t.task) } })

    let running = (try { zellij list-sessions --no-formatting | lines } catch { [] })
    let already = ($tasks | where {|t| session-running $running $t.session })
    let to_open = ($tasks | where {|t| not (session-running $running $t.session) })

    for t in $already { print $"skip \(running\):     ($t.session)" }

    if ($to_open | is-empty) {
        print "No new sessions to open."
        return
    }

    for t in $to_open {
        if $dry_run {
            print $"would open: ($t.session)  [($t.path)]"
        } else {
            print $"opening: ($t.session)"
            # Drop this shell's zellij variables: run from inside a session, the new
            # terminal would otherwise look to zellij like a nested client of *that*
            # session rather than a fresh top-level one.
            with-env {ZELLIJ: null, ZELLIJ_SESSION_NAME: null, ZELLIJ_PANE_ID: null} {
                ^setsid -f alacritty --working-directory $t.path --title $t.session -e nu --login -c $"hcdev wt '($t.task)' --project '($t.project)' --topic '($t.topic)'"
            }
        }
    }
}

# --- sync -------------------------------------------------------------------

# Every git checkout on this disk that `hwt sync` acts on: the main repos, the
# task worktrees, and the orphans.
#
# Three sources, not one directory walk. Worktrees come from `known-tasks`, which
# reads `notes/*/*/*/task.json` — the notes repo is what makes a task exist — so a
# directory under ~/fun_worktrees with no task.json behind it is not a worktree
# here but an orphan, gathered separately by `orphan-worktrees`.
#
# The notes checkout is left out of all three: it is synced by a Claude session
# rather than by git commands, so `sync-notes-session` owns it end to end and the
# mechanical path never sees it.
def sync-checkouts [project?: string] {
    let repos = try {
        ls $PROJECTS_BASE | where type == dir | get name
        | where { $in != $NOTES_REPO }
        | where { is-git-repo $in }
        | each {|p| { kind: "repo", project: ($p | path basename), name: "", path: $p } }
    } catch { [] }

    let trees = (known-tasks | where present
        | each {|t| { kind: "worktree", project: $t.project, name: $"($t.topic)/($t.task)", path: $t.path } })

    let orphans = (orphan-worktrees
        | each {|t| { kind: "orphan", project: $t.project, name: $"($t.topic)/($t.task)", path: $t.path } })

    let all = ($repos | append $trees | append $orphans)
    if $project == null { $all } else { $all | where project == $project }
}

# `git -C <path> <args>` as a trimmed string, empty when git fails.
#
# `do -i` and `complete` together are the reason this exists rather than calling
# ^git inline: without them a non-zero exit aborts the whole command and git's
# stderr lands in the middle of the report. Every probe here runs against a
# checkout that may be unborn, remote-less or mid-rebase, so non-zero is a normal
# answer rather than an error worth propagating.
def git-out [path: path, args: list<string>] {
    let r = (do -i { ^git -C $path ...$args } | complete)
    if $r.exit_code == 0 { $r.stdout | str trim } else { "" }
}

# What this machine calls itself, short hostname unless $HOBBY_MACHINE says
# otherwise — the same variable `link-dotfile.sh` uses to pick a per-machine
# variant, because a hostname is not always short or stable.
#
# It is in the subject of every wip commit, which is the only place a reader finds
# out which disk the uncommitted work was sitting on when the sync picked it up.
def sync-host [] {
    let h = ($env.HOBBY_MACHINE? | default "")
    if ($h | is-not-empty) { $h } else { (sys host).hostname | split row "." | get 0 }
}

def plural [n: int, word: string] {
    if $n == 1 { $"($n) ($word)" } else { $"($n) ($word)s" }
}

# Unsynced state for one checkout.
#
# `unpushed` counts commits on *any* local branch that no remote has, which is the
# measure that matches "would be lost with this disk" — an ahead-count of the
# current branch would miss the side branches that hold most of it.
#
# The states are ordered by what blocks the fix, not by severity: an unborn or
# remote-less checkout needs a decision (a first commit, a `gh repo create`)
# before anything can be pushed at all, so those outrank `unsynced`.
def sync-scan-one [c: record] {
    let p = $c.path
    let opted_out = ((git-out $p ["config" "--get" "hobby.sync"]) == "false")
    let has_head = ((do -i { ^git -C $p rev-parse --verify --quiet HEAD } | complete).exit_code == 0)
    let remote = (git-out $p ["remote"] | lines | get -o 0 | default "")
    let porcelain = (git-out $p ["status" "--porcelain"] | lines | where { $in | is-not-empty })
    let untracked = ($porcelain | where {|l| $l | str starts-with "??" } | length)
    let dirty = (($porcelain | length) - $untracked)

    let unpushed = if ($remote | is-empty) or (not $has_head) { 0 } else {
        (git-out $p ["log" "--branches" "--not" "--remotes" "--format=%H"] | lines
            | where { $in | is-not-empty } | length)
    }

    let state = if $opted_out { "skipped"
        } else if not $has_head { "unborn"
        } else if ($remote | is-empty) { "no-remote"
        } else if ($unpushed > 0) or ($dirty > 0) or ($untracked > 0) { "unsynced"
        } else { "ok" }

    $c | merge {
        branch: (git-out $p ["branch" "--show-current"])
        remote: $remote
        unpushed: $unpushed
        dirty: $dirty
        untracked: $untracked
        state: $state
    }
}

# Fetch one checkout.
def sync-fetch-one [row: record] {
    if ($row.remote | is-empty) { return { ok: false, err: "no remote" } }
    let a = (do -i { ^git -C $row.path fetch --prune $row.remote } | complete)
    if $a.exit_code != 0 {
        return { ok: false, err: ($a.stderr | str trim | lines | last | default "fetch failed") }
    }
    { ok: true, err: "" }
}

# How far the current branch is from its upstream, as {ahead, behind, upstream}.
#
# Both counts are zero when there is no upstream, which is a fresh task branch
# rather than a problem — the push side is what gives it one. Callers check
# `upstream` for empty to tell the two cases apart.
def sync-tracking [path: path] {
    let up = (git-out $path ["rev-parse" "--abbrev-ref" "@{upstream}"])
    if ($up | is-empty) { return { ahead: 0, behind: 0, upstream: "" } }
    let counts = (git-out $path ["rev-list" "--left-right" "--count" $"($up)...HEAD"])
    if ($counts | is-empty) { return { ahead: 0, behind: 0, upstream: $up } }
    try {
        $counts | split row "\t" | { ahead: ($in.1 | into int), behind: ($in.0 | into int), upstream: $up }
    } catch {
        { ahead: 0, behind: 0, upstream: $up }
    }
}

# Bring one main checkout current, when that can be done without losing anything.
#
# Fast-forward is the only move here, and a main checkout is why: it is where a
# clone is read, tried and scribbled in, so its dirty tree and its local commits
# belong to whatever was being done there and are nobody's task branch. Everything
# that is not a fast-forward is therefore printed rather than attempted, and the
# summary names the ones holding commits.
def sync-pull-one [row: record] {
    let label = $"  ($row.project)(if ($row.name | is-empty) { '' } else { $'/($row.name)' })"

    if ($row.branch | is-empty) {
        print $"($label)  skipped — detached HEAD"
        return
    }
    let t = (sync-tracking $row.path)
    if ($t.upstream | is-empty) {
        print $"($label)  skipped — no upstream"
        return
    }
    if $t.behind == 0 {
        return
    }
    if $t.ahead > 0 {
        print $"($label)  skipped — diverged \(($t.ahead) local / ($t.behind) remote\)"
        return
    }
    if ($row.dirty + $row.untracked) > 0 {
        print $"($label)  skipped — behind ($t.behind), dirty \(($row.dirty + $row.untracked) files\)"
        return
    }

    let before = (git-out $row.path ["rev-parse" "--short" "HEAD"])
    let r = (do -i { ^git -C $row.path merge --ff-only $t.upstream } | complete)
    if $r.exit_code != 0 {
        print $"($label)  FAILED: ($r.stderr | str trim | lines | last)"
        return
    }
    let after = (git-out $row.path ["rev-parse" "--short" "HEAD"])
    print $"($label)  ($row.branch)  ff  ($before)..($after) \(($t.behind)\)"
}

# Commit the working tree — tracked edits and untracked files alike, minus what
# .gitignore excludes — onto the branch, and say so. Returns whether it worked.
#
# It lands on the branch rather than on a ref beside it because everything after
# this step needs it there: a rebase refuses to run over a dirty tree, and a push
# carries branches and nothing else. The price is a commit that has to be squashed
# or amended before the branch is reviewed, which is what the `wip` first word and
# the body are for.
#
# The message is a file count, the machine, and the path to the task's notes. It
# never describes the change: the notes are synced too and are where the prose
# lives, so it points at them instead of paraphrasing them.
def sync-wip-commit [row: record] {
    let p = $row.path
    let staged = (do -i { ^git -C $p add --all } | complete)
    if $staged.exit_code != 0 {
        print $"    FAILED to stage: ($staged.stderr | str trim | lines | last)"
        return false
    }
    if (do -i { ^git -C $p diff --cached --quiet } | complete).exit_code == 0 {
        print $"    nothing to snapshot \(all changes ignored\)"
        return true
    }

    let notes = ($NOTES_BASE | path join $row.project $row.name)
    let msg = ([
        $"wip @ (sync-host): ($row.dirty) modified, ($row.untracked) untracked"
        ""
        "Uncommitted work picked up by `hwt sync` so it survives the machine."
        "Squash or amend it before this branch is reviewed."
    ] | append (if ($row.kind != "repo") and ($notes | path exists) { ["" $"notes: ($notes)"] } else { [] })
      | str join "\n")

    # --no-verify: a repo's commit hooks lint what a person wrote, and this commit
    # is a machine picking up a tree mid-edit. A failing hook here would block the
    # sync on work that was never claimed to be finished.
    let c = (do -i { ^git -C $p commit --no-verify -m $msg } | complete)
    if $c.exit_code != 0 {
        print $"    FAILED to commit: ($c.stderr | str trim | lines | last)"
        return false
    }
    let sha = (git-out $p ["rev-parse" "--short" "HEAD"])
    print $"    wip ($sha) \(($row.dirty) modified, ($row.untracked) untracked\)"
    true
}

# Publish one worktree: commit whatever is uncommitted, rebase onto the upstream,
# push.
#
# A task branch is the one population that is meant to travel — it exists to
# become a PR — so all three steps run without asking. A failed rebase is aborted
# rather than left mid-flight, because the next thing to touch this checkout is a
# Claude session or an editor and neither expects to arrive in a conflict.
def sync-worktree-one [row: record] {
    let p = $row.path
    let label = (if ($row.name | is-empty) { $row.project } else { $"($row.project)/($row.name)" })

    if ($row.branch | is-empty) {
        print $"  ($label)  skipped — detached HEAD"
        return
    }

    let t0 = (sync-tracking $p)
    let dirt = ($row.dirty + $row.untracked)
    if $dirt == 0 and $t0.behind == 0 and $t0.ahead == 0 and ($t0.upstream | is-not-empty) {
        return
    }
    print $"  ($label) [($row.branch)]"

    if $dirt > 0 {
        if not (sync-wip-commit $row) { return }
    }

    let t = (sync-tracking $p)
    if ($t.upstream | is-not-empty) and ($t.behind > 0) {
        let r = (do -i { ^git -C $p rebase $t.upstream } | complete)
        if $r.exit_code != 0 {
            do -i { ^git -C $p rebase --abort } | ignore
            print $"    rebase onto ($t.upstream) failed, aborted — ($r.stderr | str trim | lines | last)"
            return
        }
        print $"    rebased onto ($t.upstream) \(($t.behind) commits\)"
    }

    let t2 = (sync-tracking $p)
    let ahead = if ($t2.upstream | is-not-empty) { $t2.ahead } else {
        (git-out $p ["log" "--format=%H" $row.branch "--not" "--remotes"] | lines
            | where { $in | is-not-empty } | length)
    }
    if $ahead == 0 { return }

    # -u only when there is no upstream yet: push.autoSetupRemote covers the same
    # case globally, and saying it here keeps a fresh task branch working on a
    # machine whose git config has not been through `just bootstrap`.
    let args = if ($t2.upstream | is-empty) {
        ["push" "-u" $row.remote $row.branch]
    } else {
        ["push" $row.remote $row.branch]
    }
    let pushed = (do -i { ^git -C $p ...$args } | complete)
    if $pushed.exit_code == 0 {
        print $"    pushed ($row.branch) \(($ahead) commits\)"
    } else {
        print $"    FAILED to push ($row.branch): ($pushed.stderr | str trim | lines | last)"
    }
}

# Hand the notes checkout to a Claude session running `/sync-notes`.
#
# It is the one checkout `hwt sync` does not touch itself, and it is the one where
# the mechanical treatment is wrong in both directions: a conflict there is a
# journal entry written twice rather than two edits to one program, and the commit
# carrying a day of notes needs a message that a file count cannot write.
#
# The skill is project-scoped to that repo (`.claude/commands/sync-notes.md` in
# hobby-mimisbrunnr), not installed from here, which is why the session has to
# start in that directory rather than anywhere with the config dir set.
def sync-notes-session [] {
    print ""
    if not ($NOTES_REPO | path exists) {
        print $"notes: no checkout at ($NOTES_REPO) — run `just clone-notes` in hobby-tooling"
        return
    }
    if (which chobby | is-empty) {
        print "notes: `chobby` is not on PATH — run `just install-chobby` in hobby-tooling"
        return
    }
    # A `claude` started from inside a Claude session would nest one agent in
    # another, so the session that ran `hwt sync` is told the command instead.
    if ($env.CLAUDECODE? | default "") == "1" {
        print "notes: already inside a Claude session, so not starting a second one. Run:"
        print $"  cd ($NOTES_REPO); chobby '/sync-notes'"
        return
    }
    print $"notes: starting a hobby Claude session on /sync-notes in ($NOTES_REPO)"
    cd $NOTES_REPO
    ^chobby "/sync-notes"
}

# Bring every checkout on this disk into line with its remote.
#
# Three populations, three treatments, and that split is what the flags used to
# be — the command acts, and there is no report-only mode, because the report was
# the thing being replaced:
#
#   main checkouts   fetch and fast-forward, nothing else. A clone under
#                    ~/fun_repos is where things are read and tried, so its local
#                    commits and its dirty tree are scratch rather than work in
#                    flight; anything that is not a fast-forward is named in the
#                    summary for a decision.
#   worktrees        wip-commit the working tree, rebase onto the upstream, push.
#                    A task branch exists to become a PR, so it is meant to
#                    travel, and the wip commit is what lets the rebase run over a
#                    tree that is mid-edit.
#   the notes repo   a Claude session running `/sync-notes`, because its conflicts
#                    and its commit messages are prose.
export def "hwt sync" [
    --project (-p): string@project-completions   # Limit to one project
] {
    let rows = (sync-checkouts $project | each {|c| sync-scan-one $c })
    let live = ($rows | where state != "skipped")
    let fetchable = ($live | where {|r| ($r.remote | is-not-empty) and $r.state != "unborn" })

    print $"fetching ($fetchable | length) checkouts..."

    # In parallel because this is the one phase that is network-bound; the results
    # are collected before anything prints, so the report stays ordered.
    let fetched = ($fetchable | par-each {|r| { row: $r, res: (sync-fetch-one $r) } })
    for f in ($fetched | where {|f| not $f.res.ok }) {
        print $"  ($f.row.project): fetch failed — ($f.res.err)"
    }

    let repos = ($fetchable | where kind == "repo")
    let trees = ($fetchable | where kind != "repo")

    print ""
    print $"main checkouts \(($repos | length)\) — fast-forward only:"
    for r in $repos { sync-pull-one $r }

    print ""
    print $"worktrees \(($trees | length)\) — wip, rebase, push:"
    for r in $trees { sync-worktree-one $r }

    # What is left needs a decision rather than a command: a first commit, a
    # `gh repo create`, or a look at what those local commits are. Naming them is
    # the whole of what the old report-only mode still had to offer.
    let held = ($repos | where unpushed > 0)
    if ($held | is-not-empty) {
        print ""
        print $"(plural ($held | length) 'main checkout') holding commits no remote has \(sync only pulls these\):"
        for r in $held { print $"  ($r.project) — ($r.unpushed) commits; git -C ($r.path) push" }
    }
    let unborn = ($live | where state == "unborn")
    if ($unborn | is-not-empty) {
        print ""
        print $"(plural ($unborn | length) 'checkout') with no commits at all — every file untracked:"
        for r in $unborn { print $"  ($r.project) — ($r.path)" }
    }
    let no_remote = ($live | where state == "no-remote")
    if ($no_remote | is-not-empty) {
        print ""
        print $"(plural ($no_remote | length) 'checkout') with commits but nowhere to push them:"
        for r in $no_remote { print $"  ($r.project) — gh repo create julianbieber/($r.project) --private --source ($r.path)" }
    }

    sync-notes-session
}
