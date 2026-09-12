import { invoke } from '@tauri-apps/api/core';

export type AppConfig = {
	library_path: string | null;
};

export type LibraryStatus = {
	open: boolean;
	path: string | null;
	kindle_convert_available: boolean;
};

export type BookSummary = {
	id: number;
	uuid: string;
	title: string;
	authors: string[];
	tags: string[];
	formats: string[];
	has_cover: boolean;
	series: string | null;
	series_index: number | null;
	/** Null when the book has never been opened in the reader. */
	progress_percent: number | null;
};

export type FormatInfo = {
	format: string;
	filename: string;
	size: number;
};

export type IdentifierInfo = {
	type_name: string;
	value: string;
};

export type BookDetail = {
	id: number;
	uuid: string;
	title: string;
	authors: string[];
	tags: string[];
	formats: FormatInfo[];
	has_cover: boolean;
	series: string | null;
	series_index: number | null;
	comment: string;
	identifiers: IdentifierInfo[];
	path: string;
};

export type BookUpdate = {
	title: string;
	authors: string[];
	tags: string[];
	series: string | null;
	series_index: number | null;
	comment: string;
	identifiers?: IdentifierInfo[];
};

export type ReaderFamily =
	| 'kindle'
	| 'kobo'
	| 'pocketbook'
	| 'remarkable'
	| 'boox'
	| 'tolino'
	| 'nook'
	| 'other';

export type ConnectionKind = 'mtp' | 'mount';

export type EbookReaderDevice = {
	id: string;
	connection: ConnectionKind;
	location_id: number | null;
	mount_path: string | null;
	manufacturer: string;
	product: string;
	serial_number: string | null;
	family: ReaderFamily;
	family_label: string;
	free_space_bytes: number | null;
	display: string;
	supports_send: boolean;
	preferred_formats: string[];
	notes: string | null;
};

export type KindleDeviceInfo = {
	location_id: number;
	manufacturer: string;
	product: string;
	serial_number: string | null;
	is_kindle: boolean;
	free_space_bytes: number | null;
	display: string;
};

export type TransferResult = {
	remote_path: string;
	device_serial: string;
	format: string;
};

export type SyncRecord = {
	book_id: number;
	device_serial: string;
	last_sent_at: string;
	remote_path: string;
	format: string;
};

export async function getConfig() {
	return invoke<AppConfig>('get_config');
}

export async function getLibraryStatus() {
	return invoke<LibraryStatus>('get_library_status');
}

export async function openSavedLibrary() {
	return invoke<LibraryStatus>('open_saved_library');
}

export async function createOrOpenLibrary(path: string) {
	return invoke<LibraryStatus>('create_or_open_library', { path });
}

export async function clearSavedLibrary() {
	return invoke<LibraryStatus>('clear_saved_library');
}

export async function listEbooks(query?: string) {
	return invoke<BookSummary[]>('list_ebooks', { query: query ?? null });
}

export async function importEbook(path: string) {
	return invoke<BookDetail>('import_ebook', { path });
}

export async function getEbook(id: number) {
	return invoke<BookDetail>('get_ebook', { id });
}

export async function updateEbook(id: number, update: BookUpdate) {
	return invoke<BookDetail>('update_ebook', { id, update });
}

export async function deleteEbook(id: number) {
	return invoke<void>('delete_ebook', { id });
}

export type BookBulkPatch = {
	add_tags: string[];
	set_series: boolean;
	series: string | null;
	set_series_index: boolean;
	series_index: number | null;
	set_comment: boolean;
	comment: string;
};

export type BulkFailure = {
	id: number;
	error: string;
};

export type BulkActionResult = {
	updated: number;
	failed: BulkFailure[];
};

export async function deleteEbooks(ids: number[]) {
	return invoke<BulkActionResult>('delete_ebooks_batch', { ids });
}

export async function bulkPatchEbooks(ids: number[], patch: BookBulkPatch) {
	return invoke<BulkActionResult>('bulk_patch_ebooks_cmd', { ids, patch });
}

