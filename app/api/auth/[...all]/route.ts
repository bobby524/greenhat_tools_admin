import { auth } from "@/lib/auth";
<<<<<<< HEAD
import { NextRequest, NextResponse } from "next/server";

/**
 * Better Auth API route handler
 * This catches all /api/auth/* routes and handles them with Better Auth
 */
export async function GET(request: NextRequest) {
  // Better Auth will handle the request
  return auth.handler(request);
}

export async function POST(request: NextRequest) {
  // Better Auth will handle the request
  return auth.handler(request);
}

export async function PUT(request: NextRequest) {
  return auth.handler(request);
}

export async function DELETE(request: NextRequest) {
  return auth.handler(request);
}

export async function PATCH(request: NextRequest) {
=======

export async function GET(request: Request) {
  return auth.handler(request);
}

export async function POST(request: Request) {
>>>>>>> fix-auth-deploy
  return auth.handler(request);
}
