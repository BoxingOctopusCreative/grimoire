<script lang="ts">
	import type { BookSummary } from '$lib/api';
	import {
		bookAuthorLabels,
		bookFormatKind,
		bookStatus,
		type FormatKind,
		type ReadingStatus
	} from '$lib/library-filters';
	import Checkbox from '$lib/ui/Checkbox.svelte';

	let {
		books,
		statuses = $bindable(new Set<ReadingStatus>()),
		genres = $bindable(new Set<string>()),
		formats = $bindable(new Set<FormatKind>()),
		authors = $bindable(new Set<string>())
	}: {
		books: BookSummary[];
		statuses?: Set<ReadingStatus>;
		genres?: Set<string>;
		formats?: Set<FormatKind>;
		authors?: Set<string>;
	} = $props();

	const STATUS_OPTIONS: { value: ReadingStatus; label: string }[] = [
		{ value: 'reading', label: 'Reading' },
		{ value: 'to-read', label: 'To Read' },
		{ value: 'read', label: 'Read' }
	];

	const FORMAT_OPTIONS: { value: FormatKind; label: string }[] = [
		{ value: 'novel', label: 'Novel' },
		{ value: 'comic', label: 'Comic Book' }
	];

	const statusCounts = $derived.by(() => {
		const counts: Record<ReadingStatus, number> = {
			reading: 0,
			'to-read': 0,
			read: 0
		};
		for (const book of books) {
			counts[bookStatus(book)] += 1;
		}
		return counts;
	});

	const formatCounts = $derived.by(() => {
		const counts: Record<FormatKind, number> = { novel: 0, comic: 0 };
		for (const book of books) {
			counts[bookFormatKind(book)] += 1;
		}
		return counts;
	});

	const genreOptions = $derived.by(() => {
		const counts = new Map<string, number>();
		for (const book of books) {
			for (const tag of book.tags) {
				const name = tag.trim();
				if (!name) continue;
				counts.set(name, (counts.get(name) ?? 0) + 1);
			}
		}
		return [...counts.entries()]
			.map(([value, count]) => ({ value, count }))
			.sort((a, b) => a.value.localeCompare(b.value));
	});

	const authorOptions = $derived.by(() => {
		const counts = new Map<string, number>();
		for (const book of books) {
			for (const author of bookAuthorLabels(book)) {
				counts.set(author, (counts.get(author) ?? 0) + 1);
			}
		}
		return [...counts.entries()]
			.map(([value, count]) => ({ value, count }))
			.sort((a, b) => a.value.localeCompare(b.value));
	});

	const hasActiveFilters = $derived(
		statuses.size > 0 || genres.size > 0 || formats.size > 0 || authors.size > 0
	);

	function setFromChecked<T>(current: Set<T>, value: T, checked: boolean): Set<T> {
		const next = new Set(current);
		if (checked) next.add(value);
		else next.delete(value);
		return next;
	}

	function checkedFromEvent(event: Event): boolean {
		const el = (event.currentTarget ?? event.target) as HTMLInputElement | null;
		return Boolean(el?.checked);
	}

	function onStatusChange(value: ReadingStatus, event: Event) {
		statuses = setFromChecked(statuses, value, checkedFromEvent(event));
	}

	function onGenreChange(value: string, event: Event) {
		genres = setFromChecked(genres, value, checkedFromEvent(event));
	}

	function onFormatChange(value: FormatKind, event: Event) {
		formats = setFromChecked(formats, value, checkedFromEvent(event));
	}

	function onAuthorChange(value: string, event: Event) {
		authors = setFromChecked(authors, value, checkedFromEvent(event));
	}

	function clearFilters() {
		statuses = new Set();
		genres = new Set();
		formats = new Set();
		authors = new Set();
	}
</script>

