# Hobby Claude Dev Session Management
#
# Launch Zellij sessions with Helix + Claude Code (hobby account) side by side.
# Ported from the work `cdev` module, restructured for single-repo hobby tasks.
#
# Usage:
#   hcdev new <task> -t <topic>   - create a worktree from the current repo + session
#   hcdev wt <task>               - session in an existing worktree
#   hcdev here                    - session in the current directory
#   hcdev auto <task>             - session whose Claude pane starts /task:auto
#   hcdev coordinator <blurb>     - coordinator session: task list + Claude, from a main checkout
#   hcdev notes                   - cd to this task's notes
#   hcdev code                    - cd to this task's worktree
#   hcdev list / attach           - manage running sessions
#   hcdev close <task>            - kill its session, its processes, and its window
#   hcdev claude-pane <task>      - resolve the Claude pane id of a running auto session

const LAYOUT = "hobby-dev"
const AUTO_LAYOUT = "hobby-auto"
const COORDINATOR_TOPIC = "coordinator"
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

# --- helpers ----------------------------------------------------------------

# Pull the value of a flag out of a completion context line.
def hcdev-flag [context: string, names: list<string>] {
    let parts = ($context | str trim | split row " " | where { $in != "" })
    let hit = ($parts | window 2 | where { ($in | first) in $names })
    if ($hit | is-empty) { return null }
    try { $hit | first | get 1 } catch { null }
}

# Every task the notes repo records, as
# [{project, topic, task, path, present, done}]. Mirrors `hwt.nu`'s known-tasks:
# a task.json is what makes a task, and the worktree tree only answers `present`.
def hcdev-known-tasks [] {
    if not ($NOTES_BASE | path exists) { return [] }
    try {
        glob $"($NOTES_BASE)/*/*/*/task.json" --no-dir --no-symlink
        | each {|f|
            let rel = ($f | path dirname | path relative-to $NOTES_BASE | path split)
            let project = ($rel | get 0)
            let topic = ($rel | get 1)
            let task = ($rel | get 2)
            let wt = ($WORKTREE_BASE | path join $project $topic $task)
            {
                project: $project
                topic: $topic
                task: $task
                path: $wt
                present: ($wt | path exists)
                done: (hcdev-read-task-meta $project $topic $task).done
            }
        }
    } catch { [] }
}

def hcdev-match-task [task_name: string, project?: string, topic?: string] {
    mut pool = (hcdev-known-tasks)
    if $project != null { $pool = ($pool | where project == $project) }
    if $topic != null { $pool = ($pool | where topic == $topic) }
    if ($pool | is-empty) { return null }

    let exact = ($pool | where task == $task_name)
    let matches = if ($exact | is-not-empty) { $exact } else {
        $pool | where { $in.task | str contains $task_name }
    }
    if ($matches | is-empty) { return null }
    if ($matches | length) > 1 {
        print "Multiple matching tasks:"
        $matches | each {|m| print $"  ($m.project)/($m.topic)/($m.task)" }
        error make { msg: $"Task '($task_name)' is ambiguous — narrow it with -p <project> or -t <topic>" }
    }
    $matches | first
}

# Find a task that is checked out, optionally narrowed by project and/or topic.
# Ambiguity raises.
#
# Resolves against the notes tree so a task that exists but was `hwt rm`'d is
# reported as removed and not as missing — `hcdev` opens an editor and a Claude
# pane in the worktree, so `hwt restore` is the step in between.
def hcdev-find-task [task_name: string, project?: string, topic?: string] {
    let found = (hcdev-match-task $task_name $project $topic)
    if $found == null { return null }
    if $found.topic == $COORDINATOR_TOPIC {
        error make { msg: $"'($found.task)' is a coordinator, not a worktree — run 'hcdev coordinator ($found.task) -p ($found.project)'" }
    }
    if not $found.present {
        error make { msg: $"Task '($found.project)/($found.topic)/($found.task)' is not checked out — run 'hwt restore ($found.task)'" }
    }
    $found
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
    hcdev-clamp-to $name (hcdev-session-name-budget)
}

