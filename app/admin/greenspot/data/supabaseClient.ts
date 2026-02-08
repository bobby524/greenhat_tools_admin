import { createClient, type SupabaseClient } from "@supabase/supabase-js";

import type { Database } from "./types";

let supabaseClient: SupabaseClient<Database, "public"> | null = null;

const getSupabaseEnv = () => {
  const url =
    process.env.NEXT_PUBLIC_SUPABASE_URL ??
    process.env.SUPABASE_URL;
  const anonKey =
    process.env.NEXT_PUBLIC_SUPABASE_ANON_KEY ??
    process.env.SUPABASE_ANON_KEY;

  if (!url || !anonKey) {
    throw new Error(
      "Supabase env vars are missing. Set NEXT_PUBLIC_SUPABASE_URL and NEXT_PUBLIC_SUPABASE_ANON_KEY.",
    );
  }

  return { url, anonKey };
};

export const getSupabaseClient = (): SupabaseClient<Database, "public"> => {
  if (!supabaseClient) {
    const { url, anonKey } = getSupabaseEnv();
    supabaseClient = createClient<Database, "public">(url, anonKey);
  }

  return supabaseClient;
};
