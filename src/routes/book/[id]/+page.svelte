<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import {
		convertEbookFormat,
		deleteEbook,
		enrichEbookMetadata,
		getCover,
		getEbook,
		listEbookConvertTargets,
		listEbookReaders,
		sendBookToKindle,
		updateEbook,
		type BookDetail,
		type EbookReaderDevice,
		type IdentifierInfo
	} from '$lib/api';
	import CoverPickerModal from '$lib/CoverPickerModal.svelte';
	import { onEreaderPresence } from '$lib/device-presence.svelte';
	import Button from '$lib/ui/Button.svelte';
	import TextField from '$lib/ui/TextField.svelte';
	import { openReaderWindow } from '$lib/windows';

	let { params } = $props();
	let book = $state<BookDetail | null>(null);
	let cover = $state<string | null>(null);
	let devices = $state<EbookReaderDevice[]>([]);
	let convertTargets = $state<string[]>([]);
	let error = $state('');
	let message = $state('');
	let busy = $state(false);
	let coverPickerOpen = $state(false);

	let title = $state('');
	let authors = $state('');
	let tags = $state('');
	let series = $state('');
	let seriesIndex = $state('');
	let isbn = $state('');
	let comment = $state('');

	const bookId = $derived(Number(params.id));
	const canRead = $derived(
		book?.formats.some((f) =>
			['EPUB', 'PDF', 'TXT', 'MOBI', 'CBZ', 'CBR'].includes(f.format.toUpperCase())
		) ?? false
	);
	const kindleOnly = $derived(
		!!book &&
			book.formats.some((f) => f.format.toUpperCase() === 'AZW3') &&
			!canRead
	);
	const readableFormats = $derived(
		book?.formats
			.filter((f) =>
				['EPUB', 'PDF', 'TXT', 'MOBI', 'CBZ', 'CBR'].includes(f.format.toUpperCase())
			)
			.map((f) => f.format) ?? []
	);
	const kindleFormats = $derived(
		book?.formats
			.filter((f) => ['AZW3', 'MOBI'].includes(f.format.toUpperCase()))
			.map((f) => f.format) ?? []
	);

	function kindleSideloadFormat(format: string) {
		return ['MOBI', 'AZW3'].includes(format.toUpperCase());
	}

	const otherIdentifiers = $derived(
		book?.identifiers.filter((id) => id.type_name.toLowerCase() !== 'isbn') ?? []
	);

	const dirty = $derived(
		book != null &&
			(title !== book.title ||
				authors !== book.authors.join(', ') ||
				tags !== book.tags.join(', ') ||
				series !== (book.series ?? '') ||
				seriesIndex !== (book.series_index != null ? String(book.series_index) : '') ||
				isbn !== isbnFromBook(book) ||
				comment !== book.comment)
	);

	function isbnFromBook(detail: BookDetail): string {
		return detail.identifiers.find((id) => id.type_name.toLowerCase() === 'isbn')?.value ?? '';
	}

	function syncForm(detail: BookDetail) {
		title = detail.title;
		authors = detail.authors.join(', ');
		tags = detail.tags.join(', ');
		series = detail.series ?? '';
		seriesIndex = detail.series_index != null ? String(detail.series_index) : '';
		isbn = isbnFromBook(detail);
		comment = detail.comment;
	}

	function buildIdentifiers(detail: BookDetail): IdentifierInfo[] {
		const preserved = detail.identifiers.filter((id) => id.type_name.toLowerCase() !== 'isbn');
		const trimmed = isbn.trim();
		if (trimmed) {
			return [{ type_name: 'isbn', value: trimmed }, ...preserved];
		}
		return preserved;
	}

	function reset() {
		if (!book) return;
		syncForm(book);
		message = '';
		error = '';
	}

	async function load() {
		error = '';
		book = await getEbook(bookId);
		syncForm(book);
		cover = book.has_cover ? await getCover(bookId) : null;
		try {
			convertTargets = await listEbookConvertTargets(bookId);
		} catch {
			convertTargets = [];
		}
		try {
			devices = await listEbookReaders();
		} catch {
			devices = [];
		}
	}

	async function convertTo(target: string) {
		busy = true;
		error = '';
		message = '';
		try {
			const result = await convertEbookFormat(bookId, target);
			book = result.book;
			syncForm(result.book);
			convertTargets = await listEbookConvertTargets(bookId);
			message = result.converted
				? kindleSideloadFormat(result.format)
					? `Added ${result.format} for Kindle send. In-app reading still uses EPUB.`
					: `Converted to ${result.format}.`
				: `${result.format} already available.`;
		} catch (e) {
			error = String(e);
		} finally {
			busy = false;
		}
	}

	async function save() {
		if (!book) return;
		busy = true;
		error = '';
		message = '';
		try {
			book = await updateEbook(bookId, {
				title: title.trim() || 'Untitled',
				authors: authors
					.split(',')
					.map((s) => s.trim())
					.filter(Boolean),
				tags: tags
					.split(',')
					.map((s) => s.trim())
					.filter(Boolean),
				series: series.trim() || null,
				series_index: seriesIndex.trim() ? Number(seriesIndex) : null,
				comment,
				identifiers: buildIdentifiers(book)
			});
			syncForm(book);
			message = 'Saved.';
		} catch (e) {
			error = String(e);
		} finally {
			busy = false;
		}
	}

	async function remove() {
		if (!confirm('Delete this book from the library?')) return;
		busy = true;
		try {
			await deleteEbook(bookId);
			await goto(resolve('/'));
		} catch (e) {
			error = String(e);
			busy = false;
		}
	}

	async function fetchFromOpenLibrary() {
		busy = true;
		error = '';
		message = '';
		try {
			const result = await enrichEbookMetadata(bookId);
			book = result.book;
			syncForm(result.book);
			if (result.book.has_cover) {
				cover = await getCover(bookId);
			}
			if (result.updated_fields.length > 0) {
				message = `Updated: ${result.updated_fields.join(', ')}`;
			} else {
				message = 'No missing fields to fill.';
			}
			if (!result.book.has_cover) {
				message += ' Pick a cover from the list.';
				coverPickerOpen = true;
			}
		} catch (e) {
			error = String(e);
		} finally {
			busy = false;
		}
	}

	async function onCoverApplied(updated: BookDetail) {
		book = updated;
		syncForm(updated);
		cover = updated.has_cover ? await getCover(bookId) : null;
		message = 'Cover updated.';
		error = '';
	}

	async function openReader() {
		error = '';
		try {
			await openReaderWindow(bookId);
		} catch (e) {
			error = String(e);
		}
	}

	async function sendToDevice(device: EbookReaderDevice) {
		if (!device.supports_send) {
			error = `${device.family_label} is detected, but USB send is only available for Kindle MTP right now.`;
			return;
		}
		busy = true;
		error = '';
		message = '';
		try {
			const result = await sendBookToKindle(bookId, device.id);
			message = `Sent ${result.format} to ${device.product} as ${result.remote_path}`;
		} catch (e) {
			error = String(e);
		} finally {
			busy = false;
		}
	}

	onMount(() => {
		void load().catch((e) => (error = String(e)));
		return onEreaderPresence((event) => {
			devices = event.devices;
		});
	});
