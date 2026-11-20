// The words on Settings · Search. Every host, count and model name here is invented.
import { describe, expect, it } from 'vitest';
import type { SearchStatus } from '$lib/api/client';
import { modelName, runsOn, searchLead } from './searchStatus';

const NOW = new Date('2026-10-06T12:00:00').getTime();

const status = (over: Partial<SearchStatus> = {}): SearchStatus => ({
	mode: 'bundled',
	embed_url: 'http://127.0.0.1:18181',
	embed_locality: 'local',
	index_model: 'embedder-small',
	index_dims: 256,
	model_change: null,
	records_searchable: 1200,
	chunks: 5000,
	last_indexed_at: new Date('2026-10-06T11:56:00').toISOString(),
	reachable: true,
	busy: false,
	probe_ms: 19,
	probe_error: null,
	accelerator: null,
	accelerator_guide: null,
	...over,
});

describe('searchLead', () => {
	it('says search is answering, and what it covers', () => {
		expect(searchLead(status(), NOW)).toEqual({
			sentence: 'Search is answering.',
			sub: 'It covers 1,200 records, and your server last added to it 4 minutes ago.',
		});
		expect(searchLead(status({ records_searchable: 0, last_indexed_at: null }), NOW).sub).toBe('Nothing is in the index yet.');
	});

	it('puts a server that isn’t answering first', () => {
		expect(searchLead(status({ reachable: false, model_change: { model: 'm', done: 1, total: 2 } }), NOW).sentence).toBe('Search isn’t answering.');
		expect(searchLead(status({ reachable: false, busy: true }), NOW).sentence).toBe('Search is busy.');
	});

	it('says how far a move to another model has come', () => {
		const moving = status({ model_change: { model: '/models/embedder-large.gguf', done: 250, total: 1000 } });
		expect(searchLead(moving, NOW)).toEqual({
			sentence: 'Search is moving to a new model.',
			sub: '25% of the index is rebuilt for embedder-large. Search keeps using the current model until the rebuild finishes.',
		});
	});
});

describe('runsOn', () => {
	it('names this server’s processor for the engine Virtues runs', () => {
		expect(runsOn(status())).toBe('This server’s processor');
		expect(runsOn(status({ mode: 'dragon' }))).toBe('This server’s NPU');
	});

	it('names another machine by its host, and says when it is outside your network', () => {
		expect(runsOn(status({ mode: 'manual', embed_url: 'http://192.168.1.20:8080' }))).toBe('Your server at 192.168.1.20:8080');
		expect(runsOn(status({ mode: 'manual', embed_url: 'https://search.example.com', embed_locality: 'public' }))).toBe(
			'search.example.com, outside your network',
		);
		// Going back to the recommended setup, search uses the other machine until the new index is built.
		expect(runsOn(status({ embed_url: 'http://192.168.1.20:8080' }))).toBe('Your server at 192.168.1.20:8080');
	});
});

describe('modelName', () => {
	it('drops folders and the file extension', () => {
		expect(modelName('/var/lib/models/embedder-small-q8.gguf')).toBe('embedder-small-q8');
		expect(modelName('embedder-small')).toBe('embedder-small');
	});
});
