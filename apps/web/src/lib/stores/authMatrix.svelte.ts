import { writable } from "svelte/store";

// Store for the auth matrix message (e.g., "SENT" after email submission)
export const authMatrixMessage = writable<string | undefined>(undefined);
