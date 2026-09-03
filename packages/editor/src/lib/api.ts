import type {
  ApiErrorBody,
  CreateNoteRequest,
  ErrorCode,
  LoginResponse,
  NoteSummary,
  SyncResponse,
  TicketResponse,
} from "../generated/api";

/** A response the server rejected, carrying the structured body it sent. */
export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly body: ApiErrorBody,
  ) {
    super(body.message);
    this.name = "ApiError";
  }

  get code(): ErrorCode {
    return this.body.code;
  }
}

/** The request never reached the server — offline, DNS, refused connection. */
export class NetworkError extends Error {
  constructor(cause: unknown) {
    super("无法连接到服务器");
    this.name = "NetworkError";
    this.cause = cause;
  }
}

/**
 * Thin wrapper over the HTTP API used by the browser client.
 *
 * Only metadata crosses this client: listing, creating and deleting notes, plus
 * the ticket that opens a document socket. Note *bodies* never appear here —
 * they live in Yjs documents synced over the WebSocket, which is what lets two
 * devices edit one note without either losing an edit.
 *
 * The desktop app deliberately does not use this: its requests go through Rust,
 * which keeps the bearer token out of the webview.
 */
export class NoteDockClient {
  #baseUrl: string;
  #token: string | null;

  constructor(baseUrl = "", token: string | null = null) {
    // Trailing slashes would produce `//api/v1/...`, which some proxies redirect.
    this.#baseUrl = baseUrl.replace(/\/+$/, "");
    this.#token = token;
  }

  get token(): string | null {
    return this.#token;
  }

  setToken(token: string | null): void {
    this.#token = token;
  }

  async login(password: string, label?: string): Promise<LoginResponse> {
    const res = await this.#request<LoginResponse>("POST", "/auth/login", {
      password,
      label: label ?? null,
    });
    this.#token = res.token;
    return res;
  }

  listNotes(): Promise<NoteSummary[]> {
    return this.#request<NoteSummary[]>("GET", "/notes");
  }

  createNote(body: CreateNoteRequest): Promise<NoteSummary> {
    return this.#request<NoteSummary>("POST", "/notes", body);
  }

  getNote(id: string): Promise<NoteSummary> {
    return this.#request<NoteSummary>("GET", `/notes/${encodeURIComponent(id)}`);
  }

  async deleteNote(id: string): Promise<void> {
    await this.#request<null>("DELETE", `/notes/${encodeURIComponent(id)}`);
  }

  sync(since: number): Promise<SyncResponse> {
    return this.#request<SyncResponse>("GET", `/sync?since=${since}`);
  }

  /**
   * Trades the bearer token for a single-use WebSocket URL.
   *
   * Needed because `new WebSocket(...)` cannot set an `Authorization` header, so
   * something has to travel in the URL. What travels is a ticket that expires in
   * seconds, never the month-long token.
   */
  async socketUrl(): Promise<string> {
    const { url } = await this.#request<TicketResponse>("POST", "/ws-ticket");
    return url;
  }

  async #request<T>(method: string, path: string, body?: unknown): Promise<T> {
    const headers: Record<string, string> = {};
    if (this.#token) headers.authorization = `Bearer ${this.#token}`;
    if (body !== undefined) headers["content-type"] = "application/json";

    let res: Response;
    try {
      res = await fetch(`${this.#baseUrl}/api/v1${path}`, {
        method,
        headers,
        body: body === undefined ? undefined : JSON.stringify(body),
      });
    } catch (cause) {
      throw new NetworkError(cause);
    }

    if (res.status === 204) return null as T;

    const payload = await res.text();
    const parsed: unknown = payload ? safeParse(payload) : null;

    if (!res.ok) {
      throw new ApiError(res.status, asErrorBody(parsed, res.status));
    }

    return parsed as T;
  }
}

function safeParse(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

/**
 * Every handler returns {@link ApiErrorBody}, but a reverse proxy or a crash can
 * still produce a bare 502 with an HTML body — so failures are normalised here
 * rather than trusted.
 */
function asErrorBody(parsed: unknown, status: number): ApiErrorBody {
  if (parsed && typeof parsed === "object" && "code" in parsed && "message" in parsed) {
    return parsed as ApiErrorBody;
  }
  return {
    code: status === 401 ? "unauthorized" : "internal",
    message: `服务器返回 ${status}`,
  };
}
