<script lang="ts">
	import type { Snippet } from 'svelte';

	type Variant = 'accent' | 'primary' | 'secondary' | 'negative';

	let {
		children,
		variant = 'accent',
		disabled = false,
		type = 'button',
		onclick,
		class: className = '',
		...rest
	}: {
		children?: Snippet;
		variant?: Variant;
		disabled?: boolean;
		type?: 'button' | 'submit' | 'reset';
		onclick?: (event: MouseEvent) => void;
		class?: string;
		[key: string]: unknown;
	} = $props();

	const variantClass = $derived(
		variant === 'secondary' ? 'secondary' : variant === 'negative' ? 'danger' : ''
	);
</script>

<button
	class="btn {className}"
	class:secondary={variantClass === 'secondary'}
	class:danger={variantClass === 'danger'}
	{type}
	{disabled}
	{onclick}
	{...rest}
>
	{@render children?.()}
</button>
