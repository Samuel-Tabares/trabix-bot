import { listCases } from "@/lib/db";
import ConsoleView from "@/components/ConsoleView";

export const dynamic = "force-dynamic";

export default async function HomePage() {
  const cases = await listCases("", 200);
  return <ConsoleView initialCases={cases} />;
}
