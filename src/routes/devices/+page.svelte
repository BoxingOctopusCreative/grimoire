<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import {
		getLibraryStatus,
		getSyncHistory,
		listEbookReaders,
		type EbookReaderDevice,
		type LibraryStatus,
		type SyncRecord
	} from '$lib/api';
	import { devicePresence, onEreaderPresence } from '$lib/device-presence.svelte';
	import Button from '$lib/ui/Button.svelte';

	let devices = $state<EbookReaderDevice[]>([]);
	let history = $state<SyncRecord[]>([]);
	let status = $state<LibraryStatus | null>(null);
	let error = $state('');
	let loading = $state(true);

	function formatBytes(n: number | null) {
		if (n == null) return 'Unknown free space';
		const gb = n / 1e9;
		if (gb >= 1) return `${gb.toFixed(1)} GB free`;
		const mb = n / 1e6;
		return `${mb.toFixed(0)} MB free`;
	}

	function connectionLabel(device: EbookReaderDevice) {
		return device.connection === 'mtp' ? 'USB/MTP' : 'Mounted volume';
	}

	async function refreshSyncHistory() {
		try {
			status = await getLibraryStatus();
			if (status.open) {
				history = await getSyncHistory();
			} else {
				history = [];
			}
		} catch {
			/* keep existing history on soft presence updates */
		}
	}

	async function refresh() {
		error = '';
		loading = true;
		try {
			status = await getLibraryStatus();
			devices = await listEbookReaders();
			devicePresence.devices = devices;
			if (status.open) {
				history = await getSyncHistory();
			} else {
				history = [];
			}
		} catch (e) {
			error = String(e);
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		void refresh();
		return onEreaderPresence((event) => {
			devices = event.devices;
			void refreshSyncHistory();
		});
	});
</script>

<main class="page fade-up">
	<div class="toolbar">
		<div>
			<h1>Devices</h1>
			<p class="muted">
				Detects Kindles, Kobos, BOOX, PocketBook, reMarkable, Tolino, and Nook over USB/MTP or
				mounted volumes.
			</p>
		</div>
		<Button variant="secondary" onclick={refresh} disabled={loading}>
			{loading ? 'Scanning…' : 'Refresh'}
		</Button>
	</div>

	{#if error}
		<p class="error">{error}</p>
	{/if}

	<section class="panel block">
		<h2>Connected eReaders</h2>
		{#if loading}
			<p class="muted">Scanning MTP devices and mounted volumes…</p>
		{:else if devices.length === 0}
			<p class="muted">
				No eReaders found yet. Grimoire watches for USB/MTP and mounted volumes automatically;
				use Refresh if a device does not appear. On macOS, quit Android File Transfer and stop
				<code>ptpcamerad</code> if an MTP device stays locked.
			</p>
		{:else}
			<ul class="device-list">
				{#each devices as device (device.id)}
					<li>
						<div>
							<div class="title-row">
								<strong>{device.product}</strong>
								<span class="badge family">{device.family_label}</span>
							</div>
							<div class="muted">{device.display}</div>
							<div class="muted">
								{connectionLabel(device)} · {formatBytes(device.free_space_bytes)} · prefers
								{device.preferred_formats.join(', ')}
							</div>
							{#if device.notes}
								<div class="note">{device.notes}</div>
							{/if}
							{#if !device.supports_send}
								<div class="muted subtle">
									Detected. USB send for this family is not wired yet (Kindle MTP is supported).
								</div>
							{/if}
						</div>
						<div class="device-actions">
							<Button
								variant="secondary"
								onclick={() =>
									goto(`/devices/library?id=${encodeURIComponent(device.id)}`)
								}
							>
								View books
							</Button>
							<span class="badge" class:ready={device.supports_send}>
								{device.supports_send ? 'Send ready' : 'Detected'}
							</span>
						</div>
					</li>
				{/each}
			</ul>
		{/if}

		{#if status && !status.kindle_convert_available}
			<p class="hint muted">
				Native Kindle conversion is unavailable in this build. EPUB-only titles need an AZW3,
				MOBI, or PDF format before USB send.
			</p>
		{/if}
	</section>

	<section class="panel block">
		<h2>Recent transfers</h2>
		{#if !status?.open}
			<p class="muted">Open a library to see sync history.</p>
		{:else if history.length === 0}
			<p class="muted">No transfers recorded yet. Send a book from its detail page.</p>
		{:else}
			<table>
				<thead>
					<tr>
						<th>Book</th>
						<th>Device</th>
						<th>Format</th>
						<th>When</th>
						<th>Path</th>
					</tr>
				</thead>
				<tbody>
					{#each history as row (row.book_id + row.device_serial + row.last_sent_at)}
						<tr>
							<td><a href={`/book/${row.book_id}`}>#{row.book_id}</a></td>
							<td>{row.device_serial}</td>
							<td>{row.format}</td>
							<td>{row.last_sent_at}</td>
							<td class="path">{row.remote_path}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		{/if}
	</section>
</main>

<style>
	h1 {
		font-family: var(--font-display);
		margin: 0;
		font-size: 1.8rem;
	}

	h2 {
		font-family: var(--font-display);
		font-size: 1.2rem;
		margin: 0 0 0.75rem;
	}

	.toolbar {
		justify-content: space-between;
	}

	.block {
		padding: 1.2rem 1.3rem;
		margin-bottom: 1.1rem;
	}

	.device-list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}

	.device-list li {
		display: flex;
		justify-content: space-between;
		align-items: flex-start;
		gap: 1rem;
		padding: 0.75rem 0;
		border-top: 1px solid var(--line);
	}

	.device-list li:first-child {
		border-top: none;
		padding-top: 0;
	}

	.title-row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.45rem;
		margin-bottom: 0.2rem;
	}

	.device-actions {
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		gap: 0.45rem;
	}

	.badge {
		font-family: var(--font-ui);
		font-size: 0.8rem;
		padding: 0.25rem 0.55rem;
		border-radius: 999px;
		background: color-mix(in srgb, var(--ink-muted) 18%, transparent);
		color: var(--ink-muted);
		white-space: nowrap;
	}

	.badge.family {
		background: color-mix(in srgb, var(--accent) 14%, transparent);
		color: var(--accent);
	}

	.badge.ready {
		background: color-mix(in srgb, var(--accent) 16%, transparent);
		color: var(--accent);
	}

	.note {
		margin-top: 0.35rem;
		font-family: var(--font-ui);
		font-size: 0.88rem;
		color: var(--danger);
	}

	.subtle {
		margin-top: 0.3rem;
		font-family: var(--font-ui);
		font-size: 0.85rem;
	}

	.hint {
		margin: 1rem 0 0;
		font-family: var(--font-ui);
		font-size: 0.9rem;
	}

	table {
		width: 100%;
		border-collapse: collapse;
		font-family: var(--font-ui);
		font-size: 0.92rem;
	}

	th,
	td {
		text-align: left;
		padding: 0.55rem 0.4rem;
		border-bottom: 1px solid var(--line);
		vertical-align: top;
	}

	.path {
		word-break: break-all;
	}

	code {
		font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
		font-size: 0.9em;
	}
</style>
