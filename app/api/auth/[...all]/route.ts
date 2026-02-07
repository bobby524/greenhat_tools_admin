import { auth } from "@/lib/auth";
import { NextRequest, NextResponse } from "next/server";

/**
 * Better Auth API route handler
 * Simplified version for gradual migration
 */

export async function GET(request: NextRequest) {
  try {
    return await auth.handler(request);
  } catch (error) {
    console.error("Auth GET error:", error);
    return NextResponse.json({ error: "Auth service unavailable" }, { status: 503 });
  }
}

export async function POST(request: NextRequest) {
  try {
    return await auth.handler(request);
  } catch (error) {
    console.error("Auth POST error:", error);
    return NextResponse.json({ error: "Auth service unavailable" }, { status: 503 });
  }
}

export async function PUT(request: NextRequest) {
  try {
    return await auth.handler(request);
  } catch (error) {
    console.error("Auth PUT error:", error);
    return NextResponse.json({ error: "Auth service unavailable" }, { status: 503 });
  }
}

export async function DELETE(request: NextRequest) {
  try {
    return await auth.handler(request);
  } catch (error) {
    console.error("Auth DELETE error:", error);
    return NextResponse.json({ error: "Auth service unavailable" }, { status: 503 });
  }
}

export async function PATCH(request: NextRequest) {
  try {
    return await auth.handler(request);
  } catch (error) {
    console.error("Auth PATCH error:", error);
    return NextResponse.json({ error: "Auth service unavailable" }, { status: 503 });
  }
}