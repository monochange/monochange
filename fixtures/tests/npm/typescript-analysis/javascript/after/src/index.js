export function parse(value, options) {
	return options?.trim ? value.trim() : value;
}
