"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { CaseSummaryRow, MessageEventRow } from "@/lib/db";
import {
  actorLabel,
  caseTitle,
  channelLabel,
  dayKey,
  formatClock,
  formatCop,
  formatDayDivider,
  initials,
  previewText,
  relativeTime,
} from "@/lib/format";

interface CaseHeader {
  case_phone: string;
  customer_name: string | null;
  customer_username: string | null;
  delivery_address_last: string | null;
  total_spent_cop: number | null;
  total_units_purchased: number | null;
}

const CASE_POLL_MS = 12000;
const TIMELINE_POLL_MS = 8000;

export default function ConsoleView({
  initialCases,
}: {
  initialCases: CaseSummaryRow[];
}) {
  const [cases, setCases] = useState<CaseSummaryRow[]>(initialCases);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<string | null>(
    initialCases[0]?.case_phone ?? null
  );

  const refreshCases = useCallback(async (q: string) => {
    try {
      const res = await fetch(`/api/cases?q=${encodeURIComponent(q)}`, {
        cache: "no-store",
      });
      if (!res.ok) return;
      const data = await res.json();
      setCases(data.cases ?? []);
    } catch {
      /* keep last good list */
    }
  }, []);

  // Debounced search + periodic live refresh of the case list.
  useEffect(() => {
    const t = setTimeout(() => refreshCases(query), 250);
    return () => clearTimeout(t);
  }, [query, refreshCases]);

  useEffect(() => {
    const id = setInterval(() => refreshCases(query), CASE_POLL_MS);
    return () => clearInterval(id);
  }, [query, refreshCases]);

  return (
    <div className="console">
      <aside className="rail">
        <div className="rail-head">
          <div className="rail-title">
            Trabix <span>Conversaciones</span>
          </div>
          <input
            className="rail-search"
            placeholder="Buscar cliente o teléfono…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <div className="rail-list">
          {cases.length === 0 ? (
            <div className="rail-empty">
              Aún no hay conversaciones registradas.
              <br />
              Aparecerán aquí en cuanto el bot hable con un cliente.
            </div>
          ) : (
            cases.map((c) => (
              <button
                key={c.case_phone}
                className={`case-item${selected === c.case_phone ? " active" : ""}`}
                onClick={() => setSelected(c.case_phone)}
              >
                <div className="avatar">{initials(c.customer_name, c.case_phone)}</div>
                <div className="case-main">
                  <div className="case-row">
                    <span className="case-name">
                      {caseTitle(c.customer_name, c.customer_username, c.case_phone)}
                    </span>
                    <span className="case-time">{relativeTime(c.last_at)}</span>
                  </div>
                  <div className="case-row">
                    <span className="case-preview">
                      {previewText(c.last_actor, c.last_content_type, c.last_body)}
                    </span>
                    <span className="case-count">{c.message_count}</span>
                  </div>
                </div>
              </button>
            ))
          )}
        </div>
      </aside>

      <section className="thread">
        {selected ? (
          <CaseThread phone={selected} />
        ) : (
          <div className="thread-empty">Selecciona una conversación</div>
        )}
      </section>
    </div>
  );
}

