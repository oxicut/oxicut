---
name: log-issue
description: File a GitHub issue in the project tracker (oxicut/oxicut), either a bug or a feature request. Invoke when the user says "log an issue", "file a bug", "feature request", "open an issue", "track this", or similar. The content comes from the user's description or from the recent conversation: a bug just seen, a follow-up just mentioned, an idea that is out of scope for the current work.
---

# Log Issue

GitHub issues are this project's tickets. When something comes up mid-conversation that deserves tracking (a bug we just noticed, a follow-up to code we just touched, an idea for later), capture it as a proper issue instead of losing it in the scrollback.

## What counts as "something to log"

- A bug the user just described or that showed up during work
- A feature request or improvement the user wants tracked
- A follow-up that is explicitly out of scope for the current change
- A piece of technical debt the user flagged ("we should clean this up later")
- A sibling change the user asks to file ("create an issue to do the same for X")

Do NOT log:

- Work in progress on the current change (it belongs in the current branch and pull request)
- Questions or clarifications
- Duplicates of issues already mentioned in the conversation, or already open (search first: `gh issue list --repo oxicut/oxicut --search "<words>" --state all`)

## Inputs

1. **An explicit description** ("log an issue: the card comes out sideways when ...")
2. **A context-only trigger** ("log this", "file that", "track it"): infer from the last few turns what "this" means. It is usually the most recently discussed bug or idea.
3. **An argument** after the skill name.

If the referent is ambiguous (several things were discussed), ask one short question before going on.

## Process

### 1. Decide: bug or feature request

- **Bug**: something that should work and does not, or a result that is wrong. Label `bug`.
- **Feature request**: something new or better that the user wants. Label `enhancement`.
- Documentation only: label `documentation`. Barriers for people with disabilities: also add `accessibility`.
- If it is unclear, ask in the draft ("Bug or feature request?").

### 2. Draft the issue

Never invent facts. If reproduction steps would have to be guessed, ask the user.

**Bug**

```
## What

<one paragraph: what goes wrong, in plain words>

## Reproduce

1. Step one
2. Step two
3. What happens, and what should happen instead

## Notes

<scan or file that shows it (a path inside the repo, or say it is local), the detector and settings used, whether ScanSplitter does the same>

## Fix ideas

<optional: files likely involved, options, a recommendation>
```

**Feature request**

```
## What

<the wanted outcome, in a paragraph>

## Why

<who it helps and what is hard today>

## Ideas

<optional: options and trade-offs, files likely involved, a recommendation>
```

Title: under 70 characters, active voice, no emoji, no em dashes. For a bug, start with the symptom ("Portrait cards come out sideways in the Crops view"). For a request, start with the outcome ("Let people name each picture before export").

Body: keep it under about 2 KB. Give exact paths where known (`app/src/store.ts`). Quote code sparingly. Link related issues and pull requests as `#123`.

### 3. Show the draft first

Print the proposed type, labels, title and body, and ask "File as-is?". Wait for the answer. Skip this only when the user said to just file it.

### 4. Create the issue

Use `gh issue create` with a heredoc for the body, so quotes and backticks survive:

```bash
gh issue create --repo oxicut/oxicut --title "<title>" --label bug --body "$(cat <<'EOF'
## What

<body>
EOF
)"
```

Use `--label enhancement` for a feature request. `gh` prints the issue URL; give it back to the user as one line.

Do not add an assignee, milestone or project unless asked. Do not add attribution lines about who or what wrote the issue.

### 5. Hand off

The skill only files the issue. It does not change code, create branches or open pull requests. If the user says "file it and start on it", file first, then carry on with the normal workflow. Remember the issue number for the rest of the conversation, so the pull request can say `Fixes #123`.

## Guardrails

- Issues are public. No tokens, credentials, personal paths (use paths relative to the repo), or private scan content. Do not attach or describe family photos; describe what is wrong in general terms or point at a fixture.
- Write for the project, not for one machine: say what a person sees, in plain words, the way the app would say it. Keep OS error codes and raw logs out of the main text (put a short excerpt under Notes if it helps).
- If a problem is the same in the Python original (`reference/ScanSplitter`), say so in the issue.
- Mention only what the repository itself already says publicly. Do not add plans or ideas that are not in the repo's own README, roadmap or docs.
- Never file a duplicate: point at the existing issue instead.
