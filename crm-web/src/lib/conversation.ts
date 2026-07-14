import type { RawAgentMessage } from "./db";

export type Speaker = "cliente" | "asesor" | "bot" | "sistema";

export interface ConversationEntry {
  speaker: Speaker;
  text: string;
}

const INBOUND_PREFIX = /^Mensaje del (CLIENTE|ASESOR): /;

function speakerFromInboundText(text: string): { speaker: Speaker; body: string } {
  const match = text.match(INBOUND_PREFIX);
  if (!match) {
    return { speaker: "sistema", body: text };
  }
  const speaker: Speaker = match[1] === "CLIENTE" ? "cliente" : "asesor";
  return { speaker, body: text.slice(match[0].length) };
}

/** Aplana los mensajes crudos (formato Anthropic) en un timeline legible para humanos. */
export function toConversationEntries(messages: RawAgentMessage[]): ConversationEntry[] {
  const entries: ConversationEntry[] = [];

  for (const message of messages) {
    for (const block of message.content) {
      if (block.type === "text" && block.text.trim().length > 0) {
        if (message.role === "user") {
          const { speaker, body } = speakerFromInboundText(block.text);
          entries.push({ speaker, text: body });
        } else {
          entries.push({ speaker: "bot", text: block.text });
        }
      }
    }
  }

  return entries;
}
