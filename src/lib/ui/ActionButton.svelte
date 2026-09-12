<script lang="ts">
	import type { Snippet } from 'svelte';

	let {
		children,
		label = '',
		selected = false,
		disabled = false,
		onclick,
		class: className = '',
		tooltip,
		title,
		...rest
	}: {
		children?: Snippet;
		label?: string;
		selected?: boolean;
		disabled?: boolean;
		quiet?: boolean;
		onclick?: (event: MouseEvent) => void;
		class?: string;
		tooltip?: string;
		title?: string;
		[key: string]: unknown;
	} = $props();

	const tip = $derived(tooltip ?? title ?? label);
</script>

<button
	type="button"
	class="action-btn {className}"
	class:selected
	{disabled}
	{onclick}
	data-tooltip={tip || undefined}
	title={tip || undefined}
	aria-label={label || tip || undefined}
	aria-pressed={selected}
	{...rest}
>
	{@render children?.()}
</button>

<style>
	.action-btn {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		min-width: 2.1rem;
		min-height: 2.1rem;
		padding: 0.45rem;
		border: none;
		border-radius: 999px;
		background: transparent;
		color: var(--ink-muted);
		transition: background 160ms ease, color 160ms ease;
	}

	.action-btn:hover:not(:disabled),
	.action-btn.selected {
		background: color-mix(in srgb, var(--accent) 16%, transparent);
		color: var(--accent);
	}

	.action-btn:disabled {
		opacity: 0.55;
		cursor: not-allowed;
	}
</style>
