import React, { useEffect, useRef, useState } from "react";
import {
	AppState,
	Button,
	KeyboardAvoidingView,
	Modal,
	Platform,
	Pressable,
	SafeAreaView,
	ScrollView,
	StyleSheet,
	Text,
	TextInput,
	View,
	useColorScheme,
} from "react-native";
import {
	FeedbackClient,
	type Conversation,
	type FeedbackKind,
	type Item,
	type Receipt,
	type Suggestion,
} from "@monochange/feedback/client";
export { FeedbackClient } from "@monochange/feedback/client";
export type { FeedbackOptions, FeedbackContext, ClientToken } from "@monochange/feedback/client";

export interface FeedbackSheetProps {
	client: FeedbackClient;
	visible: boolean;
	/** Change this on logout or account switches to erase the previous user's UI. */
	identityKey: string;
	onClose: () => void;
	onSubmitted?: (receipt: Receipt) => void;
	theme?: "light" | "dark";
	labels?: Partial<typeof defaults>;
}
const defaults = {
	title: "Feedback",
	close: "Close",
	feature: "Feature or change",
	bug: "Bug report",
	send: "Find similar requests",
	reply: "Send reply",
	submit: "Submit a new request",
	another: "Start another request",
	upvote: "Upvote",
	voted: "Upvoted",
	message: "Describe the outcome you want, or what went wrong…",
	working: "Working…",
};

function useFeedback({
	client,
	visible,
	identityKey,
	onSubmitted,
}: Pick<FeedbackSheetProps, "client" | "visible" | "identityKey" | "onSubmitted">) {
	const [body, setBody] = useState("");
	const [kind, setKind] = useState<FeedbackKind>("feature_request");
	const [conversation, setConversation] = useState<Conversation>();
	const [suggestions, setSuggestions] = useState<Suggestion[]>([]);
	const [item, setItem] = useState<Item>();
	const [busy, setBusy] = useState(false);

	const [error, setError] = useState("");
	const generation = useRef(0);
	const running = useRef(false);
	const current = useRef(item);
	current.current = item;

	function reset() {
		generation.current += 1;
		client.reset();
		setBody("");
		setConversation(undefined);
		setSuggestions([]);
		setItem(undefined);
		setError("");
		setBusy(false);

		running.current = false;
	}
	useEffect(() => {
		reset();
		return () => {
			generation.current += 1;
		};
	}, [client, identityKey]);

	async function run(action: (active: () => boolean) => Promise<void>) {
		if (running.current) return;
		running.current = true;
		setBusy(true);
		setError("");
		const version = generation.current;
		const active = () => generation.current === version;
		try {
			await action(active);
		} catch (failure) {
			if (active())
				setError(failure instanceof Error ? failure.message : "Feedback request failed");
		} finally {
			if (active()) {
				running.current = false;
				setBusy(false);
			}
		}
	}
	useEffect(() => {
		if (!visible) return;
		const refresh = () => {
			if (current.current)
				void run(async (active) => {
					const next = await client.getItem(current.current!.id);
					if (active()) setItem(next);
				});
		};
		const timer = setInterval(refresh, 30_000);
		const subscription = AppState.addEventListener("change", (state) => {
			if (state === "active") refresh();
		});
		return () => {
			clearInterval(timer);
			subscription.remove();
		};
	}, [client, visible, identityKey]);

	const send = () =>
		void run(async (active) => {
			if (!body.trim()) return;
			if (item) {
				await client.reply(item.id, body);
				const next = await client.getItem(item.id);
				if (active()) setItem(next);
			} else {
				const result = conversation
					? await client.sendMessage(conversation, body)
					: await client.startConversation(body, kind);
				if (active()) {
					setConversation(result.conversation);
					setSuggestions(result.similar);
				}
			}
			if (active()) setBody("");
		});
	const submit = () =>
		void run(async (active) => {
			if (!conversation) return;
			const receipt = await client.submit(conversation.id, conversation.version);
			const result = await client.getItem(receipt.id);
			if (active()) {
				setItem(result);
				setSuggestions([]);
				onSubmitted?.(receipt);
			}
		});

	const vote = (suggestion: Suggestion) =>
		void run(async (active) => {
			const result = await client.vote(suggestion.id, !suggestion.voted);
			if (active())
				setSuggestions((previous) =>
					previous.map((value) => (value.id === suggestion.id ? { ...value, ...result } : value)),
				);
		});

	return {
		body,
		setBody,
		kind,
		setKind,
		conversation,
		suggestions,
		item,
		busy,
		error,
		reset,
		send,
		submit,
		vote,
	};
}

