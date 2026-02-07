import { betterAuth } from "better-auth";
import { admin } from "better-auth/plugins";
import { Resend } from "resend";

// Lazy initialization of Resend
function getResend(): Resend {
  const apiKey = process.env.RESEND_API_KEY;
  if (!apiKey) {
    throw new Error("RESEND_API_KEY not set");
  }
  return new Resend(apiKey);
}

// Database URL
const databaseUrl = process.env.CRM_POSTGRES_URL_NON_POOLING || 
                   process.env.crm_POSTGRES_URL_NON_POOLING ||
                   process.env.POSTGRES_URL || 
                   process.env.DATABASE_URL || 
                   process.env.CRM_POSTGRES_URL ||
                   process.env.crm_POSTGRES_URL;

console.log("[Auth] Database URL found:", databaseUrl ? "YES" : "NO");
console.log("[Auth] BETTER_AUTH_SECRET found:", process.env.BETTER_AUTH_SECRET ? "YES" : "NO");

if (!databaseUrl) {
  console.error("[Auth] Database URL not found - auth will not work properly");
}

if (!process.env.BETTER_AUTH_SECRET) {
  console.error("[Auth] BETTER_AUTH_SECRET not found");
}

// Create auth instance with error handling
let authInstance: ReturnType<typeof betterAuth> | null = null;
let initError: string | null = null;

try {
  if (databaseUrl && process.env.BETTER_AUTH_SECRET) {
    console.log("[Auth] Initializing Better Auth...");
    
    authInstance = betterAuth({
      secret: process.env.BETTER_AUTH_SECRET,
      
      database: {
        type: "postgres" as const,
        url: databaseUrl,
      },

      emailAndPassword: {
        enabled: true,
        minPasswordLength: 8,
        
        sendResetEmail: async ({ user, url }: { user: any; url: string }) => {
          const resend = getResend();
          await resend.emails.send({
            from: `Greenhat Tools <${process.env.RESEND_FROM_EMAIL || 'auth@emails.greenhatsec.com'}>`,
            to: user.email,
            subject: 'Reset your password',
            html: `<div style="font-family: Arial; max-width: 600px; margin: 0 auto;">
              <h2 style="color: #62ac4a;">Reset Your Password</h2>
              <p>Hello ${user.name || user.email},</p>
              <p>Click to reset: <a href="${url}">${url}</a></p>
              <p>Expires in 1 hour.</p>
            </div>`,
          });
        },
      },

      emailVerification: {
        sendVerificationEmail: async ({ user, url }: { user: any; url: string }) => {
          const resend = getResend();
          await resend.emails.send({
            from: `Greenhat Tools <${process.env.RESEND_FROM_EMAIL || 'auth@emails.greenhatsec.com'}>`,
            to: user.email,
            subject: 'Verify your email',
            html: `<div style="font-family: Arial; max-width: 600px; margin: 0 auto;">
              <h2 style="color: #62ac4a;">Verify Your Email</h2>
              <p>Welcome! Click to verify: <a href="${url}">${url}</a></p>
              <p>Expires in 24 hours.</p>
            </div>`,
          });
        },
      },

      socialProviders: {
        google: {
          clientId: process.env.GOOGLE_CLIENT_ID || "",
          clientSecret: process.env.GOOGLE_CLIENT_SECRET || "",
          ...(process.env.GOOGLE_ALLOWED_DOMAIN ? {
            authorization: {
              params: {
                hd: process.env.GOOGLE_ALLOWED_DOMAIN,
              },
            },
          } : {}),
        },
      },

      plugins: [
        admin({
          adminUserIds: ["09649c79-975a-4967-9299-440b2b0fadee"],
        }),
      ],

      session: {
        expiresIn: 60 * 60 * 24 * 7,
      },

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
  } else {
    initError = "Missing database URL or BETTER_AUTH_SECRET";
    console.error("[Auth] Cannot initialize:", initError);
  }
} catch (error) {
  initError = error instanceof Error ? error.message : String(error);
  console.error("[Auth] Failed to initialize Better Auth:", initError);
}

// Export with fallback
export const auth = authInstance || {
  handler: () => {
    console.error("[Auth] Handler called but auth not initialized:", initError);
    return new Response(
      JSON.stringify({ error: "Auth not configured", details: initError }), 
      { status: 503, headers: { "Content-Type": "application/json" } }
    );
  },
  api: {} as any,
};

export { adminClient } from "better-auth/client/plugins";