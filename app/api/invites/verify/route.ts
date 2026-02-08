import { NextRequest, NextResponse } from "next/server";
import { Pool } from "pg";

// Workaround for SSL certificate issues in some environments
if (process.env.NODE_TLS_REJECT_UNAUTHORIZED === undefined) {
  process.env.NODE_TLS_REJECT_UNAUTHORIZED = "0";
}

// Use the same database connection logic as auth.ts
function getDatabaseUrl(): string | null {
  return (
    process.env.crm_POSTGRES_URL_NON_POOLING ||
    process.env.POSTGRES_URL ||
    process.env.DATABASE_URL ||
    process.env.CRM_POSTGRES_URL ||
    null
  );
}

function getPool() {
  const databaseUrl = getDatabaseUrl();
  if (!databaseUrl) return null;

  const isSupabase = databaseUrl.includes("supabase.co");

  return new Pool({
    connectionString: databaseUrl,
    ssl: isSupabase ? { rejectUnauthorized: false } : undefined,
    max: 5,
    idleTimeoutMillis: 30000,
    connectionTimeoutMillis: 5000,
  });
}

/**
 * GET /api/invites/verify?token=xxx
 * Verify an invite token (public endpoint)
 */
export async function GET(request: NextRequest) {
  const pool = getPool();
  if (!pool) {
    return NextResponse.json(
      { error: "Database not configured" },
      { status: 500 }
    );
  }

  try {
    const { searchParams } = new URL(request.url);
    const token = searchParams.get("token");

    if (!token) {
      return NextResponse.json(
        { error: "Token is required" },
        { status: 400 }
      );
    }

    const client = await pool.connect();
    try {
      const result = await client.query(
        `
        SELECT 
          i.id,
          i.email,
          i.token,
          i.role,
          i."invitedBy",
          i."expiresAt",
          i."usedAt",
          i."createdAt",
          u.name as "invitedByName"
        FROM invites i
        LEFT JOIN "user" u ON i."invitedBy" = u.id
        WHERE i.token = $1
        `,
        [token]
      );

      if (result.rows.length === 0) {
        return NextResponse.json(
          { error: "Invalid invite token" },
          { status: 404 }
        );
      }

      const invite = result.rows[0];

      // Check if already used
      if (invite.usedAt) {
        return NextResponse.json(
          { error: "This invite has already been used" },
          { status: 410 }
        );
      }

      // Check if expired
      if (new Date(invite.expiresAt) < new Date()) {
        return NextResponse.json(
          { error: "This invite has expired" },
          { status: 410 }
        );
      }

      return NextResponse.json({
        valid: true,
        invite: {
          id: invite.id,
          email: invite.email,
          role: invite.role,
          invitedBy: invite.invitedBy,
          invitedByName: invite.invitedByName,
          expiresAt: invite.expiresAt,
        },
      });
    } finally {
      client.release();
    }
  } catch (error) {
    console.error("[API Invites Verify] Error verifying token:", error);
    return NextResponse.json(
      { error: "Failed to verify invite", details: error instanceof Error ? error.message : "Unknown error" },
      { status: 500 }
    );
  }
}
