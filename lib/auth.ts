import { betterAuth } from "better-auth";
import { admin } from "better-auth/plugins";
import { Pool } from "pg";

// Get database URL
function getDatabaseUrl(): string | null {
  const url = process.env.CRM_POSTGRES_URL_NON_POOLING ||
         process.env.crm_POSTGRES_URL_NON_POOLING ||
         process.env.POSTGRES_URL || 
         process.env.DATABASE_URL || 
         process.env.CRM_POSTGRES_URL ||
         process.env.crm_POSTGRES_URL;
  return url;
}

// Create database pool
function createPool() {
  const databaseUrl = getDatabaseUrl();
  if (!databaseUrl) return null;

  console.log("[Auth] Creating database pool...");
  
  return new Pool({
    connectionString: databaseUrl,
    ssl: { rejectUnauthorized: false },
    max: 10,
    idleTimeoutMillis: 30000,
    connectionTimeoutMillis: 5000,
  });
}

// Lazy initialization
let authInstance: ReturnType<typeof betterAuth> | null = null;
let initError: string | null = null;
let initAttempts = 0;

function getAuthInstance() {
  if (authInstance) return authInstance;
  
  initAttempts++;
  console.log(`[Auth] Initialization attempt ${initAttempts}`);
  
  const pool = createPool();

  if (!pool) {
    initError = "Database not configured";
    console.error("[Auth]", initError);
    return null;
  }

  if (!process.env.BETTER_AUTH_SECRET) {
    initError = "BETTER_AUTH_SECRET not configured";
    console.error("[Auth]", initError);
    return null;
  }

  try {
    console.log("[Auth] Creating Better Auth with Pool...");
    console.log("[Auth] BETTER_AUTH_SECRET:", process.env.BETTER_AUTH_SECRET ? "Set" : "Not set");
    console.log("[Auth] GOOGLE_CLIENT_ID:", process.env.GOOGLE_CLIENT_ID ? "Set" : "Not set");
    
    authInstance = betterAuth({
      secret: process.env.BETTER_AUTH_SECRET,
      baseURL: process.env.BETTER_AUTH_URL || "https://admin.greenhatsec.com",
      trustedOrigins: ["https://admin.greenhatsec.com", "https://tools.greenhatsec.com"],
      database: pool,  // Pass Pool directly - this is the correct way!
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
    });
    
    console.log("[Auth] Better Auth initialized successfully");
    return authInstance;
  } catch (error) {
    const err = error instanceof Error ? error.message : String(error);
    const stack = error instanceof Error ? error.stack : "";
    initError = err;
    console.error("[Auth] Failed to initialize:", err);
    console.error("[Auth] Stack:", stack);
    return null;
  }
}

// Handler
async function handler(request: Request): Promise<Response> {
  console.log("[Auth] Handler called:", request.method, request.url);
  
  const instance = getAuthInstance();
  
  if (!instance) {
    console.error("[Auth] Auth instance not available. Init error:", initError);
    return new Response(
      JSON.stringify({ 
        error: "Auth not configured", 
        details: initError,
        initAttempts
      }), 
      { status: 503, headers: { "Content-Type": "application/json" } }
    );
  }

  try {
    console.log("[Auth] Calling Better Auth handler...");
    const response = await instance.handler(request);
    console.log("[Auth] Better Auth response:", response.status);
    return response;
  } catch (error) {
    const err = error instanceof Error ? error.message : String(error);
    const stack = error instanceof Error ? error.stack : "";
    console.error("[Auth] Handler error:", err);
    console.error("[Auth] Handler stack:", stack);
    return new Response(
      JSON.stringify({ 
        error: "Auth handler error", 
        message: err,
        stack: stack?.substring(0, 500)
      }),
      { status: 500, headers: { "Content-Type": "application/json" } }
    );
  }
}

export const auth = { handler };