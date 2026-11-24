use async_trait::async_trait;
use tracing::info;

#[async_trait]
pub trait Mailer: Send + Sync {
    async fn send_verification_email(&self, to: &str, token: &str) -> Result<(), String>;
    async fn send_password_reset_email(&self, to: &str, token: &str) -> Result<(), String>;
}

#[derive(Clone, Default)]
pub struct ConsoleMailer;

#[async_trait]
impl Mailer for ConsoleMailer {
    async fn send_verification_email(&self, to: &str, token: &str) -> Result<(), String> {
        info!(
            "\n========================================================================\n\
             [MAILER: VERIFICATION EMAIL]\n\
             To: {}\n\
             Token: {}\n\
             Verification Link: http://localhost:3000/verify-email?token={}\n\
             ========================================================================",
            to, token, token
        );
        Ok(())
    }

    async fn send_password_reset_email(&self, to: &str, token: &str) -> Result<(), String> {
        info!(
            "\n========================================================================\n\
             [MAILER: PASSWORD RESET EMAIL]\n\
             To: {}\n\
             Token: {}\n\
             Reset Link: http://localhost:3000/reset-password?token={}\n\
             ========================================================================",
            to, token, token
        );
        Ok(())
    }
}