/** Native feedback UI. The host owns authentication, presentation, and translations. */
export function FeedbackSheet({
	client,
	visible,
	identityKey,
	onClose,
	onSubmitted,
	theme,
	labels: overrides,
}: FeedbackSheetProps) {
	const labels = { ...defaults, ...overrides };
	const system = useColorScheme();
	const dark = (theme ?? system) === "dark";
	const colors = {
		backgroundColor: dark ? "#241f3b" : "#fff",
		color: dark ? "#f5f3ff" : "#19152e",
	};
	const {
		body,
		setBody,
		kind,
		setKind,
		conversation,
		suggestions,
		item,
		busy,
		error,
		reset,
		send,
		submit,
		vote,
	} = useFeedback({ client, visible, identityKey, onSubmitted });

	return (
		<Modal
			visible={visible}
			animationType="slide"
			presentationStyle="pageSheet"
			onRequestClose={onClose}
		>
			<SafeAreaView style={[styles.fill, colors]}>
				<KeyboardAvoidingView
					style={styles.fill}
					behavior={Platform.OS === "ios" ? "padding" : "height"}
				>
					<View style={styles.header}>
						<Text accessibilityRole="header" style={[styles.title, colors]}>
							{labels.title}
						</Text>
						<Button title={labels.close} onPress={onClose} />
					</View>
					<ScrollView contentContainerStyle={styles.content} keyboardShouldPersistTaps="handled">
						{!!error && (
							<Text accessibilityRole="alert" style={styles.error}>
								{error}
							</Text>
						)}
						{item && (
							<Text accessibilityLiveRegion="polite" style={[styles.status, colors]}>
								{item.title} · {item.status}
							</Text>
						)}
						{conversation &&
							!item &&
							conversation.messages.map((message, index) => (
								<Text key={index} style={[styles.message, colors]}>
									{message.role === "assistant" ? "Assistant: " : "You: "}
									{message.body}
								</Text>
							))}
						{item?.messages.map((message, index) => (
							<Text key={index} style={[styles.message, colors]}>
								{message.author}: {message.body}
							</Text>
						))}
						{suggestions.map((suggestion) => (
							<View key={suggestion.id} style={styles.card}>
								<Text style={[styles.status, colors]}>{suggestion.title}</Text>
								<Text style={colors}>
									{suggestion.status} · {suggestion.votes}
								</Text>
								<Pressable
									accessibilityRole="button"
									accessibilityState={{ selected: suggestion.voted, disabled: busy }}
									disabled={busy}
									style={styles.vote}
									onPress={() => vote(suggestion)}
								>
									<Text style={colors}>{suggestion.voted ? labels.voted : labels.upvote}</Text>
								</Pressable>
							</View>
						))}
						{!conversation && !item && (
							<View style={styles.row}>
								{(["feature_request", "bug_report"] as const).map((value) => (
									<Pressable
										key={value}
										accessibilityRole="radio"
										accessibilityState={{ checked: kind === value }}
										onPress={() => setKind(value)}
										style={[styles.choice, kind === value && styles.selected]}
									>
										<Text style={colors}>
											{value === "bug_report" ? labels.bug : labels.feature}
										</Text>
									</Pressable>
								))}
							</View>
						)}
						<TextInput
							accessibilityLabel={labels.message}
							placeholder={labels.message}
							placeholderTextColor={dark ? "#c0b8d8" : "#5b5472"}
							multiline
							maxLength={4000}
							value={body}
							onChangeText={setBody}
							editable={!busy}
							style={[styles.input, colors]}
						/>
						<Button
							disabled={busy || !body.trim()}
							title={busy ? labels.working : conversation || item ? labels.reply : labels.send}
							onPress={send}
						/>
						{conversation && !item && (
							<Button disabled={busy} title={labels.submit} onPress={submit} />
						)}
						{item && <Button disabled={busy} title={labels.another} onPress={reset} />}
					</ScrollView>
				</KeyboardAvoidingView>
			</SafeAreaView>
		</Modal>
	);
}

const styles = StyleSheet.create({
	fill: { flex: 1 },
	header: {
		padding: 16,
		flexDirection: "row",
		justifyContent: "space-between",
		alignItems: "center",
	},
	title: { fontSize: 24, fontWeight: "700" },
	content: { padding: 20, gap: 12 },
	row: { flexDirection: "row", gap: 8 },
	input: {
		minHeight: 120,
		borderWidth: 1,
		borderColor: "#aaa",
		borderRadius: 12,
		padding: 12,
		textAlignVertical: "top",
	},
	message: { padding: 12, borderRadius: 12, borderWidth: 1, borderColor: "#8b5cf6" },
	card: { padding: 14, borderRadius: 12, borderWidth: 1, borderColor: "#aaa", gap: 8 },
	status: { fontSize: 16, fontWeight: "600" },
	vote: { padding: 12, borderRadius: 10, borderWidth: 1, borderColor: "#8b5cf6" },
	choice: { padding: 10, borderRadius: 10, borderWidth: 1, borderColor: "#aaa" },
	selected: { borderColor: "#8b5cf6", borderWidth: 2 },
	error: { color: "#c23748" },
});
