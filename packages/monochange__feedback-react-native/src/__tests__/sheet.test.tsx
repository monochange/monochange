import React from "react";
import { act, create, type ReactTestRenderer } from "react-test-renderer";
import { expect, it, vi } from "vitest";
import { Button, TextInput } from "react-native";
import type { FeedbackClient } from "@monochange/feedback/client";
import { FeedbackSheet } from "../index.tsx";

vi.mock("react-native", () => ({
	AppState: { addEventListener: () => ({ remove() {} }) },
	Button: "Button",
	KeyboardAvoidingView: "KeyboardAvoidingView",
	Modal: "Modal",
	Platform: { OS: "ios" },
	Pressable: "Pressable",
	SafeAreaView: "SafeAreaView",
	ScrollView: "ScrollView",
	Text: "Text",
	TextInput: "TextInput",
	View: "View",
	StyleSheet: { create: (styles: unknown) => styles },
	useColorScheme: () => "light",
}));

it("native conversations offer explicit submission and erase state on account changes", async () => {
	(globalThis as any).IS_REACT_ACT_ENVIRONMENT = true;
	const client = {
		reset: vi.fn(),
		startConversation: vi.fn(async () => ({
			conversation: {
				id: "draft",
				version: 1,
				messages: [{ role: "assistant", body: "Which columns?" }],
			},
			similar: [],
		})),
	} as unknown as FeedbackClient;
	let renderer: ReactTestRenderer;
	await act(async () => {
		renderer = create(
			<FeedbackSheet client={client} visible identityKey="user-a" onClose={() => {}} />,
		);
	});
	await act(async () => {
		renderer.root.findByType(TextInput).props.onChangeText("CSV export");
	});
	await act(async () => {
		renderer.root
			.findAllByType(Button)
			.find((button) => button.props.title === "Find similar requests")!
			.props.onPress();
	});
	expect(client.startConversation).toHaveBeenCalledWith("CSV export", "feature_request");
	expect(
		renderer.root
			.findAllByType(Button)
			.some((button) => button.props.title === "Submit a new request"),
	).toBe(true);
	await act(async () => {
		renderer.update(
			<FeedbackSheet client={client} visible identityKey="user-b" onClose={() => {}} />,
		);
	});
	expect(
		renderer.root
			.findAllByType(Button)
			.some((button) => button.props.title === "Submit a new request"),
	).toBe(false);
	expect(client.reset).toHaveBeenCalledTimes(2);
	await act(async () => {
		renderer.unmount();
	});
});
