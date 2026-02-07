import { betterAuth } from "better-auth";
import { admin } from "better-auth/plugins";

// Get database URL
function getDatabaseUrl(): string | null {
  let url = process.env.CRM_POSTGRES_URL_NON_POOLING ||
         process.env.crm_POSTGRES_URL_NON_POOLING ||
         process.env.POSTGRES_URL || 
         process.env.DATABASE_URL || 
         process.env.CRM_POSTGRES_URL ||
         process.env.crm_POSTGRES_URL ||
         null;
  
  if (url && url.includes('supabase') && !url.includes('sslmode=')) {
    url += url.includes('?') ? '&' : '?';
    url += 'sslmode=require';
  }
  
  return url;
}

// Lazy initialization
let authInstance: ReturnType<typeof betterAuth> | null = null;
let initError: string | null = null;

function getAuthInstance() {
  if (authInstance) return authInstance;
  
  const databaseUrl = getDatabaseUrl();

  if (!databaseUrl) {
    initError = "Database URL not configured";
    console.error("[Auth]", initError);
    return null;
  }

  if (!process.env.BETTER_AUTH_SECRET) {
    initError = "BETTER_AUTH_SECRET not configured";
    console.error("[Auth]", initError);
    return null;
  }

  try {
    console.log("[Auth] Initializing with existing schema...");
    
    authInstance = betterAuth({
      secret: process.env.BETTER_AUTH_SECRET,
      baseURL: process.env.BETTER_AUTH_URL || "https://admin.greenhatsec.com",
      trustedOrigins: ["https://admin.greenhatsec.com", "https://tools.greenhatsec.com"],
      database: databaseUrl,
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
        // Don't auto-create tables - they already exist
        database: {
          type: "postgres",
          url: databaseUrl,
        },
      },
    });
    
    console.log("[Auth] Initialized successfully");
    return authInstance;
  } catch (error) {
    const err = error instanceof Error ? error.message : String(error);
    initError = err;
    console.error("[Auth] Failed:", err);
    return null;
  }
}

// Handler
async function handler(request: Request): Promise<Response> {
  const instance = getAuthInstance();
  
  if (!instance) {
    return new Response(
      JSON.stringify({ error: "Auth not configured", details: initError }), 
      { status: 503, headers: { "Content-Type": "application/json" } }
    );
  }

  try {
    return await instance.handler(request);
  } catch (error) {
    const err = error instanceof Error ? error.message : String(error);
    return new Response(
      JSON.stringify({ error: "Auth error", message: err }),
      { status: 500, headers: { "Content-Type": "application/json" } }
    );
  }
}

export const auth = { handler };