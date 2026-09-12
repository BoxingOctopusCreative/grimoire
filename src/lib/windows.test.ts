import { describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/webviewWindow', () => ({
	WebviewWindow: class {
		static async getByLabel() {
			return null;
		}
	}
}));

import { readerWindowLabel } from './windows';

describe('windows', () => {
	it('builds a stable reader window label', () => {
		expect(readerWindowLabel(42)).toBe('reader-42');
	});
});
