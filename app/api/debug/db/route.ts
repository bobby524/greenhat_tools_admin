import { NextResponse } from "next/server";
import { Pool } from "pg";

export async function GET() {
  try {
    const databaseUrl = process.env.CRM_POSTGRES_URL_NON_POOLING ||
                       process.env.crm_POSTGRES_URL_NON_POOLING ||
                       process.env.CRM_POSTGRES_URL ||
                       process.env.crm_POSTGRES_URL;
    
    if (!databaseUrl) {
      return NextResponse.json({ 
        error: "No database URL configured",
        env: {
          CRM_POSTGRES_URL_NON_POOLING: process.env.CRM_POSTGRES_URL_NON_POOLING ? "set" : "not set",
          crm_POSTGRES_URL_NON_POOLING: process.env.crm_POSTGRES_URL_NON_POOLING ? "set" : "not set",
          CRM_POSTGRES_URL: process.env.CRM_POSTGRES_URL ? "set" : "not set",
          crm_POSTGRES_URL: process.env.crm_POSTGRES_URL ? "set" : "not set",
        }
      }, { status: 500 });
    }

    const pool = new Pool({
      connectionString: databaseUrl,
      ssl: { rejectUnauthorized: false },
    });

    const client = await pool.connect();
    const result = await client.query("SELECT NOW() as time, current_database() as db");
    client.release();
    await pool.end();

    return NextResponse.json({
      success: true,
      database: result.rows[0].db,
      time: result.rows[0].time,
      url_preview: databaseUrl.substring(0, 30) + "..."
    });
  } catch (error) {
    const err = error instanceof Error ? error.message : String(error);
    return NextResponse.json({ 
      error: "Database connection failed",
      message: err 
    }, { status: 500 });
  }
}