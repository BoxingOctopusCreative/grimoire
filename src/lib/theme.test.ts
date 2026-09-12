import { beforeEach, describe, expect, it } from 'vitest';
import {
	applyTheme,
	cycleTheme,
	getStoredTheme,
	resolveTheme,
	setThemePreference,
	themeLabel
} from './theme';

describe('theme', () => {
	beforeEach(() => {
		localStorage.clear();
		document.documentElement.removeAttribute('data-theme');
		document.documentElement.style.colorScheme = '';
	});

	it('defaults to dark when nothing is stored', () => {
		expect(getStoredTheme()).toBe('dark');
	});

	it('reads a stored preference', () => {
		localStorage.setItem('grimoire-theme', 'light');
		expect(getStoredTheme()).toBe('light');
		localStorage.setItem('grimoire-theme', 'system');
		expect(getStoredTheme()).toBe('system');
	});

	it('ignores invalid stored values', () => {
		localStorage.setItem('grimoire-theme', 'neon');
		expect(getStoredTheme()).toBe('dark');
	});

	it('resolves explicit preferences without matchMedia', () => {
		expect(resolveTheme('light')).toBe('light');
		expect(resolveTheme('dark')).toBe('dark');
	});

	it('cycles system → light → dark → system', () => {
		expect(cycleTheme('system')).toBe('light');
		expect(cycleTheme('light')).toBe('dark');
		expect(cycleTheme('dark')).toBe('system');
	});

	it('labels preferences for the UI', () => {
		expect(themeLabel('light')).toBe('Light');
		expect(themeLabel('dark')).toBe('Dark');
		expect(themeLabel('system')).toBe('System');
	});

	it('persists preference and applies dataset theme', () => {
		const resolved = setThemePreference('light');
		expect(resolved).toBe('light');
		expect(localStorage.getItem('grimoire-theme')).toBe('light');
		expect(document.documentElement.dataset.theme).toBe('light');
		expect(document.documentElement.style.colorScheme).toBe('light');
	});

	it('applyTheme updates the document without requiring storage', () => {
		expect(applyTheme('dark')).toBe('dark');
		expect(document.documentElement.dataset.theme).toBe('dark');
	});
});
