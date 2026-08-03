# Hobby Claude Dev Session Management
#
# Launch Zellij sessions with Helix + Claude Code (hobby account) side by side.
# Ported from the work `cdev` module, restructured for single-repo hobby tasks.
#
# Usage:
#   hcdev new <task> -t <topic>   - create a worktree from the current repo + session
#   hcdev wt <task>               - session in an existing worktree
#   hcdev here                    - session in the current directory
#   hcdev notes                   - cd to this task's notes
#   hcdev code                    - cd to this task's worktree
#   hcdev list / attach           - manage running sessions

const LAYOUT = "hobby-dev"
const WORKTREE_BASE = "~/fun_worktrees" | path expand
const PROJECTS_BASE = "~/fun_repos" | path expand
const NOTES_BASE = "~/fun_repos/hobby-mimisbrunnr/notes" | path expand
const HOBBY_CONFIG_DIR = "~/.claude-hobby" | path expand

# --- task.json metadata (mirrors hwt.nu) ------------------------------------

# Derive {project, topic, task} from a worktree path.
def hcdev-task-parts [task_path: path] {
    let expanded = ($task_path | path expand)
    if not ($expanded | str starts-with $"($WORKTREE_BASE)/") { return null }
    let parts = ($expanded | str replace $"($WORKTREE_BASE)/" "" | split row "/")
    if ($parts | length) < 3 { return null }
    { project: ($parts | get 0), topic: ($parts | get 1), task: ($parts | get 2) }
}

def hcdev-read-task-meta [project: string, topic: string, task: string] {
    let default = { done: false, blocked: null }
    let f = ($NOTES_BASE | path join $project $topic $task "task.json")
    if not ($f | path exists) { return $default }
    let parsed = (try { open --raw $f | from json } catch { null })
    if not (($parsed | describe) | str starts-with 'record') { return $default }
    { done: ($parsed.done? | default false), blocked: ($parsed.blocked? | default null) }
}

def hcdev-is-task-done [task_path: path] {
    let parts = (hcdev-task-parts $task_path)
    if $parts == null { return false }
    (hcdev-read-task-meta $parts.project $parts.topic $parts.task).done
}

# --- helpers ----------------------------------------------------------------

# Pull the value of a flag out of a completion context line.
def hcdev-flag [context: string, names: list<string>] {
    let parts = ($context | str trim | split row " " | where { $in != "" })
    let hit = ($parts | window 2 | where { ($in | first) in $names })
    if ($hit | is-empty) { return null }
    try { $hit | first | get 1 } catch { null }
}

