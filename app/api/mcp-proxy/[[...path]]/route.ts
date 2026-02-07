// API proxy to forward requests to MCP server or return mock data
import { NextRequest, NextResponse } from "next/server";
import { randomUUID } from "crypto";

// MCP server configuration
const MCP_URL = process.env.MCP_URL || "http://localhost:3002";
const MCP_AUTH_TOKEN = process.env.MCP_AUTH_TOKEN || "demo-token";

// Mock dashboard data for when MCP server is not available
function getMockDashboardData() {
  const now = new Date();
  const sessions = [
    {
      sessionId: "sess-" + randomUUID().slice(0, 8),
      startTime: new Date(now.getTime() - 1000 * 60 * 30).toISOString(), // 30 mins ago
      toolCalls: 45,
      errors: 0,
      blocked: 0,
      maxAclLevel: "PRIVATE" as const,
      lethalTrifecta: false,
    },
    {
      sessionId: "sess-" + randomUUID().slice(0, 8),
      startTime: new Date(now.getTime() - 1000 * 60 * 15).toISOString(), // 15 mins ago
      toolCalls: 23,
      errors: 1,
      blocked: 0,
      maxAclLevel: "PUBLIC" as const,
      lethalTrifecta: false,
    },
    {
      sessionId: "sess-" + randomUUID().slice(0, 8),
      startTime: new Date(now.getTime() - 1000 * 60 * 5).toISOString(), // 5 mins ago
      toolCalls: 8,
      errors: 0,
      blocked: 2,
      maxAclLevel: "SECRET" as const,
      lethalTrifecta: true,
    },
  ];

  const recentLogs = [
    {
      id: randomUUID(),
      timestamp: new Date(now.getTime() - 1000 * 60).toISOString(),
      sessionId: sessions[0].sessionId,
      toolName: "boards_list",
      params: { project_id: "proj-123" },
      result: "success" as const,
      durationMs: 45,
      aclLevel: "PUBLIC" as const,
      riskFlags: {
        readPrivateData: false,
        writeOperation: false,
        externalCommunication: false,
      },
    },
    {
      id: randomUUID(),
      timestamp: new Date(now.getTime() - 1000 * 120).toISOString(),
      sessionId: sessions[1].sessionId,
      toolName: "cards_create",
      params: { title: "New Task" },
      result: "success" as const,
      durationMs: 120,
      aclLevel: "PRIVATE" as const,
      riskFlags: {
        readPrivateData: false,
        writeOperation: true,
        externalCommunication: false,
      },
    },
    {
      id: randomUUID(),
      timestamp: new Date(now.getTime() - 1000 * 180).toISOString(),
      sessionId: sessions[2].sessionId,
      toolName: "projects_get",
      params: { id: "proj-456" },
      result: "blocked" as const,
      error: "Permission denied by MCP firewall",
      durationMs: 25,
      aclLevel: "SECRET" as const,
      riskFlags: {
        readPrivateData: true,
        writeOperation: false,
        externalCommunication: false,
      },
    },
    {
      id: randomUUID(),
      timestamp: new Date(now.getTime() - 1000 * 240).toISOString(),
      sessionId: sessions[0].sessionId,
      toolName: "issues_list",
      params: {},
      result: "success" as const,
      durationMs: 80,
      aclLevel: "PUBLIC" as const,
      riskFlags: {
        readPrivateData: false,
        writeOperation: false,
        externalCommunication: false,
      },
    },
    {
      id: randomUUID(),
      timestamp: new Date(now.getTime() - 1000 * 300).toISOString(),
      sessionId: sessions[1].sessionId,
      toolName: "cycles_update",
      params: { id: "cycle-789" },
      result: "error" as const,
      error: "Rate limit exceeded",
      durationMs: 200,
      aclLevel: "PRIVATE" as const,
      riskFlags: {
        readPrivateData: false,
        writeOperation: true,
        externalCommunication: false,
      },
    },
  ];

  return {
    session: sessions[0],
    recentLogs,
    riskLevel: "medium",
    alerts: [
      "Session sess-abc123 triggered lethal trifecta (read private + write + external)",
      "High rate of blocked requests detected in last 5 minutes",
    ],
    firewall: {
      enabled: true,
      defaultPolicy: "allow",
      toolsConfigured: 24,
      blockedSessions: 3,
      dataLeakPrevention: true,
      lethalTrifectaProtection: true,
    },
    allSessions: sessions,
    permissions: {
      boards_list: {
        enabled: true,
        writeOperation: false,
        readPrivateData: false,
        readUntrustedPublicData: false,
        externalCommunication: false,
        acl: "PUBLIC",
      },
      cards_create: {
        enabled: true,
        writeOperation: true,
        readPrivateData: false,
        readUntrustedPublicData: false,
        externalCommunication: false,
        acl: "PRIVATE",
      },
      projects_get: {
        enabled: false,
        writeOperation: false,
        readPrivateData: true,
        readUntrustedPublicData: false,
        externalCommunication: false,
        acl: "SECRET",
      },
    },
  };
}

async function proxyRequest(request: NextRequest, path: string) {
  // Handle dashboard endpoint with mock data if MCP server is unavailable
  if (path === "/dashboard" || path.startsWith("/dashboard?")) {
    // Try to fetch from MCP server first
    try {
      const url = `${MCP_URL}${path}`;
      const response = await fetch(url, {
        method: "GET",
        headers: {
          "Authorization": `Bearer ${MCP_AUTH_TOKEN}`,
          "X-Session-Id": randomUUID(),
        },
        signal: AbortSignal.timeout(5000), // 5 second timeout
      });

      if (response.ok) {
        const data = await response.json();
        const nextResponse = NextResponse.json(data, { status: 200 });
        nextResponse.headers.set("Cache-Control", "public, max-age=5, stale-while-revalidate=10");
        return nextResponse;
      }
    } catch (error) {
      console.log("MCP server unavailable, using mock data");
    }

    // Return mock data if MCP server is unavailable
    const mockData = getMockDashboardData();
    const nextResponse = NextResponse.json(mockData, { status: 200 });
    nextResponse.headers.set("Cache-Control", "public, max-age=5, stale-while-revalidate=10");
    nextResponse.headers.set("X-MCP-Source", "mock");
    return nextResponse;
  }

  // For other paths, try to proxy to MCP server
  const url = `${MCP_URL}${path}`;
  
  const headers: Record<string, string> = {
    "Authorization": `Bearer ${MCP_AUTH_TOKEN}`,
  };
  
  const contentType = request.headers.get("content-type");
  if (contentType) {
    headers["Content-Type"] = contentType;
  }
  
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
      signal: AbortSignal.timeout(10000), // 10 second timeout
    });

    const data = await response.json();
    
    const nextResponse = NextResponse.json(data, { status: response.status });
    nextResponse.headers.set("X-Session-Id", sessionId);
    
    if (path.includes("/firewall/permissions") || path.includes("/firewall/status")) {
      nextResponse.headers.set("Cache-Control", "public, max-age=60, stale-while-revalidate=300");
    } else if (path.includes("/dashboard")) {
      nextResponse.headers.set("Cache-Control", "public, max-age=5, stale-while-revalidate=10");
    } else {
      nextResponse.headers.set("Cache-Control", "no-store, must-revalidate");
    }
    
    return nextResponse;
  } catch (error) {
    const errorMessage = error instanceof Error ? error.message : "Unknown error";
    console.error("MCP proxy error:", errorMessage);
    return NextResponse.json(
      { error: `Proxy error: ${errorMessage}` },
      { status: 500 }
    );
  }
}

// GET /api/mcp-proxy/dashboard
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
