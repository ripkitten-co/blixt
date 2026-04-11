+++
title = "Email"
weight = 13
description = "Sending HTML and plain-text emails with SMTP, Askama templates, and SecretString-safe credentials."
+++

# Email

Blixt provides an async SMTP mailer built on [lettre](https://github.com/lettre/lettre). It supports both HTML emails (rendered from Askama templates) and plain-text emails. SMTP credentials are handled through `SecretString` and never appear in logs or debug output.

## Configuration

Load mailer configuration from environment variables using `MailerConfig::from_env()`:

```rust
use blixt::prelude::*;

let config = MailerConfig::from_env()?;
let mailer = Mailer::new(config)?;
```

### Required environment variables

| Variable | Description | Example |
|----------|-------------|---------|
| `SMTP_HOST` | SMTP server hostname | `smtp.mailgun.org` |
| `SMTP_PORT` | SMTP server port (typically 587 for STARTTLS) | `587` |
| `SMTP_USER` | SMTP authentication username | `postmaster@mg.example.com` |
| `SMTP_PASSWORD` | SMTP authentication password | `secret` |
| `FROM_NAME` | Display name in the From header | `My App` |
| `FROM_EMAIL` | Email address in the From header | `noreply@example.com` |

Add these to your `.env` file for local development:

```
SMTP_HOST=smtp.mailgun.org
SMTP_PORT=587
SMTP_USER=postmaster@mg.example.com
SMTP_PASSWORD=your-smtp-password
FROM_NAME=My App
FROM_EMAIL=noreply@example.com
```

If any variable is missing, `from_env()` returns an `Error::Internal` with a message identifying the missing variable.

## Sending HTML emails

Use Askama templates to render email content:

```rust
use askama::Template;
use blixt::prelude::*;

#[derive(Template)]
#[template(path = "emails/welcome.html")]
struct WelcomeEmail<'a> {
    name: &'a str,
    action_url: &'a str,
}

pub async fn send_welcome(mailer: &Mailer, user_email: &str, name: &str) -> Result<()> {
    let template = WelcomeEmail {
        name,
        action_url: "https://example.com/get-started",
    };

    mailer.send_html(user_email, "Welcome!", template).await
}
```

The template is rendered at call time. Rendering errors are returned as `Error::Internal`.

Place your email templates in `templates/emails/` following the project layout convention:

```
templates/
  emails/
    welcome.html
    password_reset.html
    notification.html
```

## Sending plain-text emails

For emails that don't need HTML formatting:

```rust
mailer.send_text(
    "user@example.com",
    "Your verification code",
    format!("Your code is: {code}"),
).await?;
```

## Transport

The mailer uses STARTTLS for encrypted SMTP connections. The transport is built once during `Mailer::new()` and reused across all send calls. On initialization, the mailer logs the SMTP host, port, and from address (but not the password).

## Credential safety

- `smtp_password` is stored as `SecretString` from the `secrecy` crate.
- The `Debug` implementation for `MailerConfig` prints `[REDACTED]` instead of the password.
- The `Debug` implementation for `Mailer` uses `finish_non_exhaustive()` to omit transport internals.
- Access the password with `config.smtp_password.expose_secret()` only when building SMTP credentials.

## Invalid addresses

If the recipient email address is invalid, `send_html` and `send_text` return `Error::BadRequest` with a message like `"Invalid recipient: ..."`. Invalid sender addresses (from `FROM_EMAIL`) cause `Mailer::new()` to return `Error::Internal`.

## Using with background jobs

For non-blocking email delivery, combine the mailer with the [background job runner](@/docs/guides/background-jobs.md):

```rust
use std::sync::Arc;
use blixt::prelude::*;

let mailer = Arc::new(Mailer::new(MailerConfig::from_env()?)?);
let runner = JobRunner::default_runner();

let mailer_clone = mailer.clone();
let to = "user@example.com".to_string();

runner.submit(job_fn("send_welcome", move || {
    let mailer = mailer_clone.clone();
    let to = to.clone();
    async move {
        mailer.send_text(&to, "Welcome", "Thanks for signing up!".into()).await
    }
})).await?;
```
