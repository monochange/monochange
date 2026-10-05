//! Install page — GitHub App installation and setup instructions.

use leptos::prelude::*;

/// Install page: one-click GitHub App install plus the hosted workflow setup.
#[component]
pub fn InstallPage() -> impl IntoView {
	view! {
		<div class="mx-auto max-w-4xl px-4 py-12 sm:px-6 lg:px-8">
			<div class="text-center">
				<h1 class="text-4xl font-extrabold tracking-tight text-gray-900 dark:text-white">
					"Install the monochange GitHub App"
				</h1>
				<p class="mx-auto mt-4 max-w-2xl text-lg text-gray-600 dark:text-gray-400">
					"The app opens your release pull requests under the monochange bot
					identity, so every release pull request runs your checks without a
					personal token."
				</p>
			</div>

			<div class="mt-12 rounded-2xl border border-gray-200 bg-white p-8 shadow-sm dark:border-gray-800 dark:bg-gray-900">
				<h2 class="flex items-center gap-x-2 text-xl font-semibold text-gray-900 dark:text-white">
					<span class="flex size-8 items-center justify-center rounded-full bg-brand-100 text-sm font-bold text-brand-700 dark:bg-brand-900 dark:text-brand-300">1</span>
					"Install the app"
				</h2>
				<p class="mt-3 text-gray-600 dark:text-gray-400">
					"Grant access to the repositories monochange should manage. monochange
					needs contents write and pull requests write on those repositories;
					the install screen shows exactly this list."
				</p>
				<a
					href="https://github.com/apps/monochange/installations/new"
					class="mt-6 inline-flex items-center gap-x-2 rounded-xl bg-gray-900 px-6 py-3 text-sm font-semibold text-white shadow-lg shadow-gray-900/10 transition-all hover:bg-gray-800 hover:shadow-gray-900/20 hover:-translate-y-0.5 dark:bg-white dark:text-gray-900 dark:shadow-white/10 dark:hover:bg-gray-100"
				>
					<svg class="size-5 fill-white dark:fill-gray-900" viewBox="0 0 16 16">
						<path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z" />
					</svg>
					"Install on GitHub"
				</a>
			</div>

			<div class="mt-6 rounded-2xl border border-gray-200 bg-white p-8 shadow-sm dark:border-gray-800 dark:bg-gray-900">
				<h2 class="flex items-center gap-x-2 text-xl font-semibold text-gray-900 dark:text-white">
					<span class="flex size-8 items-center justify-center rounded-full bg-brand-100 text-sm font-bold text-brand-700 dark:bg-brand-900 dark:text-brand-300">2</span>
					"Add one secret"
				</h2>
				<p class="mt-3 text-gray-600 dark:text-gray-400">
					"Store a monochange API token as the repository secret
					<code class="rounded bg-gray-100 px-1.5 py-0.5 font-mono text-sm text-brand-700 dark:bg-gray-800 dark:text-brand-300">MONOCHANGE_TOKEN</code>.
					GitHub Actions also authenticates automatically with its OIDC token, so
					the secret is a fallback for other CI systems."
				</p>
			</div>

			<div class="mt-6 rounded-2xl border border-gray-200 bg-white p-8 shadow-sm dark:border-gray-800 dark:bg-gray-900">
				<h2 class="flex items-center gap-x-2 text-xl font-semibold text-gray-900 dark:text-white">
					<span class="flex size-8 items-center justify-center rounded-full bg-brand-100 text-sm font-bold text-brand-700 dark:bg-brand-900 dark:text-brand-300">3</span>
					"Point the release workflow at the bot"
				</h2>
				<p class="mt-3 text-gray-600 dark:text-gray-400">
					"Set the release steps to the hosted backend. monochange prepares the
					release in the workflow, then the app commits and opens the pull
					request as the monochange bot."
				</p>
				<pre class="mt-4 overflow-x-auto rounded-xl bg-gray-950 p-4 text-xs leading-relaxed text-gray-100"><code>{r#"steps = [
	{ type = "PrepareRelease", name = "plan release", allow_empty_changesets = true },
	{ type = "CommitRelease", commit_backend = "hosted" },
	{ type = "OpenReleaseRequest", backend = "hosted" },
]"#}</code></pre>
				<p class="mt-4 text-sm text-gray-500 dark:text-gray-400">
					"The workflow needs no other credentials: no personal token, no
					deploy key, and no commit-identity configuration."
				</p>
			</div>
		</div>
	}
}
