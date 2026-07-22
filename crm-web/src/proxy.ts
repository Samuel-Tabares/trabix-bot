import { NextRequest, NextResponse } from "next/server";
import { AUTH_COOKIE, expectedToken, safeEqual } from "@/lib/auth";

// Gate everything except the login page and the login API.
export const config = {
  matcher: ["/((?!login|api/login|_next/static|_next/image|favicon.ico).*)"],
};

export async function proxy(request: NextRequest) {
  const expected = await expectedToken();

  // If no password is configured, fail closed: send to login (which will show
  // a clear error) rather than exposing customer data.
  const cookie = request.cookies.get(AUTH_COOKIE)?.value ?? "";
  const authed = expected !== null && safeEqual(cookie, expected);

  if (authed) return NextResponse.next();

  const loginUrl = new URL("/login", request.url);
  return NextResponse.redirect(loginUrl);
}
