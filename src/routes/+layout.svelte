<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import '../app.css';
	import { openSavedLibrary } from '$lib/api';
	import {
		devicePresence,
		dismissDeviceNotice,
		startDevicePresenceListener
	} from '$lib/device-presence.svelte';
	import ThemeToggle from '$lib/ThemeToggle.svelte';
	import WindowControls from '$lib/WindowControls.svelte';
	import { detectChromeOs, windowControlsOnLeft, type ChromeOs } from '$lib/window-chrome';
	import Library from '@lucide/svelte/icons/library';
	import TabletSmartphone from '@lucide/svelte/icons/tablet-smartphone';
	import Settings from '@lucide/svelte/icons/settings';

	let { children } = $props();

	function detectAuxWindow() {
		try {
			return getCurrentWindow().label !== 'main';
		} catch {
			return false;
		}
	}

	let isAuxWindow = $state(detectAuxWindow());
	let chromeOs = $state<ChromeOs>(detectChromeOs());
	let booting = $state(!isAuxWindow);
	let bootError = $state('');
	const controlsLeft = $derived(windowControlsOnLeft(chromeOs));

	async function onTitlebarDblClick() {
		try {
			await getCurrentWindow().toggleMaximize();
		} catch {
			/* browser preview */
		}
	}

	onMount(() => {
		chromeOs = detectChromeOs();

		if (isAuxWindow) {
			return;
		}

		void startDevicePresenceListener();
		void (async () => {
			try {
				await openSavedLibrary();
			} catch (e) {
				bootError = String(e);
			} finally {
				booting = false;
			}
		})();
	});
</script>

<svelte:head>
	<link rel="preconnect" href="https://fonts.googleapis.com" />
	<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin="anonymous" />
	<link
		href="https://fonts.googleapis.com/css2?family=Fraunces:opsz,wght@9..144,500;9..144,650&family=Source+Sans+3:wght@400;600&family=Source+Serif+4:opsz,wght@8..60,400;8..60,600&display=swap"
		rel="stylesheet"
	/>
	<title>Grimoire</title>
</svelte:head>

{#if isAuxWindow}
	{@render children()}
{:else}
	<div class="app-shell">
		<header class="topbar" data-chrome={chromeOs}>
			{#if controlsLeft}
				<WindowControls />
			{/if}
			<a class="brand" href="/">
				<img src="/grimoire_icon.svg" alt="" class="brand-logo" />
				<span class="brand-text">Grimoire<span class="brand-dot">.</span></span>
			</a>
			<div
				class="topbar-drag"
				data-tauri-drag-region
				ondblclick={onTitlebarDblClick}
				role="presentation"
			></div>
			<div class="topbar-end">
				<nav class="nav">
					<a
						href="/"
						class:active={page.url.pathname === '/'}
						data-tooltip="Library"
						title="Library"
						aria-label="Library"
					>
						<Library size={20} strokeWidth={1.75} aria-hidden="true" />
					</a>
					<a
						href="/devices"
						class:active={page.url.pathname.startsWith('/devices')}
						data-tooltip="Devices"
						title="Devices"
						aria-label="Devices"
					>
						<TabletSmartphone size={20} strokeWidth={1.75} aria-hidden="true" />
						{#if devicePresence.devices.length > 0}
							<span class="nav-count">{devicePresence.devices.length}</span>
						{/if}
					</a>
					<a
						href="/settings"
						class:active={page.url.pathname.startsWith('/settings')}
						data-tooltip="Settings"
						title="Settings"
						aria-label="Settings"
					>
						<Settings size={20} strokeWidth={1.75} aria-hidden="true" />
					</a>
				</nav>
				<ThemeToggle />
			</div>
			{#if !controlsLeft}
				<WindowControls />
			{/if}
		</header>

		{#if devicePresence.notice}
			<div
				class="device-toast"
				class:left={devicePresence.notice.kind === 'left'}
				role="status"
				aria-live="polite"
			>
				<p>{devicePresence.notice.message}</p>
				<button
					class="toast-dismiss"
					type="button"
					onclick={dismissDeviceNotice}
					aria-label="Dismiss"
				>
					×
				</button>
			</div>
		{/if}

		<div class="shell-body">
			{#if booting}
				<main class="page">
					<p class="muted">Opening library…</p>
				</main>
			{:else if bootError}
				<main class="page">
					<p class="error">{bootError}</p>
				</main>
			{:else}
				{@render children()}
			{/if}
		</div>
	</div>
{/if}

<style>
	.nav-count {
		position: absolute;
		top: -0.15rem;
		right: -0.15rem;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		min-width: 1.05rem;
		height: 1.05rem;
		padding: 0 0.28rem;
		border-radius: 999px;
		font-family: var(--font-ui);
		font-size: 0.65rem;
		font-weight: 600;
		line-height: 1;
		background: var(--accent);
		color: var(--btn-fg);
		pointer-events: none;
	}

	.device-toast {
		position: fixed;
		right: clamp(1rem, 3vw, 2rem);
		bottom: 1.25rem;
		z-index: 40;
		display: flex;
		align-items: center;
		gap: 0.75rem;
		max-width: min(22rem, calc(100vw - 2rem));
		padding: 0.75rem 0.85rem 0.75rem 1rem;
		border: 1px solid var(--line);
		border-radius: var(--radius);
		background: var(--panel);
		box-shadow: var(--shadow);
		font-family: var(--font-ui);
		animation: fade-up 220ms ease both;
	}

	.device-toast.left {
		opacity: 0.95;
	}

	.device-toast p {
		margin: 0;
		flex: 1 1 auto;
		font-size: 0.95rem;
	}

	.toast-dismiss {
		border: none;
		background: transparent;
		color: var(--ink-muted);
		font-size: 1.25rem;
		line-height: 1;
		padding: 0.15rem 0.35rem;
		cursor: pointer;
		border-radius: 4px;
	}

	.toast-dismiss:hover {
		color: var(--ink);
		background: color-mix(in srgb, var(--ink) 8%, transparent);
	}
</style>
