import type { SignalingClient } from "./signaling";
import config from "./config.json";

export type CallEvents = {
    onRemoteTrack: (stream: MediaStream) => void;
    onConnectionState: (state: RTCPeerConnectionState) => void;
};

export class WebRTCManager {
    private pc: RTCPeerConnection;
    private localStream: MediaStream | null = null;
    private readonly signaling: SignalingClient;
    private readonly events: CallEvents;
    private remoteStream: MediaStream;

    constructor(signaling: SignalingClient, events: CallEvents, iceServers?: RTCIceServer[]) {
        this.signaling = signaling;
        this.events = events;
        this.pc = new RTCPeerConnection({
            iceServers: iceServers ?? config.iceServers.map(url => ({ urls: url })),
        });
        this.remoteStream = new MediaStream();

        this.pc.ontrack = (ev) => {
            ev.streams[0]?.getTracks().forEach((t) => this.remoteStream.addTrack(t));
            this.events.onRemoteTrack(this.remoteStream);
        };

        this.pc.onconnectionstatechange = () => {
            this.events.onConnectionState(this.pc.connectionState);
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
        this.localStream.getTracks().forEach((t) => this.pc.addTrack(t, this.localStream!));
    }

    async createOffer(): Promise<void> {
        const offer = await this.pc.createOffer();
        await this.pc.setLocalDescription(offer);

        if (this.pc.iceGatheringState !== "complete") {
            await new Promise<void>((resolve) => {
                const done = () => {
                    this.pc.removeEventListener("icegatheringstatechange", done);
                    resolve();
                };
                this.pc.addEventListener("icegatheringstatechange", done);
            });
        }

        this.signaling.send({ type: "offer", sdp: this.pc.localDescription?.sdp ?? "" });
    }

    async applyAnswer(sdp: string): Promise<void> {
        await this.pc.setRemoteDescription({ type: "answer", sdp });
    }

    toggleMute(): boolean {
        if (!this.localStream) return false;
        const tracks = this.localStream.getAudioTracks();
        if (tracks.length === 0) return false;
        const newEnabled = !tracks[0]!.enabled;
        tracks.forEach((t) => (t.enabled = newEnabled));
        return !newEnabled;
    }

    hangup(): void {
        this.localStream?.getTracks().forEach((t) => t.stop());
        this.localStream = null;
        this.pc.getSenders().forEach((s) => s.track && s.track.stop());
        try {
            this.pc.close();
        } catch {}
    }
}
