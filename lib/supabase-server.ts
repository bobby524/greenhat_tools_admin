import { createClient } from "@supabase/supabase-js";

/**
 * Lazy-load Supabase client to avoid build-time errors.
 * Environment variables are only accessed at runtime, not during build.
 */
export function getSupabase() {
  const url = process.env.NEXT_PUBLIC_SUPABASE_URL;
  const key = process.env.SUPABASE_SERVICE_ROLE_KEY;

  if (!url || !key) {
    throw new Error("Missing Supabase environment variables");
  }

  return createClient(url, key);
}
