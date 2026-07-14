"use client";

import { useEffect, useMemo, useState } from "react";
import { useRouter } from "next/navigation";
import type { CustomerRow } from "@/lib/db";

type SortKey = "total_spent_cop" | "total_units_purchased" | "last_contact_at";

function formatCop(value: number): string {
  return new Intl.NumberFormat("es-CO", {
    style: "currency",
    currency: "COP",
    maximumFractionDigits: 0,
  }).format(value);
}

function formatDate(value: string | null): string {
  if (!value) return "—";
  return new Intl.DateTimeFormat("es-CO", {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(value));
}

function displayName(customer: CustomerRow): string {
  return customer.customer_name_manual || customer.customer_name_meta || "Sin nombre";
}

export default function CustomerList({ initialCustomers }: { initialCustomers: CustomerRow[] }) {
  const router = useRouter();
  const [search, setSearch] = useState("");
  const [customers, setCustomers] = useState(initialCustomers);
  const [loading, setLoading] = useState(false);
  const [sortKey, setSortKey] = useState<SortKey>("last_contact_at");
  const [sortDir, setSortDir] = useState<"asc" | "desc">("desc");

  useEffect(() => {
    const handle = setTimeout(() => {
      setLoading(true);
      fetch(`/api/customers?search=${encodeURIComponent(search)}&limit=100`)
        .then((res) => res.json())
        .then((data) => setCustomers(data.customers ?? []))
        .catch(() => {})
        .finally(() => setLoading(false));
    }, 250);
    return () => clearTimeout(handle);
  }, [search]);

  const sorted = useMemo(() => {
    const copy = [...customers];
    copy.sort((a, b) => {
      let aVal: number | string;
      let bVal: number | string;
      if (sortKey === "last_contact_at") {
        aVal = a.last_contact_at ? new Date(a.last_contact_at).getTime() : 0;
        bVal = b.last_contact_at ? new Date(b.last_contact_at).getTime() : 0;
      } else {
        aVal = a[sortKey];
        bVal = b[sortKey];
      }
      const cmp = aVal < bVal ? -1 : aVal > bVal ? 1 : 0;
      return sortDir === "asc" ? cmp : -cmp;
    });
    return copy;
  }, [customers, sortKey, sortDir]);

  function toggleSort(key: SortKey) {
    if (sortKey === key) {
      setSortDir((prev) => (prev === "asc" ? "desc" : "asc"));
    } else {
      setSortKey(key);
      setSortDir("desc");
    }
  }

  function sortIndicator(key: SortKey) {
    if (sortKey !== key) return "";
    return sortDir === "asc" ? " ▲" : " ▼";
  }

  return (
    <div>
      <input
        className="search-input"
        placeholder="Buscar por nombre, teléfono o usuario…"
        value={search}
        onChange={(event) => setSearch(event.target.value)}
      />

      <div className="table-wrap card" style={{ marginTop: 16 }}>
        <table>
          <thead>
            <tr>
              <th>Cliente</th>
              <th>Teléfono</th>
              <th>Dirección</th>
              <th onClick={() => toggleSort("total_spent_cop")}>
                Dinero gastado{sortIndicator("total_spent_cop")}
              </th>
              <th onClick={() => toggleSort("total_units_purchased")}>
                Unidades{sortIndicator("total_units_purchased")}
              </th>
              <th onClick={() => toggleSort("last_contact_at")}>
                Último contacto{sortIndicator("last_contact_at")}
              </th>
            </tr>
          </thead>
          <tbody>
            {sorted.map((customer) => (
              <tr
                key={customer.phone_number_meta}
                className="clickable"
                onClick={() => router.push(`/customers/${customer.phone_number_meta}`)}
              >
                <td>{displayName(customer)}</td>
                <td className="mono">{customer.phone_number_meta}</td>
                <td className="text-muted">{customer.delivery_address_last || "—"}</td>
                <td className="mono">{formatCop(customer.total_spent_cop)}</td>
                <td className="mono">{customer.total_units_purchased}</td>
                <td className="text-muted">{formatDate(customer.last_contact_at)}</td>
              </tr>
            ))}
            {sorted.length === 0 && !loading && (
              <tr>
                <td colSpan={6}>
                  <div className="empty-state">No se encontraron clientes.</div>
                </td>
              </tr>
            )}
          </tbody>
        </table>
        {loading && <div className="spinner-row">Buscando…</div>}
      </div>
    </div>
  );
}
