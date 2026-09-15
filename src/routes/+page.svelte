<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { message, open } from '@tauri-apps/plugin-dialog';
	import { getCurrentWebview } from '@tauri-apps/api/webview';
	import {
		bulkPatchEbooks,
		convertEbookFormat,
		createOrOpenLibrary,
		deleteEbook,
		deleteEbooks,
		enrichEbookMetadata,
		getCover,
		getLibraryStatus,
		importEbook,
		listEbookConvertTargets,
		listEbookReaders,
		listEbooks,
		sendBookToKindle,
		type BookBulkPatch,
		type BookDetail,
		type BookSummary,
		type EbookReaderDevice,
		type LibraryStatus
	} from '$lib/api';
	import { devicePresence } from '$lib/device-presence.svelte';
	import { openReaderWindow } from '$lib/windows';
	import ListChecks from '@lucide/svelte/icons/list-checks';
	import Check from '@lucide/svelte/icons/check';
	import Import from '@lucide/svelte/icons/import';
	import FolderSync from '@lucide/svelte/icons/folder-sync';
	import PanelLeft from '@lucide/svelte/icons/panel-left';
	import LibraryFilters from '$lib/LibraryFilters.svelte';
	import {
		bookMatchesFilters,
		type FormatKind,
		type ReadingStatus
	} from '$lib/library-filters';
	import ActionButton from '$lib/ui/ActionButton.svelte';
	import Button from '$lib/ui/Button.svelte';
	import Checkbox from '$lib/ui/Checkbox.svelte';
	import Dialog from '$lib/ui/Dialog.svelte';
	import SearchField from '$lib/ui/SearchField.svelte';
	import TextField from '$lib/ui/TextField.svelte';

	const SUPPORTED_EXT = new Set(['epub', 'pdf', 'azw3', 'mobi', 'txt', 'cbz', 'cbr']);
	const READABLE_FORMATS = new Set(['EPUB', 'PDF', 'TXT', 'MOBI', 'CBZ', 'CBR']);
	const KINDLE_FORMATS = new Set(['AZW3', 'MOBI']);

	type ContextMenuState = {
		bookId: number;
		title: string;
		canRead: boolean;
		convertTargets: string[];
		x: number;
		y: number;
	};

	let status = $state<LibraryStatus | null>(null);
	let books = $state<BookSummary[]>([]);
	let covers = $state<Record<number, string>>({});
	let query = $state('');
	let loading = $state(true);
	let busy = $state(false);
	let error = $state('');
	let dragActive = $state(false);
	let importMessage = $state('');
	let contextMenu = $state<ContextMenuState | null>(null);
	const hasSendableDevice = $derived(
		devicePresence.devices.some((d) => d.supports_send)
	);

	let selectMode = $state(false);
	let selectedIds = $state<Set<number>>(new Set());
	let filtersOpen = $state(true);
	let filterStatuses = $state<Set<ReadingStatus>>(new Set());
	let filterGenres = $state<Set<string>>(new Set());
	let filterFormats = $state<Set<FormatKind>>(new Set());
	let filterAuthors = $state<Set<string>>(new Set());
	let bulkEditOpen = $state(false);
	let bulkAddTags = $state('');
	let bulkApplySeries = $state(false);
	let bulkSeries = $state('');
	let bulkApplySeriesIndex = $state(false);
	let bulkSeriesIndex = $state('');
	let bulkReplaceNotes = $state(false);
	let bulkNotes = $state('');

	const selectedCount = $derived(selectedIds.size);

	const filtersActive = $derived(
		filterStatuses.size > 0 ||
			filterGenres.size > 0 ||
			filterFormats.size > 0 ||
			filterAuthors.size > 0
	);

	const filteredBooks = $derived(
		books.filter((b) =>
			bookMatchesFilters(b, filterStatuses, filterGenres, filterFormats, filterAuthors)
		)
	);

	const readingBooks = $derived(
		filteredBooks.filter(
			(b) => b.progress_percent != null && b.progress_percent < 100
		)
	);
	const toReadBooks = $derived(filteredBooks.filter((b) => b.progress_percent == null));
	const readBooks = $derived(
		filteredBooks.filter((b) => b.progress_percent != null && b.progress_percent >= 100)
	);

	const librarySections = $derived(
		[
			{ id: 'reading', title: 'Reading', books: readingBooks },
			{ id: 'to-read', title: 'To Read', books: toReadBooks },
			{ id: 'read', title: 'Read', books: readBooks }
		] as const
	);

	function closeContextMenu() {
		contextMenu = null;
	}

	function clampMenuPosition(x: number, y: number, width: number, height: number) {
		const pad = 8;
		const maxX = Math.max(pad, window.innerWidth - width - pad);
		const maxY = Math.max(pad, window.innerHeight - height - pad);
		return {
			x: Math.min(Math.max(pad, x), maxX),
			y: Math.min(Math.max(pad, y), maxY)
		};
	}

	async function openContextMenu(event: MouseEvent, book: BookSummary) {
		if (selectMode) return;
		event.preventDefault();
		event.stopPropagation();
		let convertTargets: string[] = [];
		try {
			convertTargets = await listEbookConvertTargets(book.id);
		} catch {
			convertTargets = [];
		}
		contextMenu = {
			bookId: book.id,
			title: book.title,
			canRead: book.formats.some((f) => READABLE_FORMATS.has(f.toUpperCase())),
			convertTargets,
			x: event.clientX,
			y: event.clientY
		};
	}

	function menuPlacement(pos: { x: number; y: number }) {
		return (node: HTMLElement) => {
			const rect = node.getBoundingClientRect();
			const clamped = clampMenuPosition(pos.x, pos.y, rect.width, rect.height);
			node.style.left = `${clamped.x}px`;
			node.style.top = `${clamped.y}px`;
		};
	}

	function clearSelection() {
		selectedIds = new Set();
	}

	function exitSelectMode() {
		selectMode = false;
		clearSelection();
		bulkEditOpen = false;
	}

	function toggleSelectMode() {
		if (selectMode) {
			exitSelectMode();
		} else {
			closeContextMenu();
			selectMode = true;
		}
	}

	function toggleSelected(id: number) {
		const next = new Set(selectedIds);
		if (next.has(id)) next.delete(id);
		else next.add(id);
		selectedIds = next;
	}

	function selectAllVisible() {
		selectedIds = new Set(filteredBooks.map((b) => b.id));
	}

	function onWindowKeydown(event: KeyboardEvent) {
		if (event.key !== 'Escape') return;
		if (contextMenu) {
			closeContextMenu();
			return;
		}
		if (bulkEditOpen) {
			bulkEditOpen = false;
			return;
		}
		if (selectedIds.size > 0) {
			clearSelection();
			return;
		}
		if (selectMode) {
			exitSelectMode();
		}
	}

	function openBulkEdit() {
		if (selectedIds.size === 0) return;
		bulkAddTags = '';
		bulkApplySeries = false;
		bulkSeries = '';
		bulkApplySeriesIndex = false;
		bulkSeriesIndex = '';
		bulkReplaceNotes = false;
		bulkNotes = '';
		bulkEditOpen = true;
	}

	async function applyBulkEdit() {
		const ids = [...selectedIds];
		if (ids.length === 0) return;

		const add_tags = bulkAddTags
			.split(',')
			.map((s) => s.trim())
			.filter(Boolean);
		const patch: BookBulkPatch = {
			add_tags,
			set_series: bulkApplySeries,
			series: bulkApplySeries ? bulkSeries.trim() || null : null,
			set_series_index: bulkApplySeriesIndex,
			series_index:
				bulkApplySeriesIndex && bulkSeriesIndex.trim()
					? Number(bulkSeriesIndex)
					: bulkApplySeriesIndex
						? 1
						: null,
			set_comment: bulkReplaceNotes,
			comment: bulkNotes
		};

		if (
			!patch.add_tags.length &&
			!patch.set_series &&
			!patch.set_series_index &&
			!patch.set_comment
		) {
			error = 'Choose at least one field to update.';
			return;
		}

		busy = true;
		error = '';
		importMessage = '';
		try {
			const result = await bulkPatchEbooks(ids, patch);
			await loadBooks();
			bulkEditOpen = false;
			importMessage =
				result.failed.length === 0
					? `Updated ${result.updated} book${result.updated === 1 ? '' : 's'}.`
					: `Updated ${result.updated}; ${result.failed.length} failed.`;
			if (result.failed.length > 0) {
				error = result.failed
					.slice(0, 3)
					.map((f) => `Book ${f.id}: ${f.error}`)
					.join(' · ');
			}
		} catch (e) {
			error = String(e);
		} finally {
			busy = false;
		}
	}

	async function bulkFetchMetadata() {
		const ids = [...selectedIds];
		if (ids.length === 0) return;
		busy = true;
		error = '';
		importMessage = '';
		let updated = 0;
		const failures: string[] = [];
		try {
			for (const id of ids) {
				try {
					const result = await enrichEbookMetadata(id);
					books = books.map((b) =>
						b.id === id ? summaryFromDetail(result.book, b) : b
					);
					if (result.book.has_cover) {
						const url = await getCover(id);
						if (url) covers = { ...covers, [id]: url };
					}
					if (result.updated_fields.length > 0) updated += 1;
				} catch (e) {
					failures.push(`Book ${id}: ${String(e)}`);
				}
			}
			importMessage =
				updated > 0
					? `Fetched metadata for ${updated} of ${ids.length} book${ids.length === 1 ? '' : 's'}.`
					: 'No missing fields filled for the selection.';
			if (failures.length > 0) {
				error = failures.slice(0, 3).join(' · ');
			}
		} finally {
			busy = false;
		}
	}

	async function bulkSendToDevice() {
		const ids = [...selectedIds];
		if (ids.length === 0) return;
		busy = true;
		error = '';
		importMessage = '';
		let sent = 0;
		const failures: string[] = [];
		try {
			const device = await resolveSendableDevice();
			if (!device) {
				error =
					'No Kindle connected. Connect over MTP / USB file transfer, then try again.';
				return;
			}
			for (const id of ids) {
				try {
					await sendBookToKindle(id, device.id);
					sent += 1;
				} catch (e) {
					const book = books.find((b) => b.id === id);
					failures.push(
						book ? `"${book.title}": ${String(e)}` : `Book ${id}: ${String(e)}`
					);
				}
			}
			importMessage =
				failures.length === 0
					? `Sent ${sent} book${sent === 1 ? '' : 's'} to ${device.product}.`
					: `Sent ${sent} of ${ids.length} books to ${device.product}.`;
			if (failures.length > 0) {
				error = failures.slice(0, 3).join(' · ');
			}
		} finally {
			busy = false;
		}
	}

	async function bulkDelete() {
		const ids = [...selectedIds];
		if (ids.length === 0) return;
		const ok = confirm(
			`Delete ${ids.length} book${ids.length === 1 ? '' : 's'} from the library?`
		);
		if (!ok) return;
		busy = true;
		error = '';
		importMessage = '';
		try {
			const result = await deleteEbooks(ids);
			const failedIds = new Set(result.failed.map((f) => f.id));
			books = books.filter((b) => !ids.includes(b.id) || failedIds.has(b.id));
			const nextCovers = { ...covers };
			for (const id of ids) {
				if (!failedIds.has(id)) delete nextCovers[id];
			}
			covers = nextCovers;
			selectedIds = new Set([...selectedIds].filter((id) => failedIds.has(id)));
			importMessage =
				result.failed.length === 0
					? `Deleted ${result.updated} book${result.updated === 1 ? '' : 's'}.`
					: `Deleted ${result.updated}; ${result.failed.length} failed.`;
			if (result.failed.length > 0) {
				error = result.failed
					.slice(0, 3)
					.map((f) => `Book ${f.id}: ${f.error}`)
					.join(' · ');
			}
		} catch (e) {
			error = String(e);
		} finally {
			busy = false;
		}
	}

	function goInfo(bookId: number) {
		closeContextMenu();
		void goto(resolve(`/book/${bookId}`));
	}

	function goEdit(bookId: number) {
		closeContextMenu();
		void goto(resolve(`/book/${bookId}`));
	}

	async function resolveSendableDevice(): Promise<EbookReaderDevice | null> {
		let devices: EbookReaderDevice[] = devicePresence.devices.filter(
			(d) => d.supports_send
		);
		if (!devices.length) {
			devices = (await listEbookReaders()).filter((d) => d.supports_send);
		}
		return devices[0] ?? null;
	}

	async function goSend(bookId: number, title: string) {
		closeContextMenu();
		busy = true;
		error = '';
		importMessage = '';
		try {
			const device = await resolveSendableDevice();
			if (!device) {
				error =
					'No Kindle connected. Connect over MTP / USB file transfer, then try again.';
				return;
			}
			const result = await sendBookToKindle(bookId, device.id);
			importMessage = `Sent "${title}" (${result.format}) to ${device.product} as ${result.remote_path}.`;
		} catch (e) {
			error = String(e);
		} finally {
			busy = false;
		}
	}

	async function goRead(bookId: number) {
		closeContextMenu();
		error = '';
		try {
			await openReaderWindow(bookId);
		} catch (e) {
			error = String(e);
		}
	}

	async function openBookOrExplain(book: BookSummary) {
		const canRead = book.formats.some((f) => READABLE_FORMATS.has(f.toUpperCase()));
		if (canRead) {
			await goRead(book.id);
			return;
		}

		const formats = book.formats.map((f) => f.toUpperCase()).join(', ') || 'none';
		const kindleOnly = book.formats.every((f) => KINDLE_FORMATS.has(f.toUpperCase()));
		const detail = kindleOnly
			? `This copy is ${formats}, which Grimoire keeps for Kindle send. Convert to EPUB (or import an EPUB) to read it here.`
			: `Available format${book.formats.length === 1 ? '' : 's'}: ${formats}. Grimoire can open EPUB, PDF, TXT, MOBI, CBZ, and CBR in the reader.`;

		try {
			await message(`"${book.title}" can't be opened in the reader.\n\n${detail}`, {
				title: 'Cannot open book',
				kind: 'error'
			});
		} catch {
			error = `Cannot open "${book.title}" in the reader. ${detail}`;
		}
	}

	async function goConvert(bookId: number, target: string) {
		closeContextMenu();
		busy = true;
		error = '';
		importMessage = '';
		try {
			const result = await convertEbookFormat(bookId, target);
			importMessage = result.converted
				? ['MOBI', 'AZW3'].includes(result.format.toUpperCase())
					? `Added ${result.format} to "${result.book.title}" for Kindle send. In-app reading still uses EPUB.`
					: `Converted "${result.book.title}" to ${result.format}.`
				: `${result.format} already available for "${result.book.title}".`;
			await loadBooks();
		} catch (e) {
			error = String(e);
		} finally {
			busy = false;
		}
	}

	function needsEnrichment(book: {
		has_cover: boolean;
		authors: string[];
	}) {
		const authorsMissing =
			!book.authors.length ||
			book.authors.every((a) => a.trim().toLowerCase() === 'unknown');
		return !book.has_cover || authorsMissing;
	}

	function summaryFromDetail(detail: BookDetail, previous?: BookSummary): BookSummary {
		return {
			id: detail.id,
			uuid: detail.uuid,
			title: detail.title,
			authors: detail.authors,
			tags: detail.tags,
			formats: detail.formats.map((f) => f.format),
			has_cover: detail.has_cover,
			series: detail.series,
			series_index: detail.series_index,
			progress_percent: previous?.progress_percent ?? null
		};
	}

	async function fetchMetadata(bookId: number) {
		error = '';
		importMessage = '';
		busy = true;
		closeContextMenu();
		try {
			const result = await enrichEbookMetadata(bookId);
			books = books.map((b) =>
				b.id === bookId ? summaryFromDetail(result.book, b) : b
			);
			if (result.book.has_cover) {
				const url = await getCover(bookId);
				if (url) covers = { ...covers, [bookId]: url };
			}
			importMessage =
				result.updated_fields.length > 0
					? `Updated: ${result.updated_fields.join(', ')}`
					: 'No missing fields to fill.';
		} catch (e) {
			error = String(e);
		} finally {
			busy = false;
		}
	}

	async function confirmDelete(bookId: number, title: string) {
		const ok = confirm(`Delete "${title}" from the library?`);
		if (!ok) return;
		error = '';
		try {
			await deleteEbook(bookId);
			books = books.filter((b) => b.id !== bookId);
			const next = { ...covers };
			delete next[bookId];
			covers = next;
			closeContextMenu();
		} catch (e) {
			error = String(e);
			closeContextMenu();
		}
	}

	async function refreshStatus() {
		status = await getLibraryStatus();
	}

	async function loadBooks() {
		if (!status?.open) {
			books = [];
			return;
		}
		books = await listEbooks(query.trim() || undefined);
		for (const book of books) {
			if (book.has_cover && !covers[book.id]) {
				const url = await getCover(book.id);
				if (url) covers = { ...covers, [book.id]: url };
			}
		}
	}

	function extensionOf(path: string): string {
		const base = path.split(/[/\\]/).pop() ?? path;
		const dot = base.lastIndexOf('.');
		if (dot < 0) return '';
		return base.slice(dot + 1).toLowerCase();
	}

	function filterEbookPaths(paths: string[]): string[] {
		return paths.filter((path) => SUPPORTED_EXT.has(extensionOf(path)));
	}

	async function importPaths(paths: string[]) {
		if (busy) return;

		const ebookPaths = filterEbookPaths(paths);
		if (ebookPaths.length === 0) {
			error = 'No supported eBooks in that drop. Use EPUB, PDF, AZW3, MOBI, TXT, CBZ, or CBR.';
			return;
		}
		if (!status?.open) {
			error = 'Open a library folder before importing books.';
			return;
		}

		error = '';
		importMessage = '';
		busy = true;
		let imported = 0;
		let azw3OrMobiOnly = 0;
		const failures: string[] = [];

		try {
			for (const path of ebookPaths) {
				try {
					const book = await importEbook(path);
					imported += 1;
					const formats = book.formats.map((f) => f.format.toUpperCase());
					const hasReadable = formats.some((f) => READABLE_FORMATS.has(f));
					const hasKindle = formats.some((f) => KINDLE_FORMATS.has(f));
					if (hasKindle && !hasReadable) {
						azw3OrMobiOnly += 1;
					}
					if (needsEnrichment(book)) {
						try {
							await enrichEbookMetadata(book.id);
						} catch {
							// Best-effort; import already succeeded.
						}
					}
				} catch (e) {
					const name = path.split(/[/\\]/).pop() ?? path;
					failures.push(`${name}: ${String(e)}`);
				}
			}

			await loadBooks();
			if (imported > 0) {
				const base =
					imported === 1 ? 'Imported 1 book.' : `Imported ${imported} books.`;
				importMessage =
					azw3OrMobiOnly > 0
						? `${base} AZW3-only files stay in your library for Kindle send. Amazon store AZW3 is usually DRM-protected and is not opened in the in-app reader. DRM-free MOBI, EPUB, PDF, TXT, CBZ, and CBR can be read here.`
						: base;
			}
			if (failures.length > 0) {
				error = failures.slice(0, 3).join(' · ');
			}
		} finally {
			busy = false;
		}
	}

	async function pickLibrary() {
		error = '';
		importMessage = '';
		busy = true;
		try {
			const selected = await open({
				directory: true,
				multiple: false,
				title: 'Choose or create a Grimoire library folder'
			});
			if (!selected || Array.isArray(selected)) return;
			status = await createOrOpenLibrary(selected);
			await loadBooks();
		} catch (e) {
			error = String(e);
		} finally {
			busy = false;
		}
	}

	async function importFiles() {
		error = '';
		importMessage = '';
		try {
			const selected = await open({
				multiple: true,
				title: 'Import eBooks',
				filters: [
					{
						name: 'eBooks',
						extensions: ['epub', 'pdf', 'azw3', 'mobi', 'txt', 'cbz', 'cbr']
					}
				]
			});
			if (!selected) return;
			const paths = Array.isArray(selected) ? selected : [selected];
			await importPaths(paths);
		} catch (e) {
			error = String(e);
		}
	}

	let searchTimer: ReturnType<typeof setTimeout> | undefined;
	function onSearchInput() {
		clearTimeout(searchTimer);
		searchTimer = setTimeout(() => {
			loadBooks().catch((e) => (error = String(e)));
		}, 220);
	}

	onMount(() => {
		let unlisten: (() => void) | undefined;

		(async () => {
			try {
				await refreshStatus();
				await loadBooks();
			} catch (e) {
				error = String(e);
			} finally {
				loading = false;
			}

			try {
				unlisten = await getCurrentWebview().onDragDropEvent((event) => {
					const type = event.payload.type;
					if (type === 'enter' || type === 'over') {
						dragActive = true;
						return;
					}
					if (type === 'leave') {
						dragActive = false;
						return;
					}
					if (type === 'drop') {
						dragActive = false;
						const paths = event.payload.paths ?? [];
						void importPaths(paths);
					}
				});
			} catch {
				// Browser preview without Tauri has no drag-drop bridge.
			}

			window.addEventListener('focus', onWindowFocus);
		})();

		return () => {
			unlisten?.();
			window.removeEventListener('focus', onWindowFocus);
		};
	});

	function onWindowFocus() {
		if (!status?.open) return;
		void loadBooks().catch((e) => (error = String(e)));
	}
