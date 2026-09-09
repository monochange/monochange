export class Client {
	request(path: string): Promise<string> {
		return Promise.resolve(path);
	}
}

export interface Box<T> {
	value: T;
}

export enum Mode {
	Fast,
}

export interface Options {
	strict: boolean;
}

export interface Mutable {
	value: string;
}

export interface Locked {
	readonly value: string;
}

export function parse(value: string): string {
	return value;
}

export function choose(): string {
	return "default";
}

export function configure(value: string): void {
	void value;
}

export type Scalar = string | number;

export { format } from "./format.js";
