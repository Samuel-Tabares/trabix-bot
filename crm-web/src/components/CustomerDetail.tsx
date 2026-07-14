"use client";

import { useState } from "react";
import Link from "next/link";
import type { CustomerRow, OrderRow, ReferralUsageRow } from "@/lib/db";
import ConversationThread from "./ConversationThread";
import OrderHistory from "./OrderHistory";
import ReferralUsageList from "./ReferralUsageList";

type Tab = "conversations" | "orders" | "referrals";

function formatCop(value: number): string {
  return new Intl.NumberFormat("es-CO", {
    style: "currency",
    currency: "COP",
    maximumFractionDigits: 0,
  }).format(value);
}

function formatDate(value: string | null): string {
  if (!value) return "—";
  return new Intl.DateTimeFormat("es-CO", { dateStyle: "long" }).format(new Date(value));
}

function displayName(customer: CustomerRow): string {
  return customer.customer_name_manual || customer.customer_name_meta || "Sin nombre";
}

export default function CustomerDetail({
  customer,
  orders,
  referralUsage,
}: {
  customer: CustomerRow;
  orders: OrderRow[];
  referralUsage: ReferralUsageRow[];
}) {
  const [tab, setTab] = useState<Tab>("conversations");

  return (
    <div className="page">
      <Link href="/" className="back-link">
        ← Volver a clientes
      </Link>

      <div className="card detail-header">
        <h1>{displayName(customer)}</h1>
        <div className="text-muted mono">{customer.phone_number_meta}</div>
        {customer.delivery_address_last && (
          <div className="text-muted" style={{ marginTop: 4 }}>
            {customer.delivery_address_last}
          </div>
        )}

        <div className="detail-meta">
          <div className="stat">
            <div className="stat-label">Dinero gastado</div>
            <div className="stat-value">{formatCop(customer.total_spent_cop)}</div>
          </div>
          <div className="stat">
            <div className="stat-label">Unidades compradas</div>
            <div className="stat-value">{customer.total_units_purchased}</div>
          </div>
          <div className="stat">
            <div className="stat-label">Primer contacto</div>
            <div className="stat-value" style={{ fontSize: 14 }}>
              {formatDate(customer.first_contact_at)}
            </div>
          </div>
          <div className="stat">
            <div className="stat-label">Último contacto</div>
            <div className="stat-value" style={{ fontSize: 14 }}>
              {formatDate(customer.last_contact_at)}
            </div>
          </div>
        </div>
      </div>

      <div className="tabs">
        <button
          className={`tab ${tab === "conversations" ? "active" : ""}`}
          onClick={() => setTab("conversations")}
        >
          Conversaciones
        </button>
        <button className={`tab ${tab === "orders" ? "active" : ""}`} onClick={() => setTab("orders")}>
          Pedidos ({orders.length})
        </button>
        <button
          className={`tab ${tab === "referrals" ? "active" : ""}`}
          onClick={() => setTab("referrals")}
        >
          Referral ({referralUsage.length})
        </button>
      </div>

      {tab === "conversations" && <ConversationThread phone={customer.phone_number_meta} />}
      {tab === "orders" && <OrderHistory orders={orders} />}
      {tab === "referrals" && <ReferralUsageList usage={referralUsage} />}
    </div>
  );
}
