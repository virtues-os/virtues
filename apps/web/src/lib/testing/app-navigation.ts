// `$app/navigation` for the `components` test project, which runs without
// SvelteKit's plugin (vitest.config.ts). The shell's URL sync calls these;
// a test that cares about them mocks this module.
export function pushState(_url: string, _state: unknown): void {}
export function replaceState(_url: string, _state: unknown): void {}
export async function goto(_url: string): Promise<void> {}
