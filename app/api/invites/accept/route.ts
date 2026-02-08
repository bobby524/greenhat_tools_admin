import { NextRequest, NextResponse } from "next/server";
import { Pool } from "pg";
import { randomUUID } from "crypto";

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
 * POST /api/invites/accept
 * Accept an invite and create user account
 */
export async function POST(request: NextRequest) {
  const pool = getPool();
  if (!pool) {
    return NextResponse.json(
      { error: "Database not configured" },
      { status: 500 }
    );
  }

  try {
    const body = await request.json();
    const { token, name } = body;

    if (!token) {
      return NextResponse.json(
        { error: "Token is required" },
        { status: 400 }
      );
    }

    const client = await pool.connect();
    try {
      // Start transaction
      await client.query("BEGIN");

      // Get the invite
      const inviteResult = await client.query(
        `
        SELECT 
          i.id,
          i.email,
          i.token,
          i.role,
          i."invitedBy",
          i."expiresAt",
          i."usedAt",
          i."createdAt"
        FROM invites i
        WHERE i.token = $1
        FOR UPDATE
        `,
        [token]
      );

      if (inviteResult.rows.length === 0) {
        await client.query("ROLLBACK");
        return NextResponse.json(
          { error: "Invalid invite token" },
          { status: 404 }
        );
      }

      const invite = inviteResult.rows[0];

      // Check if already used
      if (invite.usedAt) {
        await client.query("ROLLBACK");
        return NextResponse.json(
          { error: "This invite has already been used" },
          { status: 410 }
        );
      }

      // Check if expired
      if (new Date(invite.expiresAt) < new Date()) {
        await client.query("ROLLBACK");
        return NextResponse.json(
          { error: "This invite has expired" },
          { status: 410 }
        );
      }

      // Check if user already exists
      const existingUser = await client.query(
        `SELECT id FROM "user" WHERE email = $1`,
        [invite.email]
      );

      if (existingUser.rows.length > 0) {
        await client.query("ROLLBACK");
        return NextResponse.json(
          { error: "A user with this email already exists" },
          { status: 409 }
        );
      }

      const userId = randomUUID();
      const now = new Date();

      // Create user
      await client.query(
        `
        INSERT INTO "user" (id, email, name, role, "emailVerified", "createdAt", "updatedAt")
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        `,
        [userId, invite.email, name || null, invite.role, true, now, now]
      );

      // Mark invite as used
      await client.query(
        `UPDATE invites SET "usedAt" = NOW() WHERE id = $1`,
        [invite.id]
      );

      // Commit transaction
      await client.query("COMMIT");

      return NextResponse.json({
        success: true,
        message: "Account created successfully",
        user: {
          id: userId,
          email: invite.email,
          name: name || null,
          role: invite.role,
          emailVerified: true,
        },
      });
    } catch (error) {
      await client.query("ROLLBACK");
      throw error;
    } finally {
      client.release();
    }
  } catch (error) {
    console.error("[API Invites Accept] Error accepting invite:", error);
    return NextResponse.json(
      { error: "Failed to accept invite", details: error instanceof Error ? error.message : "Unknown error" },
      { status: 500 }
    );
  }
}
