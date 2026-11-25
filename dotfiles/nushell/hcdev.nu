# Hobby Claude Dev Session Management
#
# Launch Zellij sessions with Helix + Claude Code (hobby account) side by side.
# Ported from the work `cdev` module, restructured for single-repo hobby tasks.
#
# Usage:
#   hcdev new <task> -t <topic> [--claude] - create a worktree from the current repo + session
#   hcdev wt <task> [--claude]    - session in an existing worktree (editor or Claude mode)
#   hcdev here                    - session in the current directory
#   hcdev tab <name>              - open or focus a started aspect's tab in its task's session
#   hcdev notes                   - cd to this task's notes
#   hcdev code                    - cd to this task's worktree
#   hcdev list / attach           - manage running sessions
#   hcdev close <task>            - kill its session, its processes, and its window

const LAYOUT = "hobby-dev"
const CLAUDE_LAYOUT = "hobby-claude"
const WORKTREE_BASE = "~/fun_worktrees" | path expand
const PROJECTS_BASE = "~/fun_repos" | path expand
const NOTES_BASE = "~/fun_repos/hobby-mimisbrunnr/notes" | path expand
const HOBBY_CONFIG_DIR = "~/.claude-hobby" | path expand
const ASPECT_BASE = "~/fun_aspects" | path expand

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

def hcdev-flag [context: string, names: list<string>] {
    let parts = ($context | str trim | split row " " | where { $in != "" })
    let hit = ($parts | window 2 | where { ($in | first) in $names })
    if ($hit | is-empty) { return null }
    try { $hit | first | get 1 } catch { null }
}

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

def hcdev-find-task [task_name: string, project?: string, topic?: string] {
    let found = (hcdev-match-task $task_name $project $topic)
    if $found == null { return null }
    if not $found.present {
        error make { msg: $"Task '($found.project)/($found.topic)/($found.task)' is not checked out — run 'hwt restore ($found.task)'" }
    }
    $found
}

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

# The worktree of aspect `name` of a task:
# `~/fun_aspects/<project>/<topic>/<task>/<name>`. Outside `~/fun_worktrees` on
# purpose, so nothing that reads the third component there as a task sees it.
export def hcdev-aspect-worktree [project: string, topic: string, task: string, name: string] {
    $ASPECT_BASE | path join $project $topic $task $name
}

# The notes directory of aspect `name` of a task, which holds its
# `aspect.json`, `brief.md` and `findings.md`.
export def hcdev-aspect-notes [project: string, topic: string, task: string, name: string] {
    $NOTES_BASE | path join $project $topic $task "aspects" $name
}

# {project, topic, task, name} of the aspect worktree `dir` lies in, or null
# when it is not under `~/fun_aspects/<project>/<topic>/<task>/<name>`.
export def hcdev-aspect-of-path [dir: path] {
    let expanded = ($dir | path expand)
    if not ($expanded | str starts-with $"($ASPECT_BASE)/") { return null }
    let parts = ($expanded | str replace $"($ASPECT_BASE)/" "" | split row "/")
    if ($parts | length) < 4 { return null }
    { project: ($parts | get 0), topic: ($parts | get 1), task: ($parts | get 2), name: ($parts | get 3) }
}

def hcdev-aspect-parent [task?: string, project?: string, topic?: string] {
    if $task != null {
        let found = (hcdev-find-task $task $project $topic)
        if $found == null { error make { msg: $"Task '($task)' not found" } }
        return $found
    }
    let of_aspect = (hcdev-aspect-of-path (pwd))
    let ctx = if $of_aspect != null { $of_aspect } else { hcdev-task-context }
    let found = (hcdev-find-task $ctx.task $ctx.project $ctx.topic)
    if $found == null { error make { msg: $"Task '($ctx.task)' not found" } }
    $found
}

def hcdev-aspect-completions [context?: string] {
    let ctx = (try {
        let of_aspect = (hcdev-aspect-of-path (pwd))
        if $of_aspect != null { $of_aspect } else { hcdev-task-context }
    } catch { null })
    if $ctx == null { return [] }
    let notes = ($NOTES_BASE | path join $ctx.project $ctx.topic $ctx.task)
    try { ls ($notes | path join "aspects") | where type == dir | get name | path basename } catch { [] }
}

