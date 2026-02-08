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

// GET /api/crm/companies
export async function GET(request: NextRequest) {
  const pool = getPool();
  if (!pool) {
    return NextResponse.json({ error: "Database not configured" }, { status: 500 });
  }

  try {
    const { searchParams } = new URL(request.url);
    const limit = parseInt(searchParams.get("limit") || "50");
    const offset = parseInt(searchParams.get("offset") || "0");
    const search = searchParams.get("search");

    const client = await pool.connect();
    try {
      let query = `SELECT * FROM crm_companies WHERE archived = false`;
      const params: (string | number)[] = [];

      if (search) {
        query += ` AND (name ILIKE $1 OR domain ILIKE $1)`;
        params.push(`%${search}%`);
      }

      query += ` ORDER BY updated_at DESC LIMIT $${params.length + 1} OFFSET $${params.length + 2}`;
      params.push(limit, offset);

      const result = await client.query(query, params);
      
      // Get total count
      const countResult = await client.query(
        `SELECT COUNT(*) FROM crm_companies WHERE archived = false`
      );

      return NextResponse.json({
        companies: result.rows,
        total: parseInt(countResult.rows[0].count),
      });
    } finally {
      client.release();
    }
  } catch (error) {
    console.error("[CRM API] Error fetching companies:", error);
    return NextResponse.json(
      { error: "Failed to fetch companies" },
      { status: 500 }
    );
  }
}

// POST /api/crm/companies
export async function POST(request: NextRequest) {
  const pool = getPool();
  if (!pool) {
    return NextResponse.json({ error: "Database not configured" }, { status: 500 });
  }

  try {
    const body = await request.json();
    const { name, domain, industry, size, website } = body;

    if (!name) {
      return NextResponse.json({ error: "Name is required" }, { status: 400 });
    }

    const client = await pool.connect();
    try {
      const result = await client.query(
        `INSERT INTO crm_companies (name, domain, industry, size, website)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING *`,
        [name, domain || null, industry || null, size || null, website || null]
      );

      return NextResponse.json({ company: result.rows[0] });
    } finally {
      client.release();
    }
  } catch (error) {
    console.error("[CRM API] Error creating company:", error);
    return NextResponse.json(
      { error: "Failed to create company" },
      { status: 500 }
    );
  }
}
