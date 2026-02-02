// Simple Supabase client for admin operations
import { createClient } from '@supabase/supabase-js';

const supabaseUrl = process.env.SUPABASE_URL!;
const supabaseKey = process.env.SUPABASE_SERVICE_ROLE_KEY!;

export const supabase = createClient(supabaseUrl, supabaseKey);

export class ExponentialClient {
  async get(path: string, params?: Record<string, string>) {
    // Admin client uses Supabase directly, not REST API
    return { success: true };
  }

  async post(path: string, body: any) {
    return { success: true };
  }

  async patch(path: string, body: any) {
    return { success: true };
  }

  async delete(path: string) {
    return { success: true };
  }
}

export function createClient() {
  return new ExponentialClient();
}
