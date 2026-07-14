"use client";

import { useEffect, useState } from "react";
import type { ConversationEntry } from "@/lib/conversation";

export default function ConversationThread({ phone }: { phone: string }) {
  const [entries, setEntries] = useState<ConversationEntry[] | null>(null);
  const [error, setError] = useState(false);

  useEffect(() => {
    let cancelled = false;
    fetch(`/api/conversations/${phone}`)
      .then((res) => res.json())
      .then((data) => {
        if (!cancelled) setEntries(data.entries ?? []);
      })
      .catch(() => {
        if (!cancelled) setError(true);
      });
    return () => {
      cancelled = true;
    };
  }, [phone]);

  if (error) {
    return <div className="empty-state">No se pudo cargar la conversación.</div>;
  }

  if (entries === null) {
    return <div className="spinner-row">Cargando conversación…</div>;
  }

  if (entries.length === 0) {
    return <div className="empty-state">Sin mensajes registrados para este cliente.</div>;
  }

  return (
    <div className="timeline card">
      {entries.map((entry, index) => (
        <div key={index} className={`bubble bubble-${entry.speaker}`}>
          <span className="bubble-label">{entry.speaker}</span>
          {entry.text}
        </div>
      ))}
    </div>
  );
}
