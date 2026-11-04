import { useEffect, useRef } from "react";
import { mountFeedback, type FeedbackWidget, type WidgetOptions } from "../src/index.ts";

/** Keep options stable with useMemo in the host; changing identity erases the previous draft. */
export function FeedbackButton({
	options,
	identityKey,
}: {
	options: WidgetOptions;
	identityKey: string;
}) {
	const widget = useRef<FeedbackWidget | null>(null);
	useEffect(() => {
		widget.current = mountFeedback({ ...options, launcher: false });
		return () => {
			widget.current?.destroy();
			widget.current = null;
		};
	}, [options]);
	useEffect(() => widget.current?.reset(), [identityKey]);
	return (
		<button type="button" onClick={() => widget.current?.open()}>
			Share feedback
		</button>
	);
}
