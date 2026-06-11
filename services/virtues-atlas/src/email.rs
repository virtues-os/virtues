//! Transactional email via Resend.
//!
//! Atlas sends exactly one customer-facing email: a personal thank-you from the
//! founder when a pre-order deposit settles. We hit the Resend REST API
//! directly with `reqwest` (already a dependency) rather than pull in an SDK —
//! same approach virtues-core uses for magic-link auth.
//!
//! Sending is best-effort: the caller logs and swallows failures so a flaky
//! email provider never fails (and thus never makes Stripe retry) a webhook
//! whose real job — recording the deposit — already succeeded.

use anyhow::{anyhow, Context, Result};

const RESEND_API: &str = "https://api.resend.com/emails";

/// Send the founder's pre-order thank-you note.
///
/// `from` must be a Resend-verified sender; `reply_to` is where customer
/// replies land (Adam's inbox). No-op caller-side when `api_key` is empty.
pub async fn send_preorder_thanks(
    api_key: &str,
    from: &str,
    reply_to: &str,
    to: &str,
) -> Result<()> {
    if api_key.is_empty() {
        return Err(anyhow!("RESEND_API_KEY not set"));
    }

    let client = reqwest::Client::new();
    let resp = client
        .post(RESEND_API)
        .header("Authorization", format!("Bearer {api_key}"))
        .json(&serde_json::json!({
            "from": from,
            "to": to,
            "reply_to": reply_to,
            "subject": "Virtues — you're in line",
            "text": THANKS_TEXT,
            "html": THANKS_HTML,
        }))
        .send()
        .await
        .context("POST resend thank-you email")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("resend api error: {status} — {body}"));
    }
    Ok(())
}

/// Send the box-claim magic link for `[1] Log in to existing account`.
///
/// Atlas resolves a Stripe customer by email, mints a one-shot token, and
/// sends this email containing the verification link. Click → atlas looks
/// up the login_attempt → marks the bound device_link ready with a
/// billing_token for that customer → the calling box's poll picks it up.
///
/// `link` is the full URL the user clicks. We never expose the raw token
/// in logs; only sha256(token) hits the database.
pub async fn send_login_magic_link(
    api_key: &str,
    from: &str,
    to: &str,
    link: &str,
) -> Result<()> {
    if api_key.is_empty() {
        return Err(anyhow!("RESEND_API_KEY not set"));
    }
    let client = reqwest::Client::new();
    let text = format!(
        "Click this link to attach your Virtues box to your existing subscription:\n\n\
         {link}\n\n\
         The link is good for 15 minutes and can only be used once.\n\
         If you didn't request this, ignore this email — nothing changes."
    );
    let html = format!(
        r#"<div style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; max-width: 480px; margin: 0 auto; padding: 32px 24px; color: #14283d; line-height: 1.5; font-size: 15px;">
  <p style="font-size: 18px; margin: 0 0 16px;">Attach this box to your Virtues account?</p>
  <p style="margin: 0 0 24px; color: #57534e;">Click the button to attach the box that requested this link to your existing subscription.</p>
  <p style="margin: 0 0 24px;">
    <a href="{link}" style="display: inline-block; background: #14283d; color: white; text-decoration: none; padding: 12px 20px; border-radius: 8px; font-weight: 600;">Attach box</a>
  </p>
  <p style="margin: 0 0 8px; font-size: 13px; color: #78716c;">Or paste this URL into your browser:</p>
  <p style="margin: 0 0 24px; font-size: 13px; color: #78716c; word-break: break-all;">{link}</p>
  <hr style="border: none; border-top: 1px solid #e5e5e5; margin: 24px 0;" />
  <p style="margin: 0; font-size: 13px; color: #78716c;">Link expires in 15 minutes and is single-use. If you didn't request this, ignore the email — nothing changes.</p>
</div>"#
    );
    let resp = client
        .post(RESEND_API)
        .header("Authorization", format!("Bearer {api_key}"))
        .json(&serde_json::json!({
            "from": from,
            "to": to,
            "subject": "Attach your Virtues box",
            "text": text,
            "html": html,
        }))
        .send()
        .await
        .context("POST resend magic link")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("resend api error: {status} — {body}"));
    }
    Ok(())
}

const THANKS_TEXT: &str = "Hi,

It's Adam, founder of Virtues. I wanted to personally thank you for placing a deposit — you're one of the first people to bring one of these home, and that means a great deal to me.

Here's what happens next: your $50 deposit holds your place in line and stays fully refundable until your unit ships. I'll email you when your batch is ready, and that's when you'll complete your order.

If you have any questions at all — about the hardware, the software, the privacy model, anything — just reply to this email. It comes straight to me, and I'll answer personally.

Thank you for believing in this.

— Adam

P.S. Yes — this note was automated. But reply to it and I promise it's me on the other end. I read every one.";

const THANKS_HTML: &str = r#"<div style="font-family: Georgia, 'Times New Roman', serif; max-width: 540px; margin: 0 auto; padding: 24px; color: #14283d; line-height: 1.6; font-size: 16px;">
  <p>Hi,</p>
  <p>It's Adam, founder of Virtues. I wanted to personally thank you for placing a deposit — you're one of the first people to bring one of these home, and that means a great deal to me.</p>
  <p>Here's what happens next: your $50 deposit holds your place in line and stays fully refundable until your unit ships. I'll email you when your batch is ready, and that's when you'll complete your order.</p>
  <p>If you have any questions at all — about the hardware, the software, the privacy model, anything — just reply to this email. It comes straight to me, and I'll answer personally.</p>
  <p>Thank you for believing in this.</p>
  <p style="margin: 28px 0 4px;">
    <img src="https://virtues.com/images/adam_signature.png" alt="— Adam" width="180" style="display:block; width:180px; max-width:60%; height:auto;" />
  </p>
  <p style="margin: 0; color:#57534e; font-size:14px;">Adam · Founder, Virtues</p>
  <p style="margin: 22px 0 0; color:#78716c; font-size:13px; font-style:italic; line-height:1.5;">P.S. Yes — this note was automated. But reply to it and I promise it's me on the other end. I read every one.</p>
</div>"#;
