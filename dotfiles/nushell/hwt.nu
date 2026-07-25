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
const NOTES_BASE = "~/fun_repos/hobby-mimisbrunnr/notes" | path expand

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
# `hwt restore` re-create the checkout. Gitignored: this is local machine state.
#
# Unlike the work module there is no `repos` field — the project is in the path,
# and a task is exactly one repo.

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

# A topic is done iff it has at least one task and every task is done.
def is-topic-done [topic_path: path] {
    let tasks = try { ls $topic_path | where type == dir | get name } catch { return false }
    if ($tasks | is-empty) { return false }
    ($tasks | all { is-task-done $in })
}

# A project is done iff it has at least one topic and every topic is done.
def is-project-done [project_path: path] {
    let topics = try { ls $project_path | where type == dir | get name } catch { return false }
    if ($topics | is-empty) { return false }
    ($topics | all { is-topic-done $in })
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

# --- completions ------------------------------------------------------------

# Projects: git repos under ~/fun_repos (done projects sort last and are labelled)
def project-completions [] {
    if not ($PROJECTS_BASE | path exists) { return [] }
    try {
        ls $PROJECTS_BASE
        | where type == dir
        | get name
        | where { is-git-repo $in }
        | each {|p|
            let name = ($p | path basename)
            let wt = ($WORKTREE_BASE | path join $name)
            let has_tasks = ($wt | path exists)
            let done = (if $has_tasks { is-project-done $wt } else { false })
            let desc = if $done { "project [done]" } else if $has_tasks { "project" } else { "project (no tasks)" }
            { value: $name, description: $desc, done: $done }
        }
        | sort-by done value
        | each {|r| { value: $r.value, description: $r.description } }
    } catch { [] }
}

# Topics, filtered to --project/-p when that flag is already present
def topic-completions [context?: string] {
    if not ($WORKTREE_BASE | path exists) { return [] }
    let proj_filter = if $context != null { flag-from-context $context ["--project" "-p"] } else { null }
    try {
        let projects = if $proj_filter != null {
            let pp = ($WORKTREE_BASE | path join $proj_filter)
            if ($pp | path exists) { [$pp] } else { [] }
        } else {
            ls $WORKTREE_BASE | where type == dir | get name
        }
        $projects | each {|project_path|
            let project = ($project_path | path basename)
            try {
                ls $project_path | where type == dir | get name | each {|tp|
                    let done = (is-topic-done $tp)
                    let desc = if $done { $"($project) [done]" } else { $project }
                    { value: ($tp | path basename), description: $desc, done: $done }
                }
            } catch { [] }
        }
        | flatten
        | uniq-by value
        | sort-by done value
        | each {|r| { value: $r.value, description: $r.description } }
    } catch { [] }
}

# Tasks across all projects/topics, narrowed by --project/-p and --topic/-t
def task-completions [context?: string] {
    if not ($WORKTREE_BASE | path exists) { return [] }
    let proj_filter = if $context != null { flag-from-context $context ["--project" "-p"] } else { null }
    let topic_filter = if $context != null { flag-from-context $context ["--topic" "-t"] } else { null }

    try {
        let projects = if $proj_filter != null {
            let pp = ($WORKTREE_BASE | path join $proj_filter)
            if ($pp | path exists) { [$pp] } else { [] }
        } else {
            ls $WORKTREE_BASE | where type == dir | get name
        }

        $projects | each {|project_path|
            let project = ($project_path | path basename)
            let topics = try {
                if $topic_filter != null {
                    let tp = ($project_path | path join $topic_filter)
                    if ($tp | path exists) { [$tp] } else { [] }
                } else {
                    ls $project_path | where type == dir | get name
                }
            } catch { [] }

            $topics | each {|topic_path|
                let topic = ($topic_path | path basename)
                try {
                    ls $topic_path | where type == dir | get name | each {|t|
                        let done = (is-task-done $t)
                        let tag = $"($project)/($topic)"
                        let desc = if $done { $"($tag) [done]" } else { $tag }
                        { value: ($t | path basename), description: $desc, done: $done }
                    }
                } catch { [] }
            } | flatten
        }
        | flatten
        | sort-by done value
        | each {|r| { value: $r.value, description: $r.description } }
    } catch { [] }
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

# Every live task on disk, as [{project, topic, task, path}].
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

# Find a task across all projects and topics. Exact match wins over fuzzy.
# Returns {project, topic, task, path}, or null when nothing matches. Ambiguity
# raises instead of returning null, so callers report "ambiguous" rather than
# the misleading "not found".
def find-task [task_name: string, project?: string] {
    let pool = if $project != null { all-tasks | where project == $project } else { all-tasks }
    if ($pool | is-empty) { return null }

    let exact = ($pool | where task == $task_name)
    let matches = if ($exact | is-not-empty) { $exact } else {
        $pool | where { $in.task | str contains $task_name }
    }

    if ($matches | is-empty) { return null }
    if ($matches | length) == 1 { return ($matches | first) }

    print "Multiple matching tasks:"
    $matches | each {|m| print $"  ($m.project)/($m.topic)/($m.task)" }
    error make { msg: $"Task '($task_name)' is ambiguous — narrow it with -p <project>, or give the full task name" }
}

# Every task that has task.json metadata in the notes repo, as
# [{project, topic, task, path (intended worktree dir), present}].
# This sees removed tasks (worktree gone) that all-tasks cannot.
#
# The work module filters these by git-commit author, because its notes repo is
# shared with the team. Here there is exactly one author, so every notes task is
# mine and no ownership check is needed.
def notes-tasks [] {
    if not ($NOTES_BASE | path exists) { return [] }
    try {
        glob $"($NOTES_BASE)/*/*/*/task.json"
        | each {|f|
            let task_dir = ($f | path dirname)
            let rel = ($task_dir | path relative-to $NOTES_BASE | path split)
            if ($rel | length) >= 3 {
                let project = ($rel | get 0)
                let topic = ($rel | get 1)
                let task = ($rel | get 2)
                let wt = ($WORKTREE_BASE | path join $project $topic $task)
                { project: $project, topic: $topic, task: $task, path: $wt, present: ($wt | path exists) }
            }
        }
        | compact
    } catch { [] }
}

# Find a task by name among notes tasks (exact then fuzzy).
def find-task-in-notes [task_name: string] {
    let all = (notes-tasks)
    let exact = ($all | where task == $task_name)
    if ($exact | length) == 1 { return ($exact | first) }
    let matches = if ($exact | is-not-empty) { $exact } else {
        $all | where { $in.task | str contains $task_name }
    }
    if ($matches | is-empty) { return null }
    if ($matches | length) == 1 { return ($matches | first) }
    print "Multiple matching tasks:"
    $matches | each {|m| print $"  ($m.project)/($m.topic)/($m.task)" }
    error make { msg: $"Task '($task_name)' is ambiguous — give the full task name" }
}

# Completion over notes tasks (includes removed ones). For restore / forget.
def notes-task-completions [] {
    notes-tasks | each {|t|
        let tag = if $t.present { $"($t.project)/($t.topic)" } else { $"($t.project)/($t.topic) [removed]" }
        { value: $t.task, description: $tag }
    }
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
def resolve-task-arg [task?: string, project?: string] {
    if $task != null {
        find-task $task $project
    } else {
        let from_cwd = task-from-cwd
        if $from_cwd == null {
            error make { msg: $"No task given and the current directory is not under ($WORKTREE_BASE)/<project>/<topic>/<task>/" }
        }
        $from_cwd
    }
}

# Create the worktree for a task at <project>/<topic>/<task>, reusing the branch
# named after the task if it already exists.
def create-worktree [main_repo: path, worktree_dir: path, task: string, base?: string] {
    let branch_exists = (do -i { ^git -C $main_repo rev-parse --verify $task } | complete).exit_code == 0
    if $branch_exists {
        print $"Creating worktree for existing branch '($task)'..."
        ^git -C $main_repo worktree add $worktree_dir $task
    } else {
        let base_ref = if $base != null { $base } else { "HEAD" }
        print $"Creating worktree with new branch '($task)' from ($base_ref)..."
        ^git -C $main_repo worktree add -b $task $worktree_dir $base_ref
    }
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
        let found = find-task $task $proj.name
        if $found != null {
            $found.topic
        } else {
            error make { msg: "Topic is required for new tasks. Use --topic (-t)." }
        }
    }

    let worktree_dir = ($WORKTREE_BASE | path join $proj.name $resolved_topic $task)

    if ($worktree_dir | path exists) {
        ensure-notes-dir $proj.name $resolved_topic $task | ignore
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
        let found = find-task $sanitized_task $repo_name
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
    all-tasks
    | where {|t| ($project == null) or ($t.project == $project) }
    | where {|t| ($topic == null) or ($t.topic == $topic) }
    | where {|t| ($task == null) or ($t.task == $task) }
    | each {|t|
        let info = try {
            {
                head: (^git -C $t.path rev-parse --short HEAD | str trim)
                branch: (try { ^git -C $t.path symbolic-ref --short HEAD | str trim } catch { "(detached)" })
            }
        } catch {
            { head: "???", branch: "???" }
        }
        let block = (task-block-info $t.path)
        {
            project: $t.project
            topic: $t.topic
            task: $t.task
            branch: $info.branch
            head: $info.head
            done: (is-task-done $t.path)
            blocked: ($block != null)
            ready: (if $block == null { false } else { $block.ready })
            until: (if $block == null { null } else { $block.until })
            path: $t.path
        }
    }
}

# Go to a worktree directory
export def --env "hwt go" [
    task: string@task-completions                # Task name
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
    task: string@task-completions                # Task to remove
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

# Re-create a removed task's worktree from its notes metadata. The branch is the
# task name (re-created from HEAD if it no longer exists).
export def --env "hwt restore" [
    task: string@notes-task-completions   # Task to restore (from notes metadata)
    --no-cd                               # Don't cd into the restored worktree
] {
    let found = find-task-in-notes $task
    if $found == null {
        error make { msg: $"No task.json metadata found for '($task)'" }
    }
    if $found.present {
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
        error make { msg: $"No task.json metadata found for '($task)'" }
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
    all-tasks
    | each {|t|
        if (is-task-done $t.path) { return null }
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
    try {
        ls $WORKTREE_BASE | where type == dir | get name | each {|project_path|
            let project = ($project_path | path basename)
            let tasks = (all-tasks | where project == $project)
            {
                project: $project
                topics: ($tasks | get topic | uniq | length)
                tasks: ($tasks | length)
                open: ($tasks | where { not (is-task-done $in.path) } | length)
                done: (is-project-done $project_path)
                has_repo: (($PROJECTS_BASE | path join $project) | path exists)
            }
        } | sort-by done project
    } catch { [] }
}

# List topics with task counts
export def "hwt topics" [
    --project (-p): string@project-completions  # Filter by project
] {
    ensure-worktree-dir
    all-tasks
    | where {|t| ($project == null) or ($t.project == $project) }
    | group-by { |t| $"($t.project)/($t.topic)" }
    | items {|key, rows|
        let first = ($rows | first)
        {
            project: $first.project
            topic: $first.topic
            tasks: ($rows | length)
            open: ($rows | where { not (is-task-done $in.path) } | length)
            done: (is-topic-done ($WORKTREE_BASE | path join $first.project $first.topic))
            task_list: ($rows | get task | str join ", ")
        }
    }
    | sort-by done project topic
}

# List all tasks
export def "hwt tasks" [] {
    ensure-worktree-dir
    all-tasks | each {|t|
        let block = (task-block-info $t.path)
        {
            project: $t.project
            topic: $t.topic
            task: $t.task
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

    let projects = try {
        ls $WORKTREE_BASE | where type == dir | get name | sort
    } catch { return }

    for project_path in $projects {
        let project = ($project_path | path basename)
        let project_done = (is-project-done $project_path)

        if $project_done {
            print $"(ansi dark_gray)($project)  \(done\)(ansi reset)"
        } else {
            print $"(ansi cyan_bold)($project)(ansi reset)"
        }

        let topics = try { ls $project_path | where type == dir | get name | sort } catch { continue }
        let last_topic_idx = (($topics | length) - 1)

        for t_idx in 0..$last_topic_idx {
            let topic_path = ($topics | get $t_idx)
            let topic = ($topic_path | path basename)
            let topic_last = ($t_idx == $last_topic_idx)
            let topic_prefix = if $topic_last { "└── " } else { "├── " }
            let child_indent = if $topic_last { "    " } else { "│   " }
            let topic_done = (is-topic-done $topic_path)

            if $topic_done {
                print $"($topic_prefix)(ansi dark_gray)($topic)  \(done\)(ansi reset)"
            } else {
                print $"($topic_prefix)(ansi blue_bold)($topic)(ansi reset)"
            }

            let tasks = try {
                ls $topic_path | where type == dir | get name
                | sort
                | sort-by { |x|
                    # ready floats up, blocked sinks below active, done last
                    match (task-state $x) {
                        "ready" => 0
                        "active" => 1
                        "blocked" => 2
                        _ => 3
                    }
                  }
            } catch { continue }

            let last_task_idx = (($tasks | length) - 1)
            for k_idx in 0..$last_task_idx {
                let task_path = ($tasks | get $k_idx)
                let task = ($task_path | path basename)
                let prefix = if ($k_idx == $last_task_idx) { "└── " } else { "├── " }
                let done = (is-task-done $task_path)
                let block = (task-block-info $task_path)
                let active = (last-active $project $topic $task)

                let color = if $done {
                    (ansi dark_gray)
                } else if ($block != null) and $block.ready {
                    (ansi magenta_bold)
                } else if ($block != null) {
                    (ansi yellow)
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
                } else {
                    ""
                }
                print $"($child_indent)($prefix)($color)($task)(ansi reset)  (ansi dark_gray)($active)($suffix)(ansi reset)"
            }
        }
    }
}

# Git status of all worktrees
export def "hwt status" [] {
    hwt list | each {|tree|
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
    task: string@task-completions                # Task name
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

    let tasks = (all-tasks
        | where { not (is-task-done $in.path) }
        | each {|t| $t | merge { session: $"hcdev-($t.project)-($t.topic)-($t.task)" } })

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
            ^setsid -f alacritty --working-directory $t.path --title $t.session -e nu --login -c $"hcdev wt '($t.task)' --project '($t.project)' --topic '($t.topic)'"
        }
    }
}
