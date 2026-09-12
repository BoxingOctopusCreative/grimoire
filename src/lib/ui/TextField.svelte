<script lang="ts">
	let {
		value = $bindable(''),
		label = '',
		placeholder = '',
		disabled = false,
		multiline = false,
		rows = 3,
		id = undefined,
		onchange,
		oninput,
		class: className = '',
		...rest
	}: {
		value?: string;
		label?: string;
		placeholder?: string;
		disabled?: boolean;
		multiline?: boolean;
		rows?: number;
		id?: string;
		onchange?: (event: Event) => void;
		oninput?: (event: Event) => void;
		class?: string;
		[key: string]: unknown;
	} = $props();

	function syncValue(event: Event) {
		const target = event.currentTarget as HTMLInputElement | HTMLTextAreaElement;
		value = target.value ?? '';
		oninput?.(event);
	}
</script>

{#if label}
	<label class="field-label" for={id}>{label}</label>
{/if}
{#if multiline}
	<textarea
		{id}
		class={className}
		{value}
		{placeholder}
		{disabled}
		{rows}
		oninput={syncValue}
		onchange={(event) => {
			syncValue(event);
			onchange?.(event);
		}}
		{...rest}
	></textarea>
{:else}
	<input
		{id}
		class={className}
		type="text"
		{value}
		{placeholder}
		{disabled}
		oninput={syncValue}
		onchange={(event) => {
			syncValue(event);
			onchange?.(event);
		}}
		{...rest}
	/>
{/if}

<style>
	.field-label {
		display: block;
		font-family: var(--font-ui);
		font-size: 0.9rem;
		margin-bottom: 0.35rem;
	}

	input,
	textarea {
		display: block;
		width: 100%;
		border: 1px solid var(--line);
		border-radius: 8px;
		padding: 0.55rem 0.7rem;
		background: var(--control-bg);
		color: var(--ink);
		font-family: var(--font-ui);
	}

	textarea {
		resize: vertical;
		min-height: 4.5rem;
	}

	input:disabled,
	textarea:disabled {
		opacity: 0.55;
		cursor: not-allowed;
	}
</style>
