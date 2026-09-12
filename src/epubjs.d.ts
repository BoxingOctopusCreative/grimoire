declare module 'epubjs' {
	const ePub: (url: string | ArrayBuffer, options?: Record<string, unknown>) => {
		renderTo: (
			element: HTMLElement | string,
			options?: Record<string, unknown>
		) => {
			display: (target?: string) => Promise<unknown>;
			prev: () => Promise<unknown>;
			next: () => Promise<unknown>;
			on: (event: string, callback: (...args: unknown[]) => void) => void;
			currentLocation: () => {
				start?: { cfi?: string; displayed?: { page?: number; total?: number } };
				atEnd?: boolean;
			};
			destroy?: () => void;
		};
		locations: {
			generate: (chars: number) => Promise<unknown>;
			locationFromCfi: (cfi: string) => number;
			length: () => number;
		};
		destroy?: () => void;
	};
	export default ePub;
}
