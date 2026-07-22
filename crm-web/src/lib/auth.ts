// Single-operator gate. One shared password (env CRM_PASSWORD); the session
// cookie stores a SHA-256 of it, so the raw password is never placed in a
// cookie and the token can't be forged without knowing the password.
// Edge-safe: uses only Web Crypto, no Node-only APIs (so middleware can import it).

export const AUTH_COOKIE = "trabix_crm_session";

export async function sessionToken(password: string): Promise<string> {
  const data = new TextEncoder().encode(`trabix::${password}`);
  const digest = await crypto.subtle.digest("SHA-256", data);
  return Array.from(new Uint8Array(digest))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

export async function expectedToken(): Promise<string | null> {
  const password = process.env.CRM_PASSWORD;
  if (!password || password.length === 0) return null;
  return sessionToken(password);
}

/** Constant-time string comparison to avoid leaking length/prefix via timing. */
export function safeEqual(a: string, b: string): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) {
    diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  }
  return diff === 0;
}
