import { NextRequest, NextResponse } from "next/server";
import { getCaseTimeline, getCaseHeader } from "@/lib/db";

export const dynamic = "force-dynamic";

export async function GET(
  _request: NextRequest,
  { params }: { params: Promise<{ phone: string }> }
) {
  const { phone } = await params;

  try {
    const [timeline, header] = await Promise.all([
      getCaseTimeline(phone),
      getCaseHeader(phone),
    ]);
    return NextResponse.json({ header, timeline });
  } catch (error) {
    console.error(`GET /api/cases/${phone} failed`, error);
    return NextResponse.json({ error: "Failed to load case" }, { status: 500 });
  }
}
