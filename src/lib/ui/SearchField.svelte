<script lang="ts">
	let {
		value = $bindable(''),
		placeholder = 'Search',
		disabled = false,
		label = 'Search',
		oninput,
		onchange,
		class: className = '',
		...rest
	}: {
		value?: string;
		placeholder?: string;
		disabled?: boolean;
		label?: string;
		oninput?: (event: Event) => void;
		onchange?: (event: Event) => void;
		class?: string;
		[key: string]: unknown;
	} = $props();

	function syncValue(event: Event) {
		const target = event.currentTarget as HTMLInputElement;
		value = target.value ?? '';
		oninput?.(event);
	}
</script>

<input
	type="search"
	class="search {className}"
	{value}
	{placeholder}
	{disabled}
	aria-label={label}
	oninput={syncValue}
	onchange={(event) => {
		syncValue(event);
		onchange?.(event);
	}}
	{...rest}
/>
