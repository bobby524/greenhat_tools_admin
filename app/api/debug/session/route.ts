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

export async function GET(request: NextRequest) {
  const pool = getPool();
  if (!pool) {
    return NextResponse.json({ error: "Database not configured" }, { status: 500 });
  }

  const client = await pool.connect();
  try {
    // Get session table structure
    const tableInfo = await client.query(`
      SELECT column_name, data_type 
      FROM information_schema.columns 
      WHERE table_name = 'session'
      ORDER BY ordinal_position
    `);

    // Get sample session data (safely)
    const sampleSession = await client.query(`
      SELECT * FROM "session" LIMIT 1
    `);

    // Get all cookies from request
    const cookies: Record<string, string> = {};
    request.cookies.getAll().forEach(c => {
      cookies[c.name] = c.value.substring(0, 20) + "...";
    });

    // Check if any session matches cookie tokens
    const sessionCookie = request.cookies.get("greenhat_tools.session_token")?.value || 
                         request.cookies.get("__Secure-greenhat_tools.session_token")?.value;
    
    let sessionMatch = null;
    if (sessionCookie) {
      // Try different column names
      const possibleTokenColumns = ["token", "sessionToken", "id"];
      for (const col of possibleTokenColumns) {
        try {
          const result = await client.query(
            `SELECT * FROM "session" WHERE "${col}" = $1 LIMIT 1`,
            [sessionCookie]
          );
          if (result.rows.length > 0) {
            sessionMatch = { column: col, data: result.rows[0] };
            break;
          }
        } catch (e) {
          // Column doesn't exist, try next
        }
      }
    }

    return NextResponse.json({
      sessionTableColumns: tableInfo.rows,
      sampleSessionKeys: sampleSession.rows.length > 0 ? Object.keys(sampleSession.rows[0]) : [],
      sampleSession: sampleSession.rows.length > 0 ? { ...sampleSession.rows[0], token: "[REDACTED]" } : null,
      cookiesPresent: Object.keys(cookies),
      sessionMatch: sessionMatch ? { column: sessionMatch.column, userId: sessionMatch.data.userId } : null,
    });
  } catch (error) {
    return NextResponse.json(
      { error: "Debug failed", details: error instanceof Error ? error.message : "Unknown" },
      { status: 500 }
    );
  } finally {
    client.release();
  }
}
