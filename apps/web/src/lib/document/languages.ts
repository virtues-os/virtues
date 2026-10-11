/**
 * The offer in a code block's language picker, for both page editors.
 * Deliberately a short, curated list, not the hundreds of languages the
 * highlighters know: a picker is a menu, not a search index. A code block
 * still takes any language when typed: a markdown fence's info string, a
 * `language` the model writes, or, on a block page, one typed after the
 * picker's Other… (`OTHER_LANGUAGE`); this is the fast path for the common
 * cases. Each entry is (language, display label); '' is plain text.
 */
export const LANGUAGE_CHOICES: [string, string][] = [
	['', 'Plain text'],
	['js', 'JavaScript'],
	['ts', 'TypeScript'],
	['python', 'Python'],
	['rust', 'Rust'],
	['go', 'Go'],
	['sh', 'Shell'],
	['sql', 'SQL'],
	['json', 'JSON'],
	['yaml', 'YAML'],
	['html', 'HTML'],
	['css', 'CSS'],
	['swift', 'Swift'],
	['java', 'Java'],
	['cpp', 'C++'],
	['md', 'Markdown'],
];

/** The label a language shows under, or the language itself when it is not offered. */
export function languageLabel(language: string | null | undefined): string {
	const lang = language ?? '';
	return LANGUAGE_CHOICES.find(([value]) => value === lang)?.[1] ?? lang;
}

/**
 * The picker's Other… entry, which takes a language typed. No language is
 * this value: a typed one is trimmed (`typedLanguage`).
 */
export const OTHER_LANGUAGE = ' other';

/**
 * A language typed for a code block: its first word, as a markdown fence's
 * info string names its language; '' for none, which is plain text.
 */
export function typedLanguage(text: string): string {
	return text.trim().split(/\s+/)[0] ?? '';
}
