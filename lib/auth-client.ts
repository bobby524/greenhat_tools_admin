import { createAuthClient } from "better-auth/client";

/**
 * Better Auth client for React/Next.js
 * Use this in client components for authentication
 */
export const authClient = createAuthClient({
  // Base URL for auth API
  baseURL: process.env.NEXT_PUBLIC_APP_URL || "https://admin.greenhatsec.com",
  
  // Fetch configuration
  fetchOptions: {
    credentials: "include",
  },
});

// Export commonly used methods for convenience
export const {
  signIn,
  signUp,
  signOut,
  useSession,
  getSession,
  updateUser,
  changePassword,
  resetPassword,
  verifyEmail,
} = authClient;

// Type exports
export type { Session, User } from "better-auth";