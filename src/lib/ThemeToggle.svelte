<script lang="ts">
	import { onMount } from 'svelte';
	import Moon from '@lucide/svelte/icons/moon';
	import Sun from '@lucide/svelte/icons/sun';
	import Monitor from '@lucide/svelte/icons/monitor';
	import {
		applyTheme,
		cycleTheme,
		getStoredTheme,
		setThemePreference,
		themeLabel,
		type ThemePreference
	} from '$lib/theme';

	let preference = $state<ThemePreference>('dark');

	onMount(() => {
		preference = getStoredTheme();
		applyTheme(preference);

		const media = window.matchMedia('(prefers-color-scheme: dark)');
		const onChange = () => {
			if (preference === 'system') applyTheme('system');
		};
		media.addEventListener('change', onChange);
		return () => media.removeEventListener('change', onChange);
	});

	function toggle() {
		preference = cycleTheme(preference);
		setThemePreference(preference);
	}
</script>

<button
	class="theme-toggle"
	type="button"
	onclick={toggle}
	data-tooltip={`Theme: ${themeLabel(preference)}`}
	aria-label={`Theme: ${themeLabel(preference)}. Click to change.`}
	title={`Theme: ${themeLabel(preference)}`}
>
	{#if preference === 'dark'}
		<Moon size={20} strokeWidth={1.75} aria-hidden="true" />
	{:else if preference === 'light'}
		<Sun size={20} strokeWidth={1.75} aria-hidden="true" />
	{:else}
		<Monitor size={20} strokeWidth={1.75} aria-hidden="true" />
	{/if}
</button>

<style>
	.theme-toggle {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 2.1rem;
		height: 2.1rem;
		padding: 0.45rem;
		border-radius: 999px;
		border: 1px solid var(--line);
		background: var(--control-bg);
		color: var(--ink-muted);
		transition: background 160ms ease, color 160ms ease, border-color 160ms ease;
	}

	.theme-toggle:hover {
		color: var(--ink);
		background: var(--control-hover);
	}
</style>
