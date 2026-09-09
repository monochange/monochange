import type { CompilerOptions } from "typescript";

export function parse(value: CompilerOptions): string {
	return String(value.strict).trim();
}
