export interface User {
	id: string;
}

export interface AdminUser extends User {
	role: string;
}

export function createUser(id: string): User {
	return { id };
}
