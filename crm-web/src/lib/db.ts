import { Pool } from "pg";

declare global {
  // eslint-disable-next-line no-var
  var _pgPool: Pool | undefined;
}

export function getPool(): Pool {
  if (!global._pgPool) {
    global._pgPool = new Pool({
      connectionString: process.env.DATABASE_URL,
      max: 5,
    });
  }
  return global._pgPool;
}

/** True when the error is Postgres "undefined_table" (42P01). Lets the console
 *  render an empty state before the bot has created message_events. */
function isMissingTable(error: unknown): boolean {
  return (
    typeof error === "object" &&
    error !== null &&
    (error as { code?: string }).code === "42P01"
  );
}

// --- Conversation console (message_events) ---------------------------------

export type Channel = "client" | "advisor";
export type Actor = "client" | "bot" | "advisor";

export interface CaseSummaryRow {
  case_phone: string;
  customer_name: string | null;
  customer_username: string | null;
  last_body: string | null;
  last_content_type: string;
  last_actor: Actor;
  last_channel: Channel;
  last_at: string;
  message_count: number;
}

export interface MessageEventRow {
  id: string;
  channel: Channel;
  actor: Actor;
  content_type: string;
  body: string | null;
  payload: Record<string, unknown> | null;
  wa_message_id: string | null;
  created_at: string;
}

/** One row per case (customer conversation), newest activity first. */
export async function listCases(
  search: string,
  limit = 200
): Promise<CaseSummaryRow[]> {
  const pool = getPool();
  const trimmed = search.trim();
  const pattern = trimmed.length > 0 ? `%${trimmed}%` : null;

  try {
    const { rows } = await pool.query<CaseSummaryRow>(
      `
      WITH cases AS (
        SELECT DISTINCT case_phone FROM message_events
      )
      SELECT cs.case_phone,
             COALESCE(c.customer_name_manual, c.customer_name_meta) AS customer_name,
             c.customer_username,
             last.body           AS last_body,
             last.content_type   AS last_content_type,
             last.actor          AS last_actor,
             last.channel        AS last_channel,
             last.created_at     AS last_at,
             cnt.n               AS message_count
      FROM cases cs
      JOIN LATERAL (
        SELECT body, content_type, actor, channel, created_at
        FROM message_events e
        WHERE e.case_phone = cs.case_phone
        ORDER BY e.created_at DESC, e.id DESC
        LIMIT 1
      ) last ON TRUE
      JOIN LATERAL (
        SELECT COUNT(*)::int AS n FROM message_events e WHERE e.case_phone = cs.case_phone
      ) cnt ON TRUE
      LEFT JOIN customers c ON c.phone_number_meta = cs.case_phone
      WHERE $1::text IS NULL
         OR cs.case_phone ILIKE $1
         OR COALESCE(c.customer_name_meta, '') ILIKE $1
         OR COALESCE(c.customer_name_manual, '') ILIKE $1
         OR COALESCE(c.customer_username, '') ILIKE $1
      ORDER BY last.created_at DESC
      LIMIT $2
      `,
      [pattern, limit]
    );
    return rows;
  } catch (error) {
    if (isMissingTable(error)) return [];
    throw error;
  }
}

/** Full chronological trace for one case: both client and advisor lanes. */
export async function getCaseTimeline(phone: string): Promise<MessageEventRow[]> {
  const pool = getPool();
  try {
    const { rows } = await pool.query<MessageEventRow>(
      `
      SELECT id::text, channel, actor, content_type, body, payload, wa_message_id, created_at
      FROM message_events
      WHERE case_phone = $1
      ORDER BY created_at ASC, id ASC
      `,
      [phone]
    );
    return rows;
  } catch (error) {
    if (isMissingTable(error)) return [];
    throw error;
  }
}

export interface CaseHeader {
  case_phone: string;
  customer_name: string | null;
  customer_username: string | null;
  delivery_address_last: string | null;
  total_spent_cop: number | null;
  total_units_purchased: number | null;
}

export async function getCaseHeader(phone: string): Promise<CaseHeader> {
  const pool = getPool();
  const { rows } = await pool.query<CaseHeader>(
    `
    SELECT $1::text AS case_phone,
           COALESCE(customer_name_manual, customer_name_meta) AS customer_name,
           customer_username,
           delivery_address_last,
           total_spent_cop,
           total_units_purchased
    FROM customers
    WHERE phone_number_meta = $1
    `,
    [phone]
  );
  return (
    rows[0] ?? {
      case_phone: phone,
      customer_name: null,
      customer_username: null,
      delivery_address_last: null,
      total_spent_cop: null,
      total_units_purchased: null,
    }
  );
}
