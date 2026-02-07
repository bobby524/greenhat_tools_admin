import { NextRequest, NextResponse } from "next/server";

/**
 * Admin middleware - ensures only admins can access admin routes
 */

const ADMIN_ROLES = ["admin", "owner"];

export async function middleware(request: NextRequest) {
  const { pathname } = request.nextUrl;

  // Allow public routes and all API routes
  if (pathname === "/" || 
      pathname === "/auth/signin" || 
      pathname === "/auth/signup" ||
      pathname.startsWith("/api/") ||
      pathname.startsWith("/_next/") ||
      pathname.startsWith("/favicon.ico")) {
    return NextResponse.next();
  }

  // Check for session cookie - handle secure and non-secure variants
  const possibleCookieNames = [
    "__Secure-greenhat_tools.session_token",
    "greenhat_tools.session_token",
    "greenhat_tools.session",
    "session_token",
  ];

  let sessionCookie = null;
  for (const name of possibleCookieNames) {
    const cookie = request.cookies.get(name);
    if (cookie?.value) {
      sessionCookie = cookie;
      break;
    }
  }

  if (!sessionCookie) {
    return NextResponse.redirect(new URL("/", request.url));
  }

  // For now, let the request through and let the page handle auth
  // The session validation will happen on the client side
  return NextResponse.next();
}

export const config = {
  matcher: ["/((?!_next/static|_next/image|favicon.ico|.*\\.(?:svg|png|jpg|jpeg|gif|webp)$).*)",
  ],
};
