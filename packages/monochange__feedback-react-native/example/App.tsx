import React, { useMemo, useState } from "react";
import { Button, SafeAreaView } from "react-native";
import { FeedbackClient, FeedbackSheet, type ClientToken } from "@monochange/feedback-react-native";

/** Supply your app's authenticated session and transport from its existing auth layer. */
export default function App({
	userId,
	feedbackProject,
	getFeedbackSession,
}: {
	userId: string;
	feedbackProject: number;
	getFeedbackSession: () => Promise<ClientToken>;
}) {
	const [visible, setVisible] = useState(false);
	const client = useMemo(
		() =>
			new FeedbackClient({
				endpoint: "https://monochange.dev",
				project: feedbackProject,
				getClientToken: getFeedbackSession,
				getContext: () => ({ route: "Example", app_version: "1.0.0" }),
			}),
		[feedbackProject, getFeedbackSession],
	);
	React.useEffect(() => () => client.destroy(), [client]);
	return (
		<SafeAreaView>
			<Button title="Share feedback" onPress={() => setVisible(true)} />
			<FeedbackSheet
				client={client}
				identityKey={userId}
				visible={visible}
				onClose={() => setVisible(false)}
			/>
		</SafeAreaView>
	);
}
