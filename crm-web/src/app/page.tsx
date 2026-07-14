import { listCustomers } from "@/lib/db";
import CustomerList from "@/components/CustomerList";

export const dynamic = "force-dynamic";

export default async function HomePage() {
  const customers = await listCustomers("", 100);

  return (
    <div className="page">
      <div className="top-bar">
        <div>
          <div className="brand">
            Trabix <span>CRM</span>
          </div>
          <div className="subtitle">{customers.length} clientes visibles</div>
        </div>
      </div>

      <CustomerList initialCustomers={customers} />
    </div>
  );
}
