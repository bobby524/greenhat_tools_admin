import { NextRequest, NextResponse } from "next/server";

// API key auth for MCP endpoints
function checkApiKey(request: NextRequest): boolean {
  const authHeader = request.headers.get("authorization");
  if (!authHeader?.startsWith("Bearer ")) return false;
  return authHeader.slice(7) === process.env.ADMIN_MCP_TOKEN;
}

// Session cookie check
function getSessionCookie(request: NextRequest): string | null {
  const names = [
    "__Secure-greenhat_tools.session_token",
    "greenhat_tools.session_token",
  ];
  for (const name of names) {
    const cookie = request.cookies.get(name);
    if (cookie?.value) return cookie.value;
  }
  return null;
}

export async function middleware(request: NextRequest) {
  // MCP endpoints: API key OR session
  if (request.nextUrl.pathname.startsWith("/api/mcp")) {
    // Allow API key auth
    if (checkApiKey(request)) {
      return NextResponse.next();
    }
    
    // Check for session
    const sessionToken = getSessionCookie(request);
    if (!sessionToken) {
      return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
    }
    
    // Session validation happens in the API route
    return NextResponse.next();
  }
  
  // Auth endpoints - allow through to Better Auth
  if (request.nextUrl.pathname.startsWith("/api/auth")) {
    return NextResponse.next();
  }
  
  return NextResponse.next();
}

export const config = {
  matcher: ["/api/mcp/:path*", "/api/auth/:path*"],
};
