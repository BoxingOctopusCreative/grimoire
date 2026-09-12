<script lang="ts">
	import { onMount } from 'svelte';
	import { open } from '@tauri-apps/plugin-dialog';
	import {
		clearSavedLibrary,
		createOrOpenLibrary,
		getLibraryStatus,
		type LibraryStatus
	} from '$lib/api';
	import {
		getReaderFontOption,
		getStoredReaderFont,
		READER_FONTS,
		setStoredReaderFont,
		type ReaderFontId
	} from '$lib/reader-prefs';
	import Button from '$lib/ui/Button.svelte';

	let status = $state<LibraryStatus | null>(null);
	let busy = $state(false);
	let error = $state('');
	let message = $state('');
	let readerFont = $state<ReaderFontId>(getStoredReaderFont());

	function chooseReaderFont(id: ReaderFontId) {
		readerFont = setStoredReaderFont(id);
		message = `Reader font set to ${getReaderFontOption(id).label}.`;
	}

	async function refresh() {
		status = await getLibraryStatus();
	}

	async function chooseLibrary() {
		error = '';
		message = '';
		busy = true;
		try {
			const selected = await open({
				directory: true,
				multiple: false,
				title: 'Choose Grimoire library folder'
			});
			if (!selected || Array.isArray(selected)) return;
			status = await createOrOpenLibrary(selected);
			message = 'Library path saved. Grimoire will open it automatically on startup.';
		} catch (e) {
			error = String(e);
		} finally {
			busy = false;
		}
	}

	async function clearLibrary() {
		if (!confirm('Clear the saved library path? Grimoire will not open a library until you choose one again.')) {
			return;
		}
		error = '';
		message = '';
		busy = true;
		try {
			status = await clearSavedLibrary();
			message = 'Saved library path cleared.';
		} catch (e) {
			error = String(e);
		} finally {
			busy = false;
		}
	}

	onMount(() => {
		readerFont = getStoredReaderFont();
		void refresh().catch((e) => (error = String(e)));
	});
</script>

