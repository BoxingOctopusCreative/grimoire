<script lang="ts">
	import type { Snippet } from 'svelte';

	let {
		children,
		checked = $bindable(false),
		disabled = false,
		indeterminate = false,
		onchange,
		class: className = '',
		...rest
	}: {
		children?: Snippet;
		checked?: boolean;
		disabled?: boolean;
		indeterminate?: boolean;
		onchange?: (event: Event) => void;
		class?: string;
		[key: string]: unknown;
	} = $props();

	let inputEl = $state<HTMLInputElement | null>(null);

	$effect(() => {
		if (inputEl) inputEl.indeterminate = indeterminate;
	});

	function onChange(event: Event) {
		const target = event.currentTarget as HTMLInputElement;
		checked = Boolean(target.checked);
		onchange?.(event);
	}
</script>

<label class="check {className}">
	<input
		bind:this={inputEl}
		type="checkbox"
		{checked}
		{disabled}
		onchange={onChange}
		{...rest}
	/>
	<span class="check-label">
		{@render children?.()}
	</span>
</label>

<style>
	.check {
		display: inline-flex;
		align-items: flex-start;
		gap: 0.5rem;
		font-family: var(--font-ui);
		color: var(--ink);
		cursor: pointer;
	}

	.check input {
		margin-top: 0.2rem;
		accent-color: var(--accent);
	}

	.check:has(input:disabled) {
		opacity: 0.55;
		cursor: not-allowed;
	}

	.check-label {
		line-height: 1.35;
	}
</style>
