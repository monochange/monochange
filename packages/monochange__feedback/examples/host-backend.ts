/** Call only after your own server has authenticated the request. */
export async function issueFeedbackSession(
	authenticatedUser: { id: string },
	surface: "web" | "mobile",
) {
	if (!authenticatedUser?.id) throw new Error("Host authentication is required");
	const endpoint = process.env.MONOCHANGE_FEEDBACK_ENDPOINT ?? "https://monochange.dev";
	const project = process.env.MONOCHANGE_FEEDBACK_PROJECT;
	const credential = process.env.MONOCHANGE_FEEDBACK_BACKEND_TOKEN;

	if (!project || !credential) throw new Error("Configure the project's feedback integration");
	const response = await fetch(
		`${endpoint}/api/v1/feedback/${encodeURIComponent(project)}/sessions`,
		{
			method: "POST",
			headers: { Authorization: `Bearer ${credential}`, "Content-Type": "application/json" },
			body: JSON.stringify({ user_id: authenticatedUser.id, surface }),
			redirect: "error",
		},
	);

	if (!response.ok) throw new Error(`Feedback session issuance failed (${response.status})`);

	return response.json();
}
