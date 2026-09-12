import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { listEbookReaders, type EbookReaderDevice } from '$lib/api';

export const EREADER_PRESENCE_EVENT = 'ereader-presence';

export type PresenceChange = 'arrived' | 'left' | 'snapshot';

export type EreaderPresenceEvent = {
	change: PresenceChange;
	label: string | null;
	device_id: string | null;
	devices: EbookReaderDevice[];
};

export type DeviceNotice = {
	kind: 'arrived' | 'left';
	message: string;
};

/** Shared connected-reader state for the whole app shell. */
export const devicePresence = $state({
	devices: [] as EbookReaderDevice[],
	notice: null as DeviceNotice | null,
	listening: false
});

let unlisten: UnlistenFn | null = null;
let noticeTimer: ReturnType<typeof setTimeout> | null = null;
const listeners = new Set<(event: EreaderPresenceEvent) => void>();

function applyEvent(event: EreaderPresenceEvent) {
	devicePresence.devices = event.devices;

	if (event.change === 'arrived' || event.change === 'left') {
		const name = event.label?.trim() || 'eReader';
		const message =
			event.change === 'arrived' ? `${name} connected` : `${name} disconnected`;

		if (noticeTimer) clearTimeout(noticeTimer);
		devicePresence.notice = { kind: event.change, message };
		noticeTimer = setTimeout(() => {
			devicePresence.notice = null;
			noticeTimer = null;
		}, 5000);
	}

	for (const listener of listeners) {
		listener(event);
	}
}

export function onEreaderPresence(listener: (event: EreaderPresenceEvent) => void) {
	listeners.add(listener);
	return () => listeners.delete(listener);
}

export function dismissDeviceNotice() {
	if (noticeTimer) clearTimeout(noticeTimer);
	noticeTimer = null;
	devicePresence.notice = null;
}

export async function startDevicePresenceListener() {
	if (devicePresence.listening) return;
	devicePresence.listening = true;

	try {
		unlisten = await listen<EreaderPresenceEvent>(EREADER_PRESENCE_EVENT, (event) => {
			applyEvent(event.payload);
		});
		try {
			devicePresence.devices = await listEbookReaders();
		} catch {
			devicePresence.devices = [];
		}
	} catch (e) {
		devicePresence.listening = false;
		console.error('Could not listen for eReader presence', e);
	}
}

export function stopDevicePresenceListener() {
	unlisten?.();
	unlisten = null;
	devicePresence.listening = false;
	listeners.clear();
	if (noticeTimer) clearTimeout(noticeTimer);
	noticeTimer = null;
}