def hcdev-tab-target [parent: record, name: string] {
    let notes = (hcdev-aspect-notes $parent.project $parent.topic $parent.task $name)
    if not ($notes | path join "aspect.json" | path exists) {
        error make { msg: $"($parent.task) has no aspect named '($name)' — 'hwt aspect new ($name)' adds one" }
    }
    let dir = (hcdev-aspect-worktree $parent.project $parent.topic $parent.task $name)
    if not ($dir | path exists) {
        error make { msg: $"Aspect '($name)' is not started — 'hwt aspect start ($name)' checks it out and opens its tab" }
    }
    { dir: $dir, claude: "haspect" }
}

def hcdev-dev-panes [claude_cmd: string, mode: string] {
    let main = if $mode == "claude" {
        $"pane split_direction=\"vertical\" {
        pane size=\"70%\" command=\"($claude_cmd)\" name=\"claude\" close_on_exit=false
        pane size=\"30%\" command=\"bash\" name=\"overview\" close_on_exit=false {
            args \"-lc\" \"exec htab\"
        }
    }"
    } else {
        $"pane split_direction=\"vertical\" {
        pane size=\"65%\" name=\"editor\"
        pane size=\"35%\" command=\"($claude_cmd)\" name=\"claude\" close_on_exit=false
    }"
    }
    $"($main)
    pane size=1 borderless=true {
        plugin location=\"compact-bar\"
    }"
}

def hcdev-tab-layout [claude: string, mode: string] {
    $"layout {
    (hcdev-dev-panes $claude $mode)
}
"
}

def hcdev-session-mode [session: string] {
    let panes = (hcdev-session-terminal-panes $session)
    if ($panes | any {|p| $p.title == "overview" }) { "claude" } else { "editor" }
}

def hcdev-mode [claude: bool] {
    if $claude { "claude" } else { "editor" }
}

def hcdev-find-tab [session: string, name: string] {
    let names = (^zellij --session $session action query-tab-names | lines)
    $names | where {|n|
        let bare = ($n | str replace -r '^[●○] ' '')
        $bare == $name or ($bare | str starts-with $"($name) · ")
    } | get -o 0
}

def hcdev-task-tabs [parent: record] {
    let notes = ($NOTES_BASE | path join $parent.project $parent.topic $parent.task)
    let aspects = (try { ls ($notes | path join "aspects") | where type == dir | get name | path basename } catch { [] })
    | where {|name|
        let state = (try { open --raw ($notes | path join "aspects" $name "aspect.json") | from json | get -o state } catch { null })
        $state != "closed"
    }
    | each {|name| { name: $name, dir: (hcdev-aspect-worktree $parent.project $parent.topic $parent.task $name), claude: "haspect" } }
    $aspects | where {|t| $t.dir | path exists }
}

def hcdev-task-layout [task: string, tabs: list, mode: string, focus?: string] {
    let rest = ($tabs | each {|t|
        let focused = if $t.name == $focus { " focus=true" } else { "" }
        $"    tab name=($t.name | to json) cwd=($t.dir | to json)($focused) {
        (hcdev-dev-panes $t.claude $mode)
    }"
    } | str join "\n")
    let main_focus = if $focus == null { " focus=true" } else { "" }
    $"layout {
    tab name=($task | to json)($main_focus) {
        (hcdev-dev-panes 'chobby' $mode)
    }
($rest)
}
"
}

def --env hcdev-launch-task [found: record, mode: string, focus?: string, extra?: record] {
    let session = (hcdev-session-name $found.project $found.topic $found.task)
    let known = (hcdev-task-tabs $found)
    let tabs = if $extra != null and ($extra.name not-in ($known | get -o name | default [])) { $known | append $extra } else { $known }
    if ($tabs | is-empty) {
        hcdev-launch $found.path $session (if $mode == "claude" { $CLAUDE_LAYOUT } else { $LAYOUT })
        return
    }
    let layout_file = (hcdev-runtime-dir | path join $"hobby-tabs-($session).kdl")
    hcdev-task-layout $found.task $tabs $mode $focus | save -f $layout_file
    hcdev-launch $found.path $session $layout_file
}

