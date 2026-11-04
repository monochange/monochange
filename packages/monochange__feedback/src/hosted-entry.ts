import { startFeedbackSignIn, startHostedFeedback } from "./hosted.ts";

if (document.querySelector("[data-continue]")) startFeedbackSignIn(window);
else startHostedFeedback(window);
