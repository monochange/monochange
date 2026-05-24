import { formatReleasePlan } from "@acme/shared";

export function renderDashboardVersion(version: string) {
	return formatReleasePlan(`dashboard ${version}`);
}
