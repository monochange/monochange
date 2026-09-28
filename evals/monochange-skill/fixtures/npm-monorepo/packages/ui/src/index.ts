/**
 * Presentation helpers built on top of `@acme/api`.
 *
 * These helpers stay framework-free so both the web app and the docs site can
 * consume them.
 */

import { joinUrl, request, type ApiResponse } from "@acme/api";

/** User shape rendered by the profile card. */
export interface UserProfile {
  /** Stable user identifier. */
  id: string;
  /** Name shown in the UI. */
  displayName: string;
  /** Contact address shown next to the name. */
  email: string;
}

/** Fetch a user profile from the Acme API. */
export async function fetchUserProfile(
  baseUrl: string,
  userId: string,
): Promise<UserProfile> {
  const response: ApiResponse<UserProfile> = await request<UserProfile>(
    baseUrl,
    `/users/${encodeURIComponent(userId)}`,
  );

  if (response.data.id !== userId) {
    throw new Error(`api returned ${response.data.id} for requested user ${userId}`);
  }

  return response.data;
}

/** Build the absolute profile URL linked from the card. */
export function profileUrl(baseUrl: string, userId: string): string {
  return joinUrl(baseUrl, `/users/${encodeURIComponent(userId)}`);
}

/** Render the label shown under the profile avatar. */
export function profileLabel(profile: UserProfile): string {
  return `${profile.displayName} <${profile.email}>`;
}