# Every live task on disk, as [{project, topic, task, path}].
def hcdev-all-tasks [] {
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

# Find a task, optionally narrowed by project and/or topic. Ambiguity raises.
def hcdev-find-task [task_name: string, project?: string, topic?: string] {
    mut pool = (hcdev-all-tasks)
    if $project != null { $pool = ($pool | where project == $project) }
    if $topic != null { $pool = ($pool | where topic == $topic) }
    if ($pool | is-empty) { return null }

    let exact = ($pool | where task == $task_name)
    let matches = if ($exact | is-not-empty) { $exact } else {
        $pool | where { $in.task | str contains $task_name }
    }
    if ($matches | is-empty) { return null }
    if ($matches | length) == 1 { return ($matches | first) }

    print "Multiple matching tasks:"
    $matches | each {|m| print $"  ($m.project)/($m.topic)/($m.task)" }
    error make { msg: $"Task '($task_name)' is ambiguous — narrow it with -p <project> or -t <topic>" }
}

# Extract {project, topic, task} from cwd (works under ~/fun_worktrees/ or the notes tree).
def hcdev-task-context [] {
    let cwd = (pwd)
    let rel = try {
        $cwd | path relative-to $WORKTREE_BASE
    } catch {
        try {
            $cwd | path relative-to $NOTES_BASE
        } catch {
            error make { msg: $"Not in a worktree or notes dir \(cwd: ($cwd)\)" }
        }
    }
    let parts = ($rel | path split)
    if ($parts | length) < 3 {
        error make { msg: $"Can't determine project/topic/task from ($cwd)" }
    }
    { project: ($parts | get 0), topic: ($parts | get 1), task: ($parts | get 2) }
}

# The directory zellij puts its IPC sockets in — the same derivation zellij uses
# (`ZELLIJ_SOCKET_DIR`, else `$XDG_RUNTIME_DIR/zellij`, else `$TMPDIR/zellij-<uid>`,
# plus the client/server contract directory).
def hcdev-zellij-sock-dir [] {
    let tmp_fallback = $"($env.TMPDIR? | default '/tmp')/zellij-(^id -u | str trim)"
    let base = ($env.ZELLIJ_SOCKET_DIR?
        | default $"($env.XDG_RUNTIME_DIR? | default $tmp_fallback)/zellij")
    $base | path join "contract_version_1"
}

# How long a session name zellij will accept here.
#
# A session's socket is <sock-dir>/<name>, and a Unix socket path has to fit in
# sockaddr_un.sun_path — 108 bytes on Linux. zellij rejects the name outright when
# it does not, so a long task name meant `hcdev wt` died the instant its terminal
# opened. The floor keeps a nonsense sock dir from producing a zero-length budget.
def hcdev-session-name-budget [] {
    [8 (106 - (hcdev-zellij-sock-dir | str length))] | math max
}

# Bring a session name inside the budget, keeping distinct names distinct.
#
# The hash suffix is what makes truncation safe: two long names sharing a prefix
# would otherwise collapse onto one session, and `hcdev wt A` would drop you into
# B's editor and Claude pane. Every name handed to `hcdev-launch` goes through here.
export def hcdev-clamp-session-name [name: string] {
    let budget = (hcdev-session-name-budget)
    if ($name | str length) <= $budget { return $name }
    let hash = ($name | hash sha256 | str substring 0..<6)
    let head = ($name | str substring 0..<($budget - 7) | str trim --right --char '-')
    $"($head)-($hash)"
}

# Session name for a task. Three components, since the project leads the path.
# Exported so `hwt sessions` derives the same name rather than a second guess.
export def hcdev-session-name [project: string, topic: string, task: string] {
    hcdev-clamp-session-name $"hcdev-($project)-($topic)-($task)"
}

# Resolve the notes directory for a working directory, or null.
def hcdev-resolve-notes-dir [dir: string] {
    let rel = try {
        $dir | path relative-to $WORKTREE_BASE
    } catch {
        try { $dir | path relative-to $NOTES_BASE } catch { return null }
    }
    let parts = ($rel | path split)
    if ($parts | length) < 3 { return null }
    let notes_dir = ($NOTES_BASE | path join ($parts | get 0) ($parts | get 1) ($parts | get 2))
    if ($notes_dir | path exists) { $notes_dir } else { null }
}

# Launch a zellij session with the hobby-dev layout in the given directory.
#
# CLAUDE_CONFIG_DIR is exported before starting zellij so anything run in a pane
# defaults to the hobby account. That covers a `claude` typed by hand; the
# layout's Claude pane runs `chobby`, which sets the var itself, so the session
# is correct even when zellij reuses an existing server that never saw this env.
def --env hcdev-launch [dir: string, session: string] {
    let existing = try {
        zellij list-sessions --no-formatting
        | lines
        | where { $in | str starts-with $"($session) " }
        | first
    } catch {
        null
    }

    if $existing != null {
        if ($existing | str contains "EXITED") {
            print $"Cleaning up exited session: ($session)"
            zellij delete-session $session
        } else {
            # `zellij attach` *panics* when the target is the session you are already
            # in, so say it plainly instead. The dashboards do not hit this — they
            # strip the zellij variables from the terminal they spawn — but a
            # hand-typed `hcdev wt <this task>` from inside its own session would.
            if ($env.ZELLIJ_SESSION_NAME? == $session) {
                print $"Already in session: ($session)"
                return
            }
            print $"Attaching to existing session: ($session)"
            zellij attach $session
            return
        }
    }

    $env.CLAUDE_CONFIG_DIR = $HOBBY_CONFIG_DIR

    let notes_dir = (hcdev-resolve-notes-dir $dir)
    if $notes_dir != null {
        $env.HCDEV_NOTES_DIR = $notes_dir
    }

    print $"Starting hobby-dev session: ($session) in ($dir)"
    cd $dir
    zellij -s $session -n $LAYOUT
}

# --- completions ------------------------------------------------------------

def hcdev-session-completions [] {
    try {
        zellij list-sessions --short
        | lines
        | where { $in | str starts-with "hcdev-" }
        | each { { value: $in, description: "running session" } }
    } catch { [] }
}

def hcdev-project-completions [] {
    if not ($PROJECTS_BASE | path exists) { return [] }
    try {
        ls $PROJECTS_BASE
        | where type == dir
        | get name
        | where { ($in | path join ".git") | path exists }
        | each { path basename }
    } catch { [] }
}

# Topics come from the notes tree, not ~/fun_worktrees — mirroring hwt.nu, and
# for the same reason: `hwt rm` prunes an emptied topic directory on the worktree
# side while the notes keep it, so completing from the worktrees would hide every
# topic that has no checked-out task right now. `hcdev new -t <topic>` needs the
# full list most of all: it is the one command whose topic argument names a place
# that does not exist on the worktree side yet.
def hcdev-topic-completions [context?: string] {
    let proj_filter = if $context != null { hcdev-flag $context ["--project" "-p"] } else { null }

    let notes_topics = if ($NOTES_BASE | path exists) {
        try {
            glob $"($NOTES_BASE)/*/*" --no-file --no-symlink
            | each {|d|
                let rel = ($d | path relative-to $NOTES_BASE | path split)
                { project: ($rel | get 0), topic: ($rel | get 1) }
            }
        } catch { [] }
    } else { [] }

    let live = (hcdev-all-tasks)

    $notes_topics
    | append ($live | each {|t| { project: $t.project, topic: $t.topic } })
    | where {|r| ($proj_filter == null) or ($r.project == $proj_filter) }
    | uniq-by project topic
    | group-by topic
    | items {|topic, rows|
        let projects = ($rows | get project | uniq | sort | str join ", ")
        let open = ($live | where topic == $topic | where {|t| not (hcdev-is-task-done $t.path) } | length)
        {
            value: $topic
            description: (if $open > 0 { $"($projects) — ($open) open" } else { $projects })
            rank: (if $open > 0 { 0 } else { 1 })
        }
    }
    | sort-by rank value
    | each {|r| { value: $r.value, description: $r.description } }
}

# Tasks with a checkout on disk. Unlike topics this stays worktree-based: the one
# task argument on this module (`hcdev wt`) opens an editor and a Claude pane in
# the worktree, so a task from the notes tree with no checkout is not a session
# `hcdev` could start. `hwt restore <task>` is the step in between.
def hcdev-task-completions [context?: string] {
    if not ($WORKTREE_BASE | path exists) { return [] }
    let proj_filter = if $context != null { hcdev-flag $context ["--project" "-p"] } else { null }
    let topic_filter = if $context != null { hcdev-flag $context ["--topic" "-t"] } else { null }

    mut pool = (hcdev-all-tasks)
    if $proj_filter != null { $pool = ($pool | where project == $proj_filter) }
    if $topic_filter != null { $pool = ($pool | where topic == $topic_filter) }

    $pool | each {|t|
        let done = (hcdev-is-task-done $t.path)
        let tag = $"($t.project)/($t.topic)"
        { value: $t.task, description: (if $done { $"($tag) [done]" } else { $tag }), done: $done }
    }
    | sort-by done value
    | each {|r| { value: $r.value, description: $r.description } }
}

# --- commands ---------------------------------------------------------------

# Main hcdev command - shows help.
# Named `main` because a nushell module cannot export a command named after
# itself; `use hcdev.nu *` exposes it as `hcdev`.
export def main [] {
    print "Hobby Claude Dev Session Management"
    print ""
    print "Usage:"
    print "  hcdev new <task> -t <topic>      Create worktree from current repo + start session"
    print "  hcdev wt <task> [-p] [-t]        Start session in an existing worktree"
    print "  hcdev here                       Start session in the current directory"
    print "  hcdev dir <path>                 Start session in a given directory"
    print "  hcdev notes                      cd to this task's notes directory"
    print "  hcdev code                       cd to this task's worktree"
    print "  hcdev list                       List running hobby sessions"
    print "  hcdev attach <session>           Attach to an existing session"
    print ""
    print $"Layout:  ~/.config/zellij/layouts/($LAYOUT).kdl"
    print $"Account: ($HOBBY_CONFIG_DIR)  \(via chobby in the Claude pane\)"
}

# Start a session in an existing worktree
export def --env "hcdev wt" [
    task: string@hcdev-task-completions               # Task name
    --project (-p): string@hcdev-project-completions  # Disambiguate across projects
    --topic (-t): string@hcdev-topic-completions      # Disambiguate across topics
] {
    let found = (hcdev-find-task $task $project $topic)
    if $found == null {
        error make { msg: $"Task '($task)' not found in ($WORKTREE_BASE)" }
    }
    hcdev-launch $found.path (hcdev-session-name $found.project $found.topic $found.task)
}

# Create a worktree from the current git repo and start a session
export def --env "hcdev new" [
    task: string                                  # Task/branch name
    --topic (-t): string@hcdev-topic-completions  # Topic (required for new tasks)
    --base (-b): string                           # Base branch (defaults to HEAD)
] {
    let repo_root = (^git rev-parse --show-toplevel | str trim)
    let repo_name = ($repo_root | path basename)
    let main_repo = ($PROJECTS_BASE | path join $repo_name)

    if not ($main_repo | path exists) {
        error make { msg: $"No main repo at ($main_repo) — run from a repo checked out in ($PROJECTS_BASE)/" }
    }

    let resolved_topic = if $topic != null {
        $topic
    } else {
        let found = (hcdev-find-task $task $repo_name)
        if $found != null { $found.topic } else {
            error make { msg: "Topic is required for new tasks. Use --topic (-t)." }
        }
    }

    let worktree_dir = ($WORKTREE_BASE | path join $repo_name $resolved_topic $task)

    if not ($worktree_dir | path exists) {
        # git worktree add creates the leaf; only the parents need making.
        let topic_dir = ($WORKTREE_BASE | path join $repo_name $resolved_topic)
        if not ($topic_dir | path exists) { mkdir $topic_dir }

        let branch_exists = (do -i { ^git -C $main_repo rev-parse --verify $task } | complete).exit_code == 0
        if $branch_exists {
            ^git -C $main_repo worktree add $worktree_dir $task
        } else {
            let base_branch = if $base != null { $base } else { "HEAD" }
            ^git -C $main_repo worktree add -b $task $worktree_dir $base_branch
        }
        print $"Created worktree: ($worktree_dir)"
    } else {
        print $"Worktree already exists: ($worktree_dir)"
    }

    # Notes dir name must match the worktree task name so journals, prs.json, and
    # subtasks.json line up with this worktree.
    let notes_dir = ($NOTES_BASE | path join $repo_name $resolved_topic $task)
    if not ($notes_dir | path exists) {
        mkdir $notes_dir
        print $"Created notes dir: ($notes_dir)"
    }
    let meta_file = ($notes_dir | path join "task.json")
    if not ($meta_file | path exists) {
        { done: false, blocked: null } | to json --indent 2 | save -f $meta_file
    }

    hcdev-launch $worktree_dir (hcdev-session-name $repo_name $resolved_topic $task)
}

# Start a session in the current directory
export def --env "hcdev here" [] {
    let dir = (pwd)
    let parts = (hcdev-task-parts $dir)
    let session = if $parts != null {
        hcdev-session-name $parts.project $parts.topic $parts.task
    } else {
        # Outside the worktree tree: name the session after the directory.
        hcdev-clamp-session-name $"hcdev-($dir | path basename)"
    }
    hcdev-launch $dir $session
}

# Start a session in a given directory
export def --env "hcdev dir" [
    path: path  # Directory to start in
] {
    let dir = ($path | path expand)
    if not ($dir | path exists) {
        error make { msg: $"Directory not found: ($dir)" }
    }
    let parts = (hcdev-task-parts $dir)
    let session = if $parts != null {
        hcdev-session-name $parts.project $parts.topic $parts.task
    } else {
        hcdev-clamp-session-name $"hcdev-($dir | path basename)"
    }
    hcdev-launch $dir $session
}

# List running hobby sessions
export def "hcdev list" [] {
    try {
        let sessions = (zellij list-sessions --short | lines | where { $in | str starts-with "hcdev-" })
        if ($sessions | is-empty) {
            print "No running hobby sessions"
        } else {
            $sessions | each { print $in }
        }
    } catch {
        print "No running hobby sessions (zellij not running?)"
    }
}

# Attach to a running hobby session
export def --env "hcdev attach" [
    session: string@hcdev-session-completions  # Session name
] {
    zellij attach $session
}

# cd to the notes directory for the current task
export def --env "hcdev notes" [] {
    let ctx = (hcdev-task-context)
    let notes_dir = ($NOTES_BASE | path join $ctx.project $ctx.topic $ctx.task)
    if not ($notes_dir | path exists) {
        error make { msg: $"No notes for ($ctx.project)/($ctx.topic)/($ctx.task)" }
    }
    cd $notes_dir
}

# cd to the worktree for the current task. Useful from the notes dir — the
# counterpart of `hcdev notes`. (The work module's `cdev project` took a repo
# argument; a hobby task has exactly one worktree, so there is nothing to pick.)
export def --env "hcdev code" [] {
    let ctx = (hcdev-task-context)
    let wt = ($WORKTREE_BASE | path join $ctx.project $ctx.topic $ctx.task)
    if not ($wt | path exists) {
        error make { msg: $"No worktree for ($ctx.project)/($ctx.topic)/($ctx.task) — 'hwt restore ($ctx.task)' re-creates it" }
    }
    cd $wt
}
