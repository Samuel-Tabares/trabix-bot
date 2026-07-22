import { NextRequest, NextResponse } from "next/server";
import { listCases } from "@/lib/db";

export const dynamic = "force-dynamic";

export async function GET(request: NextRequest) {
  const search = request.nextUrl.searchParams.get("q") ?? "";
  const limitParam = request.nextUrl.searchParams.get("limit");
  const limit = Math.min(Math.max(parseInt(limitParam ?? "200", 10) || 200, 1), 500);

  try {
    const cases = await listCases(search, limit);
    return NextResponse.json({ cases });
  } catch (error) {
    console.error("GET /api/cases failed", error);
    return NextResponse.json({ error: "Failed to load cases" }, { status: 500 });
  }
}
