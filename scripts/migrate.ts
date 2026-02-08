import { Pool } from "pg";

// Database connection
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
  if (!databaseUrl) throw new Error("Database URL not configured");

  const isSupabase = databaseUrl.includes("supabase.co");

  return new Pool({
    connectionString: databaseUrl,
    ssl: isSupabase ? { rejectUnauthorized: false } : undefined,
    max: 5,
  });
}

async function migrate() {
  const pool = getPool();
  const client = await pool.connect();

  try {
    console.log("Creating invites table...");
    
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

    await client.query(`CREATE INDEX IF NOT EXISTS idx_invites_token ON invites(token);`);
    await client.query(`CREATE INDEX IF NOT EXISTS idx_invites_email ON invites(email);`);
    await client.query(`CREATE INDEX IF NOT EXISTS idx_invites_invited_by ON invites("invitedBy");`);

    console.log("✅ Migration complete!");
  } catch (error) {
    console.error("❌ Migration failed:", error);
    process.exit(1);
  } finally {
    client.release();
    await pool.end();
  }
}

migrate();
