import config from "./config.json";
import type { SignalingClient } from "./signaling";

export type CallEvents = {
    onRemoteTrack: (stream: MediaStream) => void;
    onConnectionState: (state: RTCPeerConnectionState) => void;
};

export class WebRTCManager {
    private peerConnection: RTCPeerConnection;
    private localStream: MediaStream | null = null;
    private readonly signaling: SignalingClient;
    private readonly events: CallEvents;
    private remoteStream: MediaStream;

    constructor(signaling: SignalingClient, events: CallEvents, iceServers?: RTCIceServer[]) {
        this.signaling = signaling;
        this.events = events;

        this.peerConnection = new RTCPeerConnection({
            iceServers: iceServers ?? config.iceServers.map((url) => ({ urls: url })),
        });
        this.remoteStream = new MediaStream();

        this.peerConnection.ontrack = (ev) => {
            ev.streams[0]?.getTracks().forEach((track) => {
                this.remoteStream.addTrack(track);
            });
            this.events.onRemoteTrack(this.remoteStream);
        };

        this.peerConnection.onconnectionstatechange = () => {
            this.events.onConnectionState(this.peerConnection.connectionState);
        };
    }

    async acquireMic(): Promise<void> {
        this.localStream = await navigator.mediaDevices.getUserMedia({
            audio: {
                echoCancellation: true,
                noiseSuppression: true,
                autoGainControl: true,
            },
            video: false,
        });
        this.localStream.getTracks().forEach((track) => {
            // biome-ignore lint/style/noNonNullAssertion: see just above
            this.peerConnection.addTrack(track, this.localStream!);
        });
    }

    async createOffer(): Promise<void> {
        const offer = await this.peerConnection.createOffer();
        await this.peerConnection.setLocalDescription(offer);

        // Wait for ICE gathering to finish so all candidates are bundled
        // in the offer SDP. The check guards against the rare case where
        // gathering is already complete before we attach the listener.
        await new Promise<void>((resolve) => {
            if (this.peerConnection.iceGatheringState === "complete") {
                resolve();
                return;
            }
            const handler = () => {
                if (this.peerConnection.iceGatheringState === "complete") {
                    this.peerConnection.removeEventListener("icegatheringstatechange", handler);
                    resolve();
                }
            };
            this.peerConnection.addEventListener("icegatheringstatechange", handler);
        });

        this.signaling.send({
            type: "offer",
            // biome-ignore lint/style/noNonNullAssertion: it's set just above
            sdp: this.peerConnection.localDescription!.sdp,
        });
    }

    async applyAnswer(sdp: string): Promise<void> {
        await this.peerConnection.setRemoteDescription({ type: "answer", sdp });
    }

    toggleMute(): boolean {
        if (!this.localStream) return false;
        const tracks = this.localStream.getAudioTracks();
        if (tracks.length === 0) return false;
        // biome-ignore lint/style/noNonNullAssertion: we check tracks.length above
        const newEnabled = !tracks[0]!.enabled;
        tracks.forEach((track) => {
            track.enabled = newEnabled;
        });
        return !newEnabled;
    }

    hangup(): void {
        this.localStream?.getTracks().forEach((track) => {
            track.stop();
        });
        this.localStream = null;
        try {
            this.peerConnection.close();
        } catch {}
    }
}
