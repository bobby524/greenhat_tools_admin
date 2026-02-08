import { NextRequest, NextResponse } from "next/server";
import { Pool } from "pg";

// Workaround for SSL certificate issues
if (process.env.NODE_TLS_REJECT_UNAUTHORIZED === undefined) {
  process.env.NODE_TLS_REJECT_UNAUTHORIZED = "0";
}

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
  });
}

// Simple session check - in production, verify admin properly
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

export async function POST(request: NextRequest) {
  // Check if user is authenticated
  const sessionToken = getSessionCookie(request);
  if (!sessionToken) {
    return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
  }

  const pool = getPool();
  if (!pool) {
    return NextResponse.json({ error: "Database not configured" }, { status: 500 });
  }

  try {
    const client = await pool.connect();

    try {
      // Create invites table
      await client.query(`
        CREATE TABLE IF NOT EXISTS invites (
          id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
          email VARCHAR(255) NOT NULL,
          token VARCHAR(255) UNIQUE NOT NULL,
          role VARCHAR(50) NOT NULL DEFAULT 'user',
          "invitedBy" UUID NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
          "expiresAt" TIMESTAMP WITH TIME ZONE NOT NULL,
          "usedAt" TIMESTAMP WITH TIME ZONE,
          "createdAt" TIMESTAMP WITH TIME ZONE DEFAULT NOW()
        );
      `);

      // Create indexes
      await client.query(`CREATE INDEX IF NOT EXISTS idx_invites_token ON invites(token);`);
      await client.query(`CREATE INDEX IF NOT EXISTS idx_invites_email ON invites(email);`);
      await client.query(`CREATE INDEX IF NOT EXISTS idx_invites_invited_by ON invites("invitedBy");`);

      return NextResponse.json({ success: true, message: "Invites table created successfully" });
    } finally {
      client.release();
    }
  } catch (error) {
    console.error("[Migrate] Error:", error);
    return NextResponse.json(
      { error: "Migration failed", details: error instanceof Error ? error.message : "Unknown error" },
      { status: 500 }
    );
  }
}
