import { NextRequest, NextResponse } from "next/server";
import { Pool } from "pg";
import { sendInviteEmail } from "@/lib/email";
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

// Verify session and get user info
async function verifySession(sessionToken: string): Promise<{ id: string; role: string } | null> {
  const pool = getPool();
  if (!pool) return null;

  const client = await pool.connect();
  try {
    // Query the session and user
    const result = await client.query(
      `
      SELECT u.id, u.role
      FROM "session" s
      JOIN "user" u ON s."userId" = u.id
      WHERE s.token = $1 AND s.expires > NOW()
      `,
      [sessionToken]
    );

    if (result.rows.length === 0) return null;
    return result.rows[0];
  } finally {
    client.release();
  }
}

// Check if user is admin
async function isAdmin(request: NextRequest): Promise<boolean> {
  const sessionToken = getSessionCookie(request);
  if (!sessionToken) return false;

  const user = await verifySession(sessionToken);
  return user?.role === "admin";
}

/**
 * GET /api/invites
 * List all invites (admin only)
 */
export async function GET(request: NextRequest) {
  // Check admin permission
  if (!await isAdmin(request)) {
    return NextResponse.json(
      { error: "Unauthorized - Admin access required" },
      { status: 403 }
    );
  }

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
      const result = await client.query(`
        SELECT 
          i.id,
          i.email,
          i.token,
          i.role,
          i."invitedBy",
          i."expiresAt",
          i."usedAt",
          i."createdAt",
          u.name as "invitedByName",
          u.email as "invitedByEmail"
        FROM invites i
        LEFT JOIN "user" u ON i."invitedBy" = u.id
        ORDER BY i."createdAt" DESC
      `);

      const invites = result.rows.map((row) => ({
        id: row.id,
        email: row.email,
        token: row.token,
        role: row.role,
        invitedBy: row.invitedBy,
        invitedByName: row.invitedByName,
        invitedByEmail: row.invitedByEmail,
        expiresAt: row.expiresAt,
        usedAt: row.usedAt,
        createdAt: row.createdAt,
        status: row.usedAt ? "used" : new Date(row.expiresAt) < new Date() ? "expired" : "pending",
      }));

      return NextResponse.json({ invites });
    } finally {
      client.release();
    }
  } catch (error) {
    console.error("[API Invites] Error fetching invites:", error);
    return NextResponse.json(
      { error: "Failed to fetch invites", details: error instanceof Error ? error.message : "Unknown error" },
      { status: 500 }
    );
  }
}

/**
 * POST /api/invites
 * Create a new invite (admin only)
 */
export async function POST(request: NextRequest) {
  // Check admin permission
  if (!await isAdmin(request)) {
    return NextResponse.json(
      { error: "Unauthorized - Admin access required" },
      { status: 403 }
    );
  }

  const pool = getPool();
  if (!pool) {
    return NextResponse.json(
      { error: "Database not configured" },
      { status: 500 }
    );
  }

  try {
    const body = await request.json();
    const { email, role = "user" } = body;

    if (!email) {
      return NextResponse.json(
        { error: "Email is required" },
        { status: 400 }
      );
    }

    // Validate role
    const validRoles = ["admin", "user", "viewer"];
    if (!validRoles.includes(role)) {
      return NextResponse.json(
        { error: `Invalid role. Must be one of: ${validRoles.join(", ")}` },
        { status: 400 }
      );
    }

    // Get current user from session
    const sessionToken = getSessionCookie(request);
    const currentUser = sessionToken ? await verifySession(sessionToken) : null;
    
    if (!currentUser) {
      return NextResponse.json(
        { error: "Session not found" },
        { status: 401 }
      );
    }

    const client = await pool.connect();
    try {
      // Check if there's already a pending invite for this email
      const existingInvite = await client.query(
        `SELECT id FROM invites WHERE email = $1 AND "usedAt" IS NULL AND "expiresAt" > NOW()`,
        [email]
      );

      if (existingInvite.rows.length > 0) {
        return NextResponse.json(
          { error: "An active invite already exists for this email" },
          { status: 409 }
        );
      }

      // Check if user already exists
      const existingUser = await client.query(
        `SELECT id FROM "user" WHERE email = $1`,
        [email]
      );

      if (existingUser.rows.length > 0) {
        return NextResponse.json(
          { error: "A user with this email already exists" },
          { status: 409 }
        );
      }

      // Generate unique token and expiration
      const token = randomUUID();
      const expiresAt = new Date();
      expiresAt.setDate(expiresAt.getDate() + 7); // 7 days from now

      // Create invite
      const result = await client.query(
        `
        INSERT INTO invites (email, token, role, "invitedBy", "expiresAt")
        VALUES ($1, $2, $3, $4, $5)
        RETURNING *
        `,
        [email, token, role, currentUser.id, expiresAt]
      );

      const invite = result.rows[0];

      // Get inviter info for email
      const inviterResult = await client.query(
        `SELECT name, email FROM "user" WHERE id = $1`,
        [currentUser.id]
      );
      const inviter = inviterResult.rows[0];

      // Send invite email
      const emailResult = await sendInviteEmail({
        email,
        token,
        role,
        invitedByName: inviter?.name,
      });

      if (!emailResult.success) {
        console.error("[API Invites] Failed to send email:", emailResult.error);
        // Don't fail the request, just log the error
      }

      return NextResponse.json({ 
        invite: {
          id: invite.id,
          email: invite.email,
          token: invite.token,
          role: invite.role,
          invitedBy: invite.invitedBy,
          expiresAt: invite.expiresAt,
          createdAt: invite.createdAt,
          status: "pending",
        },
        emailSent: emailResult.success 
      });
    } finally {
      client.release();
    }
  } catch (error) {
    console.error("[API Invites] Error creating invite:", error);
    return NextResponse.json(
      { error: "Failed to create invite", details: error instanceof Error ? error.message : "Unknown error" },
      { status: 500 }
    );
  }
}

/**
 * DELETE /api/invites
 * Revoke an invite (admin only)
 */
export async function DELETE(request: NextRequest) {
  // Check admin permission
  if (!await isAdmin(request)) {
    return NextResponse.json(
      { error: "Unauthorized - Admin access required" },
      { status: 403 }
    );
  }

  const pool = getPool();
  if (!pool) {
    return NextResponse.json(
      { error: "Database not configured" },
      { status: 500 }
    );
  }

  try {
    const { searchParams } = new URL(request.url);
    const inviteId = searchParams.get("id");

    if (!inviteId) {
      return NextResponse.json(
        { error: "Invite ID is required" },
        { status: 400 }
      );
    }

    const client = await pool.connect();
    try {
      // Mark as used (effectively revoking it)
      const result = await client.query(
        `
        UPDATE invites
        SET "usedAt" = NOW()
        WHERE id = $1 AND "usedAt" IS NULL
        RETURNING *
        `,
        [inviteId]
      );

      if (result.rowCount === 0) {
        return NextResponse.json(
          { error: "Invite not found or already used/revoked" },
          { status: 404 }
        );
      }

      return NextResponse.json({ success: true, message: "Invite revoked successfully" });
    } finally {
      client.release();
    }
  } catch (error) {
    console.error("[API Invites] Error revoking invite:", error);
    return NextResponse.json(
      { error: "Failed to revoke invite", details: error instanceof Error ? error.message : "Unknown error" },
      { status: 500 }
    );
  }
}
