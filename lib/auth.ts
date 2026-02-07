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

if (!databaseUrl) {
  console.warn("Database URL not found - auth will not work properly");
}

/**
 * Better Auth with Admin Plugin
 * Features: Email/password auth, Google OAuth, RBAC, Admin dashboard
 */
export const auth = betterAuth({
  secret: process.env.BETTER_AUTH_SECRET || "dev-secret-change-in-production",
  
  database: databaseUrl ? {
    type: "postgres" as const,
    url: databaseUrl,
  } : undefined,

  // Email/password auth
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

  // Social providers - Google OAuth
  socialProviders: {
    google: {
      clientId: process.env.GOOGLE_CLIENT_ID || "",
      clientSecret: process.env.GOOGLE_CLIENT_SECRET || "",
      // Optional: restrict to your domain
      ...(process.env.GOOGLE_ALLOWED_DOMAIN ? {
        authorization: {
          params: {
            hd: process.env.GOOGLE_ALLOWED_DOMAIN,
          },
        },
      } : {}),
    },
  },

  // Admin plugin - gives you built-in user/role management
  plugins: [
    admin({
      adminUserIds: ["09649c79-975a-4967-9299-440b2b0fadee"],
    }),
  ],

  session: {
    expiresIn: 60 * 60 * 24 * 7, // 7 days
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

// Admin client for dashboard
export { adminClient } from "better-auth/client/plugins";