import { NextRequest, NextResponse } from "next/server";
import { Pool } from "pg";

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
      let query = `
        SELECT c.*, comp.name as company_name 
        FROM crm_contacts c
        LEFT JOIN crm_companies comp ON c.company_id = comp.id
        WHERE c.archived = false
      `;
      const params: (string | number)[] = [];

      if (search) {
        query += ` AND (c.name ILIKE $1 OR c.email ILIKE $1)`;
        params.push(`%${search}%`);
      }

      query += ` ORDER BY c.updated_at DESC LIMIT $${params.length + 1} OFFSET $${params.length + 2}`;
      params.push(limit, offset);

      const result = await client.query(query, params);
      
      const countResult = await client.query(
        `SELECT COUNT(*) FROM crm_contacts WHERE archived = false`
      );

      return NextResponse.json({
        contacts: result.rows,
        total: parseInt(countResult.rows[0].count),
      });
    } finally {
      client.release();
    }
  } catch (error) {
    console.error("[CRM API] Error fetching contacts:", error);
    return NextResponse.json({ error: "Failed to fetch contacts" }, { status: 500 });
  }
}

export async function POST(request: NextRequest) {
  const pool = getPool();
  if (!pool) {
    return NextResponse.json({ error: "Database not configured" }, { status: 500 });
  }

  try {
    const body = await request.json();
    const { name, email, phone, company_id, lifecycle_stage } = body;

    if (!name || !email) {
      return NextResponse.json({ error: "Name and email are required" }, { status: 400 });
    }

    const client = await pool.connect();
    try {
      const result = await client.query(
        `INSERT INTO crm_contacts (name, email, phone, company_id, lifecycle_stage)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING *`,
        [name, email, phone || null, company_id || null, lifecycle_stage || "Lead"]
      );

      return NextResponse.json({ contact: result.rows[0] });
    } finally {
      client.release();
    }
  } catch (error) {
    console.error("[CRM API] Error creating contact:", error);
    return NextResponse.json({ error: "Failed to create contact" }, { status: 500 });
  }
}
