export type ChromeOs = 'macos' | 'windows' | 'linux';

/** OS for window-chrome placement and styling. Sync-safe for SSR. */
export function detectChromeOs(): ChromeOs {
	if (typeof navigator === 'undefined') return 'macos';

	const platform = navigator.platform?.toLowerCase() ?? '';
	const ua = navigator.userAgent.toLowerCase();

	if (platform.includes('mac') || ua.includes('mac os')) return 'macos';
	if (platform.includes('win') || ua.includes('windows')) return 'windows';
	return 'linux';
}

export function windowControlsOnLeft(os: ChromeOs = detectChromeOs()) {
	return os === 'macos';
}
