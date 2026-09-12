export type ReaderFontId = 'publisher' | 'serif' | 'sans' | 'opendyslexic';

export type ReaderFontOption = {
	id: ReaderFontId;
	label: string;
	description: string;
	/** CSS font-family stack, or null to keep publisher fonts in EPUB. */
	cssFamily: string | null;
};

const STORAGE_KEY = 'grimoire-reader-font';

export const READER_FONTS: ReaderFontOption[] = [
	{
		id: 'publisher',
		label: 'Publisher default',
		description: "Keep the ebook's own fonts when possible.",
		cssFamily: null
	},
	{
		id: 'serif',
		label: 'Source Serif',
		description: 'Grimoire’s reading serif.',
		cssFamily: "'Source Serif 4', Georgia, 'Times New Roman', serif"
	},
	{
		id: 'sans',
		label: 'Source Sans',
		description: 'Clean sans-serif for long stretches of text.',
		cssFamily: "'Source Sans 3', 'Segoe UI', system-ui, sans-serif"
	},
	{
		id: 'opendyslexic',
		label: 'OpenDyslexic',
		description: 'Shapes designed to help with some symptoms of dyslexia.',
		cssFamily: "'OpenDyslexic', 'Source Sans 3', sans-serif"
	}
];

export function isReaderFontId(value: string | null | undefined): value is ReaderFontId {
	return READER_FONTS.some((font) => font.id === value);
}

export function getReaderFontOption(id: ReaderFontId): ReaderFontOption {
	return READER_FONTS.find((font) => font.id === id) ?? READER_FONTS[0];
}

export function getStoredReaderFont(): ReaderFontId {
	if (typeof localStorage === 'undefined') return 'serif';
	const value = localStorage.getItem(STORAGE_KEY);
	return isReaderFontId(value) ? value : 'serif';
}

export function setStoredReaderFont(id: ReaderFontId): ReaderFontId {
	if (typeof localStorage !== 'undefined') {
		localStorage.setItem(STORAGE_KEY, id);
	}
	return id;
}

/** Absolute @font-face CSS for injecting into EPUB iframes. */
export function openDyslexicFontFaceCss(origin = typeof window !== 'undefined' ? window.location.origin : '') {
	const base = `${origin}/fonts/opendyslexic`;
	return `
@font-face {
  font-family: 'OpenDyslexic';
  font-style: normal;
  font-weight: 400;
  font-display: swap;
  src: url('${base}/OpenDyslexic-Regular.woff2') format('woff2');
}
@font-face {
  font-family: 'OpenDyslexic';
  font-style: italic;
  font-weight: 400;
  font-display: swap;
  src: url('${base}/OpenDyslexic-Italic.woff2') format('woff2');
}
@font-face {
  font-family: 'OpenDyslexic';
  font-style: normal;
  font-weight: 700;
  font-display: swap;
  src: url('${base}/OpenDyslexic-Bold.woff2') format('woff2');
}
@font-face {
  font-family: 'OpenDyslexic';
  font-style: italic;
  font-weight: 700;
  font-display: swap;
  src: url('${base}/OpenDyslexic-BoldItalic.woff2') format('woff2');
}
`.trim();
}
