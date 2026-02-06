import { betterAuth, BetterAuthOptions } from "better-auth";
import { admin } from "better-auth/plugins";

// Get database URL - prefer non-pooling for Better Auth
function getDatabaseUrl(): string | null {
  // Try non-pooling URL first (better for Better Auth)
  const url = process.env.CRM_POSTGRES_URL_NON_POOLING ||
         process.env.crm_POSTGRES_URL_NON_POOLING ||
         process.env.POSTGRES_URL || 
         process.env.DATABASE_URL || 
         process.env.CRM_POSTGRES_URL ||
         process.env.crm_POSTGRES_URL || null;
  console.log("[Auth] Selected DB URL type:", process.env.CRM_POSTGRES_URL_NON_POOLING ? "NON_POOLING" : "POOLING");
  return url;
}

// Export auth config
export function getAuthConfig(): BetterAuthOptions | null {
  const databaseUrl = getDatabaseUrl();
  
  if (!databaseUrl) {
    console.error("[Auth] Database not configured");
    return null;
  }
  
  if (!process.env.BETTER_AUTH_SECRET) {
    console.error("[Auth] BETTER_AUTH_SECRET not configured");
    return null;
  }
  
  // Parse the URL to check if it's Supabase
  const isSupabase = databaseUrl.includes('supabase.co') || databaseUrl.includes('pooler.supabase');
  
  // Build connection string with proper SSL settings for Supabase
  let connectionString = databaseUrl;
  if (isSupabase && !connectionString.includes('sslmode=')) {
    connectionString += connectionString.includes('?') ? '&' : '?';
    connectionString += 'sslmode=require';
  }
  
  console.log("[Auth] Database URL configured:", connectionString.substring(0, 40) + "...");
  console.log("[Auth] Is Supabase:", isSupabase);
  
  return {
    secret: process.env.BETTER_AUTH_SECRET,
    baseURL: process.env.BETTER_AUTH_URL || "https://admin.greenhatsec.com",
    trustedOrigins: ["https://admin.greenhatsec.com", "https://tools.greenhatsec.com"],
    database: {
      provider: "pg",
      url: connectionString,
    },
    emailAndPassword: {
      enabled: true,
      minPasswordLength: 8,
    },
    socialProviders: {
      google: {
        clientId: process.env.GOOGLE_CLIENT_ID || "",
        clientSecret: process.env.GOOGLE_CLIENT_SECRET || "",
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
  console.log("[Auth] Creating auth instance...");
  
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
    
    console.log("[Auth] Initializing Better Auth...");
    const instance = betterAuth(config);
    console.log("[Auth] Better Auth initialized successfully");
    return instance;
  } catch (error) {
    const err = error instanceof Error ? error.message : "Unknown error";
    console.error("[Auth] Failed to initialize:", err);
    if (error instanceof Error && error.stack) {
      console.error("[Auth] Stack:", error.stack);
    }
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
  console.log("[Auth] Handler called for:", request.url);
  const instance = getAuthInstance();
  
  if (!instance) {
    console.error("[Auth] Auth instance not available:", initError);
    return new Response(
      JSON.stringify({ 
        error: "Auth not configured", 
        details: initError || "Unknown initialization error",
      }), 
      { status: 503, headers: { "Content-Type": "application/json" } }
    );
  }

  try {
    return await instance.handler(request);
  } catch (error) {
    const err = error instanceof Error ? error.message : String(error);
    console.error("[Auth] Handler error:", err);
    return new Response(
      JSON.stringify({ error: "Auth handler error", message: err }),
      { status: 500, headers: { "Content-Type": "application/json" } }
    );
  }
}

// Export auth object
export const auth = {
  handler,
  get api() {
    const instance = getAuthInstance();
    return instance?.api || {};
  },
};