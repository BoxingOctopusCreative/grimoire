import { beforeEach, describe, expect, it } from 'vitest';
import { getStoredViewMode, setStoredViewMode } from './view-mode';

describe('view-mode', () => {
	beforeEach(() => {
		localStorage.clear();
	});

	it('defaults to grid', () => {
		expect(getStoredViewMode()).toBe('grid');
	});

	it('persists list mode', () => {
		expect(setStoredViewMode('list')).toBe('list');
		expect(getStoredViewMode()).toBe('list');
	});

	it('ignores invalid stored values', () => {
		localStorage.setItem('grimoire-device-view', 'masonry');
		expect(getStoredViewMode()).toBe('grid');
	});
});
