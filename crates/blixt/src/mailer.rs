use askama::Template;
use lettre::message::Mailbox;
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use secrecy::{ExposeSecret, SecretString};
use std::fmt;

/// Configuration for the SMTP mailer.
///
/// All fields are required. Use [`MailerConfig::from_env`] to load from
/// environment variables, or construct manually for testing.
pub struct MailerConfig {
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_user: String,
    pub smtp_password: SecretString,
    pub from_name: String,
    pub from_email: String,
}

impl fmt::Debug for MailerConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MailerConfig")
            .field("smtp_host", &self.smtp_host)
            .field("smtp_port", &self.smtp_port)
            .field("smtp_user", &self.smtp_user)
            .field("smtp_password", &"[REDACTED]")
            .field("from_name", &self.from_name)
            .field("from_email", &self.from_email)
            .finish()
    }
}

impl MailerConfig {
    /// Loads mailer configuration from environment variables.
    ///
    /// Required variables: `SMTP_HOST`, `SMTP_PORT`, `SMTP_USER`,
    /// `SMTP_PASSWORD`, `FROM_NAME`, `FROM_EMAIL`.
    pub fn from_env() -> crate::error::Result<Self> {
        Ok(Self {
            smtp_host: require_env("SMTP_HOST")?,
            smtp_port: require_env("SMTP_PORT")?
                .parse::<u16>()
                .map_err(|e| crate::error::Error::Internal(format!("Invalid SMTP_PORT: {e}")))?,
            smtp_user: require_env("SMTP_USER")?,
            smtp_password: SecretString::from(require_env("SMTP_PASSWORD")?),
            from_name: require_env("FROM_NAME")?,
            from_email: require_env("FROM_EMAIL")?,
        })
    }
}

/// Reads a required environment variable, returning a clear error if missing.
fn require_env(key: &str) -> crate::error::Result<String> {
    std::env::var(key).map_err(|_| {
        crate::error::Error::Internal(format!("Missing required environment variable: {key}"))
    })
}

/// Async email sender backed by an SMTP transport.
///
/// Supports both HTML (via Askama templates) and plain-text emails.
/// The SMTP password is never logged or exposed in debug output.
pub struct Mailer {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
}

impl fmt::Debug for Mailer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mailer")
            .field("from", &self.from)
            .finish_non_exhaustive()
    }
}

impl Mailer {
    /// Creates a new mailer from the given configuration.
    ///
    /// Builds an encrypted SMTP transport (STARTTLS) with credentials
    /// derived from the config.
    pub fn new(config: MailerConfig) -> crate::error::Result<Self> {
        let credentials = Credentials::new(
            config.smtp_user.clone(),
            config.smtp_password.expose_secret().to_owned(),
        );
        let transport = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.smtp_host)
            .map_err(|e| crate::error::Error::Internal(format!("SMTP transport error: {e}")))?
            .port(config.smtp_port)
            .credentials(credentials)
            .build();

        let from = build_mailbox(&config.from_name, &config.from_email)?;

        tracing::info!(
            smtp_host = %config.smtp_host,
            smtp_port = config.smtp_port,
            from = %from,
            "Mailer initialized"
        );

        Ok(Self { transport, from })
    }

    /// Sends an HTML email rendered from an Askama template.
    ///
    /// The template is rendered at call time; rendering errors are
    /// returned as `Error::Internal`.
    pub async fn send_html<T: Template>(
        &self,
        to: &str,
        subject: &str,
        template: T,
    ) -> crate::error::Result<()> {
        let html = template
            .render()
            .map_err(|e| crate::error::Error::Internal(format!("Template error: {e}")))?;

        let email = self.build_message(to, subject, ContentType::TEXT_HTML, html)?;
        self.dispatch(email).await
    }

    /// Sends a plain-text email.
    pub async fn send_text(
        &self,
        to: &str,
        subject: &str,
        body: String,
    ) -> crate::error::Result<()> {
        let email = self.build_message(to, subject, ContentType::TEXT_PLAIN, body)?;
        self.dispatch(email).await
    }

    /// Constructs a `Message` with the given content type and body.
    fn build_message(
        &self,
        to: &str,
        subject: &str,
        content_type: ContentType,
        body: String,
    ) -> crate::error::Result<Message> {
        let to_mailbox: Mailbox = to
            .parse()
            .map_err(|e| crate::error::Error::BadRequest(format!("Invalid recipient: {e}")))?;

        Message::builder()
            .from(self.from.clone())
            .to(to_mailbox)
            .subject(subject)
            .header(content_type)
            .body(body)
            .map_err(|e| crate::error::Error::Internal(format!("Email build error: {e}")))
    }

    /// Sends a pre-built message via the SMTP transport.
    async fn dispatch(&self, message: Message) -> crate::error::Result<()> {
        self.transport
            .send(message)
            .await
            .map_err(|e| crate::error::Error::Internal(format!("SMTP send error: {e}")))?;
        Ok(())
    }
}

