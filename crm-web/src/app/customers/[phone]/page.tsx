import { notFound } from "next/navigation";
import { getCustomer, getOrdersForCustomer, getReferralUsageForCustomer } from "@/lib/db";
import CustomerDetail from "@/components/CustomerDetail";

export const dynamic = "force-dynamic";

export default async function CustomerPage({
  params,
}: {
  params: Promise<{ phone: string }>;
}) {
  const { phone } = await params;
  const customer = await getCustomer(phone);

  if (!customer) {
    notFound();
  }

  const [orders, referralUsage] = await Promise.all([
    getOrdersForCustomer(phone, 5),
    getReferralUsageForCustomer(phone),
  ]);

  return <CustomerDetail customer={customer} orders={orders} referralUsage={referralUsage} />;
}
