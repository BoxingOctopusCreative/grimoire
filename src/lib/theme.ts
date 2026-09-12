export type ThemePreference = 'light' | 'dark' | 'system';
export type ResolvedTheme = 'light' | 'dark';

const STORAGE_KEY = 'grimoire-theme';

export function getStoredTheme(): ThemePreference {
	if (typeof localStorage === 'undefined') return 'dark';
	const value = localStorage.getItem(STORAGE_KEY);
	if (value === 'light' || value === 'dark' || value === 'system') return value;
	return 'dark';
}

export function resolveTheme(preference: ThemePreference): ResolvedTheme {
	if (preference === 'light' || preference === 'dark') return preference;
	if (typeof window === 'undefined' || !window.matchMedia) return 'dark';
	return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

export function applyTheme(preference: ThemePreference): ResolvedTheme {
	const resolved = resolveTheme(preference);
	if (typeof document !== 'undefined') {
		document.documentElement.dataset.theme = resolved;
		document.documentElement.style.colorScheme = resolved;
	}
	return resolved;
}

export function setThemePreference(preference: ThemePreference): ResolvedTheme {
	if (typeof localStorage !== 'undefined') {
		localStorage.setItem(STORAGE_KEY, preference);
	}
	return applyTheme(preference);
}

export function cycleTheme(current: ThemePreference): ThemePreference {
	if (current === 'system') return 'light';
	if (current === 'light') return 'dark';
	return 'system';
}

export function themeLabel(preference: ThemePreference): string {
	if (preference === 'light') return 'Light';
	if (preference === 'dark') return 'Dark';
	return 'System';
}