<main class="page fade-up">
	<div class="toolbar">
		<div>
			<h1>Settings</h1>
			<p class="muted">Choose a library folder to open automatically when Grimoire starts.</p>
		</div>
	</div>

	{#if error}
		<p class="error">{error}</p>
	{/if}
	{#if message}
		<p class="ok">{message}</p>
	{/if}

	<section class="panel block">
		<h2>Library location</h2>
		<p class="muted">
			Grimoire stores <code>metadata.db</code> and book files in this folder. The path is saved in
			your OS config directory and loaded on every launch.
		</p>

		<div class="path-box">
			<span class="label">Current path</span>
			{#if status?.path}
				<code class="path">{status.path}</code>
				<p class="status-line muted">
					{status.open ? 'Open and ready' : 'Saved, but not open right now'}
				</p>
			{:else}
				<p class="muted">No library path saved yet.</p>
			{/if}
		</div>

		<div class="actions">
			<Button onclick={chooseLibrary} disabled={busy}>
				{busy ? 'Working…' : status?.path ? 'Change library folder' : 'Choose library folder'}
			</Button>
			{#if status?.path}
				<Button variant="secondary" onclick={clearLibrary} disabled={busy}>Clear saved path</Button>
			{/if}
		</div>
	</section>

	<section class="panel block">
		<h2>Reader font</h2>
		<p class="muted">
			Applies to EPUB and TXT reading. OpenDyslexic is bundled in Grimoire for offline use
			(SIL Open Font License).
		</p>

		<fieldset class="font-options" disabled={busy}>
			<legend class="sr-only">Reader font</legend>
			{#each READER_FONTS as font (font.id)}
				<label class="font-option" class:selected={readerFont === font.id}>
					<input
						type="radio"
						name="reader-font"
						value={font.id}
						checked={readerFont === font.id}
						onchange={() => chooseReaderFont(font.id)}
					/>
					<span class="font-copy">
						<span class="font-label">{font.label}</span>
						<span class="font-desc muted">{font.description}</span>
					</span>
				</label>
			{/each}
		</fieldset>

		<p
			class="font-preview"
			style:font-family={getReaderFontOption(readerFont).cssFamily ?? 'var(--font-body)'}
		>
			The quick brown fox jumps over the lazy dog. Pack my box with five dozen liquor jugs.
		</p>
	</section>

	<section class="panel block">
		<h2>Formats and reading</h2>
		<p class="muted">
			The in-app reader opens <strong>EPUB</strong>, <strong>PDF</strong>, <strong>TXT</strong>,
			<strong>CBZ</strong>/<strong>CBR</strong> comics, and DRM-free <strong>MOBI</strong> (MOBI is
			converted to EPUB for display).
		</p>
		<p class="muted">
			<strong>AZW3</strong> can still be imported and sent to a Kindle. It is not opened in the
			Grimoire reader. Kindle Store purchases are usually <strong>DRM-protected</strong>, so
			Grimoire cannot decrypt them.
		</p>
		<p class="muted">
			For reading in Grimoire, prefer a DRM-free EPUB, PDF, TXT, CBZ/CBR, or MOBI. For Kindle
			reading, keep or convert to AZW3/MOBI and use <strong>Send to device</strong>.
		</p>
	</section>

	<section class="panel block">
		<h2>Metadata</h2>
		<p class="muted">
			Missing details and covers are looked up on
			<a href="https://openlibrary.org/" target="_blank" rel="noreferrer">Open Library</a>. No API
			key is required.
		</p>
	</section>
</main>

<style>
	h1 {
		font-family: var(--font-display);
		margin: 0;
		font-size: 1.8rem;
	}

	h2 {
		font-family: var(--font-display);
		font-size: 1.2rem;
		margin: 0 0 0.5rem;
	}

	.block {
		padding: 1.2rem 1.3rem;
		max-width: 40rem;
	}

	.block + .block {
		margin-top: 1rem;
	}

	.path-box {
		margin: 1rem 0;
		padding: 0.85rem 1rem;
		border: 1px solid var(--line);
		border-radius: 8px;
		background: color-mix(in srgb, var(--ink) 3%, transparent);
		font-family: var(--font-ui);
	}

	.label {
		display: block;
		font-size: 0.8rem;
		color: var(--ink-muted);
		margin-bottom: 0.35rem;
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}

	.path {
		display: block;
		word-break: break-all;
		font-size: 0.95rem;
	}

	.status-line {
		margin: 0.45rem 0 0;
		font-size: 0.88rem;
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 0.6rem;
	}

	.ok {
		color: var(--accent);
		font-family: var(--font-ui);
	}

	code {
		font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
		font-size: 0.9em;
	}

	a {
		color: var(--accent);
		text-decoration: underline;
		text-underline-offset: 2px;
	}

	.block p.muted {
		margin: 0 0 0.65rem;
	}

	.block p.muted:last-child {
		margin-bottom: 0;
	}

	.font-options {
		margin: 1rem 0 0.85rem;
		padding: 0;
		border: none;
		display: flex;
		flex-direction: column;
		gap: 0.45rem;
	}

	.font-option {
		display: flex;
		align-items: flex-start;
		gap: 0.65rem;
		padding: 0.7rem 0.8rem;
		border: 1px solid var(--line);
		border-radius: 8px;
		background: color-mix(in srgb, var(--ink) 3%, transparent);
		cursor: pointer;
		font-family: var(--font-ui);
	}

	.font-option.selected {
		border-color: color-mix(in srgb, var(--accent) 55%, var(--line));
		background: color-mix(in srgb, var(--accent) 10%, transparent);
	}

	.font-option input {
		margin-top: 0.2rem;
	}

	.font-copy {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
		min-width: 0;
	}

	.font-label {
		font-weight: 600;
		color: var(--ink);
	}

	.font-desc {
		font-size: 0.88rem;
		margin: 0;
	}

	.font-preview {
		margin: 0;
		padding: 0.85rem 1rem;
		border: 1px solid var(--line);
		border-radius: 8px;
		background: var(--paper);
		color: var(--ink);
		font-size: 1.05rem;
		line-height: 1.6;
	}

	.sr-only {
		position: absolute;
		width: 1px;
		height: 1px;
		padding: 0;
		margin: -1px;
		overflow: hidden;
		clip: rect(0, 0, 0, 0);
		white-space: nowrap;
		border: 0;
	}

	strong {
		font-weight: 600;
		color: var(--ink);
	}
</style>