export async function getCover(id: number) {
	return invoke<string | null>('get_cover', { id });
}

export type EnrichmentResult = {
	book: BookDetail;
	updated_fields: string[];
	source: string;
};

export async function enrichEbookMetadata(id: number) {
	return invoke<EnrichmentResult>('enrich_ebook_metadata', { id });
}

export type CoverCandidate = {
	id: string;
	volume_id: string | null;
	title: string;
	authors: string;
	preview_url: string;
	download_urls: string[];
};

export async function listEbookCoverCandidates(id: number) {
	return invoke<CoverCandidate[]>('list_ebook_cover_candidates', { id });
}

export async function setEbookCoverFromUrls(id: number, urls: string[]) {
	return invoke<BookDetail>('set_ebook_cover_from_urls', { id, urls });
}

export type ReaderSource = {
	book_id: number;
	title: string;
	authors: string[];
	format: string;
	path: string;
	available_formats: string[];
};

export type ReadingProgress = {
	book_id: number;
	format: string;
	location: string;
	percent: number;
	updated_at: string;
};

export type ReadingProgressUpdate = {
	format: string;
	location: string;
	percent: number;
};

export async function getReaderSource(bookId: number, format?: string) {
	return invoke<ReaderSource>('get_reader_source', {
		bookId,
		format: format ?? null
	});
}

export async function getEbookReadingProgress(bookId: number) {
	return invoke<ReadingProgress | null>('get_ebook_reading_progress', { bookId });
}

export async function saveEbookReadingProgress(bookId: number, update: ReadingProgressUpdate) {
	return invoke<ReadingProgress>('save_ebook_reading_progress', { bookId, update });
}

export type ComicPageList = {
	book_id: number;
	format: string;
	page_count: number;
	pages: string[];
};

export type ComicPage = {
	book_id: number;
	index: number;
	page_count: number;
	name: string;
	data_url: string;
};

export async function listComicPages(bookId: number) {
	return invoke<ComicPageList>('list_comic_pages', { bookId });
}

export async function getComicPage(bookId: number, index: number) {
	return invoke<ComicPage>('get_comic_page', { bookId, index });
}

export type ConvertResult = {
	book: BookDetail;
	format: string;
	path: string;
	converted: boolean;
};

export async function listEbookConvertTargets(bookId: number) {
	return invoke<string[]>('list_ebook_convert_targets', { bookId });
}

export async function convertEbookFormat(bookId: number, targetFormat: string) {
	return invoke<ConvertResult>('convert_ebook_format', { bookId, targetFormat });
}

export async function listKindleDevices() {
	return invoke<KindleDeviceInfo[]>('list_kindle_devices');
}

export async function listEbookReaders() {
	return invoke<EbookReaderDevice[]>('list_ebook_reader_devices');
}

export type DeviceBook = {
	title: string;
	format: string;
	filename: string;
	path: string;
	size: number;
	handle: number | null;
};

export type DeviceLibrary = {
	device_id: string;
	device_name: string;
	connection: ConnectionKind;
	books: DeviceBook[];
};

export type DeviceBookTarget = {
	path: string;
	filename: string;
	handle: number | null;
};

export type DeviceBulkFailure = {
	path: string;
	error: string;
};

export type DeviceBulkResult = {
	removed: number;
	failed: DeviceBulkFailure[];
};

export async function scanEbookReader(deviceId: string) {
	return invoke<DeviceLibrary>('scan_ebook_reader', { deviceId });
}

export async function deleteEbookReaderBooks(deviceId: string, books: DeviceBookTarget[]) {
	return invoke<DeviceBulkResult>('delete_ebook_reader_books', { deviceId, books });
}

export async function sendBookToKindle(bookId: number, deviceId: string) {
	return invoke<TransferResult>('send_book_to_kindle', {
		bookId,
		deviceId
	});
}

export async function getSyncHistory() {
	return invoke<SyncRecord[]>('get_sync_history');
}
