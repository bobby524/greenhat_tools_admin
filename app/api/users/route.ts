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

export interface User {
  id: string;
  email: string;
  name: string | null;
  image: string | null;
  emailVerified: boolean;
  role: string;
  createdAt: string;
  updatedAt: string;
}

/**
 * GET /api/users
 * Fetch all users from the database
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
    const client = await pool.connect();
    
    try {
      // Query the user table directly
      const result = await client.query(`
        SELECT 
          id, 
          email, 
          name, 
          image, 
          "emailVerified", 
          COALESCE(role, 'user') as role, 
          "createdAt", 
          "updatedAt"
        FROM "user"
        ORDER BY "createdAt" DESC
      `);

      const users: User[] = result.rows.map((row) => ({
        id: row.id,
        email: row.email,
        name: row.name,
        image: row.image,
        emailVerified: row.emailVerified || false,
        role: row.role || "user",
        createdAt: row.createdAt,
        updatedAt: row.updatedAt,
      }));

      return NextResponse.json({ users });
    } finally {
      client.release();
    }
  } catch (error) {
    console.error("[API Users] Error fetching users:", error);
    return NextResponse.json(
      { error: "Failed to fetch users", details: error instanceof Error ? error.message : "Unknown error" },
      { status: 500 }
    );
  }
}

/**
 * PATCH /api/users
 * Update a user's role
 * Body: { userId: string, role: string }
 */
export async function PATCH(request: NextRequest) {
  const pool = getPool();
  
  if (!pool) {
    return NextResponse.json(
      { error: "Database not configured" },
      { status: 500 }
    );
  }

  try {
    const body = await request.json();
    const { userId, role } = body;

    if (!userId || !role) {
      return NextResponse.json(
        { error: "Missing required fields: userId and role" },
        { status: 400 }
      );
    }

    // Validate role value
    const validRoles = ["admin", "user", "viewer"];
    if (!validRoles.includes(role)) {
      return NextResponse.json(
        { error: `Invalid role. Must be one of: ${validRoles.join(", ")}` },
        { status: 400 }
      );
    }

    const client = await pool.connect();
    
    try {
      // Update the user's role
      const result = await client.query(
        `
        UPDATE "user"
        SET role = $1, "updatedAt" = NOW()
        WHERE id = $2
        RETURNING 
          id, 
          email, 
          name, 
          image, 
          "emailVerified", 
          COALESCE(role, 'user') as role, 
          "createdAt", 
          "updatedAt"
        `,
        [role, userId]
      );

      if (result.rowCount === 0) {
        return NextResponse.json(
          { error: "User not found" },
          { status: 404 }
        );
      }

      const user: User = {
        id: result.rows[0].id,
        email: result.rows[0].email,
        name: result.rows[0].name,
        image: result.rows[0].image,
        emailVerified: result.rows[0].emailVerified || false,
        role: result.rows[0].role || "user",
        createdAt: result.rows[0].createdAt,
        updatedAt: result.rows[0].updatedAt,
      };

      return NextResponse.json({ user, success: true });
    } finally {
      client.release();
    }
  } catch (error) {
    console.error("[API Users] Error updating user:", error);
    return NextResponse.json(
      { error: "Failed to update user", details: error instanceof Error ? error.message : "Unknown error" },
      { status: 500 }
    );
  }
}
