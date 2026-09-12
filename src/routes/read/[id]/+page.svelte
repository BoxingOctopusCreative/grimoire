<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import { SvelteMap } from 'svelte/reactivity';
	import { convertFileSrc } from '@tauri-apps/api/core';
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import ePub from 'epubjs';
	import {
		getComicPage,
		getEbookReadingProgress,
		getReaderSource,
		listComicPages,
		saveEbookReadingProgress,
		type ComicPage,
		type ReaderSource,
		type ReadingProgress
	} from '$lib/api';
	import WindowControls from '$lib/WindowControls.svelte';
	import Button from '$lib/ui/Button.svelte';
	import { detectChromeOs, windowControlsOnLeft, type ChromeOs } from '$lib/window-chrome';
	import { applyTheme, getStoredTheme, resolveTheme, type ResolvedTheme } from '$lib/theme';
	import {
		getReaderFontOption,
		getStoredReaderFont,
		openDyslexicFontFaceCss,
		type ReaderFontId
	} from '$lib/reader-prefs';

	let { params } = $props();
	const bookId = $derived(Number(params.id));
	let chromeOs = $state<ChromeOs>(detectChromeOs());
	const controlsLeft = $derived(windowControlsOnLeft(chromeOs));
	let contentTheme = $state<ResolvedTheme>('light');
	let readerFont = $state<ReaderFontId>(getStoredReaderFont());
	const textFontFamily = $derived(
		getReaderFontOption(readerFont).cssFamily ?? 'var(--font-body)'
	);

	function readContentTheme(): ResolvedTheme {
		const attr = document.documentElement.dataset.theme;
		if (attr === 'dark' || attr === 'light') return attr;
		return resolveTheme('system');
	}

	function contentPalette(theme: ResolvedTheme) {
		if (theme === 'dark') {
			return {
				bg: '#1a1714',
				fg: '#efe6d8',
				muted: '#b0a493',
				link: '#7fb5a4'
			};
		}
		return {
			bg: '#faf6ee',
			fg: '#1c1814',
			muted: '#5c534a',
			link: '#2f5d50'
		};
	}

	function injectEpubFontFaces(contents: { document?: Document | null }) {
		const doc = contents.document;
		if (!doc?.head) return;
		let style = doc.getElementById('grimoire-reader-fonts') as HTMLStyleElement | null;
		if (!style) {
			style = doc.createElement('style');
			style.id = 'grimoire-reader-fonts';
			doc.head.appendChild(style);
		}
		style.textContent = openDyslexicFontFaceCss();
	}

	function applyEpubReaderStyles(
		theme: ResolvedTheme = contentTheme,
		fontId: ReaderFontId = readerFont
	) {
		if (!rendition?.themes) return;
		const c = contentPalette(theme);
		const font = getReaderFontOption(fontId);
		const textSelectors = 'p, div, span, li, td, th, h1, h2, h3, h4, h5, h6, blockquote';
		const bodyRules: Record<string, string> = {
			background: `${c.bg} !important`,
			color: `${c.fg} !important`
		};
		const textRules: Record<string, string> = {
			color: `${c.fg} !important`
		};
		if (font.cssFamily) {
			bodyRules['font-family'] = `${font.cssFamily} !important`;
			textRules['font-family'] = `${font.cssFamily} !important`;
		}
		rendition.themes.default({
			body: bodyRules,
			[textSelectors]: textRules,
			a: {
				color: `${c.link} !important`
			},
			'code, pre': {
				color: `${c.fg} !important`,
				background: 'transparent !important'
			}
		});
	}

	let source = $state<ReaderSource | null>(null);
	const formatLabel = $derived.by(() => {
		const src = source;
		if (!src) return '';
		const opened = src.format;
		const openedUpper = opened.toUpperCase();
		const kindle = ['MOBI', 'AZW3'].filter(
			(fmt) =>
				fmt !== openedUpper &&
				src.available_formats.some((f) => f.toUpperCase() === fmt)
		);
		if (kindle.length === 0) return opened;
		return `${opened} · also ${kindle.join(', ')} for Kindle`;
	});
	let savedProgress = $state<ReadingProgress | null>(null);
	let error = $state('');
	let loading = $state(true);
	let percent = $state(0);
	let locationLabel = $state('');
	let ready = $state(false);

	let viewerEl = $state<HTMLDivElement | null>(null);
	let textEl = $state<HTMLElement | null>(null);
	let textContent = $state('');
	let pdfUrl = $state<string | null>(null);

	let comicPageIndex = $state(0);
	let comicPageCount = $state(0);
	let comicDataUrl = $state('');
	let comicPageName = $state('');
	const comicCache = new SvelteMap<number, ComicPage>();

	let book: ReturnType<typeof ePub> | null = null;
	let rendition: any = null;
	let saveTimer: ReturnType<typeof setTimeout> | undefined;
	let epubStarted = false;
	let textStarted = false;
	let pdfStarted = false;
	let comicStarted = false;
	let destroyed = false;

	function isComicFormat(format: string) {
		return format === 'CBZ' || format === 'CBR';
	}

	function scheduleSave(format: string, location: string, nextPercent: number) {
		percent = Math.max(0, Math.min(100, nextPercent));
		clearTimeout(saveTimer);
		saveTimer = setTimeout(() => {
			void saveEbookReadingProgress(bookId, {
				format,
				location,
				percent
			}).catch((e) => {
				console.error('Could not save reading progress', e);
			});
		}, 700);
	}

	function teardownEpub() {
		try {
			rendition?.destroy?.();
		} catch {
			/* ignore */
		}
		try {
			book?.destroy?.();
		} catch {
			/* ignore */
		}
		rendition = null;
		book = null;
		epubStarted = false;
	}

	function resetComic() {
		comicStarted = false;
		comicCache.clear();
		comicPageIndex = 0;
		comicPageCount = 0;
		comicDataUrl = '';
		comicPageName = '';
	}

	async function setupEpub(el: HTMLDivElement, src: ReaderSource, saved: ReadingProgress | null) {
		teardownEpub();
		epubStarted = true;
		const url = convertFileSrc(src.path);
		book = ePub(url);
		rendition = book.renderTo(el, {
			width: '100%',
			height: '100%',
			flow: 'paginated',
			allowScriptedContent: false
		});

		applyEpubReaderStyles();

		rendition.hooks.content.register((contents: { document?: Document | null }) => {
			injectEpubFontFaces(contents);
			applyEpubReaderStyles();
		});

		const start =
			saved && saved.format === 'EPUB' && saved.location ? saved.location : undefined;
		await rendition.display(start);
		applyEpubReaderStyles();
		ready = true;

		rendition.on('rendered', () => {
			applyEpubReaderStyles();
		});

		rendition.on(
			'relocated',
			(location: {
				start?: { cfi?: string; displayed?: { page?: number; total?: number } };
				atEnd?: boolean;
			}) => {
				const cfi = location.start?.cfi ?? '';
				let nextPercent = percent;
				const displayed = location.start?.displayed;
				if (displayed?.total && displayed.total > 0 && displayed.page != null) {
					nextPercent = (displayed.page / displayed.total) * 100;
					locationLabel = `Page ${displayed.page} of ${displayed.total}`;
				} else if (location.atEnd) {
					nextPercent = 100;
					locationLabel = 'End';
				}
				if (cfi) scheduleSave('EPUB', cfi, nextPercent);
			}
		);

		try {
			await book.locations.generate(1600);
			if (destroyed || !rendition || !book) return;
			const loc = rendition.currentLocation();
			const cfi = loc?.start?.cfi;
			if (cfi && book.locations?.length) {
				const idx = book.locations.locationFromCfi(cfi);
				const total = book.locations.length();
				if (typeof idx === 'number' && total > 0) {
					percent = (idx / total) * 100;
					locationLabel = `${Math.round(percent)}%`;
					scheduleSave('EPUB', cfi, percent);
				}
			}
		} catch {
			// Location generation is best-effort.
		}
	}

	async function setupText(src: ReaderSource, saved: ReadingProgress | null) {
		textStarted = true;
		const url = convertFileSrc(src.path);
		const response = await fetch(url);
		if (!response.ok) {
			throw new Error(`Could not load text file (HTTP ${response.status})`);
		}
		textContent = await response.text();
		percent = saved?.format === 'TXT' ? saved.percent : 0;
		locationLabel = `${Math.round(percent)}%`;
		ready = true;
	}

	function restoreTextScroll(el: HTMLElement, saved: ReadingProgress | null) {
		if (saved?.format === 'TXT' && saved.location) {
			const top = Number(saved.location);
			if (!Number.isNaN(top)) {
				el.scrollTop = top;
				return;
			}
		}
		if (saved?.format === 'TXT' && saved.percent > 0) {
			el.scrollTop = (saved.percent / 100) * (el.scrollHeight - el.clientHeight);
		}
	}

	function onTextScroll() {
		if (!textEl || !source) return;
		const max = textEl.scrollHeight - textEl.clientHeight;
		const nextPercent = max > 0 ? (textEl.scrollTop / max) * 100 : 0;
		locationLabel = `${Math.round(nextPercent)}%`;
		scheduleSave('TXT', String(Math.round(textEl.scrollTop)), nextPercent);
	}

	function setupPdf(src: ReaderSource, saved: ReadingProgress | null) {
		pdfStarted = true;
		let url = convertFileSrc(src.path);
		if (saved?.format === 'PDF' && saved.location) {
			const page = Number(saved.location);
			if (!Number.isNaN(page) && page > 1) {
				url = `${url}#page=${page}`;
			}
			percent = saved.percent;
		}
		pdfUrl = url;
		locationLabel = 'PDF';
		ready = true;
		scheduleSave('PDF', saved?.location || '1', percent || 0);
	}

	async function showComicPage(index: number, format: string) {
		if (index < 0) return;
		if (comicPageCount > 0 && index >= comicPageCount) return;

		let page = comicCache.get(index);
		if (!page) {
			page = await getComicPage(bookId, index);
			comicCache.set(index, page);
		}

		comicPageIndex = page.index;
		comicPageCount = page.page_count;
		comicDataUrl = page.data_url;
		comicPageName = page.name;
		locationLabel = `Page ${page.index + 1} of ${page.page_count}`;
		const nextPercent =
			page.page_count > 0 ? ((page.index + 1) / page.page_count) * 100 : 0;
		scheduleSave(format, String(page.index), nextPercent);
		ready = true;

		const next = page.index + 1;
		if (next < page.page_count && !comicCache.has(next)) {
			void getComicPage(bookId, next)
				.then((p) => {
					comicCache.set(next, p);
				})
				.catch(() => {
					/* preload is best-effort */
				});
		}
	}

	async function setupComic(src: ReaderSource, saved: ReadingProgress | null) {
		comicStarted = true;
		comicCache.clear();
		const list = await listComicPages(bookId);
		comicPageCount = list.page_count;

		let index = 0;
		if (saved && saved.format === src.format && saved.location) {
			const parsed = Number.parseInt(saved.location, 10);
			if (!Number.isNaN(parsed) && parsed >= 0 && parsed < list.page_count) {
				index = parsed;
			}
		}

		await showComicPage(index, src.format);
	}

	async function goPrev() {
		if (rendition) {
			await rendition.prev();
			return;
		}
		if (source && isComicFormat(source.format) && comicPageIndex > 0) {
			await showComicPage(comicPageIndex - 1, source.format);
		}
	}

	async function goNext() {
		if (rendition) {
			await rendition.next();
			return;
		}
		if (source && isComicFormat(source.format) && comicPageIndex + 1 < comicPageCount) {
			await showComicPage(comicPageIndex + 1, source.format);
		}
	}

	function onKeydown(event: KeyboardEvent) {
		const fmt = source?.format;
		if (fmt !== 'EPUB' && !isComicFormat(fmt ?? '')) return;
		if (event.key === 'ArrowLeft') {
			event.preventDefault();
			void goPrev();
		} else if (event.key === 'ArrowRight') {
			event.preventDefault();
			void goNext();
		}
	}

	async function onTitlebarDblClick() {
		try {
			await getCurrentWindow().toggleMaximize();
		} catch {
			/* browser preview */
		}
	}

	onMount(() => {
		chromeOs = detectChromeOs();
		contentTheme = readContentTheme();
		readerFont = getStoredReaderFont();

		const root = document.documentElement;
		const themeObserver = new MutationObserver(() => {
			const next = readContentTheme();
			if (next === contentTheme) return;
			contentTheme = next;
			applyEpubReaderStyles(next, readerFont);
		});
		themeObserver.observe(root, { attributes: true, attributeFilter: ['data-theme'] });

		const onStorage = (event: StorageEvent) => {
			if (event.key === 'grimoire-theme') {
				applyTheme(getStoredTheme());
				contentTheme = readContentTheme();
				applyEpubReaderStyles(contentTheme, readerFont);
				return;
			}
			if (event.key === 'grimoire-reader-font') {
				readerFont = getStoredReaderFont();
				applyEpubReaderStyles(contentTheme, readerFont);
			}
		};
		window.addEventListener('storage', onStorage);

		return () => {
			themeObserver.disconnect();
			window.removeEventListener('storage', onStorage);
		};
	});

	$effect(() => {
		const id = bookId;
		if (!Number.isFinite(id) || id <= 0) {
			error = 'Invalid book id.';
			loading = false;
			return;
		}

		let cancelled = false;
		void (async () => {
			loading = true;
			error = '';
			ready = false;
			textContent = '';
			pdfUrl = null;
			teardownEpub();
			resetComic();
			textStarted = false;
			pdfStarted = false;
			try {
				const nextSource = await getReaderSource(id);
				const nextProgress = await getEbookReadingProgress(id);
				if (cancelled) return;
				source = nextSource;
				savedProgress = nextProgress;
				percent = nextProgress?.percent ?? 0;
				await getCurrentWindow().setTitle(`${nextSource.title} · Grimoire`);
			} catch (e) {
				if (!cancelled) error = String(e);
			} finally {
				if (!cancelled) loading = false;
			}
		})();

		return () => {
			cancelled = true;
		};
	});

	$effect(() => {
		const src = source;
		const saved = savedProgress;
		const el = viewerEl;
		if (loading || error || !src || src.format !== 'EPUB' || !el || epubStarted) return;
		void setupEpub(el, src, saved).catch((e) => {
			error = String(e);
		});
	});

	$effect(() => {
		const src = source;
		const saved = savedProgress;
		if (loading || error || !src || src.format !== 'TXT' || textStarted) return;
		void setupText(src, saved).catch((e) => {
			error = String(e);
		});
	});

	$effect(() => {
		const el = textEl;
		const saved = savedProgress;
		if (!el || !textContent || source?.format !== 'TXT') return;
		requestAnimationFrame(() => restoreTextScroll(el, saved));
	});

	$effect(() => {
		const src = source;
		const saved = savedProgress;
		if (loading || error || !src || src.format !== 'PDF' || pdfStarted) return;
		setupPdf(src, saved);
	});

	$effect(() => {
		const src = source;
		const saved = savedProgress;
		if (loading || error || !src || !isComicFormat(src.format) || comicStarted) return;
		void setupComic(src, saved).catch((e) => {
			error = String(e);
		});
	});

	onDestroy(() => {
		destroyed = true;
		clearTimeout(saveTimer);
		teardownEpub();
		resetComic();
	});