function CaseThread({ phone }: { phone: string }) {
  const [header, setHeader] = useState<CaseHeader | null>(null);
  const [events, setEvents] = useState<MessageEventRow[]>([]);
  const [loading, setLoading] = useState(true);
  const scrollRef = useRef<HTMLDivElement>(null);
  const lastCountRef = useRef(0);

  const load = useCallback(async () => {
    try {
      const res = await fetch(`/api/cases/${encodeURIComponent(phone)}`, {
        cache: "no-store",
      });
      if (!res.ok) return;
      const data = await res.json();
      setHeader(data.header ?? null);
      setEvents(data.timeline ?? []);
    } catch {
      /* keep last good */
    } finally {
      setLoading(false);
    }
  }, [phone]);

  useEffect(() => {
    setLoading(true);
    setEvents([]);
    lastCountRef.current = 0;
    load();
    const id = setInterval(load, TIMELINE_POLL_MS);
    return () => clearInterval(id);
  }, [load]);

  // Auto-scroll to bottom when new messages arrive.
  useEffect(() => {
    if (events.length !== lastCountRef.current) {
      lastCountRef.current = events.length;
      const el = scrollRef.current;
      if (el) el.scrollTop = el.scrollHeight;
    }
  }, [events]);

  const grouped = useMemo(() => groupByDay(events), [events]);

  return (
    <>
      <header className="thread-head">
        <div className="avatar lg">
          {initials(header?.customer_name ?? null, phone)}
        </div>
        <div className="thread-id">
          <div className="thread-name">
            {caseTitle(header?.customer_name ?? null, header?.customer_username ?? null, phone)}
          </div>
          <div className="thread-sub">
            {phone}
            {header?.customer_username ? ` · @${header.customer_username}` : ""}
          </div>
        </div>
        <div className="thread-stats">
          {header?.total_units_purchased ? (
            <span className="stat">{header.total_units_purchased} und</span>
          ) : null}
          {header?.total_spent_cop ? (
            <span className="stat">{formatCop(header.total_spent_cop)}</span>
          ) : null}
        </div>
      </header>

      <div className="thread-scroll" ref={scrollRef}>
        {loading && events.length === 0 ? (
          <div className="thread-empty">Cargando…</div>
        ) : events.length === 0 ? (
          <div className="thread-empty">Sin mensajes registrados en esta conversación.</div>
        ) : (
          grouped.map((group) => (
            <div key={group.day}>
              <div className="day-divider">
                <span>{formatDayDivider(group.events[0].created_at)}</span>
              </div>
              {group.events.map((ev, i) => (
                <MessageBubble
                  key={ev.id}
                  event={ev}
                  prev={i > 0 ? group.events[i - 1] : undefined}
                />
              ))}
            </div>
          ))
        )}
      </div>
    </>
  );
}

function MessageBubble({
  event,
  prev,
}: {
  event: MessageEventRow;
  prev?: MessageEventRow;
}) {
  // client -> left, bot -> right, advisor -> left (own lane color).
  const side = event.actor === "bot" ? "right" : "left";
  const laneChanged = !prev || prev.channel !== event.channel;

  return (
    <>
      {laneChanged ? (
        <div className={`lane-chip lane-${event.channel}`}>
          {channelLabel(event.channel)}
        </div>
      ) : null}
      <div className={`bubble-row ${side}`}>
        <div className={`bubble actor-${event.actor}`}>
          <div className="bubble-actor">{actorLabel(event.actor)}</div>
          {event.body ? <div className="bubble-body">{event.body}</div> : null}
          <BubbleExtras event={event} />
          <div className="bubble-time">{formatClock(event.created_at)}</div>
        </div>
      </div>
    </>
  );
}

function BubbleExtras({ event }: { event: MessageEventRow }) {
  const p = event.payload ?? {};
  if (event.content_type === "buttons" && Array.isArray((p as any).buttons)) {
    return (
      <div className="chips">
        {(p as any).buttons.map((b: any, i: number) => (
          <span className="chip" key={i}>
            {b?.reply?.title ?? b?.title ?? "botón"}
          </span>
        ))}
      </div>
    );
  }
  if (event.content_type === "list" && Array.isArray((p as any).sections)) {
    const rows = (p as any).sections.flatMap((s: any) => s?.rows ?? []);
    return (
      <div className="chips">
        {rows.map((r: any, i: number) => (
          <span className="chip" key={i}>
            {r?.title ?? "opción"}
          </span>
        ))}
      </div>
    );
  }
  if (event.content_type === "image") {
    return <div className="chip img-chip">📷 Imagen{event.body ? "" : " (sin texto)"}</div>;
  }
  return null;
}

interface DayGroup {
  day: string;
  events: MessageEventRow[];
}

function groupByDay(events: MessageEventRow[]): DayGroup[] {
  const groups: DayGroup[] = [];
  for (const ev of events) {
    const day = dayKey(ev.created_at);
    const last = groups[groups.length - 1];
    if (last && last.day === day) {
      last.events.push(ev);
    } else {
      groups.push({ day, events: [ev] });
    }
  }
  return groups;
}
