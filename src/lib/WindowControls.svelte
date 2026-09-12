<script lang="ts">
	import { onMount } from 'svelte';
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import { detectChromeOs, type ChromeOs } from '$lib/window-chrome';

	let os = $state<ChromeOs>(detectChromeOs());
	let maximized = $state(false);
	let focused = $state(true);

	onMount(() => {
		os = detectChromeOs();
		const win = getCurrentWindow();

		const syncMaximized = async () => {
			try {
				maximized = await win.isMaximized();
				document.documentElement.classList.toggle('window-maximized', maximized);
			} catch {
				/* browser preview */
			}
		};

		const syncFocus = async () => {
			try {
				focused = await win.isFocused();
			} catch {
				focused = document.hasFocus();
			}
		};

		void syncMaximized();
		void syncFocus();

		const cleanups: Array<() => void> = [];

		void win
			.onResized(() => {
				void syncMaximized();
			})
			.then((fn) => cleanups.push(fn))
			.catch(() => {});

		void win
			.onFocusChanged((event) => {
				focused = event.payload;
			})
			.then((fn) => cleanups.push(fn))
			.catch(() => {});

		return () => {
			for (const fn of cleanups) fn();
		};
	});

	async function minimize() {
		try {
			await getCurrentWindow().minimize();
		} catch {
			/* browser preview */
		}
	}

	async function toggleMaximize() {
		try {
			const win = getCurrentWindow();
			await win.toggleMaximize();
			maximized = await win.isMaximized();
			document.documentElement.classList.toggle('window-maximized', maximized);
		} catch {
			/* browser preview */
		}
	}

	async function close() {
		try {
			await getCurrentWindow().close();
		} catch {
			/* browser preview */
		}
	}
</script>

{#if os === 'macos'}
	<div class="window-controls macos" class:inactive={!focused}>
		<button
			class="traffic close"
			type="button"
			title="Close"
			aria-label="Close"
			onclick={close}
		>
			<svg viewBox="0 0 12 12" aria-hidden="true">
				<path d="M3.5 3.5l5 5M8.5 3.5l-5 5" />
			</svg>
		</button>
		<button
			class="traffic minimize"
			type="button"
			title="Minimize"
			aria-label="Minimize"
			onclick={minimize}
		>
			<svg viewBox="0 0 12 12" aria-hidden="true">
				<path d="M2.5 6h7" />
			</svg>
		</button>
		<button
			class="traffic zoom"
			type="button"
			title={maximized ? 'Restore' : 'Zoom'}
			aria-label={maximized ? 'Restore' : 'Zoom'}
			onclick={toggleMaximize}
		>
			{#if maximized}
				<svg viewBox="0 0 12 12" aria-hidden="true">
					<path d="M4 2.5h5.5V8M2.5 4v5.5H8" />
				</svg>
			{:else}
				<svg viewBox="0 0 12 12" aria-hidden="true">
					<path d="M4.5 2.5v5h5M7.5 9.5h-5v-5" />
				</svg>
			{/if}
		</button>
	</div>
{:else}
	<div class="window-controls {os}" class:inactive={!focused}>
		<button
			class="win-btn minimize"
			type="button"
			title="Minimize"
			aria-label="Minimize"
			onclick={minimize}
		>
			<svg viewBox="0 0 10 10" aria-hidden="true">
				<path d="M1 5h8" />
			</svg>
		</button>
		<button
			class="win-btn maximize"
			type="button"
			title={maximized ? 'Restore' : 'Maximize'}
			aria-label={maximized ? 'Restore' : 'Maximize'}
			onclick={toggleMaximize}
		>
			{#if maximized}
				<svg viewBox="0 0 10 10" aria-hidden="true">
					<path d="M2.5 3.5h5v5h-5zM3.5 2.5h5v5" />
				</svg>
			{:else}
				<svg viewBox="0 0 10 10" aria-hidden="true">
					<path d="M1.5 1.5h7v7h-7z" />
				</svg>
			{/if}
		</button>
		<button
			class="win-btn close"
			type="button"
			title="Close"
			aria-label="Close"
			onclick={close}
		>
			<svg viewBox="0 0 10 10" aria-hidden="true">
				<path d="M2 2l6 6M8 2l-6 6" />
			</svg>
		</button>
	</div>
{/if}

<style>
	.window-controls {
		display: inline-flex;
		align-items: center;
		flex: 0 0 auto;
		-webkit-app-region: no-drag;
	}

	/* --- macOS traffic lights --- */
	.macos {
		gap: 8px;
		padding: 0 0.35rem 0 0.15rem;
		height: 100%;
	}

	.traffic {
		position: relative;
		width: 12px;
		height: 12px;
		padding: 0;
		border: none;
		border-radius: 50%;
		box-shadow: inset 0 0 0 0.5px rgba(0, 0, 0, 0.18);
		color: transparent;
		transition: filter 120ms ease;
	}

	.traffic svg {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		opacity: 0;
		stroke: rgba(0, 0, 0, 0.55);
		stroke-width: 1.2;
		stroke-linecap: round;
		fill: none;
		pointer-events: none;
	}

	.macos:hover .traffic svg,
	.traffic:focus-visible svg {
		opacity: 1;
	}

	.traffic.close {
		background: #ff5f57;
	}

	.traffic.minimize {
		background: #febc2e;
	}

	.traffic.zoom {
		background: #28c840;
	}

	.macos.inactive .traffic {
		background: #dcdcdc;
		box-shadow: inset 0 0 0 0.5px rgba(0, 0, 0, 0.08);
	}

	.macos.inactive .traffic svg {
		opacity: 0;
	}

	.traffic:hover {
		filter: brightness(0.92);
	}

	.traffic:active {
		filter: brightness(0.84);
	}

	/* --- Windows / Linux --- */
	.windows,
	.linux {
		align-self: stretch;
		height: auto;
		margin: 0;
		gap: 0;
	}

	.win-btn {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 46px;
		height: 100%;
		min-height: 2.75rem;
		padding: 0;
		border: none;
		border-radius: 0;
		background: transparent;
		color: var(--ink);
		transition:
			background 80ms ease,
			color 80ms ease;
	}

	.win-btn svg {
		width: 10px;
		height: 10px;
		stroke: currentColor;
		stroke-width: 1.15;
		stroke-linecap: round;
		stroke-linejoin: round;
		fill: none;
	}

	.win-btn:hover {
		background: color-mix(in srgb, var(--ink) 8%, transparent);
	}

	.win-btn:active {
		background: color-mix(in srgb, var(--ink) 14%, transparent);
	}

	.win-btn.close:hover {
		background: #e81123;
		color: #fff;
	}

	.win-btn.close:active {
		background: #f1707a;
		color: #fff;
	}

	.linux .win-btn {
		width: 40px;
		border-radius: 6px;
		margin: 0.35rem 0.1rem;
		min-height: 2rem;
		height: calc(100% - 0.7rem);
	}

	.linux {
		margin-right: 0.15rem;
	}

	:global([data-theme='dark']) .macos.inactive .traffic {
		background: #636363;
		box-shadow: inset 0 0 0 0.5px rgba(0, 0, 0, 0.25);
	}
</style>