</script>

<svelte:head>
	<title>{source?.title ? `${source.title} · Grimoire` : 'Reading · Grimoire'}</title>
</svelte:head>

<svelte:window onkeydown={onKeydown} />

<div class="reader-shell">
	<header class="reader-bar" data-chrome={chromeOs}>
		{#if controlsLeft}
			<WindowControls />
		{/if}
		<div
			class="bar-left"
			data-tauri-drag-region
			ondblclick={onTitlebarDblClick}
			role="presentation"
		>
			<p class="title" data-tauri-drag-region>{source?.title ?? 'Reading'}</p>
			{#if source}
				<p class="meta muted" data-tauri-drag-region>
					{source.authors.join(', ') || 'Unknown author'} · {formatLabel}
					{#if locationLabel}
						· {locationLabel}
					{/if}
				</p>
			{/if}
		</div>
		<div class="bar-right">
			{#if (source?.format === 'EPUB' || (source && isComicFormat(source.format))) && ready}
				<Button variant="secondary" onclick={goPrev}>Previous</Button>
				<Button variant="secondary" onclick={goNext}>Next</Button>
			{/if}
			<span class="percent" aria-live="polite">{Math.round(percent)}%</span>
		</div>
		{#if !controlsLeft}
			<WindowControls />
		{/if}
	</header>

	{#if error}
		<main class="page">
			<p class="error">{error}</p>
		</main>
	{:else if loading}
		<main class="page">
			<p class="muted">Opening book…</p>
		</main>
	{:else if source?.format === 'EPUB'}
		<div class="viewer" bind:this={viewerEl}></div>
	{:else if source?.format === 'TXT'}
		<article class="text-view" bind:this={textEl} onscroll={onTextScroll}>
			<pre style:font-family={textFontFamily}>{textContent || 'Loading text…'}</pre>
		</article>
	{:else if source?.format === 'PDF' && pdfUrl}
		<iframe class="pdf-frame" title={source.title} src={pdfUrl}></iframe>
		<p class="muted pdf-note">
			PDF page tracking is limited. Grimoire remembers that you opened this file.
		</p>
	{:else if source && isComicFormat(source.format)}
		<div class="comic-view">
			{#if comicDataUrl}
				<img
					src={comicDataUrl}
					alt={comicPageName || `Page ${comicPageIndex + 1}`}
				/>
			{:else}
				<p class="muted">Loading page…</p>
			{/if}
		</div>
	{/if}
</div>

<style>
	.reader-shell {
		height: 100vh;
		display: flex;
		flex-direction: column;
		background: var(--page-bg);
		color: var(--ink);
		border-radius: var(--window-radius);
		overflow: auto;
		overflow-x: hidden;
	}

	:global(html.window-maximized) .reader-shell {
		border-radius: 0;
	}

	.reader-bar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		padding: 0.7rem 1rem;
		border-bottom: 1px solid var(--line);
		background: var(--topbar-bg);
		backdrop-filter: blur(8px);
		position: sticky;
		top: 0;
		z-index: 5;
	}

	.reader-bar[data-chrome='macos'] {
		padding-left: 0.85rem;
		gap: 0.65rem;
	}

	.reader-bar[data-chrome='windows'] {
		padding-right: 0;
		padding-top: 0;
		padding-bottom: 0;
		min-height: 2.75rem;
	}

	.reader-bar[data-chrome='linux'] {
		padding-right: 0.35rem;
	}

	.bar-left {
		min-width: 0;
		flex: 1 1 auto;
	}

	.reader-bar[data-chrome='windows'] .bar-left,
	.reader-bar[data-chrome='windows'] .bar-right {
		padding-block: 0.7rem;
	}

	.title {
		margin: 0;
		font-family: var(--font-display);
		font-size: 1.1rem;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.meta {
		margin: 0.15rem 0 0;
		font-family: var(--font-ui);
		font-size: 0.82rem;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.bar-right {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		flex-shrink: 0;
	}

	.percent {
		font-family: var(--font-ui);
		font-size: 0.9rem;
		min-width: 3rem;
		text-align: right;
		color: var(--ink-muted);
	}

	.viewer {
		flex: 1 1 auto;
		min-height: 0;
		height: calc(100vh - 4.2rem);
		background: var(--paper);
	}

	.text-view {
		flex: 1 1 auto;
		min-height: 0;
		height: calc(100vh - 4.2rem);
		overflow: auto;
		padding: 1.5rem clamp(1rem, 4vw, 3rem) 3rem;
		background: var(--paper);
		color: var(--ink);
	}

	.text-view pre {
		margin: 0 auto;
		max-width: 42rem;
		white-space: pre-wrap;
		word-break: break-word;
		font-family: var(--font-body);
		font-size: 1.08rem;
		line-height: 1.65;
		color: var(--ink);
		background: transparent;
	}

	.pdf-frame {
		flex: 1 1 auto;
		width: 100%;
		height: calc(100vh - 5.5rem);
		border: 0;
		background: var(--panel);
	}

	.pdf-note {
		margin: 0;
		padding: 0.35rem 1rem 0.6rem;
		font-family: var(--font-ui);
		font-size: 0.82rem;
	}

	.comic-view {
		flex: 1 1 auto;
		min-height: 0;
		height: calc(100vh - 4.2rem);
		overflow: auto;
		display: flex;
		align-items: flex-start;
		justify-content: center;
		background: color-mix(in srgb, var(--panel) 92%, transparent);
	}

	.comic-view img {
		display: block;
		width: 100%;
		height: auto;
	}

	.page {
		padding: 1.5rem;
	}
</style>
