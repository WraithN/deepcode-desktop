const OPENCODE_BASE_URL = 'http://127.0.0.1:3001';

interface Session {
  id: string;
  slug: string;
  title: string;
  directory: string;
}

interface MessageResponse {
  info: {
    id: string;
    role: string;
    sessionID: string;
  };
  parts: Array<{
    type: string;
    text?: string;
    snapshot?: string;
  }>;
}

class OpenCodeClient {
  private baseUrl: string;
  private sessionId: string | null = null;

  constructor(baseUrl = OPENCODE_BASE_URL) {
    this.baseUrl = baseUrl;
  }

  async createSession(): Promise<Session> {
    const response = await fetch(`${this.baseUrl}/session`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({}),
    });

    if (!response.ok) {
      throw new Error(`Failed to create session: ${response.status}`);
    }

    const session = await response.json();
    this.sessionId = session.id;
    return session;
  }

  async sendMessage(content: string): Promise<MessageResponse> {
    if (!this.sessionId) {
      await this.createSession();
    }

    const response = await fetch(`${this.baseUrl}/session/${this.sessionId}/message`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        parts: [{ type: 'text', text: content }],
      }),
    });

    if (!response.ok) {
      throw new Error(`Failed to send message: ${response.status}`);
    }

    return response.json();
  }

  setSessionId(sessionId: string) {
    this.sessionId = sessionId;
  }

  getSessionId(): string | null {
    return this.sessionId;
  }
}

export const opencodeClient = new OpenCodeClient();
export type { Session, MessageResponse };