def hcdev-clamp-to [name: string, budget: int] {
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

# Session names for many tasks at once, in input order, from records of `project`, `topic` and
# `task` — what `hdash` asks for. Equal to `hcdev-session-name` on each record.
export def hcdev-session-names []: list<record> -> list<string> {
    let budget = (hcdev-session-name-budget)
    $in | each {|t| hcdev-clamp-to $"hcdev-($t.project)-($t.topic)-($t.task)" $budget }
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
def --env hcdev-launch [dir: string, session: string, layout: string = $LAYOUT] {
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

    print $"Starting ($layout | path basename | str replace '.kdl' '') session: ($session) in ($dir)"
    cd $dir
    zellij -s $session -n $layout
}

# Open `cmd` in a new detached alacritty window and return at once. Strips the
# zellij env vars so the new terminal is not mistaken for a pane of the caller's
# session, and redirects the terminal's stdout/stderr to /dev/null so a caller
# that reads its own output to EOF (`nu -l -c "…"`, `$(…)`) does not block on the
# window's lifetime — a terminal otherwise inherits the caller's stdio. A failed
# `cmd` holds the window open with its error instead of the window vanishing.
export def hcdev-spawn-window [
    dir: string      # working directory for the new terminal
    title: string    # window title
    cmd: string      # nu command to run in it
    --class: string  # WM_CLASS, for a window-manager rule
] {
    let held = $"try { ($cmd) } catch {|e| print $e.msg; print ''; print 'Failed. Press enter to close.'; input }"
    let class_args = if $class != null { ["--class" $"($class),($class)"] } else { [] }
    with-env {ZELLIJ: null, ZELLIJ_SESSION_NAME: null, ZELLIJ_PANE_ID: null} {
        ^setsid -f alacritty ...$class_args --working-directory $dir --title $title -e nu --login -c $held o+e> /dev/null
    }
}

def hcdev-session-live [session: string] {
    let lines = (try { zellij list-sessions --no-formatting | lines } catch { [] })
    let m = ($lines | where { $in | str starts-with $"($session) " })
    if ($m | is-empty) { false } else { not (($m | first) | str contains "EXITED") }
}

def hcdev-session-terminal-panes [session: string] {
    let result = (do -i { ^zellij --session $session action list-panes -a --json } | complete)
    if $result.exit_code != 0 { return [] }
    let panes = (try { $result.stdout | from json } catch { [] })
    $panes
    | where { not $in.is_plugin }
    | each {|p| {
        id: $"terminal_($p.id)"
        terminal_command: ($p.terminal_command? | default null)
        pane_command: ($p.pane_command? | default null)
        title: ($p.title? | default null)
    }}
}

def hcdev-worktree-pids [dir: string] {
    let result = (do -i {
        ^find /proc -mindepth 2 -maxdepth 2 -type l "(" -name cwd -o -name exe ")" "(" -lname $dir -o -lname $"($dir)/*" ")" -printf "%h\n"
    } | complete)
    if $result.exit_code != 0 and ($result.stdout | str trim) == "" { return [] }
    $result.stdout
    | lines
    | where { ($in | str trim) != "" }
    | each { $in | str replace "/proc/" "" | into int }
    | uniq
}

def hcdev-protected-pids [] {
    let table = (try { ^ps -e -o pid= -o ppid= | lines | each {|l|
        let parts = ($l | str trim | split row -r '\s+')
        { pid: ($parts | get 0 | into int), ppid: ($parts | get 1 | into int) }
    } } catch { [] })

    mut protected = [$nu.pid]
    mut current = $nu.pid
    loop {
        let row = ($table | where pid == $current)
        if ($row | is-empty) { break }
        let ppid = ($row | first | get ppid)
        if $ppid == 0 or $ppid == 1 or $ppid in $protected { break }
        $protected = ($protected | append $ppid)
        $current = $ppid
    }
    $protected
}

def hcdev-close-processes [dir: string] {
    let protected = (hcdev-protected-pids)
    let targets = (hcdev-worktree-pids $dir | where { $in not-in $protected })
    if ($targets | is-empty) { return 0 }

    $targets | each {|p| ^kill -TERM $p | complete | ignore }

    mut waited_ms = 0
    while $waited_ms < 3000 {
        let remaining = (hcdev-worktree-pids $dir | where { $in in $targets })
        if ($remaining | is-empty) { break }
        sleep 250ms
        $waited_ms = $waited_ms + 250
    }
    let remaining = (hcdev-worktree-pids $dir | where { $in in $targets })
    $remaining | each {|p| ^kill -KILL $p | complete | ignore }

    $targets | length
}

def hcdev-runtime-dir [] {
    $env.XDG_RUNTIME_DIR? | default ($env.TMPDIR? | default "/tmp")
}

def hcdev-coordinator-layout [notes_dir: string] {
    let quoted = ($notes_dir | to json)
    $"layout {
    pane split_direction=\"vertical\" {
        pane size=\"15%\" name=\"tasks\" command=\"hcoord\" close_on_exit=false {
            args \"tui\" ($quoted)
        }
        pane name=\"claude\" command=\"env\" close_on_exit=false {
            args ($"HCOORD_NOTES_DIR=($notes_dir)" | to json) \"chobby\"
        }
    }
}
"
}

