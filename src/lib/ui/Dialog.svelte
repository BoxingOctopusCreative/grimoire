<script lang="ts">
	import type { Snippet } from 'svelte';
	import Button from '$lib/ui/Button.svelte';

	let {
		open = false,
		title = '',
		children,
		footer,
		onclose,
		size = 'm'
	}: {
		open?: boolean;
		title?: string;
		children?: Snippet;
		footer?: Snippet;
		onclose?: () => void;
		size?: 's' | 'm' | 'l';
	} = $props();

	function close() {
		onclose?.();
	}

	function onKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') close();
	}
</script>

{#if open}
	<div class="dialog-root" role="presentation">
		<button type="button" class="dialog-underlay" aria-label="Dismiss dialog" onclick={close}
		></button>
		<div
			class="dialog-frame panel"
			class:size-s={size === 's'}
			class:size-l={size === 'l'}
			role="dialog"
			aria-modal="true"
			aria-label={title || 'Dialog'}
			tabindex="-1"
			onkeydown={onKeydown}
		>
			{#if title}
				<header class="dialog-header">
					<h2>{title}</h2>
					<button type="button" class="dialog-close" aria-label="Close" onclick={close}
						>×</button
					>
				</header>
			{/if}
			<div class="dialog-body">
				{@render children?.()}
			</div>
			<div class="dialog-actions">
				{#if footer}
					{@render footer()}
				{:else}
					<Button variant="secondary" onclick={close}>Close</Button>
				{/if}
			</div>
		</div>
	</div>
{/if}

<style>
	.dialog-root {
		position: fixed;
		inset: 0;
		z-index: 70;
		display: grid;
		place-items: center;
		padding: 1.25rem;
	}

	.dialog-underlay {
		position: absolute;
		inset: 0;
		border: none;
		padding: 0;
		background: color-mix(in srgb, var(--ink) 35%, transparent);
		cursor: pointer;
	}

	.dialog-frame {
		position: relative;
		z-index: 1;
		width: min(32rem, 100%);
		max-height: min(90vh, 44rem);
		overflow: auto;
		padding: 1.1rem 1.2rem 1.2rem;
		outline: none;
	}

	.dialog-frame.size-s {
		width: min(22rem, 100%);
	}

	.dialog-frame.size-l {
		width: min(44rem, 100%);
	}

	.dialog-header {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 0.75rem;
		margin-bottom: 0.85rem;
	}

	.dialog-header h2 {
		margin: 0;
		font-family: var(--font-display);
		font-size: 1.35rem;
	}

	.dialog-close {
		border: none;
		background: transparent;
		color: var(--ink-muted);
		font-size: 1.35rem;
		line-height: 1;
		padding: 0.1rem 0.35rem;
		border-radius: 4px;
	}

	.dialog-close:hover {
		color: var(--ink);
		background: color-mix(in srgb, var(--ink) 8%, transparent);
	}

	.dialog-body {
		font-family: var(--font-ui);
		color: var(--ink);
	}

	.dialog-actions {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
		justify-content: flex-end;
		margin-top: 1rem;
	}
</style>
