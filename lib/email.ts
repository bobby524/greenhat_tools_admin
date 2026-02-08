import { Resend } from "resend";

const resend = new Resend(process.env.RESEND_API_KEY);

const FROM_EMAIL = process.env.RESEND_FROM_EMAIL || "noreply@greenhatsec.com";
const BASE_URL = process.env.BETTER_AUTH_URL || "https://admin.greenhatsec.com";

export interface InviteEmailData {
  email: string;
  token: string;
  role: string;
  invitedByName?: string | null;
}

/**
 * Send an invitation email to a new user
 */
export async function sendInviteEmail(data: InviteEmailData): Promise<{ success: boolean; error?: string }> {
  try {
    const inviteUrl = `${BASE_URL}/invite?token=${data.token}`;
    
    const { error } = await resend.emails.send({
      from: `Greenhat Tools <${FROM_EMAIL}>`,
      to: data.email,
      subject: "You've been invited to Greenhat Tools",
      html: `
        <!DOCTYPE html>
        <html>
        <head>
          <meta charset="utf-8">
          <meta name="viewport" content="width=device-width, initial-scale=1.0">
          <title>Invitation to Greenhat Tools</title>
          <style>
            body {
              font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif;
              line-height: 1.6;
              color: #333;
              margin: 0;
              padding: 0;
              background-color: #f5f5f5;
            }
            .container {
              max-width: 600px;
              margin: 0 auto;
              padding: 40px 20px;
            }
            .email-wrapper {
              background-color: #ffffff;
              border-radius: 12px;
              overflow: hidden;
              box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
            }
            .header {
              background: linear-gradient(135deg, #62ac4a 0%, #41734a 100%);
              padding: 40px 30px;
              text-align: center;
            }
            .header h1 {
              color: #ffffff;
              margin: 0;
              font-size: 24px;
              font-weight: 600;
            }
            .header p {
              color: rgba(255, 255, 255, 0.9);
              margin: 8px 0 0 0;
              font-size: 14px;
            }
            .content {
              padding: 40px 30px;
            }
            .content p {
              margin: 0 0 20px 0;
              color: #555;
              font-size: 16px;
            }
            .role-badge {
              display: inline-block;
              background-color: #e8f5e9;
              color: #2e7d32;
              padding: 8px 16px;
              border-radius: 20px;
              font-size: 14px;
              font-weight: 500;
              margin: 10px 0 20px 0;
            }
            .button {
              display: inline-block;
              background: linear-gradient(135deg, #62ac4a 0%, #41734a 100%);
              color: #ffffff;
              text-decoration: none;
              padding: 16px 32px;
              border-radius: 8px;
              font-size: 16px;
              font-weight: 600;
              margin: 20px 0;
              box-shadow: 0 2px 4px rgba(98, 172, 74, 0.3);
            }
            .button:hover {
              box-shadow: 0 4px 8px rgba(98, 172, 74, 0.4);
            }
            .footer {
              padding: 30px;
              background-color: #f8f9fa;
              text-align: center;
              border-top: 1px solid #e9ecef;
            }
            .footer p {
              margin: 0;
              color: #6c757d;
              font-size: 13px;
            }
            .footer a {
              color: #62ac4a;
              text-decoration: none;
            }
            .expires {
              font-size: 13px;
              color: #6c757d;
              margin-top: 20px;
              padding: 12px;
              background-color: #fff3cd;
              border-radius: 6px;
              border: 1px solid #ffeaa7;
            }
          </style>
        </head>
        <body>
          <div class="container">
            <div class="email-wrapper">
              <div class="header">
                <h1>🌿 Greenhat Tools</h1>
                <p>Admin Portal Invitation</p>
              </div>
              <div class="content">
                <p>Hello,</p>
                <p>
                  <strong>${data.invitedByName || "Someone"}</strong> has invited you to join 
                  <strong>Greenhat Tools</strong> as a team member.
                </p>
                <div style="text-align: center;">
                  <span class="role-badge">${data.role.charAt(0).toUpperCase() + data.role.slice(1)} Access</span>
                </div>
                <p style="text-align: center;">
                  Click the button below to accept your invitation and set up your account:
                </p>
                <div style="text-align: center;">
                  <a href="${inviteUrl}" class="button">Accept Invitation</a>
                </div>
                <p class="expires">
                  ⏰ This invitation will expire in 7 days and can only be used once.
                </p>
                <p style="font-size: 13px; color: #6c757d; margin-top: 30px;">
                  If the button doesn't work, copy and paste this link into your browser:<br>
                  <a href="${inviteUrl}" style="word-break: break-all;">${inviteUrl}</a>
                </p>
              </div>
              <div class="footer">
                <p>
                  Greenhat Security Tools<br>
                  <a href="https://admin.greenhatsec.com">admin.greenhatsec.com</a>
                </p>
              </div>
            </div>
          </div>
        </body>
        </html>
      `,
    });

    if (error) {
      console.error("[Email] Failed to send invite email:", error);
      return { success: false, error: error.message };
    }

    return { success: true };
  } catch (error) {
    console.error("[Email] Error sending invite email:", error);
    return { 
      success: false, 
      error: error instanceof Error ? error.message : "Unknown error" 
    };
  }
}