</script>

<main class="page fade-up">
	<p class="back"><a href={resolve('/')}>← Library</a></p>

	{#if error}
		<p class="error">{error}</p>
	{/if}
	{#if message}
		<p class="ok">{message}</p>
	{/if}

	{#if !book}
		<p class="muted">Loading…</p>
	{:else}
		<section class="detail panel">
			<div class="hero-cover-col">
				<div class="hero-cover cover">
					{#if cover}
						<img src={cover} alt="" />
					{:else}
						<div class="placeholder"><span>{book.title.slice(0, 1)}</span></div>
					{/if}
				</div>
				<Button
					class="choose-cover"
					variant="secondary"
					onclick={() => (coverPickerOpen = true)}
					disabled={busy}
				>
					Choose cover
				</Button>
			</div>

			<div class="meta">
				<div class="field">
					<TextField id="title" label="Title" bind:value={title} />
				</div>
				<div class="field">
					<TextField id="authors" label="Authors (comma-separated)" bind:value={authors} />
				</div>
				<div class="field">
					<TextField id="tags" label="Tags (comma-separated)" bind:value={tags} />
				</div>
				<div class="row">
					<div class="field grow">
						<TextField id="series" label="Series" bind:value={series} />
					</div>
					<div class="field narrow">
						<TextField id="seriesIndex" label="Index" bind:value={seriesIndex} />
					</div>
				</div>
				<div class="field">
					<TextField id="isbn" label="ISBN" bind:value={isbn} />
				</div>
				<div class="field">
					<TextField id="comment" label="Notes" multiline rows={4} bind:value={comment} />
				</div>

				<div class="readonly">
					<p class="formats">
						{#if readableFormats.length || kindleFormats.length}
							{#if readableFormats.length}
								Readable: {readableFormats.join(', ')}
							{/if}
							{#if readableFormats.length && kindleFormats.length}
								·
							{/if}
							{#if kindleFormats.length}
								Kindle: {kindleFormats.join(', ')}
							{/if}
						{:else}
							Formats: None
						{/if}
					</p>
					{#if kindleOnly}
						<p class="format-note">
							AZW3 is not opened in the in-app reader. Kindle Store books are usually
							DRM-protected, so Grimoire cannot decrypt them. Use Send to device for Kindle,
							or add a DRM-free EPUB, PDF, TXT, or MOBI to read here. See Settings for details.
						</p>
					{/if}
					{#if otherIdentifiers.length}
						<p class="identifiers">
							Other identifiers: {otherIdentifiers
								.map((id) => `${id.type_name}: ${id.value}`)
								.join(' · ')}
						</p>
					{/if}
				</div>

				<div class="actions">
					{#if canRead}
						<Button onclick={openReader} disabled={busy}>Read</Button>
					{/if}
					<Button variant="secondary" onclick={save} disabled={busy}>Save metadata</Button>
					<Button variant="secondary" onclick={reset} disabled={busy || !dirty}>Reset</Button>
					<Button variant="secondary" onclick={fetchFromOpenLibrary} disabled={busy}
						>Fetch from Open Library</Button
					>
					<Button variant="negative" onclick={remove} disabled={busy}>Delete</Button>
				</div>
				{#if convertTargets.length}
					<div class="convert-row">
						<span class="convert-label">Convert to</span>
						{#each convertTargets as target (target)}
							<Button variant="secondary" onclick={() => convertTo(target)} disabled={busy}>
								{target}
							</Button>
						{/each}
					</div>
					<p class="muted hint">
						Native conversion for Kindle sideload (EPUB → MOBI/AZW3) and TXT → EPUB. DRM-free
						MOBI can be opened in the reader (converted to EPUB for display). Amazon store AZW3
						is usually DRM-locked and is not opened here.
					</p>
				{/if}
				<p class="muted hint">Only fills empty fields and missing covers from Open Library.</p>

				<div id="send" class="send panel-inset">
					<h2>Send to device</h2>
					<p class="muted">
						Detected eReaders appear here. Kindle USB/MTP send converts EPUB to MOBI
						natively when needed.
					</p>
					{#if devices.length === 0}
						<p class="muted">
							No eReader detected yet. Plug in over USB; Grimoire detects devices automatically.
							You can also refresh.
						</p>
						<Button variant="secondary" onclick={() => load()} disabled={busy}
							>Refresh devices</Button
						>
					{:else}
						<ul>
							{#each devices as device (device.id)}
								<li>
									<div>
										<strong>{device.product}</strong>
										<span class="muted"
											>{device.family_label} · {device.connection === 'mtp'
												? 'USB/MTP'
												: 'Mounted'}</span
										>
									</div>
									<Button
										disabled={busy || !device.supports_send}
										onclick={() => sendToDevice(device)}
									>
										{device.supports_send ? 'Send' : 'Detected'}
									</Button>
								</li>
							{/each}
						</ul>
					{/if}
				</div>
			</div>
		</section>

		<CoverPickerModal
			open={coverPickerOpen}
			{bookId}
			onclose={() => (coverPickerOpen = false)}
			onapplied={onCoverApplied}
		/>
	{/if}
</main>

<style>
	.back {
		margin: 0 0 1rem;
		font-family: var(--font-ui);
	}

	.detail {
		display: grid;
		grid-template-columns: minmax(160px, 240px) 1fr;
		gap: 1.5rem;
		padding: 1.4rem;
	}

	.hero-cover-col {
		display: flex;
		flex-direction: column;
		gap: 0.65rem;
		align-items: stretch;
	}

	.hero-cover {
		width: 100%;
	}

	.hero-cover-col :global(.choose-cover) {
		width: 100%;
	}

	.meta {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}

	.readonly {
		font-family: var(--font-ui);
		font-size: 0.95rem;
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
		margin-top: 0.25rem;
	}

	.formats,
	.identifiers {
		margin: 0;
	}

	.format-note {
		margin: 0.35rem 0 0;
		padding: 0.65rem 0.75rem;
		border-radius: 8px;
		border: 1px solid color-mix(in srgb, var(--accent) 28%, var(--line));
		background: color-mix(in srgb, var(--accent) 8%, transparent);
		font-size: 0.88rem;
		line-height: 1.45;
		color: var(--ink);
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 0.6rem;
		margin: 0.25rem 0 0.15rem;
	}

	.convert-row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.5rem;
		margin-top: 0.35rem;
	}

	.convert-label {
		font-family: var(--font-ui);
		font-size: 0.9rem;
		color: var(--ink-muted);
		margin-right: 0.15rem;
	}

	.hint {
		margin: 0 0 0.5rem;
		font-size: 0.85rem;
	}

	.row {
		display: flex;
		gap: 0.75rem;
	}

	.grow {
		flex: 1;
	}

	.narrow {
		width: 6rem;
	}

	.send {
		margin-top: 0.5rem;
	}

	.panel-inset {
		background: color-mix(in srgb, var(--ink) 3%, transparent);
		border: 1px solid var(--line);
		border-radius: 8px;
		padding: 1rem;
	}

	.send h2 {
		font-family: var(--font-display);
		font-size: 1.2rem;
		margin: 0 0 0.35rem;
	}

	ul {
		list-style: none;
		padding: 0;
		margin: 0.8rem 0 0;
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
	}

	li {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 1rem;
	}

	.ok {
		color: var(--accent);
		font-family: var(--font-ui);
	}

	.placeholder {
		width: 100%;
		height: 100%;
		display: grid;
		place-items: center;
		font-family: var(--font-display);
		font-size: 3rem;
		color: color-mix(in srgb, var(--ink) 45%, transparent);
	}

	@media (max-width: 720px) {
		.detail {
			grid-template-columns: 1fr;
		}

		.hero-cover-col {
			max-width: 220px;
		}
	}
</style>
