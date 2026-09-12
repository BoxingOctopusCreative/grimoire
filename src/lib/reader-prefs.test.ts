import { beforeEach, describe, expect, it } from 'vitest';
import {
	getReaderFontOption,
	getStoredReaderFont,
	isReaderFontId,
	openDyslexicFontFaceCss,
	setStoredReaderFont
} from './reader-prefs';

describe('reader-prefs', () => {
	beforeEach(() => {
		localStorage.clear();
	});

	it('validates font ids', () => {
		expect(isReaderFontId('serif')).toBe(true);
		expect(isReaderFontId('opendyslexic')).toBe(true);
		expect(isReaderFontId('comic-sans')).toBe(false);
		expect(isReaderFontId(null)).toBe(false);
	});

	it('defaults stored font to serif', () => {
		expect(getStoredReaderFont()).toBe('serif');
	});

	it('persists a valid reader font', () => {
		expect(setStoredReaderFont('opendyslexic')).toBe('opendyslexic');
		expect(getStoredReaderFont()).toBe('opendyslexic');
	});

	it('returns publisher option with null css family', () => {
		expect(getReaderFontOption('publisher').cssFamily).toBeNull();
		expect(getReaderFontOption('serif').cssFamily).toContain('Source Serif');
	});

	it('builds OpenDyslexic @font-face CSS from an origin', () => {
		const css = openDyslexicFontFaceCss('https://example.test');
		expect(css).toContain("font-family: 'OpenDyslexic'");
		expect(css).toContain(
			"url('https://example.test/fonts/opendyslexic/OpenDyslexic-Regular.woff2')"
		);
	});
});
