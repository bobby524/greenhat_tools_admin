import { NextRequest, NextResponse } from "next/server";
import { auth } from "@/lib/auth";

/**
 * Admin middleware - ensures only admins can access admin routes
 */

const ADMIN_ROLES = ["admin", "owner"];

export async function middleware(request: NextRequest) {
  const { pathname } = request.nextUrl;

  // Allow public routes
  if (pathname === "/" || 
      pathname === "/auth/signin" || 
      pathname === "/auth/signup" ||
      pathname.startsWith("/api/auth/") ||
      pathname.startsWith("/_next/") ||
      pathname.startsWith("/favicon.ico")) {
    return NextResponse.next();
  }

  // Check for session cookie
  const sessionCookie = 
    request.cookies.get("__Secure-greenhat_tools.session_token") ||
    request.cookies.get("greenhat_tools.session_token");

  if (!sessionCookie) {
    return NextResponse.redirect(new URL("/auth/signin", request.url));
  }

  try {
    // Verify session and check admin role
    const session = await auth.api.getSession({
      headers: request.headers,
    });

    if (!session?.user) {
      return NextResponse.redirect(new URL("/auth/signin", request.url));
    }

    // Check if user has admin role
    const userRole = session.user.role || "user";
    if (!ADMIN_ROLES.includes(userRole)) {
      return NextResponse.redirect(new URL("/", request.url));
    }

    return NextResponse.next();
  } catch (error) {
    console.error("Admin middleware error:", error);
    return NextResponse.redirect(new URL("/auth/signin", request.url));
  }
}

export const config = {
  matcher: ["/((?!_next/static|_next/image|favicon.ico|.*\\.(?:svg|png|jpg|jpeg|gif|webp)$).*)",
  ],
};