// API Configuration
// Uses separate API subdomain to bypass Cloudflare Access

export const API_BASE_URL = process.env.NODE_ENV === "production"
  ? "https://api.greenhatsec.com"
  : "";

// Helper to build API URLs
export function apiUrl(path: string): string {
  const cleanPath = path.startsWith("/") ? path : `/${path}`;
  return `${API_BASE_URL}${cleanPath}`;
}
