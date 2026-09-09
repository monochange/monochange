export class Client {
	request(path: string): Promise<string> {
		return Promise.resolve(path);
	}

	close(): void {}
}

export interface Box<T extends string> {
	value: T;
}

export enum Mode {
	Fast,
	Safe,
}

export interface Options {
	cache?: boolean;
	strict: boolean;
}

export interface Mutable {
	readonly value: string;
}

export interface Locked {
	value: string;
}

export function parse(value: string): string;
export function parse(value: number): number;
export function parse(value: string | number): string | number {
	return value;
}

export function choose(): "default" {
	return "default";
}

export function configure(value: string, options?: Options): void {
	void value;
	void options;
}

export type Scalar = string;

export { format } from "./format.js";
