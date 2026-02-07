import { NextResponse } from "next/server";

export async function GET() {
  // Check all possible database env vars
  const envCheck = {
    CRM_POSTGRES_URL_NON_POOLING: process.env.CRM_POSTGRES_URL_NON_POOLING ? "✅" : "❌",
    crm_POSTGRES_URL_NON_POOLING: process.env.crm_POSTGRES_URL_NON_POOLING ? "✅" : "❌",
    CRM_POSTGRES_URL: process.env.CRM_POSTGRES_URL ? "✅" : "❌",
    crm_POSTGRES_URL: process.env.crm_POSTGRES_URL ? "✅" : "❌",
    POSTGRES_URL: process.env.POSTGRES_URL ? "✅" : "❌",
    DATABASE_URL: process.env.DATABASE_URL ? "✅" : "❌",
    BETTER_AUTH_SECRET: process.env.BETTER_AUTH_SECRET ? "✅" : "❌",
    BETTER_AUTH_URL: process.env.BETTER_AUTH_URL ? "✅" : "❌",
    GOOGLE_CLIENT_ID: process.env.GOOGLE_CLIENT_ID ? "✅" : "❌",
    GOOGLE_CLIENT_SECRET: process.env.GOOGLE_CLIENT_SECRET ? "✅" : "❌",
    NODE_ENV: process.env.NODE_ENV || "not set",
  };

  const hasDatabase = Object.entries(envCheck)
    .filter(([k]) => k.includes("POSTGRES") || k === "DATABASE_URL")
    .some(([_, v]) => v === "✅");

  return NextResponse.json({
    status: hasDatabase ? "ok" : "missing_database",
    env: envCheck,
    timestamp: new Date().toISOString(),
  });
}