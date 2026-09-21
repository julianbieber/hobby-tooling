# task:goal — the goal file

| Field        | Value                                                        |
| ------------ | ------------------------------------------------------------ |
| Type         | file format, checked by `hcoord goal lint`                   |
| Owner        | @julianbieber                                                |
| Dependencies | `hcoord` on PATH; an app whose window can be captured        |
| Status       | experimental — `/task:goal <blurb>` runs the interview; the first goal was written by hand along the same procedure |

## Description

A goal file is the UX spec an unattended run is judged against. It is written
once, at the front, in one sitting with me, and nothing downstream paraphrases
it: an issue, a brief, an acceptance check and a PR's test bullets quote its
scenarios by id. It lives at
`~/fun_repos/hobby-mimisbrunnr/notes/<project>/planning/<blurb>/goal.md`, with
the captured screens beside it under `screens/`.

## Why this exists

Seven coordinator runs, 48 merged PRs, and the result of each was "kind of what
the description said, far from what I expected". The description I write is
three to six lines of intent; everything after it — the questions, the issue
text, the acceptance — was the model's reading of its own answers, and my
picture of the screen never entered the record. Every divergence those runs
left behind was an unstated screen (a panel overlapping the bar it was placed
under, a drag-slider two issues assumed and no build had), a spec at odds with
itself (`lowest free number` beside an example that increments), or a bound
nobody had named. Nothing in the pipeline asked for any of it, and the one
gate that would have shown it — a person at the running app — is off in every
unattended run.

## The format

Markdown, fixed sections, screens drawn as indented ASCII. A heading may carry
a parenthetical after its name (`## Decisions (my words, verbatim)`,
`### editor.main (target)`); the lint reads the name and ignores the rest.

```markdown
---
title: "<blurb> — goal"
created: <YYYY-MM-DD>
tags:
  - goal
---

## Intent
> my prose, verbatim, untouched

## Decisions (my words, verbatim)
- **<handle>** — "what I said"

## Current state (<date>, captured before anything was asked)
- what the app does today, as found in the code and on screen

## Project on disk            (optional: a directory tree is a screen too)
    <tree>

## Screens
### <name> (exists — screens/<name>.png)
    <ASCII of the captured screen>
### <name> (target)
    <ASCII of the screen after the change>
    changes:   what is different
    unchanged: what stays
    hides:     what disappears, and when

## Scenarios
### S<n> <title>                                 screen: <name>
    1. <one gesture: click [x], type `y`, press Enter, run `cmd`>
    2. ...
    hand                                          <- only when nobody can script it
    -> <the one observation>
    widgets: <name> (<note>), ... | none new

## Widgets
| widget | exists | provided by |
|--------|--------|-------------|
| <name> | yes / no / in feathers, not in this app | S<n>, or the task that adds it |

## Left to the build
- <a bound or choice I do not care about; the build picks one and names it>

## Tunables
- <a visual constant nobody can pre-specify; exposed live, set at judgement>

## Not this time
- <deliberately out>
```

Each section closes one class of divergence:

| section | closes |
|---|---|
| **Intent** | the words are mine; every later artefact quotes them |
| **Decisions** | the business context, in the file the scenarios cite, with a handle a commit body can name |
| **Current state** | the target is a delta against something captured, not against a guess |
| **Screens** with `changes / unchanged / hides` | panel overlap, leftover chrome, a control placed as a sibling when a row was meant |
| **Scenarios** with a gesture per step and one `->` | the phantom slider, the invisible drag, the redo nobody mentioned |
| `hand` | an honest statement that a scenario cannot be scripted; it is never proxied by a check that passes while the observation fails. The plan stage writes it as a hand check with no command, `verify` captures the screen it names to `screens/<id>.png` in the task's notes dir, and `hcoord wait` / `hcoord tui` count it apart. Marking a scenario `hand` says its screen is worth a picture; it is not a way to opt out of acceptance. The picture is what `/task:judge` shows beside the sketch when the run is finished, so `hand` is also a statement about what I want to be shown at the end |
| **Widgets** | the affordance inventory, checked once for the whole goal instead of discovered twice in two builds |
| **Left to the build** | the 16 / 24 / 128 bounds surfaced as choices, not buried |
| **Tunables** | hillshade gain: exposed and set at judgement, not guessed and shipped |

A `?` anywhere outside backticks is an open question. It is how the file is
written — every guess is marked `?` and answered in my words — and a file with
one left is not finished.

## The lint

```
hcoord goal lint <goal.md>                     the file on its own
hcoord goal lint <goal.md> --tasks <tasks.json> and the split
hcoord goal show <goal.md> S1 S3               scenarios verbatim, for a brief
```

Checks: the five required sections are present (`Intent`, `Decisions`,
`Screens`, `Scenarios`, `Widgets`); every screen says `(exists)` or
`(target)`; every scenario names a screen that is drawn, has a numbered step
and exactly one `->`, and names only widgets the table lists; a widget that
does not exist names what provides it; no `?` remains. With `--tasks`, every
scenario is owned by exactly one task's `scenarios` and every task owns at
least one.

What the lint cannot check is whether a scenario's `->` is reachable — whether
`@range 0 2` can produce a value above 1 — and whether a `Depends on` agrees
with what the scenarios assume. Those are the split's job, against the code.

## The procedure `/task:goal` runs

1. Record the intent verbatim.
2. Launch the app, capture every screen the intent touches with
   `hcoord capture`, draw each as ASCII, and write **Current state** from the
   code.
3. Draft the target screens and scenarios with every guess marked `?`.
4. **Questions gate.** Ask the questions the `?`s are, open where the answer is
   a picture, closed only where it is a binary technical choice. Record every
   answer verbatim under **Decisions** and resolve its `?`.
5. **Read gate.** Hand the whole file over to be read once. `hcoord goal lint`
   must print `ok`.
6. Split from the scenarios: each task owns a set of ids, and a widget with
   `exists: no` becomes an `enabling` task naming its consumer. Write
   `tasks.json` in the coordinator's dir with `scenarios` per task and `goal`
   pointing at the file, and lint with `--tasks`.

Three choices in it:

- **`hcoord capture` is the capture mechanism.** One command covers a window
  and a file, and it refuses an ambiguous window title instead of guessing.
- **No `AskUserQuestion`.** An answer picked from options is not my words, and
  Decisions are verbatim.
- **A decision task cannot appear in the split.** The lint requires every task
  to own a scenario, so the decisions are made at the questions gate instead.

## First goal

`notes/bevy_pbr_generator/planning/texture_projects/goal.md`, 2026-09-19:
six scenarios, two target screens, eight decisions. Writing it added four
things to the format the plan had not foreseen — Decisions as a section of the
goal, Current state, a directory tree as a screen, and Left to the build in
place of "bounds I do not care about".