<aside class="library-filters" aria-label="Library filters">
	<div class="filters-header">
		<h2 class="filters-title">Filters</h2>
		{#if hasActiveFilters}
			<button type="button" class="clear-btn" onclick={clearFilters}>Clear</button>
		{/if}
	</div>

	<section class="facet" aria-labelledby="filter-status">
		<p class="ctx-label" id="filter-status">Status</p>
		<ul class="facet-list">
			{#each STATUS_OPTIONS as option (option.value)}
				<li>
					<Checkbox
						checked={statuses.has(option.value)}
						onchange={(e) => onStatusChange(option.value, e)}
					>
						<span class="option-row">
							<span class="option-label">{option.label}</span>
							<span class="option-count">{statusCounts[option.value]}</span>
						</span>
					</Checkbox>
				</li>
			{/each}
		</ul>
	</section>

	{#if genreOptions.length > 0}
		<section class="facet" aria-labelledby="filter-genre">
			<p class="ctx-label" id="filter-genre">Genre</p>
			<ul class="facet-list scrollable">
				{#each genreOptions as option (option.value)}
					<li>
						<Checkbox
							checked={genres.has(option.value)}
							onchange={(e) => onGenreChange(option.value, e)}
						>
							<span class="option-row">
								<span class="option-label">{option.value}</span>
								<span class="option-count">{option.count}</span>
							</span>
						</Checkbox>
					</li>
				{/each}
			</ul>
		</section>
	{/if}

	<section class="facet" aria-labelledby="filter-format">
		<p class="ctx-label" id="filter-format">Format</p>
		<ul class="facet-list">
			{#each FORMAT_OPTIONS as option (option.value)}
				<li>
					<Checkbox
						checked={formats.has(option.value)}
						onchange={(e) => onFormatChange(option.value, e)}
					>
						<span class="option-row">
							<span class="option-label">{option.label}</span>
							<span class="option-count">{formatCounts[option.value]}</span>
						</span>
					</Checkbox>
				</li>
			{/each}
		</ul>
	</section>

	{#if authorOptions.length > 0}
		<section class="facet" aria-labelledby="filter-author">
			<p class="ctx-label" id="filter-author">Author</p>
			<ul class="facet-list scrollable">
				{#each authorOptions as option (option.value)}
					<li>
						<Checkbox
							checked={authors.has(option.value)}
							onchange={(e) => onAuthorChange(option.value, e)}
						>
							<span class="option-row">
								<span class="option-label">{option.value}</span>
								<span class="option-count">{option.count}</span>
							</span>
						</Checkbox>
					</li>
				{/each}
			</ul>
		</section>
	{/if}
</aside>

<style>
	.library-filters {
		flex: 0 0 240px;
		width: 240px;
		max-width: 100%;
		align-self: stretch;
		height: 100%;
		min-height: 0;
		overflow-y: auto;
		padding: 1.1rem 1rem 1.5rem clamp(1rem, 3vw, 1.25rem);
		border-right: 1px solid var(--line);
		background: color-mix(in srgb, var(--panel) 88%, transparent);
		font-family: var(--font-ui);
	}

	.filters-header {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 0.5rem;
		margin-bottom: 0.85rem;
	}

	.filters-title {
		margin: 0;
		font-family: var(--font-display);
		font-size: 1.05rem;
		font-weight: 600;
		letter-spacing: -0.02em;
		color: var(--ink);
	}

	.clear-btn {
		margin: 0;
		padding: 0.15rem 0.35rem;
		border: none;
		border-radius: 4px;
		background: transparent;
		color: var(--accent);
		font: inherit;
		font-size: 0.82rem;
		font-weight: 600;
		cursor: pointer;
	}

	.clear-btn:hover {
		background: color-mix(in srgb, var(--accent) 14%, transparent);
	}

	.facet + .facet {
		margin-top: 1rem;
		padding-top: 0.85rem;
		border-top: 1px solid var(--line);
	}

	.ctx-label {
		margin: 0 0 0.45rem;
		font-size: 0.72rem;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--ink-muted);
	}

	.facet-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}

	.facet-list :global(.check) {
		display: flex;
		width: 100%;
	}

	.facet-list :global(.check-label) {
		flex: 1;
		min-width: 0;
	}

	.facet-list.scrollable {
		max-height: 11rem;
		overflow-y: auto;
		padding-right: 0.15rem;
	}

	.option-row {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 0.55rem;
		width: 100%;
		min-width: 0;
	}

	.option-label {
		min-width: 0;
		overflow-wrap: anywhere;
	}

	.option-count {
		flex: 0 0 auto;
		font-size: 0.78rem;
		font-weight: 600;
		color: var(--ink-muted);
	}

	@media (max-width: 720px) {
		.library-filters {
			flex: 0 0 auto;
			width: 100%;
			height: auto;
			max-height: min(50vh, 22rem);
			border-right: none;
			border-bottom: 1px solid var(--line);
			z-index: 6;
		}
	}
</style>