def hcdev-open-tab [session: string, name: string, dir: string, claude: string] {
    let existing = (hcdev-find-tab $session $name)
    if $existing != null {
        ^zellij --session $session action go-to-tab-name $existing | ignore
        return
    }
    let mode = (hcdev-session-mode $session)
    let layout_file = (hcdev-runtime-dir | path join $"hobby-tab-($claude)-($mode).kdl")
    hcdev-tab-layout $claude $mode | save -f $layout_file
    ^zellij --session $session action new-tab --layout $layout_file --name $name --cwd $dir | ignore
}

def hcdev-zellij-sock-dir [] {
    let tmp_fallback = $"($env.TMPDIR? | default '/tmp')/zellij-(^id -u | str trim)"
    let base = ($env.ZELLIJ_SOCKET_DIR?
        | default $"($env.XDG_RUNTIME_DIR? | default $tmp_fallback)/zellij")
    $base | path join "contract_version_1"
}

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
] {
    let held = $"try { ($cmd) } catch {|e| print $e.msg; print ''; print 'Failed. Press enter to close.'; input }"
    with-env {ZELLIJ: null, ZELLIJ_SESSION_NAME: null, ZELLIJ_PANE_ID: null} {
        ^setsid -f alacritty --working-directory $dir --title $title -e nu --login -c $held o+e> /dev/null
    }
}

# Kill every running zellij session with no client attached, then delete every
# exited one. Sessions of any account and any task are affected, not only hcdev's.
export def zclean [] {
    let running = (^zellij list-sessions --no-formatting | lines | where $it !~ 'EXITED' | each { split row ' ' | first })
    for s in $running {
        let clients = (^zellij --session $s action list-clients | lines | skip 1 | where $it != '')
        if ($clients | is-empty) {
            ^zellij kill-session $s
            print $"killed ($s)"
        }
    }
    ^zellij delete-all-sessions --yes
}

