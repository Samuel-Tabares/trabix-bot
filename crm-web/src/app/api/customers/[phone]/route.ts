import { NextRequest, NextResponse } from "next/server";
import { getCustomer, getOrdersForCustomer, getReferralUsageForCustomer } from "@/lib/db";

export async function GET(
  _request: NextRequest,
  { params }: { params: Promise<{ phone: string }> }
) {
  const { phone } = await params;

  try {
    const customer = await getCustomer(phone);
    if (!customer) {
      return NextResponse.json({ error: "Customer not found" }, { status: 404 });
    }

    const [orders, referralUsage] = await Promise.all([
      getOrdersForCustomer(phone, 5),
      getReferralUsageForCustomer(phone),
    ]);

    return NextResponse.json({ customer, orders, referralUsage });
  } catch (error) {
    console.error(`GET /api/customers/${phone} failed`, error);
    return NextResponse.json({ error: "Failed to load customer" }, { status: 500 });
  }
}
