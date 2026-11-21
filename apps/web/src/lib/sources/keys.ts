/**
 * What the paste form says for a source connected with a key: a name for
 * each field the catalog declares, and where in the service the key is made.
 *
 * The catalog (`applets/sources.toml`) names fields the way the sync reads
 * them (`api_key`, `token`), which is not how a person reads them. A source
 * missing here gets its field names as they are.
 */

export interface KeyHelp {
	/** Where the person makes the key, as a sentence. */
	where?: string;
	/** The field's label, by catalog field name. */
	labels?: Record<string, string>;
}

const KEY_HELP: Record<string, KeyHelp> = {
	github: {
		where: 'On GitHub, open Settings, then Developer settings, and create a fine-grained token with read-only access.',
		labels: { token: 'Access token' }
	},
	fathom: {
		where: 'In Fathom, open Settings, then API Access, and create a key.',
		labels: { api_key: 'API key', email: 'Your email in Fathom' }
	},
	fireflies: {
		where: 'In Fireflies.ai, open Integrations, then Fireflies API, and copy your key.',
		labels: { api_key: 'API key' }
	},
	granola: {
		where:
			'In the Granola app, open Settings, then Connectors, then API keys. Create a key with only Personal notes. Granola offers keys on its Business and Enterprise plans.',
		labels: { api_key: 'API key' }
	}
};

export function keyHelp(sourceId: string): KeyHelp {
	return KEY_HELP[sourceId] ?? {};
}

/** The label for one field: the source's own, else the catalog name. */
export function fieldLabel(sourceId: string, field: string): string {
	return KEY_HELP[sourceId]?.labels?.[field] ?? field;
}

/** Fields that hold a secret are masked; an email is not a secret. */
export function isSecretField(field: string): boolean {
	return field !== 'email';
}
