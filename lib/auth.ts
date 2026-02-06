import { betterAuth, BetterAuthOptions } from "better-auth";
import { admin } from "better-auth/plugins";
import { Pool } from "pg";

// Get database URL
function getDatabaseUrl(): string | null {
  return process.env.POSTGRES_URL || 
         process.env.DATABASE_URL || 
         process.env.crm_POSTGRES_URL || null;
}

// Get database pool
function getDatabasePool() {
  const databaseUrl = getDatabaseUrl();
  if (!databaseUrl) return null;
  
  const dbUrl = process.env.crm_POSTGRES_URL || databaseUrl;
  const isSupabase = dbUrl.includes('supabase.co');
  
  return new Pool({
    connectionString: dbUrl,
    ssl: isSupabase 
      ? { rejectUnauthorized: false }
      : undefined,
    max: 20,
    idleTimeoutMillis: 30000,
    connectionTimeoutMillis: 5000,
  });
}

// Export auth config
export function getAuthConfig(): BetterAuthOptions | null {
  const databaseUrl = getDatabaseUrl();
  const pool = getDatabasePool();
  
  if (!databaseUrl || !pool) {
    console.error("[Auth] Database not configured");
    return null;
  }
  
  if (!process.env.BETTER_AUTH_SECRET) {
    console.error("[Auth] BETTER_AUTH_SECRET not configured");
    return null;
  }
  
  return {
    secret: process.env.BETTER_AUTH_SECRET,
    baseURL: process.env.BETTER_AUTH_URL || "https://admin.greenhatsec.com",
    trustedOrigins: ["https://admin.greenhatsec.com", "https://tools.greenhatsec.com"],
    database: pool,
    emailAndPassword: {
      enabled: true,
      minPasswordLength: 8,
    },
    socialProviders: {
      google: {
        clientId: process.env.GOOGLE_CLIENT_ID || "",
        clientSecret: process.env.GOOGLE_CLIENT_SECRET || "",
        ...(process.env.GOOGLE_ALLOWED_DOMAIN ? {
          authorization: {
            params: { hd: process.env.GOOGLE_ALLOWED_DOMAIN },
          },
        } : {}),
      },
    },
    plugins: [
      admin({
        adminUserIds: ["09649c79-975a-4967-9299-440b2b0fadee"],
      }),
    ],
    session: { expiresIn: 60 * 60 * 24 * 7 },
    advanced: {
      useSecureCookies: process.env.NODE_ENV === "production",
      cookiePrefix: "greenhat_tools",
      crossSubDomainCookies: {
        enabled: true,
        domain: ".greenhatsec.com",
      },
    },
  };
}

// Singleton auth instance
let authInstance: ReturnType<typeof betterAuth> | null = null;
let initError: string | null = null;

function createAuth() {
  const databaseUrl = getDatabaseUrl();

  if (!databaseUrl) {
    const err = "Database URL not configured";
    console.error("[Auth] " + err);
    initError = err;
    return null;
  }

  if (!process.env.BETTER_AUTH_SECRET) {
    const err = "BETTER_AUTH_SECRET not configured";
    console.error("[Auth] " + err);
    initError = err;
    return null;
  }

  try {
    const config = getAuthConfig();
    if (!config) {
      throw new Error("Failed to create auth config");
    }
    
    const instance = betterAuth(config);
    return instance;
  } catch (error) {
    const err = error instanceof Error ? error.message : "Unknown error";
    console.error("[Auth] Failed to initialize:", err);
    initError = err;
    return null;
  }
}

function getAuthInstance() {
  if (!authInstance) {
    authInstance = createAuth();
  }
  return authInstance;
}

// Handler that initializes on first use
async function handler(request: Request): Promise<Response> {
  const instance = getAuthInstance();
  
  if (!instance) {
    return new Response(
      JSON.stringify({ 
        error: "Auth not configured", 
        details: initError || "Unknown initialization error",
      }), 
      { status: 503, headers: { "Content-Type": "application/json" } }
    );
  }

  return await instance.handler(request);
}

// Export auth object
export const auth = {
  handler,
  get api() {
    const instance = getAuthInstance();
    return instance?.api || {};
  },
};