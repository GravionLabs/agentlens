---
name: grill-me
description: 'Interview the user until no open decision is left. Use for "grill me", "ask me the open questions" or to answer a plan''s questions[] or an issue''s open decisions.'
---

Interview the user until no open question is left. Adapted from the `grilling` skill of [mattpocock/skills](https://github.com/mattpocock/skills) (MIT License, Copyright (c) Matt Pocock). No dependency on that repository.

## Input

One of:

- an issue number: read it (`gh issue view <nr>`), derive the open decisions from its body and comments;
- a plan's `questions[]` (format: `planner` `references/questions.md`);
- a free description of what is undecided.

## Loop

1. List the open questions. Questions are independent of each other; a question that only arises from an answer belongs to the next round.
2. Facts are never asked. Look up anything the repository, the issue or the docs can answer; only product, scope and risk choices reach the user.
3. Ask a round: all independent open questions at once. Each has its options, the `recommended` option first, worded so "yes" accepts it, and a short reason (cost if wrong).
4. Take the answers. Adjust the remaining questions; an answer may remove questions or add new ones. Start the next round with those.
5. Repeat until no open question is left.

## Option `--docs`

`/grill-me --docs …`: load `glossary` and apply its rules while grilling: a term that conflicts with `GLOSSARY.md` is challenged, a fuzzy term gets a proposed canonical term, claims are checked against the code. Each resolved term is written with `glossary` (the file is created with the first one). Offer an ADR via `adr` for a decision only when all three hold: it is hard to reverse, it is surprising without context, and it is a real trade-off (alternatives existed). Otherwise no ADR is offered. The ADR is written only after the user agrees; its Status stays `Proposed`. Without `--docs` neither skill is loaded and nothing is written.

## Clients

- Claude Code: `AskUserQuestion`, at most 4 questions per call; split a larger round into several calls of the same round.
- No question tool: ask the same round as numbered plain text.

## Deferred decisions

Offer "defer" only for a question that blocks something (plan input: non-empty `blocks`). A deferred question stays open: label the affected issue `needs-decision` and add an "Owner decision required" block with the question, options and recommendation. With a plan as input, mark it `deferred: true` in the returned `questions[]`.

## Result

When no open question is left:

- **Issue or free description:** write the answers to the issue under `## Decisions`, one line each: `<question> — <answer>`. Without an issue, show the list in the reply.
- **Plan input:** return the answered `questions[]` (answer added per question, deferred ones marked) instead of writing to an issue.
