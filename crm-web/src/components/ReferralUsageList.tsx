import type { ReferralUsageRow } from "@/lib/db";

function formatCop(value: number | null): string {
  if (value === null) return "—";
  return new Intl.NumberFormat("es-CO", {
    style: "currency",
    currency: "COP",
    maximumFractionDigits: 0,
  }).format(value);
}

function formatDate(value: string): string {
  return new Intl.DateTimeFormat("es-CO", { dateStyle: "medium" }).format(new Date(value));
}

export default function ReferralUsageList({ usage }: { usage: ReferralUsageRow[] }) {
  if (usage.length === 0) {
    return <div className="empty-state card">Este cliente no ha usado códigos de referido.</div>;
  }

  return (
    <div className="table-wrap card">
      <table>
        <thead>
          <tr>
            <th>Código</th>
            <th>Pedido</th>
            <th>Fecha</th>
            <th>Descuento aplicado</th>
            <th>Comisión generada</th>
          </tr>
        </thead>
        <tbody>
          {usage.map((row) => (
            <tr key={row.order_id}>
              <td>
                <span className="pill">{row.code}</span>
              </td>
              <td className="mono">#{row.order_id}</td>
              <td className="text-muted">{formatDate(row.created_at)}</td>
              <td className="mono">{formatCop(row.discount_total)}</td>
              <td className="mono">{formatCop(row.commission_total)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
