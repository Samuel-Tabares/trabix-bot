import { NextRequest, NextResponse } from "next/server";
import { getAgentMessages } from "@/lib/db";
import { toConversationEntries } from "@/lib/conversation";

export async function GET(
  _request: NextRequest,
  { params }: { params: Promise<{ phone: string }> }
) {
  const { phone } = await params;

  try {
    const { messages, updated_at } = await getAgentMessages(phone);
    const entries = toConversationEntries(messages);
    return NextResponse.json({ entries, updated_at });
  } catch (error) {
    console.error(`GET /api/conversations/${phone} failed`, error);
    return NextResponse.json({ error: "Failed to load conversation" }, { status: 500 });
  }
}
