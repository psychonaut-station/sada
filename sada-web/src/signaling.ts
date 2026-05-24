export type ClientMessage =
    | { type: "offer"; sdp: string }
    | { type: "answer"; sdp: string }
    | { type: "close" };

export type TrackMapEntry = {
    speakerId: string | null;
    mid: string | null;
};

export type ServerMessage =
    | { type: "answer"; sdp: string }
    | { type: "offer"; sdp: string }
    | { type: "track_map"; tracks: TrackMapEntry[] }
    | { type: "close" };

export type SignalingHandlers = {
    onServerMessage: (msg: ServerMessage) => void;
    onOpen?: () => void;
    onClose?: (e: CloseEvent) => void;
    onError?: (e: Event) => void;
};

export class SignalingClient {
    private webSocket: WebSocket | null = null;
    private readonly url: string;
    private readonly handlers: SignalingHandlers;

    constructor(baseUrl: string, token: string, handlers: SignalingHandlers) {
        const url = new URL(baseUrl);
        url.searchParams.set("token", token);
        this.url = url.toString();
        this.handlers = handlers;
    }

    connect(): Promise<void> {
        return new Promise((resolve, reject) => {
            const webSocket = new WebSocket(this.url);
            this.webSocket = webSocket;
            webSocket.onopen = () => {
                this.handlers.onOpen?.();
                resolve();
            };
            webSocket.onmessage = (ev) => {
                const msg = this.onServerMessage(typeof ev.data === "string" ? ev.data : "");
                if (!msg) {
                    console.error("Failed to parse signaling message", ev.data);
                    return;
                }
                this.handlers.onServerMessage(msg);
            };
            webSocket.onclose = (ev) => this.handlers.onClose?.(ev);
            webSocket.onerror = (ev) => {
                this.handlers.onError?.(ev);
                if (webSocket.readyState !== WebSocket.OPEN)
                    reject(new Error("WebSocket connect failed"));
            };
        });
    }

    send(msg: ClientMessage): void {
        if (!this.webSocket || this.webSocket.readyState !== WebSocket.OPEN) {
            throw new Error("signaling: not connected");
        }
        this.webSocket.send(JSON.stringify(msg));
    }

    close(): void {
        if (this.webSocket && this.webSocket.readyState === WebSocket.OPEN) {
            try {
                this.webSocket.send(JSON.stringify({ type: "close" } satisfies ClientMessage));
            } catch {}
        }
        this.webSocket?.close();
        this.webSocket = null;
    }

    get state(): number {
        return this.webSocket?.readyState ?? WebSocket.CLOSED;
    }

    private onServerMessage(raw: string): ServerMessage | null {
        let rawObj: unknown;
        try {
            rawObj = JSON.parse(raw);
        } catch {
            return null;
        }

        if (typeof rawObj !== "object" || rawObj === null) return null;
        const obj = rawObj as Record<string, unknown>;
        if (typeof obj.type !== "string") return null;

        const str = (key: string): string | undefined =>
            typeof obj[key] === "string" ? (obj[key] as string) : undefined;

        switch (str("type")) {
            case "answer": {
                const sdp = str("sdp");
                if (!sdp) return null;
                return { type: "answer", sdp };
            }
            case "offer": {
                const sdp = str("sdp");
                if (!sdp) return null;
                return { type: "offer", sdp };
            }
            case "track_map": {
                if (!Array.isArray(obj.tracks)) return null;
                const tracks: TrackMapEntry[] = [];
                for (const track of obj.tracks) {
                    if (typeof track !== "object" || track === null) return null;
                    const entry = track as Record<string, unknown>;
                    const speakerId = entry.speakerId;
                    const mid = entry.mid;
                    if (speakerId !== null && typeof speakerId !== "string") return null;
                    if (mid !== null && typeof mid !== "string") return null;
                    tracks.push({ speakerId, mid });
                }
                return { type: "track_map", tracks };
            }
            case "close":
                return { type: "close" };
            default:
                return null;
        }
    }
}
