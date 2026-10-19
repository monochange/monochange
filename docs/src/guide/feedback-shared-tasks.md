# Share tasks with contributors

> **Experimental.** Projects can share and screen tasks today, and anyone can read them through the task API. The `monochange tasks` command that runs them on a contributor's machine and sends the work back comes next.

A project with more accepted feedback than time can share work as tasks. A contributor picks one and runs it with their own coding agent and model key. monochange sits between the two: it screens every task before anyone can run it, and it will screen the work before a maintainer sees it.

## Turn it on

Open the project's feedback console and choose **Turn on** under **Shared compute**. Only projects whose repositories are all public can turn it on, because every shared task is public. Turning it off stops every task from listing; turning it back on lists the unexpired ones again.

## Share a task

An accepted item with a GitHub issue shows a **Contributor tasks** form. A task has four parts:

- **Title.** One line, at most 120 characters.
- **Acceptance criteria.** One per line, at most 10, each at most 300 characters.
- **Paths it may change.** One per line, at most 20, such as `src/totals/**`.
- **Check command.** The one command that proves the work, such as `cargo test -p invoices`.

monochange pins the task to the commit at the tip of the repository's default branch when you share it, and links the item's issue. The contributor's agent receives only the task: not the issue, its discussion, or anything a user wrote. A task lists for 14 days, until the item moves on (for example when a pull request for it opens), or until you withdraw it. A project lists at most 20 tasks at once.

## Screening

Rules a program checks run before a task lists. A task that fails stays in the console with the reasons and never lists. The rules refuse:

- text that points at the contributor's machine, such as environment variables, `printenv`, credentials, API keys, SSH keys, or the home directory;
- links anywhere but the task's own repository on GitHub;
- line breaks, invisible or direction-changing characters, and long encoded blobs;
- phrases that read as instructions to the agent's safety rules, such as "ignore all previous instructions";
- paths that leave the repository, start with a wildcard, or cover `.github`, `.git`, or `.monochange`;
- check commands that don't start with a known build tool (such as `cargo`, `npm`, `pnpm`, `go`, `pytest`, or `python`), or that use shell syntax, quotes, variables, absolute or parent paths, or inline code flags such as `-c` and `--eval`.

Rules are a tripwire. The guarantee comes from the sandbox the contributor's agent runs in, which holds nothing worth taking. A screening model that reads the whole task is planned on top of these rules.

## Read the tasks

Anyone can read a project's open tasks, without signing in, so anyone can audit what a project asked for:

```sh
curl https://monochange.dev/api/projects/acme/invoices/tasks
curl https://monochange.dev/api/projects/acme/invoices/tasks/task-1
```

The second request returns one task exactly as an agent will receive it:

```json
{
	"id": "task-1",
	"item": "fb-12",
	"repository": "acme/invoices",
	"commit": "0123456789abcdef0123456789abcdef01234567",
	"issue_number": 42,
	"title": "Round quarterly totals to cents",
	"acceptance": [
		"Quarterly totals match the sum of their invoices",
		"A test covers a quarter with fractional cents"
	],
	"paths": [
		"src/totals/**"
	],
	"check": "cargo test totals",
	"expires_at": 1761609600
}
```

`expires_at` is in Unix seconds. A task that expired, was withdrawn, or failed screening answers `404`.
