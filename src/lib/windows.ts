import { WebviewWindow } from '@tauri-apps/api/webviewWindow';

export function readerWindowLabel(bookId: number) {
	return `reader-${bookId}`;
}

/** Open (or focus) the in-app reader for a book. */
export async function openReaderWindow(bookId: number) {
	const label = readerWindowLabel(bookId);
	const existing = await WebviewWindow.getByLabel(label);
	if (existing) {
		await existing.setFocus();
		return;
	}

	const reader = new WebviewWindow(label, {
		url: `/read/${bookId}`,
		title: 'Reading',
		width: 980,
		height: 860,
		minWidth: 640,
		minHeight: 520,
		center: true,
		resizable: true,
		focus: true,
		decorations: false,
		transparent: true,
		shadow: true
	});

	await new Promise<void>((resolve, reject) => {
		reader.once('tauri://created', () => resolve());
		reader.once('tauri://error', (event) => {
			reject(new Error(String(event.payload ?? 'Could not open reader window')));
		});
	});
}