export def hcdev-session-live [session: string] {
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

# Main hcdev command - shows help.
# Named `main` because a nushell module cannot export a command named after
# itself; `use hcdev.nu *` exposes it as `hcdev`.
export def main [] {
    print "Hobby Claude Dev Session Management"
    print ""
    print "Usage:"
    print "  hcdev new <task> -t <topic> [-c] Create worktree from current repo + start session (-c: Claude mode)"
    print "  hcdev wt <task> [-p] [-t] [-c]   Start session in an existing worktree (-c: Claude mode)"
    print "  hcdev here                       Start session in the current directory"
    print "  hcdev dir <path>                 Start session in a given directory"
    print "  hcdev tab <name> [--task] [-a] [-c]  Open or focus a started aspect's tab in its task's session"
    print "  hcdev notes                      cd to this task's notes directory"
    print "  hcdev code                       cd to this task's worktree"
    print "  hcdev list                       List running hobby sessions"
    print "  hcdev attach <session>           Attach to an existing session"
    print "  hcdev close <task> [-p] [-t]     Kill the session, stop its processes, close the window"
    print ""
    print $"Layouts: ~/.config/zellij/layouts/($LAYOUT).kdl \(editor\), ($CLAUDE_LAYOUT).kdl \(Claude, with the htab overview\)"
    print $"Account: ($HOBBY_CONFIG_DIR)  \(via chobby in the Claude pane\)"
}

# Start a session in an existing worktree, or attach to its running one. A new
# session opens with the task's tab first and one more tab per aspect that is
# not closed and whose worktree is checked out — so a session closed or lost
# comes back as it was. Every tab is Helix beside Claude, or with `--claude`
# Claude beside the `htab` overview of that tab's task or aspect; a running
# session keeps the mode it was started in.
export def --env "hcdev wt" [
    task: string@hcdev-task-completions               # Task name
    --project (-p): string@hcdev-project-completions  # Disambiguate across projects
    --topic (-t): string@hcdev-topic-completions      # Disambiguate across topics
    --claude (-c)                                     # Claude mode: Claude and the overview, no editor
] {
    let found = (hcdev-find-task $task $project $topic)
    if $found == null {
        error make { msg: $"Task '($task)' not found in ($WORKTREE_BASE)" }
    }
    hcdev-launch-task $found (hcdev-mode $claude)
}

# Create a worktree from the current git repo and start a session in it,
# Helix beside Claude, or with `--claude` Claude beside the `htab` overview.
export def --env "hcdev new" [
    task: string                                  # Task/branch name
    --topic (-t): string@hcdev-topic-completions  # Topic (required for new tasks)
    --base (-b): string                           # Base branch (defaults to HEAD)
    --claude (-c)                                 # Claude mode: Claude and the overview, no editor
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

    let notes_dir = ($NOTES_BASE | path join $repo_name $resolved_topic $task)
    if not ($notes_dir | path exists) {
        mkdir $notes_dir
        print $"Created notes dir: ($notes_dir)"
    }
    let meta_file = ($notes_dir | path join "task.json")
    if not ($meta_file | path exists) {
        { done: false, blocked: null } | to json --indent 2 | save -f $meta_file
    }

    let layout = if $claude { $CLAUDE_LAYOUT } else { $LAYOUT }
    hcdev-launch $worktree_dir (hcdev-session-name $repo_name $resolved_topic $task) $layout
}

# Open started aspect `name` of the task as a tab in the task's session, or
# switch to that tab when it is already open. The tab runs `haspect` in the
# aspect's worktree, whatever the aspect's kind, in the session's mode. The task is the one the current
# directory lies in (its worktree, or any of its aspect worktrees) unless
# `--task` names it. When the task's session is not running it is started, in
# the foreground, with the new tab focused. With `--attach`, a running session
# is also attached when the caller is not already inside it. Errors when the
# task is unknown or not checked out, when it has no aspect `name`, or when the
# aspect is not started (`hwt aspect start <name>` starts it). `--claude` picks
# Claude mode for a session this starts, and is ignored by a running one.
export def --env "hcdev tab" [
    name: string@hcdev-aspect-completions             # Aspect name
    --task: string@hcdev-task-completions             # Parent task (defaults to the current directory's)
    --project (-p): string@hcdev-project-completions  # Disambiguate the task across projects
    --topic (-t): string@hcdev-topic-completions      # Disambiguate the task across topics
    --attach (-a)                                     # Attach to the running session after opening the tab
    --claude (-c)                                     # Claude mode, when this starts the session
] {
    let parent = (hcdev-aspect-parent $task $project $topic)
    let target = (hcdev-tab-target $parent $name)
    let session = (hcdev-session-name $parent.project $parent.topic $parent.task)
    if (hcdev-session-live $session) {
        hcdev-open-tab $session $name $target.dir $target.claude
        if $attach and ($env.ZELLIJ_SESSION_NAME? != $session) {
            hcdev-launch $parent.path $session
        }
        return
    }
    hcdev-launch-task $parent (hcdev-mode $claude) $name { name: $name, dir: $target.dir, claude: $target.claude }
}

# Start a session in the current directory
export def --env "hcdev here" [] {
    let dir = (pwd)
    let parts = (hcdev-task-parts $dir)
    let session = if $parts != null {
        hcdev-session-name $parts.project $parts.topic $parts.task
    } else {
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

# Close a task's session: kill and delete its zellij session, stop every process
# still running out of its worktree or its aspects' worktrees, and
# take its window with them (the window dies with its pane process once the
# session is gone). Idempotent — a session and worktree already gone is
# `Nothing to close`, not an error.
#
# Errors when the task is the session you are currently in.
export def "hcdev close" [
    task: string@hcdev-task-completions               # Task name
    --project (-p): string@hcdev-project-completions  # Disambiguate across projects
    --topic (-t): string@hcdev-topic-completions       # Disambiguate across topics
] {
    let found = (hcdev-match-task $task $project $topic)
    if $found == null {
        error make { msg: $"Task '($task)' not found" }
    }

    let session = (hcdev-session-name $found.project $found.topic $found.task)
    if $env.ZELLIJ_SESSION_NAME? == $session {
        error make { msg: $"($session) is the session you are in — hcdev close can't close its own caller" }
    }

    let had_session = (hcdev-session-live $session)
    do -i { ^zellij kill-session $session } | complete | ignore
    do -i { ^zellij delete-session --force $session } | complete | ignore

    let aspects_dir = ($ASPECT_BASE | path join $found.project $found.topic $found.task)
    let killed = ([$found.path $aspects_dir]
        | where { $in | path exists }
        | reduce -f 0 {|d, acc| $acc + (hcdev-close-processes $d) })

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
