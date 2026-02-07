// API proxy to forward requests to MCP server (bypasses CORS)
import { NextRequest, NextResponse } from "next/server";
import { randomUUID } from "crypto";

// Use environment variable or default to localhost:3002
// Note: We're NOT using /api/mcp-proxy here to avoid infinite loop
const MCP_URL = process.env.MCP_URL || "http://localhost:3002";
const MCP_AUTH_TOKEN = process.env.MCP_AUTH_TOKEN || "demo-token";

async function proxyRequest(request: NextRequest, path: string) {
  const url = `${MCP_URL}${path}`;
  
  const headers: Record<string, string> = {
    "Authorization": `Bearer ${MCP_AUTH_TOKEN}`,
  };
  
  // Forward content-type if present
  const contentType = request.headers.get("content-type");
  if (contentType) {
    headers["Content-Type"] = contentType;
  }
  
  // Generate or forward session ID
  let sessionId = request.headers.get("x-session-id");
  if (!sessionId) {
    sessionId = randomUUID();
  }
  headers["X-Session-Id"] = sessionId;

  try {
    const response = await fetch(url, {
      method: request.method,
      headers,
      body: request.method !== "GET" && request.method !== "HEAD" 
        ? await request.text() 
        : undefined,
    });

    const data = await response.json();
    
    // Create response with CORS headers and caching
    const nextResponse = NextResponse.json(data, { status: response.status });
    nextResponse.headers.set("X-Session-Id", sessionId);
    
    // Cache static config data (permissions, firewall config) for 60 seconds
    // Dynamic data (sessions, logs) - no cache
    if (path.includes("/firewall/permissions") || path.includes("/firewall/status")) {
      nextResponse.headers.set("Cache-Control", "public, max-age=60, stale-while-revalidate=300");
    } else if (path.includes("/dashboard")) {
      // Dashboard has mixed data - short cache for the static parts
      nextResponse.headers.set("Cache-Control", "public, max-age=5, stale-while-revalidate=10");
    } else {
      nextResponse.headers.set("Cache-Control", "no-store, must-revalidate");
    }
    
    return nextResponse;
  } catch (error) {
    const errorMessage = error instanceof Error ? error.message : "Unknown error";
    return NextResponse.json(
      { error: `Proxy error: ${errorMessage}` },
      { status: 500 }
    );
  }
}

// GET /api/mcp-proxy/firewall/status
export async function GET(request: NextRequest) {
  const path = request.nextUrl.pathname.replace("/api/mcp-proxy", "");
  return proxyRequest(request, path + request.nextUrl.search);
}

// Handle all other methods
export async function POST(request: NextRequest) {
  const path = request.nextUrl.pathname.replace("/api/mcp-proxy", "");
  return proxyRequest(request, path);
}

export async function PUT(request: NextRequest) {
  const path = request.nextUrl.pathname.replace("/api/mcp-proxy", "");
  return proxyRequest(request, path);
}

export async function DELETE(request: NextRequest) {
  const path = request.nextUrl.pathname.replace("/api/mcp-proxy", "");
  return proxyRequest(request, path);
}
