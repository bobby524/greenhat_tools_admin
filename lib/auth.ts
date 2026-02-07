import { betterAuth } from "better-auth";
import { admin } from "better-auth/plugins";

// Lazy initialization - only create auth when first needed
let authInstance: ReturnType<typeof betterAuth> | null = null;
let initError: string | null = null;

function getAuthInstance() {
  if (authInstance) return authInstance;
  
  const databaseUrl = process.env.CRM_POSTGRES_URL_NON_POOLING ||
         process.env.crm_POSTGRES_URL_NON_POOLING ||
         process.env.POSTGRES_URL || 
         process.env.DATABASE_URL || 
         process.env.CRM_POSTGRES_URL ||
         process.env.crm_POSTGRES_URL;

  if (!databaseUrl) {
    initError = "Database URL not configured. Checked: CRM_POSTGRES_URL_NON_POOLING, crm_POSTGRES_URL_NON_POOLING, POSTGRES_URL, DATABASE_URL, CRM_POSTGRES_URL, crm_POSTGRES_URL";
    console.error("[Auth]", initError);
    return null;
  }

  if (!process.env.BETTER_AUTH_SECRET) {
    initError = "BETTER_AUTH_SECRET not configured";
    console.error("[Auth]", initError);
    return null;
  }

  try {
    console.log("[Auth] Initializing with database...");
    
    // Add sslmode for Supabase
    let url = databaseUrl;
    if (url.includes('supabase') && !url.includes('sslmode=')) {
      url += url.includes('?') ? '&' : '?';
      url += 'sslmode=require';
    }
    
    authInstance = betterAuth({
      secret: process.env.BETTER_AUTH_SECRET,
      baseURL: process.env.BETTER_AUTH_URL || "https://admin.greenhatsec.com",
      trustedOrigins: ["https://admin.greenhatsec.com", "https://tools.greenhatsec.com"],
      database: url,  // String URL - Better Auth creates its own adapter
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
    
    console.log("[Auth] Initialized successfully");
    return authInstance;
  } catch (error) {
    initError = error instanceof Error ? error.message : String(error);
    console.error("[Auth] Failed to initialize:", initError);
    return null;
  }
}

// Handler
async function handler(request: Request): Promise<Response> {
  console.log("[Auth] Handler called:", request.method, request.url);
  
  const instance = getAuthInstance();
  
  if (!instance) {
    return new Response(
      JSON.stringify({ 
        error: "Auth not configured", 
        details: initError 
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
      JSON.stringify({ error: "Auth error", message: err }),
      { status: 500, headers: { "Content-Type": "application/json" } }
    );
  }
}

export const auth = { handler };