def hcdev-main-checkout-project [] {
    let root = (do -i { ^git rev-parse --show-toplevel } | complete)
    if $root.exit_code != 0 {
        error make { msg: $"Not in a git repo — run from ($PROJECTS_BASE)/<project>, or pass -p <project>" }
    }
    let path = ($root.stdout | str trim)
    if ($path | path dirname) != $PROJECTS_BASE {
        error make { msg: $"($path) is not a main checkout under ($PROJECTS_BASE) — run from there, or pass -p <project>" }
    }
    $path | path basename
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

def hcdev-coordinator-completions [context?: string] {
    let proj_filter = if $context != null { hcdev-flag $context ["--project" "-p"] } else { null }
    if not ($NOTES_BASE | path exists) { return [] }
    try {
        glob $"($NOTES_BASE)/*/($COORDINATOR_TOPIC)/*/tasks.json" --no-dir --no-symlink
        | each {|f|
            let rel = ($f | path dirname | path relative-to $NOTES_BASE | path split)
            { project: ($rel | get 0), blurb: ($rel | get 2) }
        }
        | where {|r| ($proj_filter == null) or ($r.project == $proj_filter) }
        | each {|r| { value: $r.blurb, description: $r.project } }
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

    let live = (hcdev-known-tasks)

    $notes_topics
    | append ($live | each {|t| { project: $t.project, topic: $t.topic } })
    | where {|r| ($proj_filter == null) or ($r.project == $proj_filter) }
    | uniq-by project topic
    | group-by topic
    | items {|topic, rows|
        let projects = ($rows | get project | uniq | sort | str join ", ")
        let open = ($live | where topic == $topic | where {|t| not $t.done } | length)
        {
            value: $topic
            description: (if $open > 0 { $"($projects) — ($open) open" } else { $projects })
            rank: (if $open > 0 { 0 } else { 1 })
        }
    }
    | sort-by rank value
    | each {|r| { value: $r.value, description: $r.description } }
}

# Tasks with a checkout on disk. Unlike topics this filters on `present`: the one
# task argument on this module (`hcdev wt`) opens an editor and a Claude pane in
# the worktree, so a task with no checkout is not a session `hcdev` could start.
# `hwt restore <task>` is the step in between — and `hcdev-find-task` names it,
# so a removed task typed out in full still gets the right error.
def hcdev-task-completions [context?: string] {
    let proj_filter = if $context != null { hcdev-flag $context ["--project" "-p"] } else { null }
    let topic_filter = if $context != null { hcdev-flag $context ["--topic" "-t"] } else { null }

    mut pool = (hcdev-known-tasks | where present)
    if $proj_filter != null { $pool = ($pool | where project == $proj_filter) }
    if $topic_filter != null { $pool = ($pool | where topic == $topic_filter) }

    $pool | each {|t|
        let tag = $"($t.project)/($t.topic)"
        { value: $t.task, description: (if $t.done { $"($tag) [done]" } else { $tag }), done: $t.done }
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
    print "  hcdev auto <task> [-p] [-t] [-w] Session whose Claude pane starts /task:auto (-w: new window)"
    print "  hcdev coordinator <blurb> [-p]   Coordinator session (task list + Claude) from a main checkout"
    print "  hcdev notes                      cd to this task's notes directory"
    print "  hcdev code                       cd to this task's worktree"
    print "  hcdev list                       List running hobby sessions"
    print "  hcdev attach <session>           Attach to an existing session"
    print "  hcdev close <task> [-p] [-t]     Kill the session, stop its processes, close the window"
    print "  hcdev claude-pane <task> [-p] [-t] [--json]  Resolve the Claude pane id of a running auto session"
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

# Start a session in an existing worktree whose Claude pane runs `hauto`, the
# /task:auto stage driver (a headless plan process, then a fresh build session).
# Errors as `hcdev wt` does: unknown, ambiguous, or not checked out. With
# `--window` it opens in a new alacritty window, detached, and returns once that
# session is up (or errors within 20s if it never comes up) — even from inside a
# zellij pane; a failure in the window holds it open with its message. That
# window carries WM_CLASS `hcdev-auto`, so a window-manager rule can send it to a
# fixed desktop (leftwm: a `window_rules` entry on that class). Without
# `--window` this stays in the foreground, same as `hcdev wt`.
export def --env "hcdev auto" [
    task: string@hcdev-task-completions               # Task name
    --project (-p): string@hcdev-project-completions  # Disambiguate across projects
    --topic (-t): string@hcdev-topic-completions      # Disambiguate across topics
    --window (-w)                                     # Open in a new terminal window, detached
] {
    let found = (hcdev-find-task $task $project $topic)
    if $found == null {
        error make { msg: $"Task '($task)' not found in ($WORKTREE_BASE)" }
    }
    let session = (hcdev-session-name $found.project $found.topic $found.task)
    if $window {
        let cmd = $"hcdev auto '($found.task)' --project '($found.project)' --topic '($found.topic)'"
        hcdev-spawn-window $found.path $session $cmd --class "hcdev-auto"
        mut waited_ms = 0
        while (not (hcdev-session-live $session)) and $waited_ms < 20000 {
            sleep 250ms
            $waited_ms = $waited_ms + 250
        }
        if not (hcdev-session-live $session) {
            error make { msg: $"Session ($session) did not come up within 20s" }
        }
        print $"Opened ($session) in a new window"
        return
    }
    hcdev-launch $found.path $session $AUTO_LAYOUT
}

# Start (or attach to) a coordinator session: the task list on the left, Claude on
# the right, in the project's main checkout. Without -p the current directory must
# be a main checkout under ~/fun_repos. Creates the coordinator's notes directory,
# `task.json` and an empty `tasks.json` when absent; needs `hcoord` on PATH.
export def --env "hcdev coordinator" [
    blurb: string@hcdev-coordinator-completions       # Coordinator name (its notes dir)
    --project (-p): string@hcdev-project-completions  # Project (defaults to the current main checkout)
] {
    let proj = if $project != null { $project } else { hcdev-main-checkout-project }
    let main_repo = ($PROJECTS_BASE | path join $proj)
    if not ($main_repo | path exists) {
        error make { msg: $"No main checkout at ($main_repo)" }
    }

    let notes_dir = ($NOTES_BASE | path join $proj $COORDINATOR_TOPIC $blurb)
    if not ($notes_dir | path exists) {
        mkdir $notes_dir
        print $"Created coordinator notes dir: ($notes_dir)"
    }
    let meta_file = ($notes_dir | path join "task.json")
    if not ($meta_file | path exists) {
        { done: false, blocked: null } | to json --indent 2 | save -f $meta_file
    }
    let tasks_file = ($notes_dir | path join "tasks.json")
    if not ($tasks_file | path exists) {
        {
            project: $proj
            blurb: $blurb
            description: ""
            created: (date now | format date "%Y-%m-%d")
            model: "sonnet"
            tasks: []
        } | to json --indent 2 | save -f $tasks_file
    }

    let session = (hcdev-session-name $proj $COORDINATOR_TOPIC $blurb)
    let layout_file = (hcdev-runtime-dir | path join $"hobby-coordinator-($session).kdl")
    hcdev-coordinator-layout $notes_dir | save -f $layout_file
    hcdev-launch $main_repo $session $layout_file
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

# Resolve the Claude pane of a running `hcdev auto` session to its zellij pane id.
# Errors naming `hcdev auto <task>` when the session is not live.
#
# Prefers the id `hauto` recorded for itself in the task's `claude-pane.json`
# (kept fresh on every start, so a stale record from a prior session is never
# trusted once its pane is gone); otherwise falls back to the single live
# terminal pane whose `terminal_command` is `hauto`/`chobby`, then to the single
# one whose `pane_command` starts with `claude`. Two or more candidates at a
# step is not a match — it never guesses between them. Prints the bare pane id
# (e.g. `terminal_1`), so `zellij action focus-pane-id (hcdev claude-pane
# <task>)` composes; `--json` also prints the session name and which source won.
export def "hcdev claude-pane" [
    task: string@hcdev-task-completions               # Task name
    --project (-p): string@hcdev-project-completions  # Disambiguate across projects
    --topic (-t): string@hcdev-topic-completions       # Disambiguate across topics
    --json                                             # Print {session, pane_id, source} instead of the bare id
] {
    let found = (hcdev-match-task $task $project $topic)
    if $found == null {
        error make { msg: $"Task '($task)' not found" }
    }
    let session = (hcdev-session-name $found.project $found.topic $found.task)
    if not (hcdev-session-live $session) {
        error make { msg: $"($session) is not running — start it with 'hcdev auto ($found.task)'" }
    }

    let panes = (hcdev-session-terminal-panes $session)
    let recorded_file = ($NOTES_BASE | path join $found.project $found.topic $found.task "claude-pane.json")
    let recorded = if ($recorded_file | path exists) {
        try { open --raw $recorded_file | from json } catch { null }
    } else { null }

    let resolved = if ($recorded != null) and ($recorded.session? == $session) and ($recorded.pane_id? in ($panes | get id)) {
        { pane_id: $recorded.pane_id, source: "recorded" }
    } else {
        let by_terminal_command = ($panes | where { $in.terminal_command in ["hauto" "chobby"] })
        let by_pane_command = ($panes | where { ($in.pane_command | default "") | str starts-with "claude" })
        if ($by_terminal_command | length) == 1 {
            { pane_id: ($by_terminal_command | first | get id), source: "terminal_command" }
        } else if ($by_pane_command | length) == 1 {
            { pane_id: ($by_pane_command | first | get id), source: "pane_command" }
        } else {
            error make { msg: $"Could not resolve the Claude pane of ($session) — saw: ($panes | to json --raw)" }
        }
    }

    if $json {
        print ({ session: $session, pane_id: $resolved.pane_id, source: $resolved.source } | to json --raw)
    } else {
        print $resolved.pane_id
    }
}

# Close a task's session: kill and delete its zellij session, stop every process
# still running out of its worktree, and take its window with them (the window
# dies with its pane process once the session is gone). Idempotent — a session
# and worktree already gone is `Nothing to close`, not an error.
#
# Refuses a coordinator (the reserved topic) and refuses the session you are
# currently in, so this can never be the thing that kills its own caller —
# closing the session you're in from inside it is #48's job, not this one's.
export def "hcdev close" [
    task: string@hcdev-task-completions               # Task name
    --project (-p): string@hcdev-project-completions  # Disambiguate across projects
    --topic (-t): string@hcdev-topic-completions       # Disambiguate across topics
] {
    let found = (hcdev-match-task $task $project $topic)
    if $found == null {
        error make { msg: $"Task '($task)' not found" }
    }
    if $found.topic == $COORDINATOR_TOPIC {
        error make { msg: $"'($found.task)' is a coordinator — hcdev close does not close coordinator sessions" }
    }

    let session = (hcdev-session-name $found.project $found.topic $found.task)
    if $env.ZELLIJ_SESSION_NAME? == $session {
        error make { msg: $"($session) is the session you are in — hcdev close can't close its own caller" }
    }

    let had_session = (hcdev-session-live $session)
    do -i { ^zellij kill-session $session } | complete | ignore
    do -i { ^zellij delete-session --force $session } | complete | ignore

    let killed = if ($found.path | path exists) { hcdev-close-processes $found.path } else { 0 }

    if not $had_session and $killed == 0 {
        print "Nothing to close"
    } else {
        let session_note = if $had_session { "deleted" } else { "already gone" }
        print $"Closed ($session): session ($session_note), ($killed) process\(es\) stopped"
    }
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
