import type { BookSummary } from '$lib/api';

export type ReadingStatus = 'reading' | 'to-read' | 'read';
export type FormatKind = 'novel' | 'comic';

export function bookStatus(b: BookSummary): ReadingStatus {
	if (b.progress_percent == null) return 'to-read';
	if (b.progress_percent >= 100) return 'read';
	return 'reading';
}

export function bookFormatKind(b: BookSummary): FormatKind {
	return b.formats.some((f) => {
		const u = f.toUpperCase();
		return u === 'CBZ' || u === 'CBR';
	})
		? 'comic'
		: 'novel';
}

export function bookAuthorLabels(b: BookSummary): string[] {
	const cleaned = b.authors.map((a) => a.trim()).filter(Boolean);
	return cleaned.length ? cleaned : ['Unknown'];
}

export function bookMatchesFilters(
	b: BookSummary,
	statuses: Set<ReadingStatus>,
	genres: Set<string>,
	formats: Set<FormatKind>,
	authors: Set<string>
): boolean {
	if (statuses.size > 0 && !statuses.has(bookStatus(b))) return false;
	if (formats.size > 0 && !formats.has(bookFormatKind(b))) return false;
	if (genres.size > 0 && !b.tags.some((t) => genres.has(t.trim()))) return false;
	if (authors.size > 0 && !bookAuthorLabels(b).some((a) => authors.has(a))) return false;
	return true;
}
