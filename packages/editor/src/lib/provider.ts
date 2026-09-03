import * as Y from "yjs";

/**
 * Frame tags. Must match `notedock_server::ws`.
 *
 * Deliberately not the y-websocket wire format: both ends of this protocol are
 * ours, and three message types are easier to keep provably in step than
 * matching someone else's varint encoding.
 */
const MSG_STATE_VECTOR = 1;
const MSG_DIFF = 2;
const MSG_UPDATE = 3;

/** Reconnect backoff, doubling from the first to the second. */
const RETRY_MIN_MS = 500;
const RETRY_MAX_MS = 15_000;

export type ConnectionState = "connecting" | "live" | "offline";

export interface ProviderOptions {
  doc: Y.Doc;
  /**
   * Resolves to an absolute `ws://`/`wss://` URL carrying a fresh single-use
   * ticket. Called again for every reconnect, because a ticket is spent the
   * moment it is used — and because a fresh one also re-checks that the session
   * is still valid.
   */
  ticket: () => Promise<string>;
  noteId: string;
  onState?: (state: ConnectionState) => void;
}

/**
 * Keeps one Y.Doc in step with the server over a WebSocket.
 *
 * Offline editing needs no queue of its own: a Yjs document *is* the queue. On
 * reconnect the two sides exchange state vectors and each sends only what the
 * other is missing, so an edit made with the network down is indistinguishable
 * from one made a moment before it dropped.
 */
export class NoteProvider {
  #options: ProviderOptions;
  #socket: WebSocket | null = null;
  #retry = RETRY_MIN_MS;
  #timer: ReturnType<typeof setTimeout> | null = null;
  #destroyed = false;
  #state: ConnectionState = "connecting";

  /**
   * Relays a local edit to the server.
   *
   * A field rather than a method so the same reference can be handed to both
   * `doc.on` and `doc.off` — a private method cannot be bound and reassigned.
   * `origin === this` marks updates this provider applied itself, which is what
   * stops a received update from being echoed straight back.
   */
  #onLocalUpdate = (update: Uint8Array, origin: unknown): void => {
    if (origin === this) return;
    this.#send(MSG_UPDATE, update);
  };

  constructor(options: ProviderOptions) {
    this.#options = options;
    options.doc.on("update", this.#onLocalUpdate);
    // The provider starts in `connecting`; publish that initial state too.
    // Without this first notification, hosts that initialize their own state to
    // `offline` show a false offline dot until the WebSocket eventually opens.
    options.onState?.(this.#state);
    void this.#connect();
  }

  get state(): ConnectionState {
    return this.#state;
  }

  destroy(): void {
    this.#destroyed = true;
    this.#options.doc.off("update", this.#onLocalUpdate);
    if (this.#timer) clearTimeout(this.#timer);
    // 1000 = normal closure, so the server evicts the room cleanly rather than
    // waiting for a timeout.
    this.#socket?.close(1000);
    this.#socket = null;
  }

  #setState(state: ConnectionState): void {
    if (this.#state === state) return;
    this.#state = state;
    this.#options.onState?.(state);
  }

  async #connect(): Promise<void> {
    if (this.#destroyed) return;
    this.#setState("connecting");

    let url: string;
    try {
      url = await this.#options.ticket();
    } catch {
      // No ticket means no session, or no server. Either way: retry.
      this.#scheduleReconnect();
      return;
    }
    if (this.#destroyed) return;

    const socket = new WebSocket(`${url}&note=${encodeURIComponent(this.#options.noteId)}`);
    socket.binaryType = "arraybuffer";
    this.#socket = socket;

    socket.onopen = () => {
      this.#retry = RETRY_MIN_MS;
      this.#setState("live");
      // Tell the server what we have; it answers with the difference.
      this.#send(MSG_STATE_VECTOR, Y.encodeStateVector(this.#options.doc));
    };

    socket.onmessage = (event) => this.#receive(event.data as ArrayBuffer);

    socket.onclose = () => {
      if (this.#socket === socket) this.#socket = null;
      this.#setState("offline");
      this.#scheduleReconnect();
    };

    // `onclose` always follows `onerror`, so reconnecting is handled there.
    socket.onerror = () => socket.close();
  }

  #scheduleReconnect(): void {
    if (this.#destroyed || this.#timer) return;
    const delay = this.#retry;
    this.#retry = Math.min(this.#retry * 2, RETRY_MAX_MS);
    this.#timer = setTimeout(() => {
      this.#timer = null;
      void this.#connect();
    }, delay);
  }

  #send(kind: number, payload: Uint8Array): void {
    const socket = this.#socket;
    if (socket?.readyState !== WebSocket.OPEN) return;

    const frame = new Uint8Array(payload.length + 1);
    frame[0] = kind;
    frame.set(payload, 1);
    socket.send(frame);
  }

  #receive(data: ArrayBuffer): void {
    const bytes = new Uint8Array(data);
    if (bytes.length === 0) return;

    const kind = bytes[0];
    const body = bytes.subarray(1);

    switch (kind) {
      case MSG_STATE_VECTOR: {
        // The server's state vector: send what it lacks, then ask for what we
        // lack. Both directions are needed — neither side knows the other's
        // history after a disconnect.
        this.#send(MSG_DIFF, Y.encodeStateAsUpdate(this.#options.doc, body));
        this.#send(MSG_STATE_VECTOR, Y.encodeStateVector(this.#options.doc));
        break;
      }
      case MSG_DIFF:
      case MSG_UPDATE:
        // Tagging the origin with `this` is what stops the echo: the update
        // handler above ignores anything this provider applied.
        Y.applyUpdate(this.#options.doc, body, this);
        break;
    }
  }
}