</script>

<svelte:window
	onkeydown={onWindowKeydown}
	onscroll={closeContextMenu}
	onresize={closeContextMenu}
/>

<main
	class="page fade-up"
	class:dragging={dragActive}
	class:library-open={Boolean(status?.open) && !loading}
>
	{#if dragActive}
		<div class="drop-overlay" aria-live="polite">
			<div class="drop-card panel">
				<h2>Drop to import</h2>
				<p class="muted">
					{#if status?.open}
						EPUB, PDF, MOBI, AZW3, TXT, CBZ, or CBR (AZW3 is Kindle send; comics and DRM-free
						MOBI can be read)
					{:else}
						Open a library folder first, then drop books here
					{/if}
				</p>
			</div>
		</div>
	{/if}

	{#if loading}
		<p class="muted">Opening library…</p>
	{:else if !status?.open}
		<section class="panel empty">
			<h2>Your library starts here</h2>
			<p class="muted">
				If you already chose a folder in Settings it should open automatically. If you still
				see this, set the library in Settings. Grimoire stores
				<code>metadata.db</code> and your book files locally, Calibre-style.
			</p>
			<Button variant="accent" onclick={pickLibrary} disabled={busy}>
				{busy ? 'Working…' : 'Choose library folder'}
			</Button>
			<p class="muted">
				<a href="/settings">Open Settings</a>
			</p>
			{#if error}
				<p class="error">{error}</p>
			{/if}
		</section>
	{:else}
		<header class="library-menubar" aria-label="Library controls">
			<div class="library-menubar-inner">
				<SearchField
					class="menubar-search"
					placeholder="Search title, author, or tag"
					bind:value={query}
					oninput={onSearchInput}
				/>
				<div class="menubar-actions">
					<ActionButton
						label={filtersOpen ? 'Hide filters' : 'Show filters'}
						selected={filtersOpen}
						tooltip={filtersOpen ? 'Hide filters' : 'Show filters'}
						onclick={() => (filtersOpen = !filtersOpen)}
					>
						<PanelLeft size={18} strokeWidth={1.75} aria-hidden="true" />
					</ActionButton>
					<ActionButton
						label={selectMode ? 'Done selecting' : 'Select books'}
						selected={selectMode}
						disabled={busy || filteredBooks.length === 0}
						tooltip={selectMode ? 'Done selecting' : 'Select books'}
						onclick={toggleSelectMode}
					>
						{#if selectMode}
							<Check size={18} strokeWidth={1.75} aria-hidden="true" />
						{:else}
							<ListChecks size={18} strokeWidth={1.75} aria-hidden="true" />
						{/if}
					</ActionButton>
					<ActionButton
						label="Import books"
						quiet={false}
						disabled={busy}
						tooltip="Import books"
						onclick={importFiles}
					>
						<Import size={18} strokeWidth={1.75} aria-hidden="true" />
					</ActionButton>
					<ActionButton
						label="Switch library"
						disabled={busy}
						tooltip="Switch library"
						onclick={pickLibrary}
					>
						<FolderSync size={18} strokeWidth={1.75} aria-hidden="true" />
					</ActionButton>
				</div>
			</div>
		</header>

		<div class="library-body" class:with-filters={filtersOpen}>
			{#if filtersOpen}
				<LibraryFilters
					{books}
					bind:statuses={filterStatuses}
					bind:genres={filterGenres}
					bind:formats={filterFormats}
					bind:authors={filterAuthors}
				/>
			{/if}
			<div class="library-content">
			{#if importMessage}
				<p class="ok">{importMessage}</p>
			{/if}
			{#if error}
				<p class="error">{error}</p>
			{/if}

			{#if selectMode && selectedCount > 0}
				<div class="selection-bar panel" role="toolbar" aria-label="Selection actions">
					<span class="selection-count">{selectedCount} selected</span>
					<Button variant="secondary" onclick={selectAllVisible} disabled={busy}>Select all</Button>
					<Button variant="secondary" onclick={clearSelection} disabled={busy}>Clear</Button>
					<Button variant="secondary" onclick={bulkFetchMetadata} disabled={busy}
						>Fetch metadata</Button
					>
					<Button variant="secondary" onclick={openBulkEdit} disabled={busy}>Edit metadata</Button>
					<Button
						variant="secondary"
						onclick={bulkSendToDevice}
						disabled={busy || !hasSendableDevice}
						>Send to device</Button
					>
					<Button variant="negative" onclick={bulkDelete} disabled={busy}>Delete</Button>
				</div>
			{/if}

			{#if books.length === 0}
				<section class="panel empty">
					<h2>No books yet</h2>
					<p class="muted">
						Import EPUB, PDF, TXT, CBZ/CBR comics, or DRM-free MOBI to read in Grimoire. AZW3 can be
						imported for Kindle send; Amazon store AZW3 is usually DRM-protected and is not opened
						in the reader.
					</p>
				</section>
			{:else if filteredBooks.length === 0}
				<section class="panel empty">
					<h2>No books match these filters</h2>
					<p class="muted">
						{#if filtersActive}
							Try clearing some filters, or adjust your search.
						{:else}
							Nothing to show.
						{/if}
					</p>
				</section>
			{:else}
				<div class="library-sections">
					{#each librarySections as section (section.id)}
						{#if section.books.length > 0}
							<section class="library-section" aria-labelledby={`section-${section.id}`}>
								<h2 class="section-heading" id={`section-${section.id}`}>
									{section.title}
									<span class="section-count">{section.books.length}</span>
								</h2>
								<div class="grid" class:selecting={selectMode}>
									{#each section.books as book (book.id)}
										{@render bookCard(book)}
									{/each}
								</div>
							</section>
						{/if}
					{/each}
				</div>
			{/if}
			</div>
		</div>
	{/if}

	{#snippet bookCard(book: BookSummary)}
		{#if selectMode}
			<button
				type="button"
				class="book-card select-card"
				class:selected={selectedIds.has(book.id)}
				onclick={() => toggleSelected(book.id)}
				aria-pressed={selectedIds.has(book.id)}
			>
				<span class="select-check" aria-hidden="true">
					<input type="checkbox" checked={selectedIds.has(book.id)} tabindex="-1" readonly />
				</span>
				{@render bookCover(book)}
				<h3>{book.title}</h3>
				<p>{book.authors.join(', ') || 'Unknown'}</p>
			</button>
		{:else}
			<button
				type="button"
				class="book-card"
				class:menu-active={contextMenu?.bookId === book.id}
				onclick={() => void openBookOrExplain(book)}
				oncontextmenu={(e) => openContextMenu(e, book)}
			>
				{@render bookCover(book)}
				<h3>{book.title}</h3>
				<p>{book.authors.join(', ') || 'Unknown'}</p>
			</button>
		{/if}
	{/snippet}

	{#snippet bookCover(book: BookSummary)}
		<div class="cover">
			{#if covers[book.id]}
				<img src={covers[book.id]} alt="" />
			{:else}
				<div class="placeholder">
					<span>{book.title.slice(0, 1)}</span>
				</div>
			{/if}
			{#if book.progress_percent != null && book.progress_percent < 100}
				<span class="progress-badge" title={`${Math.round(book.progress_percent)}% finished`}>
					{Math.round(book.progress_percent)}%
				</span>
			{/if}
		</div>
	{/snippet}

	<Dialog
		open={bulkEditOpen}
		title={`Edit ${selectedCount} book${selectedCount === 1 ? '' : 's'}`}
		onclose={() => (bulkEditOpen = false)}
	>
		<p class="muted">Only checked fields are applied. Tags are added (not replaced).</p>
		<div class="bulk-field">
			<TextField
				id="bulk-tags"
				label="Tags to add (comma-separated)"
				bind:value={bulkAddTags}
				disabled={busy}
			/>
		</div>
		<div class="bulk-field row">
			<Checkbox bind:checked={bulkApplySeries} disabled={busy}>Apply series</Checkbox>
			<TextField
				placeholder="Series name (blank clears)"
				bind:value={bulkSeries}
				disabled={busy || !bulkApplySeries}
			/>
		</div>
		<div class="bulk-field row">
			<Checkbox bind:checked={bulkApplySeriesIndex} disabled={busy}>Apply series index</Checkbox>
			<TextField
				placeholder="Index"
				bind:value={bulkSeriesIndex}
				disabled={busy || !bulkApplySeriesIndex}
			/>
		</div>
		<div class="bulk-field">
			<Checkbox bind:checked={bulkReplaceNotes} disabled={busy}>Replace notes</Checkbox>
			<TextField
				multiline
				rows={3}
				bind:value={bulkNotes}
				disabled={busy || !bulkReplaceNotes}
			/>
		</div>
		{#snippet footer()}
			<Button variant="secondary" onclick={() => (bulkEditOpen = false)} disabled={busy}
				>Cancel</Button
			>
			<Button variant="accent" onclick={applyBulkEdit} disabled={busy}>Apply</Button>
		{/snippet}
	</Dialog>

	{#if contextMenu && !selectMode}
		{@const menu = contextMenu}
		<button
			type="button"
			class="ctx-catcher"
			aria-label="Close menu"
			onclick={closeContextMenu}
			oncontextmenu={(e) => {
				e.preventDefault();
				closeContextMenu();
			}}
		></button>
		<div
			class="ctx-menu"
			role="menu"
			style:left="{menu.x}px"
			style:top="{menu.y}px"
			{@attach menuPlacement({ x: menu.x, y: menu.y })}
		>
			<button
				type="button"
				class="ctx-item"
				role="menuitem"
				onclick={() => goInfo(menu.bookId)}
			>
				Info
			</button>
			{#if menu.canRead}
				<button
					type="button"
					class="ctx-item"
					role="menuitem"
					onclick={() => goRead(menu.bookId)}
				>
					Read
				</button>
			{/if}
			<button
				type="button"
				class="ctx-item"
				role="menuitem"
				onclick={() => goEdit(menu.bookId)}
			>
				Edit metadata
			</button>
			<button
				type="button"
				class="ctx-item"
				role="menuitem"
				onclick={() => fetchMetadata(menu.bookId)}
			>
				Fetch metadata
			</button>
			<button
				type="button"
				class="ctx-item"
				role="menuitem"
				disabled={!hasSendableDevice || busy}
				onclick={() => goSend(menu.bookId, menu.title)}
			>
				Send to device
			</button>
			{#if menu.convertTargets.length}
				<div class="ctx-sep" role="separator"></div>
				<p class="ctx-label">Convert to</p>
				{#each menu.convertTargets as target (target)}
					<button
						type="button"
						class="ctx-item"
						role="menuitem"
						onclick={() => goConvert(menu.bookId, target)}
					>
						{target}
					</button>
				{/each}
			{/if}
			<div class="ctx-sep" role="separator"></div>
			<button
				type="button"
				class="ctx-item danger"
				role="menuitem"
				onclick={() => confirmDelete(menu.bookId, menu.title)}
			>
				Delete
			</button>
		</div>
	{/if}
</main>

<style>
	.page {
		position: relative;
	}

	.page.library-open {
		padding: 0;
		width: 100%;
		max-width: none;
		margin: 0;
		flex: 1 1 auto;
		min-height: 0;
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	.library-menubar {
		flex: 0 0 auto;
		z-index: 12;
		border-bottom: 1px solid var(--line);
		background: color-mix(in srgb, var(--accent) 9%, var(--paper-deep));
		backdrop-filter: blur(8px);
	}

	.library-menubar-inner {
		display: flex;
		flex-wrap: wrap;
		gap: 0.75rem;
		align-items: center;
		width: min(1200px, 100%);
		margin: 0 auto;
		padding: 0.65rem clamp(1rem, 3vw, 2rem);
	}

	:global(.menubar-search) {
		flex: 1 1 220px;
		min-width: 180px;
		width: auto;
	}

	.menubar-actions {
		display: flex;
		align-items: center;
		gap: 0.35rem;
		flex: 0 0 auto;
	}

	.library-body {
		display: flex;
		align-items: stretch;
		flex: 1 1 auto;
		min-height: 0;
		width: 100%;
	}

	.library-body.with-filters {
		flex-wrap: nowrap;
	}

	.library-content {
		flex: 1 1 auto;
		min-width: 0;
		min-height: 0;
		overflow-y: auto;
		padding: 1.25rem clamp(1rem, 3vw, 2rem) 2.5rem;
		width: min(1200px, 100%);
		margin: 0 auto;
	}

	.library-body.with-filters .library-content {
		width: auto;
		max-width: none;
		margin: 0;
	}

	@media (max-width: 720px) {
		.library-body.with-filters {
			flex-direction: column;
		}
	}

	.page.dragging {
		outline: 2px dashed color-mix(in srgb, var(--accent) 55%, transparent);
		outline-offset: -8px;
		border-radius: 12px;
	}

	.drop-overlay {
		position: fixed;
		inset: 0;
		z-index: 40;
		display: grid;
		place-items: center;
		background: color-mix(in srgb, var(--paper) 55%, transparent);
		backdrop-filter: blur(2px);
		pointer-events: none;
	}

	.drop-card {
		padding: 1.6rem 2rem;
		text-align: center;
		min-width: min(360px, 86vw);
	}

	.drop-card h2 {
		font-family: var(--font-display);
		margin: 0 0 0.35rem;
	}

	.ok {
		color: var(--accent);
		font-family: var(--font-ui);
		margin: 0 0 0.75rem;
	}

	.placeholder {
		width: 100%;
		height: 100%;
		display: grid;
		place-items: center;
		font-family: var(--font-display);
		font-size: 2.4rem;
		color: color-mix(in srgb, var(--ink) 45%, transparent);
	}

	.library-sections {
		display: flex;
		flex-direction: column;
		gap: 0;
	}

	.library-section + .library-section {
		margin-top: 1.5rem;
		padding-top: 1.5rem;
		border-top: 1px solid var(--line);
	}

	.library-section .grid {
		margin-top: 0.75rem;
	}

	.section-heading {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
		margin: 0;
		font-family: var(--font-display);
		font-size: 1.25rem;
		font-weight: 600;
		letter-spacing: -0.02em;
	}

	.section-count {
		font-family: var(--font-ui);
		font-size: 0.85rem;
		font-weight: 600;
		color: var(--ink-muted);
	}

	.progress-badge {
		position: absolute;
		right: 0.4rem;
		bottom: 0.4rem;
		z-index: 1;
		padding: 0.15rem 0.4rem;
		border-radius: 999px;
		font-family: var(--font-ui);
		font-size: 0.7rem;
		font-weight: 600;
		line-height: 1.2;
		background: color-mix(in srgb, var(--panel) 88%, transparent);
		color: var(--ink);
		border: 1px solid var(--line);
		backdrop-filter: blur(6px);
		pointer-events: none;
	}

	code {
		font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
		font-size: 0.9em;
	}

	button.book-card {
		display: block;
		width: 100%;
		text-align: left;
		margin: 0;
		padding: 0;
		border: none;
		background: transparent;
		color: inherit;
		font: inherit;
		cursor: pointer;
		position: relative;
	}

	.book-card.menu-active {
		outline: 2px solid color-mix(in srgb, var(--accent) 45%, transparent);
		outline-offset: 3px;
		border-radius: 8px;
	}

	.book-card.selected {
		outline: 2px solid var(--accent);
		outline-offset: 3px;
		border-radius: 8px;
	}

	.select-check {
		position: absolute;
		top: 0.45rem;
		left: 0.45rem;
		z-index: 2;
		width: 1.25rem;
		height: 1.25rem;
		display: grid;
		place-items: center;
		border-radius: 4px;
		background: color-mix(in srgb, var(--panel) 88%, transparent);
		border: 1px solid var(--line);
	}

	.select-check input {
		margin: 0;
		pointer-events: none;
	}

	.selection-bar {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.5rem;
		padding: 0.65rem 0.85rem;
		margin: 0 0 0.9rem;
		position: sticky;
		top: 0;
		z-index: 8;
	}

	.selection-count {
		font-family: var(--font-ui);
		font-size: 0.92rem;
		margin-right: 0.35rem;
		min-width: 6rem;
	}

	.bulk-field {
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
		font-family: var(--font-ui);
		font-size: 0.92rem;
		margin-bottom: 0.75rem;
	}

	.bulk-field.row {
		gap: 0.45rem;
	}

	.ctx-catcher {
		position: fixed;
		inset: 0;
		z-index: 50;
		margin: 0;
		padding: 0;
		border: none;
		background: transparent;
		cursor: default;
	}

	.ctx-menu {
		position: fixed;
		z-index: 51;
		min-width: 180px;
		padding: 0.35rem;
		background: var(--panel);
		border: 1px solid var(--line);
		border-radius: 8px;
		box-shadow: 0 10px 28px color-mix(in srgb, var(--ink) 16%, transparent);
		font-family: var(--font-ui);
	}

	.ctx-item {
		display: block;
		width: 100%;
		text-align: left;
		margin: 0;
		padding: 0.45rem 0.7rem;
		border: none;
		border-radius: 6px;
		background: transparent;
		color: var(--ink);
		font: inherit;
		cursor: pointer;
	}

	.ctx-item:hover {
		background: color-mix(in srgb, var(--accent) 14%, transparent);
		color: var(--accent);
	}

	.ctx-item.danger {
		color: var(--danger);
	}

	.ctx-item.danger:hover {
		background: color-mix(in srgb, var(--danger) 14%, transparent);
		color: var(--danger);
	}

	.ctx-label {
		margin: 0.2rem 0.7rem 0.1rem;
		font-size: 0.72rem;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--ink-muted);
	}

	.ctx-sep {
		height: 1px;
		margin: 0.3rem 0.35rem;
		background: var(--line);
	}
</style>
