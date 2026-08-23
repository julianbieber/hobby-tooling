---
description: Take every comment in the branch diff apart clause by clause, judge which parts are actually required, and cut the rest — the trimming pass, never an expanding one
argument-hint: '[<base>]'
allowed-tools: Bash(git diff:*), Bash(git log:*), Bash(git -C:*), Bash(git rev-parse:*), Bash(pwd:*), Bash(*/ai-policy-tier.sh:*), Read, Grep, Glob, Edit
---

Comments arrive over-written. `/explain-back`'s comment pass writes what is needed
to make a comment *true*; this skill decides how much of it is needed at all, and
deletes the rest.

**You only ever remove.** Nothing in this skill adds a comment, extends one, or
"clarifies" one. If a comment is missing, that was the previous pass's job and it
is now a finding to report, not an edit to make.

## Stage 0 — Scope

1. Resolve the tier with `scripts/ai-policy-tier.sh`. On `upstream`, report the
   verdicts and **change nothing**: the comments there are hand-written and are
   not yours to cut.
2. Base is the argument if given, else the merge base with the default branch.
3. Collect every comment in `git diff <base>...HEAD`, plus comments on items the
   diff touched. Report the count before starting.

## Stage 1 — The critical pass

Split each comment into **clauses** — a sentence, a bullet, a parenthetical, a
`# Panics` block. Judge every clause on its own. A comment is rarely wholly
required or wholly redundant.

**Cut a clause that:**

- restates what the next line of code plainly does;
- repeats the item's own name back (`/// Creates a new Foo.` on `Foo::new`);
- states something the signature already states — the parameter types, the return
  type, that an `Option` may be `None`, that a `Result` may fail;
- describes the implementation rather than the contract, on a public item;
- hedges without committing ("this may need revisiting", "for now");
- narrates the change rather than the code ("now also handles X"), which belongs
  in the commit message and dates the moment it lands.

**Keep a clause that:**

- gives a reason the code cannot give — why this order, why this bound, why not
  the obvious alternative;
- states an invariant or precondition a caller must uphold;
- names a real consequence — what panics, what blocks, what allocates, what is
  O(n²);
- points somewhere the reader would not find alone: an issue, a spec section, an
  upstream bug, a scary interaction two modules apart;
- is an example that would not compile in the reader's head.

**Print the verdict table before editing anything:**

| file:line | item | clause | verdict |
|---|---|---|---|

Verdicts are **keep** / **cut: \<which rule\>** / **merge into \<other clause\>**.

## Stage 2 — Adjust down

Apply the table:

- Every clause cut, cut.
- A comment whose clauses were all cut is deleted entirely, not left as an empty
  shell or a `TODO`.
- Merged clauses collapse into the shortest phrasing that keeps both facts. Reuse
  the words already there — this is a cut, not a rewrite.
- A comment on a private item that survived the cut becomes
  `// TODO(jb-comment): <what needs explaining>` on both tiers. The private tier
  relaxes doc comments on public items, and nothing else.

Never touch a non-comment line. If cutting a comment would leave the code
unreadable, that is a finding about the code — report it and leave the comment.

## Report

- comments seen, comments changed, comments deleted, clauses cut;
- the surviving comments in full, so I read what is actually left;
- anything reported rather than edited: missing comments, comments whose code
  looks wrong, comments kept only because the code was unreadable without them.

## Rules

- One pass. Do not re-judge a clause you already kept.
- Do not reformat, re-wrap or re-order surviving text beyond what a cut forces.
- Say the tier out loud at the start, and stop at Stage 1 on `upstream`.
