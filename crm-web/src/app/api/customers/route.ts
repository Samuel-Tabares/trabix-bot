import { NextRequest, NextResponse } from "next/server";
import { listCustomers } from "@/lib/db";

export async function GET(request: NextRequest) {
  const search = request.nextUrl.searchParams.get("search") ?? "";
  const limitParam = request.nextUrl.searchParams.get("limit");
  const limit = Math.min(Math.max(parseInt(limitParam ?? "50", 10) || 50, 1), 200);

  try {
    const customers = await listCustomers(search, limit);
    return NextResponse.json({ customers });
  } catch (error) {
    console.error("GET /api/customers failed", error);
    return NextResponse.json({ error: "Failed to load customers" }, { status: 500 });
  }
}
