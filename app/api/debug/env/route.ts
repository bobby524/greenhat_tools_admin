import { NextResponse } from "next/server";

export async function GET() {
  return NextResponse.json({
    env: {
      hasCrmPostgresUrl: !!process.env.crm_POSTGRES_URL_NON_POOLING,
      hasPostgresUrl: !!process.env.POSTGRES_URL,
      hasDatabaseUrl: !!process.env.DATABASE_URL,
      hasCrmPostgres: !!process.env.CRM_POSTGRES_URL,
      hasAuthSecret: !!process.env.BETTER_AUTH_SECRET,
      hasAuthUrl: !!process.env.BETTER_AUTH_URL,
      nodeEnv: process.env.NODE_ENV,
      vercelEnv: process.env.VERCEL_ENV,
    },
    timestamp: new Date().toISOString(),
  });
}