/// Parses a name and email into a [`Mailbox`].
fn build_mailbox(name: &str, email: &str) -> crate::error::Result<Mailbox> {
    let address = email
        .parse()
        .map_err(|e| crate::error::Error::Internal(format!("Invalid from address: {e}")))?;
    Ok(Mailbox::new(Some(name.to_owned()), address))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Sets environment variables for the duration of the closure, then
    /// restores previous values. Serialized via `ENV_LOCK` to avoid races.
    fn with_env_vars<F, R>(vars: &[(&str, Option<&str>)], f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let _guard = ENV_LOCK.lock().expect("env lock poisoned");

        let mut previous: Vec<(&str, Option<String>)> = Vec::new();
        for &(key, value) in vars {
            previous.push((key, std::env::var(key).ok()));
            // SAFETY: protected by ENV_LOCK mutex; tests run serially
            unsafe {
                match value {
                    Some(v) => std::env::set_var(key, v),
                    None => std::env::remove_var(key),
                }
            }
        }

        let result = f();

        for (key, prev) in previous {
            // SAFETY: protected by ENV_LOCK mutex; restoring original values
            unsafe {
                match prev {
                    Some(v) => std::env::set_var(key, v),
                    None => std::env::remove_var(key),
                }
            }
        }

        result
    }

    const ALL_MAILER_VARS: [&str; 6] = [
        "SMTP_HOST",
        "SMTP_PORT",
        "SMTP_USER",
        "SMTP_PASSWORD",
        "FROM_NAME",
        "FROM_EMAIL",
    ];

    fn env_with_all_set() -> Vec<(&'static str, Option<&'static str>)> {
        vec![
            ("SMTP_HOST", Some("mail.example.com")),
            ("SMTP_PORT", Some("587")),
            ("SMTP_USER", Some("user@example.com")),
            ("SMTP_PASSWORD", Some("hunter2")),
            ("FROM_NAME", Some("Test App")),
            ("FROM_EMAIL", Some("noreply@example.com")),
        ]
    }

    fn env_with_var_removed(skip: &str) -> Vec<(&'static str, Option<&'static str>)> {
        let mut vars = env_with_all_set();
        for entry in &mut vars {
            if entry.0 == skip {
                entry.1 = None;
            }
        }
        vars
    }

    #[test]
    fn from_env_fails_when_smtp_host_missing() {
        let vars = env_with_var_removed("SMTP_HOST");
        with_env_vars(&vars, || {
            let result = MailerConfig::from_env();
            assert!(result.is_err());
            let err = result.unwrap_err().to_string();
            assert!(
                err.contains("SMTP_HOST"),
                "error should mention SMTP_HOST, got: {err}"
            );
        });
    }

    #[test]
    fn from_env_fails_when_any_required_var_missing() {
        for var_name in &ALL_MAILER_VARS {
            let vars = env_with_var_removed(var_name);
            with_env_vars(&vars, || {
                let result = MailerConfig::from_env();
                assert!(result.is_err(), "expected error when {var_name} is missing");
            });
        }
    }

    #[test]
    fn from_env_fails_with_invalid_port() {
        let mut vars = env_with_all_set();
        for entry in &mut vars {
            if entry.0 == "SMTP_PORT" {
                entry.1 = Some("not_a_number");
            }
        }
        with_env_vars(&vars, || {
            let result = MailerConfig::from_env();
            assert!(result.is_err());
            let err = result.unwrap_err().to_string();
            assert!(
                err.contains("SMTP_PORT"),
                "error should mention SMTP_PORT, got: {err}"
            );
        });
    }

    #[test]
    fn from_env_succeeds_with_all_vars_set() {
        let vars = env_with_all_set();
        with_env_vars(&vars, || {
            let config = MailerConfig::from_env().expect("should succeed");
            assert_eq!(config.smtp_host, "mail.example.com");
            assert_eq!(config.smtp_port, 587);
            assert_eq!(config.smtp_user, "user@example.com");
            assert_eq!(config.smtp_password.expose_secret(), "hunter2");
            assert_eq!(config.from_name, "Test App");
            assert_eq!(config.from_email, "noreply@example.com");
        });
    }

    #[test]
    fn debug_output_redacts_smtp_password() {
        let config = MailerConfig {
            smtp_host: "mail.example.com".to_string(),
            smtp_port: 587,
            smtp_user: "user@example.com".to_string(),
            smtp_password: SecretString::from("super-secret-password"),
            from_name: "Test".to_string(),
            from_email: "test@example.com".to_string(),
        };

        let debug = format!("{config:?}");
        assert!(
            !debug.contains("super-secret-password"),
            "debug output must not contain the SMTP password"
        );
        assert!(
            debug.contains("[REDACTED]"),
            "debug output must show [REDACTED]"
        );
    }

    #[test]
    fn build_message_produces_valid_html_email() {
        let config = MailerConfig {
            smtp_host: "localhost".to_string(),
            smtp_port: 587,
            smtp_user: "user".to_string(),
            smtp_password: SecretString::from("pass"),
            from_name: "Blixt App".to_string(),
            from_email: "noreply@blixt.dev".to_string(),
        };

        let from =
            build_mailbox(&config.from_name, &config.from_email).expect("valid from address");

        let to = "recipient@example.com";
        let to_mailbox: Mailbox = to.parse().expect("valid to address");
        let subject = "Welcome!";
        let body = "<h1>Hello</h1>".to_string();

        let message = Message::builder()
            .from(from.clone())
            .to(to_mailbox)
            .subject(subject)
            .header(ContentType::TEXT_HTML)
            .body(body)
            .expect("valid message");

        let envelope = message.envelope();
        assert_eq!(
            envelope.from().expect("has sender").to_string(),
            "noreply@blixt.dev"
        );
        assert_eq!(envelope.to().len(), 1);
        assert_eq!(envelope.to()[0].to_string(), "recipient@example.com");
    }

    #[test]
    fn build_message_produces_valid_text_email() {
        let from = build_mailbox("Sender", "sender@example.com").expect("valid from address");

        let to_mailbox: Mailbox = "user@example.com".parse().expect("valid to");

        let message = Message::builder()
            .from(from)
            .to(to_mailbox)
            .subject("Plain text test")
            .header(ContentType::TEXT_PLAIN)
            .body("Hello, world!".to_string())
            .expect("valid message");

        let envelope = message.envelope();
        assert_eq!(envelope.to().len(), 1);
    }

    #[test]
    fn invalid_recipient_address_is_rejected() {
        let result: Result<Mailbox, _> = "not-an-email".parse();
        assert!(
            result.is_err(),
            "parsing an invalid email address should fail"
        );
    }

    #[test]
    fn build_mailbox_rejects_invalid_email() {
        let result = build_mailbox("Name", "definitely not an email");
        assert!(result.is_err());
    }

    #[derive(Template)]
    #[template(source = "<h1>Hello, {{ name }}!</h1>", ext = "html")]
    struct TestTemplate<'a> {
        name: &'a str,
    }

    #[test]
    fn askama_template_renders_for_email() {
        let tmpl = TestTemplate { name: "World" };
        let rendered = tmpl.render().expect("template should render");
        assert_eq!(rendered, "<h1>Hello, World!</h1>");
    }
}
