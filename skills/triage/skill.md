---
description: Review and triage open action items from journals and known issues
allowed-tools: Bash(ls:*), Bash(pwd:*), Read, Write, Edit, Glob, Grep
---

Present all open action items for triage. The user decides what to do with each
one.

## Instructions

1. **Collect action items** from these sources:

   - `~/.claude-hobby/context/known-issues.md` — the Action Items section (lines
     starting with `- [ ]`)
   - Recent daily journals in `~/fun_repos/hobby-mimisbrunnr/notes/`
     — scan for `## Action Items` sections in files modified in the last 30
     days:
     `find ~/fun_repos/hobby-mimisbrunnr/notes -name "*.md" -mtime -30 -exec grep -l "Action Items" {} \;`

2. **Deduplicate** — if the same item appears in both known-issues and a
   journal, show it once with both sources noted.

3. **Present each item** as a numbered list with context. For each item show:

   - The action item text
   - Where it originated (which journal/investigation)
   - How long it's been open (date recommended vs today)
   - Any related known-issue pattern

4. **Ask the user to decide** on each item. Offer these options per item:

   - **do** — Create a task for it (an issue in the notes, or a worktree). Ask for priority.
   - **defer** — Keep it open, optionally set a review date
   - **drop** — Remove it (not worth doing). Mark as `- [x] DROPPED:` with
     reason
   - **done** — Already resolved. Mark as `- [x] DONE` with date
   - **delegate** — Someone else's responsibility. Note who.

   The user can batch decisions (e.g. "drop 3,4,5" or "defer all jenkins
   items").

5. **Apply decisions**:

   - Update `~/.claude-hobby/context/known-issues.md` Action Items section
     (check/uncheck, add notes)
   - For **do** items: offer to write an issue via `/issue-create` or set
     up a worktree
   - For **drop**/**done** items: move from `- [ ]` to `- [x]` with annotation
   - For **defer** items: append `(deferred until YYYY-MM-DD)` if a date was
     given

6. **Summary** — after all decisions, print a quick count: N items triaged (X
   do, Y defer, Z drop, W done, V delegate).
