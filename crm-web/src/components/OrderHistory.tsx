import type { OrderRow } from "@/lib/db";

function formatCop(value: number | null): string {
  if (value === null) return "—";
  return new Intl.NumberFormat("es-CO", {
    style: "currency",
    currency: "COP",
    maximumFractionDigits: 0,
  }).format(value);
}

function formatDate(value: string): string {
  return new Intl.DateTimeFormat("es-CO", {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(value));
}

const STATUS_LABELS: Record<string, string> = {
  draft: "Borrador",
  draft_payment: "Esperando pago",
  confirmed: "Confirmado",
  manual_followup: "Seguimiento manual",
  cancelled: "Cancelado",
};

export default function OrderHistory({ orders }: { orders: OrderRow[] }) {
  if (orders.length === 0) {
    return <div className="empty-state card">Este cliente todavía no tiene pedidos registrados.</div>;
  }

  return (
    <div className="card">
      {orders.map((order) => (
        <div className="order-card" key={order.id}>
          <div className="order-card-head">
            <div>
              <strong>Pedido #{order.id}</strong>{" "}
              <span className="pill">{STATUS_LABELS[order.status] ?? order.status}</span>
            </div>
            <div className="text-muted">{formatDate(order.created_at)}</div>
          </div>

          <div className="order-items">
            {order.items.map((item) => (
              <div key={item.id}>
                {item.quantity}× {item.flavor} {item.has_liquor ? "(con licor)" : "(sin licor)"} —{" "}
                {formatCop(item.subtotal)}
              </div>
            ))}
          </div>

          <div style={{ marginTop: 10, display: "flex", gap: 20, flexWrap: "wrap", fontSize: 13 }}>
            <span className="text-muted">
              Entrega: {order.delivery_type === "immediate" ? "inmediata" : "programada"}
              {order.scheduled_date_text ? ` (${order.scheduled_date_text} ${order.scheduled_time_text ?? ""})` : ""}
            </span>
            <span className="text-muted">Pago: {order.payment_method}</span>
            {order.referral_code && (
              <span className="text-muted">Código: {order.referral_code}</span>
            )}
            {order.delivery_cost !== null && (
              <span className="text-muted">Domicilio: {formatCop(order.delivery_cost)}</span>
            )}
            <strong>Total: {formatCop(order.total_final ?? order.total_estimated)}</strong>
          </div>
        </div>
      ))}
    </div>
  );
}
