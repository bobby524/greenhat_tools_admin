import { Pool } from "pg";

// Workaround for SSL certificate issues in some environments
if (process.env.NODE_TLS_REJECT_UNAUTHORIZED === undefined) {
  process.env.NODE_TLS_REJECT_UNAUTHORIZED = "0";
}

// Singleton pool instance
let poolInstance: Pool | null = null;

/**
 * Get the database URL from environment variables
 * Checks multiple possible env var names for flexibility across environments
 */
export function getDatabaseUrl(): string | null {
  return (
    process.env.crm_POSTGRES_URL_NON_POOLING ||
    process.env.POSTGRES_URL ||
    process.env.DATABASE_URL ||
    process.env.CRM_POSTGRES_URL ||
    null
  );
}

/**
 * Get a singleton Pool instance for database connections
 * Configured with SSL settings for Supabase compatibility
 */
export function getPool(): Pool | null {
  // Return existing pool if already created
  if (poolInstance) {
    return poolInstance;
  }

  const databaseUrl = getDatabaseUrl();
  if (!databaseUrl) return null;

  const isSupabase = databaseUrl.includes("supabase.co");

  poolInstance = new Pool({
    connectionString: databaseUrl,
    ssl: isSupabase ? { rejectUnauthorized: false } : undefined,
    // Keep low for Supabase session poolers in local dev to avoid max-client exhaustion.
    max: Number(process.env.PG_POOL_MAX || 1),
    idleTimeoutMillis: 30000,
    connectionTimeoutMillis: 5000,
  });

  return poolInstance;
}

/**
 * Get a database pool with custom max connections
 * Useful for auth.ts which needs a larger pool
 */
export function getPoolWithOptions(options: { max: number }): Pool | null {
  const databaseUrl = getDatabaseUrl();
  if (!databaseUrl) return null;

  const isSupabase = databaseUrl.includes("supabase.co");

  return new Pool({
    connectionString: databaseUrl,
    ssl: isSupabase ? { rejectUnauthorized: false } : undefined,
    max: Number(process.env.PG_POOL_MAX || options.max || 1),
    idleTimeoutMillis: 30000,
    connectionTimeoutMillis: 5000,
  });
}
