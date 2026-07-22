import type { Actor, Channel } from "./db";

const TZ = "America/Bogota";

export function actorLabel(actor: Actor): string {
  switch (actor) {
    case "client":
      return "Cliente";
    case "advisor":
      return "Asesor";
    case "bot":
      return "Bot";
  }
}

export function channelLabel(channel: Channel): string {
  return channel === "advisor" ? "Bot ⇄ Asesor" : "Cliente ⇄ Bot";
}

/** Short pretty name for a case, falling back to a masked phone. */
export function caseTitle(name: string | null, username: string | null, phone: string): string {
  if (name && name.trim().length > 0) return name.trim();
  if (username && username.trim().length > 0) return `@${username.trim()}`;
  return phone;
}

export function initials(name: string | null, phone: string): string {
  const source = name && name.trim().length > 0 ? name.trim() : phone;
  const parts = source.split(/\s+/).filter(Boolean);
  if (parts.length === 0) return "?";
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
}

export function formatClock(iso: string): string {
  return new Date(iso).toLocaleTimeString("es-CO", {
    timeZone: TZ,
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function formatDayDivider(iso: string): string {
  return new Date(iso).toLocaleDateString("es-CO", {
    timeZone: TZ,
    weekday: "long",
    day: "numeric",
    month: "long",
  });
}

export function dayKey(iso: string): string {
  return new Date(iso).toLocaleDateString("en-CA", { timeZone: TZ });
}

/** Relative "hace X" label for the case list. */
export function relativeTime(iso: string): string {
  const then = new Date(iso).getTime();
  const diffSec = Math.max(0, Math.round((Date.now() - then) / 1000));
  if (diffSec < 60) return "ahora";
  const diffMin = Math.round(diffSec / 60);
  if (diffMin < 60) return `${diffMin} min`;
  const diffHr = Math.round(diffMin / 60);
  if (diffHr < 24) return `${diffHr} h`;
  const diffDay = Math.round(diffHr / 24);
  if (diffDay < 7) return `${diffDay} d`;
  return new Date(iso).toLocaleDateString("es-CO", {
    timeZone: TZ,
    day: "numeric",
    month: "short",
  });
}

export function formatCop(value: number | null): string {
  if (value === null || value === undefined) return "—";
  return new Intl.NumberFormat("es-CO", {
    style: "currency",
    currency: "COP",
    maximumFractionDigits: 0,
  }).format(value);
}

/** Compact one-line preview for the case list. */
export function previewText(
  actor: Actor,
  contentType: string,
  body: string | null
): string {
  const prefix = actor === "bot" ? "Bot: " : actor === "advisor" ? "Asesor: " : "";
  if (body && body.trim().length > 0) {
    return prefix + body.replace(/\s+/g, " ").trim();
  }
  const kind =
    contentType === "image"
      ? "📷 Imagen"
      : contentType === "buttons"
        ? "Botones"
        : contentType === "list"
          ? "Lista"
          : "Mensaje";
  return prefix + kind;
}
