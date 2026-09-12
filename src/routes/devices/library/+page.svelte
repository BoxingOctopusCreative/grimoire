<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import {
		deleteEbookReaderBooks,
		listEbookReaders,
		scanEbookReader,
		type DeviceBook,
		type DeviceLibrary,
		type EbookReaderDevice
	} from '$lib/api';
	import { getStoredViewMode, setStoredViewMode, type ViewMode } from '$lib/view-mode';
	import Button from '$lib/ui/Button.svelte';
	import ActionButton from '$lib/ui/ActionButton.svelte';
	import SearchField from '$lib/ui/SearchField.svelte';

	type ContextMenuState = {
		key: string;
		book: DeviceBook;
		x: number;
		y: number;
	};

	let device = $state<EbookReaderDevice | null>(null);
	let library = $state<DeviceLibrary | null>(null);
	let viewMode = $state<ViewMode>(getStoredViewMode());
	let query = $state('');
	let error = $state('');
	let message = $state('');
	let loading = $state(true);
	let busy = $state(false);
	let selectMode = $state(false);
	let selectedKeys = $state<Set<string>>(new Set());
	let contextMenu = $state<ContextMenuState | null>(null);

	const deviceId = $derived(page.url.searchParams.get('id') ?? '');
	const selectedCount = $derived(selectedKeys.size);

	const filteredBooks = $derived.by(() => {
		const books = library?.books ?? [];
		const q = query.trim().toLowerCase();
		if (!q) return books;
		return books.filter(
			(book) =>
				book.title.toLowerCase().includes(q) ||
				book.format.toLowerCase().includes(q) ||
				book.path.toLowerCase().includes(q) ||
				book.filename.toLowerCase().includes(q)
		);
	});

	function formatBytes(n: number) {
		if (n < 1024) return `${n} B`;
		if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
		if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
		return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
	}

	function setView(mode: ViewMode) {
		viewMode = setStoredViewMode(mode);
	}

	function bookKey(book: DeviceBook) {
		return `${book.path}|${book.handle ?? ''}|${book.filename}`;
	}

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

	function openContextMenu(event: MouseEvent, book: DeviceBook) {
		if (selectMode) return;
		event.preventDefault();
		event.stopPropagation();
		contextMenu = {
			key: bookKey(book),
			book,
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
		selectedKeys = new Set();
	}

	function exitSelectMode() {
		selectMode = false;
		clearSelection();
	}

	function toggleSelectMode() {
		if (selectMode) {
			exitSelectMode();
		} else {
			closeContextMenu();
			selectMode = true;
		}
	}

	function toggleSelected(key: string) {
		const next = new Set(selectedKeys);
		if (next.has(key)) next.delete(key);
		else next.add(key);
		selectedKeys = next;
	}

	function selectAllVisible() {
		selectedKeys = new Set(filteredBooks.map(bookKey));
	}

	function onWindowKeydown(event: KeyboardEvent) {
		if (event.key !== 'Escape') return;
		if (contextMenu) {
			closeContextMenu();
			return;
		}
		if (selectedKeys.size > 0) {
			clearSelection();
			return;
		}
		if (selectMode) {
			exitSelectMode();
		}
	}

	async function removeFromDevice(book: DeviceBook) {
		if (!library || !deviceId) return;
		const ok = confirm(`Remove "${book.title}" from this device?`);
		if (!ok) return;

		busy = true;
		error = '';
		message = '';
		closeContextMenu();
		try {
			const result = await deleteEbookReaderBooks(deviceId, [
				{
					path: book.path,
					filename: book.filename,
					handle: book.handle
				}
			]);
			const failedPaths = new Set(result.failed.map((f) => f.path));
			if (!failedPaths.has(book.path)) {
				library = {
					...library,
					books: library.books.filter((b) => bookKey(b) !== bookKey(book))
				};
			}
			message =
				result.failed.length === 0
					? `Removed ${result.removed} book${result.removed === 1 ? '' : 's'}.`
					: `Removed ${result.removed}; ${result.failed.length} failed.`;
			if (result.failed.length > 0) {
				error = result.failed
					.slice(0, 3)
					.map((f) => `${f.path}: ${f.error}`)
					.join(' · ');
			}
		} catch (e) {
			error = String(e);
		} finally {
			busy = false;
		}
	}

	async function bulkRemove() {
		const keys = [...selectedKeys];
		if (keys.length === 0 || !library || !deviceId) return;
		const ok = confirm(
			`Remove ${keys.length} book${keys.length === 1 ? '' : 's'} from this device?`
		);
		if (!ok) return;

		busy = true;
		error = '';
		message = '';
		try {
			const targets = library.books
				.filter((b) => selectedKeys.has(bookKey(b)))
				.map((b) => ({
					path: b.path,
					filename: b.filename,
					handle: b.handle
				}));
			const result = await deleteEbookReaderBooks(deviceId, targets);
			const failedPaths = new Set(result.failed.map((f) => f.path));
			library = {
				...library,
				books: library.books.filter(
					(b) => !selectedKeys.has(bookKey(b)) || failedPaths.has(b.path)
				)
			};
			selectedKeys = new Set(
				library.books.map(bookKey).filter((key) => keys.includes(key))
			);
			message =
				result.failed.length === 0
					? `Removed ${result.removed} book${result.removed === 1 ? '' : 's'}.`
					: `Removed ${result.removed}; ${result.failed.length} failed.`;
			if (result.failed.length > 0) {
				error = result.failed
					.slice(0, 3)
					.map((f) => `${f.path}: ${f.error}`)
					.join(' · ');
			}
		} catch (e) {
			error = String(e);
		} finally {
			busy = false;
		}
	}

	async function scan() {
		if (!deviceId) {
			error = 'Missing device id.';
			loading = false;
			return;
		}
		error = '';
		message = '';
		loading = true;
		closeContextMenu();
		clearSelection();
		try {
			library = await scanEbookReader(deviceId);
		} catch (e) {
			error = String(e);
			library = null;
		} finally {
			loading = false;
		}
	}

	onMount(async () => {
		if (!deviceId) {
			error = 'No device selected.';
			loading = false;
			return;
		}
		try {
			const readers = await listEbookReaders();
			device = readers.find((d) => d.id === deviceId) ?? null;
			if (!device) {
				error = 'That eReader is not connected right now.';
				loading = false;
				return;
			}
			await scan();
		} catch (e) {
			error = String(e);
			loading = false;
		}
	});
</script>

<svelte:window
	onkeydown={onWindowKeydown}
	onscroll={closeContextMenu}
	onresize={closeContextMenu}
/>

<main class="page fade-up">
	<p class="back"><a href="/devices">← Devices</a></p>

	<div class="toolbar">
		<div>
			<h1>{device?.product ?? 'Device library'}</h1>
			{#if device}
				<p class="muted">
					{device.family_label} · {device.connection === 'mtp' ? 'USB/MTP' : 'Mounted volume'}
					{#if library}
						· {library.books.length}
						{library.books.length === 1 ? 'ebook' : 'ebooks'}
					{/if}
				</p>
			{/if}
		</div>
		<div class="actions">
			<SearchField
				class="search"
				placeholder="Filter device books…"
				bind:value={query}
				disabled={loading || !library}
				label="Filter device books"
			/>
			<div class="view-toggle" role="group" aria-label="View mode">
				<ActionButton
					label="Grid"
					selected={viewMode === 'grid'}
					onclick={() => setView('grid')}
					quiet={false}
				>
					Grid
				</ActionButton>
				<ActionButton
					label="List"
					selected={viewMode === 'list'}
					onclick={() => setView('list')}
					quiet={false}
				>
					List
				</ActionButton>
			</div>
			<Button
				variant="secondary"
				onclick={toggleSelectMode}
				disabled={loading || busy || !(library?.books.length)}
			>
				{selectMode ? 'Done' : 'Select'}
			</Button>
			<Button variant="secondary" onclick={scan} disabled={loading || busy}>
				{loading ? 'Scanning…' : 'Rescan'}
			</Button>
		</div>
	</div>

	{#if message}
		<p class="ok">{message}</p>
	{/if}
	{#if error}
		<p class="error">{error}</p>
		{#if !device}
			<Button variant="secondary" onclick={() => goto('/devices')}>Back to devices</Button>
		{/if}
	{/if}

	{#if selectMode && selectedCount > 0}
		<div class="selection-bar panel" role="toolbar" aria-label="Selection actions">
			<span class="selection-count">{selectedCount} selected</span>
			<Button variant="secondary" onclick={selectAllVisible} disabled={busy}>Select all</Button>
			<Button variant="secondary" onclick={clearSelection} disabled={busy}>Clear</Button>
			<Button variant="negative" onclick={bulkRemove} disabled={busy}>Remove from device</Button>
		</div>
	{/if}

	<section class="panel block">
		{#if loading && !library}
			<p class="muted">Scanning the device for ebooks…</p>
		{:else if library && library.books.length === 0}
			<div class="empty">
				<h2>No ebooks found</h2>
				<p class="muted">
					Grimoire looked for EPUB, PDF, AZW3, MOBI, and similar files under the device storage.
				</p>
			</div>
		{:else if library && filteredBooks.length === 0}
			<p class="muted">No books match "{query}".</p>
		{:else if library && viewMode === 'grid'}
			<div class="grid" class:selecting={selectMode}>
				{#each filteredBooks as book (bookKey(book))}
					{#if selectMode}
						<button
							type="button"
							class="book-card device-card select-card"
							class:selected={selectedKeys.has(bookKey(book))}
							onclick={() => toggleSelected(bookKey(book))}
							aria-pressed={selectedKeys.has(bookKey(book))}
						>
							<span class="select-check" aria-hidden="true">
								<input
									type="checkbox"
									checked={selectedKeys.has(bookKey(book))}
									tabindex="-1"
									readonly
								/>
							</span>
							<div class="cover">
								<div class="placeholder">
									<span>{book.title.slice(0, 1)}</span>
									<em>{book.format}</em>
								</div>
							</div>
							<h3>{book.title}</h3>
							<p>{book.format} · {formatBytes(book.size)}</p>
							<p class="path" title={book.path}>{book.path}</p>
						</button>
					{:else}
						<article
							class="book-card device-card"
							class:menu-active={contextMenu?.key === bookKey(book)}
							oncontextmenu={(e) => openContextMenu(e, book)}
						>
							<div class="cover">
								<div class="placeholder">
									<span>{book.title.slice(0, 1)}</span>
									<em>{book.format}</em>
								</div>
							</div>
							<h3>{book.title}</h3>
							<p>{book.format} · {formatBytes(book.size)}</p>
							<p class="path" title={book.path}>{book.path}</p>
						</article>
					{/if}
				{/each}
			</div>
		{:else if library}
			<table class:selecting={selectMode}>
				<thead>
					<tr>
						{#if selectMode}
							<th class="check-col" aria-label="Selected"></th>
						{/if}
						<th>Title</th>
						<th>Format</th>
						<th>Size</th>
						<th>Path</th>
					</tr>
				</thead>
				<tbody>
					{#each filteredBooks as book (bookKey(book))}
						{#if selectMode}
							<tr
								class="select-row"
								class:selected={selectedKeys.has(bookKey(book))}
								onclick={() => toggleSelected(bookKey(book))}
							>
								<td class="check-col">
									<input
										type="checkbox"
										checked={selectedKeys.has(bookKey(book))}
										tabindex="-1"
										readonly
										onclick={(e) => e.stopPropagation()}
									/>
								</td>
								<td>{book.title}</td>
								<td>{book.format}</td>
								<td class="size">{formatBytes(book.size)}</td>
								<td class="path" title={book.path}>{book.path}</td>
							</tr>
						{:else}
							<tr
								class:menu-active={contextMenu?.key === bookKey(book)}
								oncontextmenu={(e) => openContextMenu(e, book)}
							>
								<td>{book.title}</td>
								<td>{book.format}</td>
								<td class="size">{formatBytes(book.size)}</td>
								<td class="path" title={book.path}>{book.path}</td>
							</tr>
						{/if}
					{/each}
				</tbody>
			</table>
		{/if}
	</section>

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
				class="ctx-item danger"
				role="menuitem"
				disabled={busy}
				onclick={() => removeFromDevice(menu.book)}
			>
				Remove from device
			</button>
		</div>
	{/if}
</main>

<style>
	.back {
		margin: 0 0 1rem;
		font-family: var(--font-ui);
	}

	h1 {
		font-family: var(--font-display);
		margin: 0;
		font-size: 1.8rem;
	}

	.toolbar {
		justify-content: space-between;
		align-items: flex-start;
		gap: 1rem;
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
		align-items: center;
		justify-content: flex-end;
	}

	.view-toggle {
		display: inline-flex;
		gap: 0.25rem;
	}

	.ok {
		color: var(--accent);
		font-family: var(--font-ui);
		margin: 0 0 0.75rem;
	}

	.block {
		padding: 1rem;
	}

	.device-card {
		display: block;
		text-decoration: none;
		color: inherit;
	}

	.placeholder {
		width: 100%;
		height: 100%;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 0.35rem;
		color: var(--btn-fg);
		font-family: var(--font-display);
		font-size: 2.2rem;
	}

	.placeholder em {
		font-style: normal;
		font-family: var(--font-ui);
		font-size: 0.72rem;
		letter-spacing: 0.04em;
		opacity: 0.9;
	}

	.path {
		margin-top: 0.25rem;
		font-size: 0.78rem;
		color: var(--ink-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	table {
		width: 100%;
		border-collapse: collapse;
		font-family: var(--font-ui);
		font-size: 0.92rem;
	}

	th,
	td {
		text-align: left;
		padding: 0.55rem 0.35rem;
		border-bottom: 1px solid var(--line);
		vertical-align: top;
	}

	.size {
		white-space: nowrap;
		color: var(--ink-muted);
	}

	td.path {
		max-width: 28rem;
	}

	.book-card.selected {
		outline: 2px solid var(--accent);
		outline-offset: 3px;
		border-radius: 8px;
	}

	.book-card.menu-active,
	tr.menu-active {
		outline: 2px solid color-mix(in srgb, var(--accent) 45%, transparent);
		outline-offset: 3px;
		border-radius: 8px;
	}

	tr.menu-active {
		outline-offset: -2px;
		background: color-mix(in srgb, var(--accent) 8%, transparent);
	}

	.select-card {
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
		top: 0.5rem;
		z-index: 8;
	}

	.selection-count {
		font-family: var(--font-ui);
		font-size: 0.92rem;
		margin-right: 0.35rem;
		min-width: 6rem;
	}

	.check-col {
		width: 2rem;
		vertical-align: middle;
	}

	.check-col input {
		margin: 0;
		pointer-events: none;
	}

	.select-row {
		cursor: pointer;
	}

	.select-row:hover {
		background: color-mix(in srgb, var(--accent) 8%, transparent);
	}

	.select-row.selected {
		background: color-mix(in srgb, var(--accent) 14%, transparent);
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

	.ctx-item:disabled {
		opacity: 0.55;
		cursor: not-allowed;
	}
</style>
