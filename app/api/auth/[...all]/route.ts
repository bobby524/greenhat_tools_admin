import { auth } from "@/lib/auth";
import { NextRequest, NextResponse } from "next/server";

/**
 * Better Auth API route handler
 */

async function handleAuth(request: NextRequest) {
  try {
    console.log("[Auth API] Handling request:", request.method, request.url);
    const response = await auth.handler(request);
    console.log("[Auth API] Response status:", response.status);
    return response;
  } catch (error) {
    const errorMessage = error instanceof Error ? error.message : String(error);
    const errorStack = error instanceof Error ? error.stack : "";
    console.error("[Auth API] Error:", errorMessage);
    console.error("[Auth API] Stack:", errorStack);
    
    return NextResponse.json({ 
      error: "Auth service error",
      message: errorMessage,
      stack: process.env.NODE_ENV === "development" ? errorStack : undefined
    }, { status: 500 });
  }
}

export async function GET(request: NextRequest) {
  return handleAuth(request);
}

export async function POST(request: NextRequest) {
  return handleAuth(request);
}

export async function PUT(request: NextRequest) {
  return handleAuth(request);
}

export async function DELETE(request: NextRequest) {
  return handleAuth(request);
}

export async function PATCH(request: NextRequest) {
  return handleAuth(request);
}