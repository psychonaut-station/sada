export type ClientMessage =
    | { type: "offer"; sdp: string }
    | { type: "ice"; candidate: string }
    | { type: "close" };

export type ServerMessage =
    | { type: "offer"; call_id: string; sdp: string }
    | { type: "answer"; sdp: string }
    | { type: "ice"; candidate: string }
    | { type: "close" };

export type SignalingHandlers = {
    onServerMessage: (msg: ServerMessage) => void;
    onOpen?: () => void;
    onClose?: (e: CloseEvent) => void;
    onError?: (e: Event) => void;
};

export class SignalingClient {
    private ws: WebSocket | null = null;
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
            const ws = new WebSocket(this.url);
            this.ws = ws;
            ws.onopen = () => {
                this.handlers.onOpen?.();
                resolve();
            };
            ws.onmessage = (ev) => {
                const msg = this.onServerMessage(typeof ev.data === "string" ? ev.data : "");
                if (!msg) {
                    console.error("Failed to parse signaling message", ev.data);
                    return;
                }
                this.handlers.onServerMessage(msg);
            };
            ws.onclose = (ev) => this.handlers.onClose?.(ev);
            ws.onerror = (ev) => {
                this.handlers.onError?.(ev);
                if (ws.readyState !== WebSocket.OPEN) reject(new Error("WebSocket connect failed"));
            };
        });
    }

    send(msg: ClientMessage): void {
        if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
            throw new Error("signaling: not connected");
        }
        this.ws.send(JSON.stringify(msg));
    }

    close(): void {
        if (this.ws && this.ws.readyState === WebSocket.OPEN) {
            try {
                this.ws.send(JSON.stringify({ type: "close" } satisfies ClientMessage));
            } catch {}
        }
        this.ws?.close();
        this.ws = null;
    }

    get state(): number {
        return this.ws?.readyState ?? WebSocket.CLOSED;
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
            case "offer": {
                const call_id = str("call_id");
                const sdp = str("sdp");
                if (!call_id || !sdp) return null;
                return { type: "offer", call_id, sdp };
            }
            case "answer": {
                const sdp = str("sdp");
                if (!sdp) return null;
                return { type: "answer", sdp };
            }
            case "ice": {
                const candidate = str("candidate");
                if (!candidate) return null;
                return { type: "ice", candidate };
            }
            case "close":
                return { type: "close" };
            default:
                return null;
        }
    }
}
