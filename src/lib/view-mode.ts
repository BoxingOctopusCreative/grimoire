export type ViewMode = 'grid' | 'list';

const STORAGE_KEY = 'grimoire-device-view';

export function getStoredViewMode(): ViewMode {
	if (typeof localStorage === 'undefined') return 'grid';
	const value = localStorage.getItem(STORAGE_KEY);
	if (value === 'grid' || value === 'list') return value;
	return 'grid';
}

export function setStoredViewMode(mode: ViewMode): ViewMode {
	if (typeof localStorage !== 'undefined') {
		localStorage.setItem(STORAGE_KEY, mode);
	}
	return mode;
}
