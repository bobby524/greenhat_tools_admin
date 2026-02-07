import { NextRequest, NextResponse } from "next/server";
import { getSupabase, User } from "@/lib/supabase-server";

/**
 * GET /api/users
 * Fetch all users from the database
 */
export async function GET(request: NextRequest) {
  try {
    const supabase = getSupabase();

    // Fetch users from the user table (Better Auth default schema)
    // Note: Using raw query to avoid schema cache issues
    const { data: users, error } = await supabase
      .from('user')
      .select('id, email, name, image, "emailVerified", role, "createdAt", "updatedAt"')
      .order('createdAt', { ascending: false });

    if (error) {
      console.error("[API Users] Error fetching users:", error);
      return NextResponse.json(
        { error: "Failed to fetch users", details: error.message },
        { status: 500 }
      );
    }

    // Transform data to match our User interface
    const transformedUsers: User[] = (users || []).map((user: any) => ({
      id: user.id,
      email: user.email,
      name: user.name,
      image: user.image,
      emailVerified: user.emailVerified || false,
      role: user.role || "user",
      createdAt: user.createdAt,
      updatedAt: user.updatedAt,
    }));

    return NextResponse.json({ users: transformedUsers });
  } catch (error) {
    console.error("[API Users] Unexpected error:", error);
    return NextResponse.json(
      { error: "Internal server error", details: error instanceof Error ? error.message : "Unknown error" },
      { status: 500 }
    );
  }
}

/**
 * PATCH /api/users
 * Update a user's role
 * Body: { userId: string, role: string }
 */
export async function PATCH(request: NextRequest) {
  try {
    const body = await request.json();
    const { userId, role } = body;

    if (!userId || !role) {
      return NextResponse.json(
        { error: "Missing required fields: userId and role" },
        { status: 400 }
      );
    }

    // Validate role value
    const validRoles = ["admin", "user", "viewer"];
    if (!validRoles.includes(role)) {
      return NextResponse.json(
        { error: `Invalid role. Must be one of: ${validRoles.join(", ")}` },
        { status: 400 }
      );
    }

    const supabase = getSupabase();

    // Update the user's role
    const { data, error } = await supabase
      .from('user')
      .update({ role, updatedAt: new Date().toISOString() })
      .eq('id', userId)
      .select('id, email, name, image, "emailVerified", role, "createdAt", "updatedAt"')
      .single();

    if (error) {
      console.error("[API Users] Error updating user role:", error);
      return NextResponse.json(
        { error: "Failed to update user role", details: error.message },
        { status: 500 }
      );
    }

    if (!data) {
      return NextResponse.json(
        { error: "User not found" },
        { status: 404 }
      );
    }

    // Transform the response
    const updatedUser: User = {
      id: data.id,
      email: data.email,
      name: data.name,
      image: data.image,
      emailVerified: data.emailVerified || false,
      role: data.role || "user",
      createdAt: data.createdAt,
      updatedAt: data.updatedAt,
    };

    return NextResponse.json({ user: updatedUser, success: true });
  } catch (error) {
    console.error("[API Users] Unexpected error:", error);
    return NextResponse.json(
      { error: "Internal server error", details: error instanceof Error ? error.message : "Unknown error" },
      { status: 500 }
    );
  }
}
