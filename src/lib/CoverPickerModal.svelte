<script lang="ts">
	import { SvelteSet } from 'svelte/reactivity';
	import {
		listEbookCoverCandidates,
		setEbookCoverFromUrls,
		type BookDetail,
		type CoverCandidate
	} from '$lib/api';
	import Button from '$lib/ui/Button.svelte';
	import Dialog from '$lib/ui/Dialog.svelte';

	let {
		open,
		bookId,
		onclose,
		onapplied
	}: {
		open: boolean;
		bookId: number;
		onclose: () => void;
		onapplied: (book: BookDetail) => void;
	} = $props();

	let candidates = $state<CoverCandidate[]>([]);
	let loading = $state(false);
	let applying = $state(false);
	let error = $state('');
	let selectedId = $state<string | null>(null);
	let brokenIds = new SvelteSet<string>();

	const selected = $derived(candidates.find((c) => c.id === selectedId) ?? null);

	$effect(() => {
		if (!open) return;
		void loadCandidates(bookId);
	});

	async function loadCandidates(id: number) {
		loading = true;
		error = '';
		candidates = [];
		selectedId = null;
		brokenIds.clear();
		try {
			candidates = await listEbookCoverCandidates(id);
		} catch (e) {
			error = String(e);
		} finally {
			loading = false;
		}
	}

	function close() {
		if (applying) return;
		onclose();
	}

	function markBroken(id: string) {
		brokenIds.add(id);
	}

	function letterFor(candidate: CoverCandidate) {
		const ch = candidate.title.trim().slice(0, 1);
		return ch || '?';
	}

	async function apply() {
		if (!selected || applying) return;
		applying = true;
		error = '';
		try {
			const book = await setEbookCoverFromUrls(bookId, selected.download_urls);
			onapplied(book);
			onclose();
		} catch (e) {
			error = String(e);
		} finally {
			applying = false;
		}
	}
</script>

<div class="cover-picker">
	<Dialog {open} title="Choose a cover" size="l" onclose={close}>
		<p class="muted help">
			Pick the edition that matches your book. Covers come from Open Library.
		</p>

		{#if error}
			<p class="error">{error}</p>
		{/if}

		<div class="body">
			{#if loading}
				<p class="muted status">Loading covers…</p>
			{:else if candidates.length === 0 && !error}
				<p class="muted status">No cover candidates found for this book.</p>
			{:else}
				<div class="grid" role="listbox" aria-label="Cover candidates">
					{#each candidates as candidate (candidate.id)}
						<button
							type="button"
							class="candidate"
							class:selected={selectedId === candidate.id}
							role="option"
							aria-selected={selectedId === candidate.id}
							disabled={applying}
							onclick={() => (selectedId = candidate.id)}
						>
							<div class="thumb">
								{#if brokenIds.has(candidate.id)}
									<div class="letter" aria-hidden="true">{letterFor(candidate)}</div>
								{:else}
									<img
										src={candidate.preview_url}
										alt=""
										onerror={() => markBroken(candidate.id)}
									/>
								{/if}
							</div>
							<span class="cand-title">{candidate.title}</span>
							<span class="cand-authors muted">{candidate.authors || 'Unknown author'}</span>
						</button>
					{/each}
				</div>
			{/if}
		</div>

		{#snippet footer()}
			<Button variant="secondary" onclick={close} disabled={applying}>Cancel</Button>
			<Button variant="accent" onclick={apply} disabled={applying || !selected || loading}>
				{applying ? 'Applying…' : 'Use this cover'}
			</Button>
		{/snippet}
	</Dialog>
</div>

<style>
	.cover-picker :global(.dialog-frame) {
		width: min(720px, 100%);
		max-height: min(88vh, 900px);
	}

	.help {
		margin: 0 0 0.75rem;
		font-family: var(--font-ui);
		font-size: 0.9rem;
		line-height: 1.4;
	}

	.body {
		min-height: 8rem;
		max-height: min(60vh, 28rem);
		overflow: auto;
	}

	.status {
		margin: 1rem 0;
		font-family: var(--font-ui);
		text-align: center;
	}

	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
		gap: 0.75rem;
	}

	.candidate {
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
		align-items: stretch;
		text-align: left;
		padding: 0.45rem;
		border: 2px solid transparent;
		border-radius: 10px;
		background: color-mix(in srgb, var(--ink) 3%, transparent);
		color: inherit;
		font: inherit;
		cursor: pointer;
		transition: border-color 140ms ease, background 140ms ease;
	}

	.candidate:hover:not(:disabled) {
		background: color-mix(in srgb, var(--ink) 6%, transparent);
	}

	.candidate.selected {
		border-color: var(--accent);
		background: color-mix(in srgb, var(--accent) 10%, transparent);
	}

	.candidate:disabled {
		opacity: 0.7;
		cursor: not-allowed;
	}

	.thumb {
		aspect-ratio: 2 / 3;
		width: 100%;
		border-radius: 6px;
		overflow: hidden;
		background: color-mix(in srgb, var(--ink) 8%, transparent);
		display: grid;
		place-items: center;
	}

	.thumb img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
	}

	.letter {
		font-family: var(--font-display);
		font-size: 2rem;
		color: color-mix(in srgb, var(--ink) 45%, transparent);
	}

	.cand-title {
		font-family: var(--font-ui);
		font-size: 0.82rem;
		font-weight: 600;
		line-height: 1.25;
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	.cand-authors {
		font-family: var(--font-ui);
		font-size: 0.75rem;
		line-height: 1.25;
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	.error {
		margin: 0 0 0.75rem;
	}
</style>
