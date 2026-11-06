# Connect a local or self-hosted feedback agent

Connect an agent on a developer machine or server to prepare plans and implement approved requests. The worker connects outward over HTTPS, so no inbound port is required. Model credentials stay on that machine. GitHub Actions is an independent existing execution path.

## Pair the worker

In the project's feedback console, choose **Connect a local or self-hosted agent**, give it a name, and select its repository. Copy the pairing code; it expires in ten minutes and works once.

Install `@monochange/feedback-worker`, then create `.monochange/local/feedback-worker.json`:

```json
{
	"endpoint": "https://monochange.dev",
	"project": 123,
	"repositories": { "acme/web": "/absolute/path/to/web" },
	"model": {
		"endpoint": "https://openrouter.ai/api/v1/chat/completions",
		"name": "your-chosen-model",
		"keyEnvironment": "OPENROUTER_API_KEY"
	},
	"container": {
		"engine": "docker",
		"image": "your-preinstalled-check-image",
		"checks": [["npm", "test", "--", "--runInBand"]],
		"timeoutSeconds": 300
	},
	"maxTurns": 30
}
```

Map repository names exactly as they appear in the project. Repository directories must be absolute local paths containing the pinned commits. On Windows, use an absolute Windows path with JSON escaping. Docker or Podman must be available for implementation checks, and the configured image must already be installed. It should contain the toolchain and dependencies your checks need: check containers have no network, and the worker never pulls images.

Set `MONOCHANGE_PAIRING_CODE` and your model key in the worker environment, then run:

```sh
monochange-feedback-worker pair .monochange/local/feedback-worker.json
monochange-feedback-worker run .monochange/local/feedback-worker.json
```

Pairing saves a revocable worker credential to that local file. Keep it private; POSIX permissions are set to `0600`. The code is not printed or placed in command arguments. Revoke a worker from the console when disconnecting it. `run CONFIG.json --once` processes one available job or reconciles one pending result and exits.

## Choose what gets a plan

Manual selection is the default. Select a request and repository, then choose **Generate plan**. Requests must finish triage before planning; acceptance and issue creation are not prerequisites for a plan.

To plan new feature requests automatically, enable **Automatically plan feature requests** for the project and choose its repository and daily planning limit. Automatic planning skips bugs, duplicates, held reports, and incomplete briefs. Existing backlog items require explicit selection. Disabling the setting cancels queued automatic jobs and prevents new ones; running jobs can be cancelled separately. Planning never accepts a request or approves implementation.

The daily limit counts planning attempts in a UTC day. At most two jobs run concurrently per project. Worker turn, check-time, memory, and CPU limits bound each attempt. Set provider spending limits appropriate to your chosen model as well.

## Review and approve

The agent returns the summary, acceptance criteria, proposed changes, allowed file paths, checks, and risks. The plan is attached to a material brief revision and repository commit. Review the complete plan in the console:

- **Approve and build** queues implementation of that exact plan. First accept the request and create its repository issue using the existing console controls.
- **Request changes** records your rationale. Generate a new plan to incorporate it; the previous revision cannot be approved.
- **Reject plan** records your decision and queues no implementation. It does not automatically decline the feedback request.

Changing the report, its discussion, the requested revision, or its material scope invalidates approval. If the repository's default branch moved before approval, generate a fresh plan. Voting never authorizes implementation. Projects using this flow cannot bypass approval with the old direct build action.

## Execution and recovery

The worker exports regular files from the pinned git commit to a private temporary directory. It copies no git credentials, uncommitted files, home directory, environment, or Docker socket into agent execution. The model can read bounded repository files. Planning can only read and return a plan.

Implementation writes only paths the reviewed plan permits. The model can request a check by index from the operator's configured argument arrays; it cannot select a host executable or shell command. Checks run in Docker or Podman with network disabled, dropped capabilities, resource caps, and only the temporary checkout mounted. All configured checks must pass before changes are submitted. Only bounded UTF-8 text changes are accepted; CI configuration, git internals, and secret files are excluded. A model is advisory; these tool boundaries enforce the limits.

The server validates the current lease, brief version, and approved paths again, creates a verified commit on a dedicated feedback branch, and opens a draft PR through the GitHub App. It compares the exact git tree and parent before reusing a branch after an interrupted response. Nothing merges or publishes automatically. Normal pull request and release observation updates the feedback item.

Worker leases last two minutes and renew every 30 seconds. Losing the lease stops the worker's model call or container. Cancellation is cooperative; the console distinguishes a request to cancel from a confirmed stop. A disconnected implementation job is not reassigned automatically. After its lease expires, verify the process stopped and use **Confirm worker has stopped** before retrying.

Results are saved next to the local config as `feedback-worker.json.pending.json` before submission. Keep that file until the server acknowledges it: reconnecting resends the same result without running the agent again. A publishing result can be reconciled by its assigned worker even after a disconnect. Never delete pending results merely to clear a job; investigate an ambiguous draft PR first. Confirmed failed or cancelled attempts can be retried explicitly in the console.

The protocol uses project-scoped `/workers/pair`, `/workers/claim`, `/jobs/{id}/heartbeat`, and `/jobs/{id}/result` endpoints under `/api/v1/feedback/{project}`. Feedback client tokens cannot call them, and a worker credential cannot act as a maintainer. Worker jobs and approvals persist with feedback under the same transactional version check.
