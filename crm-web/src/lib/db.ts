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

export interface CustomerRow {
  phone_number_meta: string;
  phone_number_manual: string | null;
  customer_name_meta: string | null;
  customer_name_manual: string | null;
  customer_username: string | null;
  delivery_address_last: string | null;
  total_spent_cop: number;
  total_units_purchased: number;
  first_contact_at: string | null;
  last_contact_at: string | null;
}

export interface OrderItemRow {
  id: number;
  flavor: string;
  has_liquor: boolean;
  quantity: number;
  unit_price: number;
  subtotal: number;
}

export interface OrderRow {
  id: number;
  delivery_type: string;
  scheduled_date_text: string | null;
  scheduled_time_text: string | null;
  payment_method: string;
  referral_code: string | null;
  referral_discount_total: number | null;
  ambassador_commission_total: number | null;
  delivery_cost: number | null;
  total_estimated: number;
  total_final: number | null;
  status: string;
  created_at: string;
  items: OrderItemRow[];
}

export interface ReferralUsageRow {
  code: string;
  order_id: number;
  created_at: string;
  discount_total: number | null;
  commission_total: number | null;
}

export async function listCustomers(
  search: string,
  limit: number
): Promise<CustomerRow[]> {
  const pool = getPool();
  const trimmed = search.trim();

  if (trimmed.length === 0) {
    const { rows } = await pool.query<CustomerRow>(
      `SELECT phone_number_meta, phone_number_manual, customer_name_meta, customer_name_manual,
              customer_username, delivery_address_last, total_spent_cop, total_units_purchased,
              first_contact_at, last_contact_at
       FROM customers
       ORDER BY last_contact_at DESC NULLS LAST
       LIMIT $1`,
      [limit]
    );
    return rows;
  }

  const pattern = `%${trimmed}%`;
  const { rows } = await pool.query<CustomerRow>(
    `SELECT phone_number_meta, phone_number_manual, customer_name_meta, customer_name_manual,
            customer_username, delivery_address_last, total_spent_cop, total_units_purchased,
            first_contact_at, last_contact_at
     FROM customers
     WHERE phone_number_meta ILIKE $1
        OR COALESCE(phone_number_manual, '') ILIKE $1
        OR COALESCE(customer_name_meta, '') ILIKE $1
        OR COALESCE(customer_name_manual, '') ILIKE $1
        OR COALESCE(customer_username, '') ILIKE $1
     ORDER BY last_contact_at DESC NULLS LAST
     LIMIT $2`,
    [pattern, limit]
  );
  return rows;
}

export async function getCustomer(phone: string): Promise<CustomerRow | null> {
  const pool = getPool();
  const { rows } = await pool.query<CustomerRow>(
    `SELECT phone_number_meta, phone_number_manual, customer_name_meta, customer_name_manual,
            customer_username, delivery_address_last, total_spent_cop, total_units_purchased,
            first_contact_at, last_contact_at
     FROM customers
     WHERE phone_number_meta = $1`,
    [phone]
  );
  return rows[0] ?? null;
}

export async function getOrdersForCustomer(
  phone: string,
  limit = 5
): Promise<OrderRow[]> {
  const pool = getPool();
  const { rows: orders } = await pool.query<Omit<OrderRow, "items">>(
    `SELECT o.id, o.delivery_type, o.scheduled_date_text, o.scheduled_time_text,
            o.payment_method, o.referral_code, o.referral_discount_total,
            o.ambassador_commission_total, o.delivery_cost, o.total_estimated,
            o.total_final, o.status, o.created_at
     FROM orders o
     JOIN conversations c ON c.id = o.conversation_id
     WHERE c.phone_number = $1
     ORDER BY o.created_at DESC
     LIMIT $2`,
    [phone, limit]
  );

  if (orders.length === 0) return [];

  const orderIds = orders.map((o) => o.id);
  const { rows: items } = await pool.query<OrderItemRow & { order_id: number }>(
    `SELECT id, order_id, flavor, has_liquor, quantity, unit_price, subtotal
     FROM order_items
     WHERE order_id = ANY($1)
     ORDER BY id ASC`,
    [orderIds]
  );

  return orders.map((order) => ({
    ...order,
    items: items.filter((item) => item.order_id === order.id),
  }));
}

export async function getReferralUsageForCustomer(
  phone: string
): Promise<ReferralUsageRow[]> {
  const pool = getPool();
  const { rows } = await pool.query<ReferralUsageRow>(
    `SELECT o.referral_code AS code, o.id AS order_id, o.created_at,
            o.referral_discount_total AS discount_total,
            o.ambassador_commission_total AS commission_total
     FROM orders o
     JOIN conversations c ON c.id = o.conversation_id
     WHERE c.phone_number = $1 AND o.referral_code IS NOT NULL
     ORDER BY o.created_at DESC`,
    [phone]
  );
  return rows;
}

export interface RawAgentMessage {
  role: string;
  content: Array<
    | { type: "text"; text: string }
    | { type: "tool_use"; id: string; name: string; input: unknown }
    | { type: "tool_result"; tool_use_id: string; content: string; is_error?: boolean }
  >;
}

export async function getAgentMessages(
  phone: string
): Promise<{ messages: RawAgentMessage[]; updated_at: string | null }> {
  const pool = getPool();
  const { rows } = await pool.query<{ messages: RawAgentMessage[]; updated_at: string }>(
    `SELECT messages, updated_at FROM agent_case_messages WHERE phone_number = $1`,
    [phone]
  );
  if (rows.length === 0) {
    return { messages: [], updated_at: null };
  }
  return { messages: rows[0].messages, updated_at: rows[0].updated_at };
}
