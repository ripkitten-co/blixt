use std::fs;
use std::path::Path;

use chrono::Utc;
use console::style;

use crate::fields::{DbDialect, detect_dialect};

use super::fs_utils::{current_dir, ensure_dir_exists, update_mod_file, write_file};

/// Generates a complete auth scaffold: users migration, sessions migration,
/// user model, session model, auth controller, and page templates.
pub fn generate_auth() -> Result<(), String> {
    let base = current_dir()?;
    generate_auth_in(&base)
}

pub fn generate_auth_in(base: &Path) -> Result<(), String> {
    let dialect = detect_dialect();

    write_users_migration(base, dialect)?;
    write_sessions_migration(base, dialect)?;
    write_user_model(base, dialect)?;
    write_session_model(base, dialect)?;
    write_auth_controller(base)?;
    update_mod_file(base, "models", "user")?;
    update_mod_file(base, "models", "session")?;
    update_mod_file(base, "controllers", "auth")?;
    write_template(base, "pages/auth/register.html", register_template())?;
    write_template(base, "pages/auth/login.html", login_template())?;
    write_template(
        base,
        "pages/auth/forgot_password.html",
        forgot_password_template(),
    )?;
    write_template(
        base,
        "pages/auth/reset_password.html",
        reset_password_template(),
    )?;

    println!(
        "  {} auth scaffold (models, controller, migrations, templates)",
        style("created").green().bold()
    );
    print_route_hints();
    Ok(())
}

// --- Migrations ---

fn write_users_migration(base: &Path, dialect: DbDialect) -> Result<(), String> {
    let timestamp = Utc::now().format("%Y%m%d%H%M%S");
    let dir = base.join("migrations");
    let path = dir.join(format!("{timestamp}_create_users.sql"));

    let content = match dialect {
        DbDialect::Postgres => {
            "\
CREATE TABLE IF NOT EXISTS users (
    id BIGSERIAL PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'user',
    email_verified_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_users_email ON users (email);
"
        }
        DbDialect::Sqlite => {
            "\
CREATE TABLE IF NOT EXISTS users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'user',
    email_verified_at DATETIME,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_users_email ON users (email);
"
        }
    };

    ensure_dir_exists(&dir)?;
    write_file(&path, content)
}

fn write_sessions_migration(base: &Path, dialect: DbDialect) -> Result<(), String> {
    // offset by 1 second to ensure ordering after users migration
    let timestamp = (Utc::now() + chrono::Duration::seconds(1)).format("%Y%m%d%H%M%S");
    let dir = base.join("migrations");
    let path = dir.join(format!("{timestamp}_create_sessions.sql"));

    let content = match dialect {
        DbDialect::Postgres => {
            "\
CREATE TABLE IF NOT EXISTS sessions (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions (user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_token_hash ON sessions (token_hash);
"
        }
        DbDialect::Sqlite => {
            "\
CREATE TABLE IF NOT EXISTS sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    expires_at DATETIME NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions (user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_token_hash ON sessions (token_hash);
"
        }
    };

    ensure_dir_exists(&dir)?;
    write_file(&path, content)
}

// --- Models ---

fn write_user_model(base: &Path, _dialect: DbDialect) -> Result<(), String> {
    let dir = base.join("src/models");
    let path = dir.join("user.rs");

    let content = r#"use blixt::prelude::*;
use blixt::auth::password;
use blixt::validate::Validator;
use sqlx::types::chrono::{DateTime, Utc};

const TABLE: &str = "users";
const COLUMNS: &[&str] = &["id", "email", "password_hash", "role", "email_verified_at", "created_at", "updated_at"];

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub email: String,
    pub password_hash: String,
    pub role: String,
    pub email_verified_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl User {
    pub async fn find_by_id(pool: &DbPool, id: i64) -> Result<Self> {
        Select::from(TABLE).columns(COLUMNS).where_eq("id", id)
            .fetch_one::<Self>(pool).await
    }

    pub async fn find_by_email(pool: &DbPool, email: &str) -> Result<Self> {
        Select::from(TABLE).columns(COLUMNS).where_eq("email", email)
            .fetch_one::<Self>(pool).await
    }

    pub async fn create(pool: &DbPool, email: &str, password: &str) -> Result<Self> {
        let hash = password::hash_password(password)?;
        Insert::into(TABLE)
            .set("email", email)
            .set("password_hash", &hash)
            .returning::<Self>(COLUMNS)
            .execute(pool).await
    }

    pub async fn verify_credentials(pool: &DbPool, email: &str, password: &str) -> Result<Self> {
        // constant-time: always hash even when user not found (prevents timing-based enumeration)
        const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$AAAAAAAAAAAAAAAAAAAAAA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let (user, hash) = match Self::find_by_email(pool, email).await {
            Ok(u) => {
                let h = u.password_hash.clone();
                (Some(u), h)
            }
            Err(_) => (None, DUMMY_HASH.to_owned()),
        };
        let valid = password::verify_password(password, &hash).unwrap_or(false);
        match (user, valid) {
            (Some(u), true) => Ok(u),
            _ => Err(Error::Unauthorized),
        }
    }

    pub async fn update_password(pool: &DbPool, id: i64, password: &str) -> Result<()> {
        let hash = password::hash_password(password)?;
        Update::table(TABLE)
            .set("password_hash", &hash)
            .set_timestamp("updated_at")
            .where_eq("id", id)
            .execute_no_return(pool).await
    }

    pub fn validate_registration(email: &str, password: &str, password_confirm: &str) -> Result<()> {
        let mut v = Validator::new();
        v.str_field(email, "email").not_empty().max_length(255).pattern(blixt::validate::EMAIL_PATTERN, "must be a valid email");
        v.str_field(password, "password").min_length(8).max_length(128);
        v.check()?;
        if password != password_confirm {
            return Err(Error::BadRequest("Passwords do not match".into()));
        }
        Ok(())
    }
}
"#;

    ensure_dir_exists(&dir)?;
    write_file(&path, content)
}

fn write_session_model(base: &Path, _dialect: DbDialect) -> Result<(), String> {
    let dir = base.join("src/models");
    let path = dir.join("session.rs");

    let content = r#"use blixt::prelude::*;
use sqlx::types::chrono::{DateTime, Utc};

const TABLE: &str = "sessions";
const COLUMNS: &[&str] = &["id", "user_id", "token_hash", "expires_at", "created_at"];

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct Session {
    pub id: i64,
    pub user_id: i64,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl Session {
    pub async fn create(pool: &DbPool, user_id: i64, token_hash: &str, expires_at: DateTime<Utc>) -> Result<Self> {
        Insert::into(TABLE)
            .set("user_id", user_id)
            .set("token_hash", token_hash)
            .set("expires_at", &expires_at.to_rfc3339())
            .returning::<Self>(COLUMNS)
            .execute(pool).await
    }

    pub async fn find_by_token_hash(pool: &DbPool, token_hash: &str) -> Result<Self> {
        Select::from(TABLE).columns(COLUMNS)
            .where_eq("token_hash", token_hash)
            .fetch_one::<Self>(pool).await
    }

    pub async fn delete(pool: &DbPool, id: i64) -> Result<()> {
        Delete::from(TABLE).where_eq("id", id).execute(pool).await
    }

    pub async fn delete_all_for_user(pool: &DbPool, user_id: i64) -> Result<()> {
        Delete::from(TABLE).where_eq("user_id", user_id).execute(pool).await
    }

    pub async fn delete_expired(pool: &DbPool) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        Delete::from(TABLE).where_lt("expires_at", &*now).execute(pool).await
    }
}
"#;

    ensure_dir_exists(&dir)?;
    write_file(&path, content)
}

// --- Controller ---

fn write_auth_controller(base: &Path) -> Result<(), String> {
    let dir = base.join("src/controllers");
    let path = dir.join("auth.rs");

    let content = r#"use blixt::prelude::*;
use blixt::auth::{self, cookie as auth_cookie, jwt};
use blixt::validate::Validator;

use crate::models::session::Session;
use crate::models::user::User;

const SESSION_TTL_SECS: u64 = 7 * 24 * 60 * 60; // 7 days

// --- Page templates ---

#[derive(Template)]
#[template(path = "pages/auth/register.html")]
pub struct RegisterPage {
    pub csrf: String,
}

#[derive(Template)]
#[template(path = "pages/auth/login.html")]
pub struct LoginPage {
    pub csrf: String,
}

#[derive(Template)]
#[template(path = "pages/auth/forgot_password.html")]
pub struct ForgotPasswordPage {
    pub csrf: String,
}

#[derive(Template)]
#[template(path = "pages/auth/reset_password.html")]
pub struct ResetPasswordPage {
    pub csrf: String,
    pub token: String,
}

// --- Form data ---

#[derive(Deserialize)]
pub struct RegisterForm {
    pub email: String,
    pub password: String,
    pub password_confirm: String,
}

#[derive(Deserialize)]
pub struct LoginForm {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct ForgotPasswordForm {
    pub email: String,
}

#[derive(Deserialize)]
pub struct ResetPasswordForm {
    pub token: String,
    pub password: String,
    pub password_confirm: String,
}

// --- Handlers ---

pub async fn register_page(csrf: CsrfToken) -> Result<impl IntoResponse> {
    render!(RegisterPage {
        csrf: csrf.value().to_owned(),
    })
}

pub async fn register(
    State(ctx): State<AppContext>,
    Form(form): Form<RegisterForm>,
) -> Result<impl IntoResponse> {
    User::validate_registration(&form.email, &form.password, &form.password_confirm)?;

    let user = User::create(&ctx.db, &form.email, &form.password).await
        .map_err(|_| Error::BadRequest("Unable to create account. If you already have one, try signing in.".into()))?;

    info!(user_id = user.id, "registration");
    let (_, response) = create_session(&ctx, user.id, Some("user")).await?;
    Ok(response)
}

pub async fn login_page(csrf: CsrfToken) -> Result<impl IntoResponse> {
    render!(LoginPage {
        csrf: csrf.value().to_owned(),
    })
}

pub async fn login(
    State(ctx): State<AppContext>,
    Form(form): Form<LoginForm>,
) -> Result<impl IntoResponse> {
    let mut v = Validator::new();
    v.str_field(&form.email, "email").not_empty();
    v.str_field(&form.password, "password").not_empty();
    v.check()?;

    let user = match User::verify_credentials(&ctx.db, &form.email, &form.password).await {
        Ok(u) => {
            info!(user_id = u.id, "login_success");
            u
        }
        Err(_) => {
            warn!(email = %form.email, "login_failure");
            return Err(Error::BadRequest("Invalid email or password".into()));
        }
    };

    let (_, response) = create_session(&ctx, user.id, Some(&user.role)).await?;
    Ok(response)
}

pub async fn logout(
    State(ctx): State<AppContext>,
    user: AuthUser,
) -> Result<impl IntoResponse> {
    let uid: i64 = user.user_id.parse()
        .map_err(|_| Error::Internal("invalid user_id in token".into()))?;
    Session::delete_all_for_user(&ctx.db, uid).await?;
    info!(user_id = uid, "logout");

    let mut response = Redirect::to("/login").with_flash(Flash::info("Signed out")).into_response();
    auth_cookie::clear(&mut response);
    Ok(response)
}

pub async fn forgot_password_page(csrf: CsrfToken) -> Result<impl IntoResponse> {
    render!(ForgotPasswordPage {
        csrf: csrf.value().to_owned(),
    })
}

pub async fn forgot_password(
    State(_ctx): State<AppContext>,
    Form(form): Form<ForgotPasswordForm>,
) -> Result<impl IntoResponse> {
    let mut v = Validator::new();
    v.str_field(&form.email, "email").not_empty().pattern(blixt::validate::EMAIL_PATTERN, "must be a valid email");
    v.check()?;

    // Always show success to prevent email enumeration
    // TODO: Look up user, generate reset token, send email
    // let token = generate_reset_token();
    // let hash = sha256_hex(&token);
    // Store hash + expiry on user record, email the raw token
    info!(email = %form.email, "Password reset requested");

    Ok(Redirect::to("/login").with_flash(Flash::info(
        "If that email exists, we sent password reset instructions",
    )))
}

pub async fn reset_password_page(
    csrf: CsrfToken,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse> {
    let token = params.get("token").cloned().unwrap_or_default();
    if token.is_empty() {
        return Err(Error::BadRequest("Missing reset token".into()));
    }
    render!(ResetPasswordPage {
        csrf: csrf.value().to_owned(),
        token,
    })
}

pub async fn reset_password(
    State(_ctx): State<AppContext>,
    Form(form): Form<ResetPasswordForm>,
) -> Result<impl IntoResponse> {
    let mut v = Validator::new();
    v.str_field(&form.password, "password").min_length(8).max_length(128);
    v.check()?;
    if form.password != form.password_confirm {
        return Err(Error::BadRequest("Passwords do not match".into()));
    }
    if form.token.is_empty() {
        return Err(Error::BadRequest("Missing reset token".into()));
    }

    // TODO: implement password reset token validation
    // 1. Look up user by token hash: User::find_by_reset_token(&ctx.db, &form.token).await?
    // 2. Update password: User::update_password(&ctx.db, user.id, &form.password).await?
    // 3. Invalidate sessions: Session::delete_all_for_user(&ctx.db, user.id).await?
    // 4. Clear the reset token on the user record
    // Until implemented, reject all reset attempts:
    Err(Error::BadRequest("Password reset is not yet configured. Please contact support.".into()))
}

// --- Session helpers ---

async fn create_session(
    ctx: &AppContext,
    user_id: i64,
    role: Option<&str>,
) -> Result<(String, axum::response::Response)> {
    let secret = ctx.config.jwt_secret().ok_or_else(|| {
        Error::Internal("JWT_SECRET not configured".into())
    })?;

    let token = jwt::create_token(
        &user_id.to_string(),
        role,
        secret,
        SESSION_TTL_SECS,
    )?;

    let token_hash = auth::sha256_hex(&token);
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(SESSION_TTL_SECS as i64);
    Session::create(&ctx.db, user_id, &token_hash, expires_at).await?;

    let is_production = ctx.config.is_production();
    let mut response = Redirect::to("/").with_flash(Flash::success("Welcome!")).into_response();
    auth_cookie::set(&mut response, &token, SESSION_TTL_SECS, is_production);

    Ok((token, response))
}
"#;

    ensure_dir_exists(&dir)?;
    write_file(&path, content)
}

// --- Templates ---

fn register_template() -> &'static str {
    r##"{% extends "layouts/app.html" %}
{% block title %}Register{% endblock %}
{% block content %}
<main class="min-h-screen flex justify-center px-4 pt-16 pb-12 sm:pt-24">
  <div class="w-full max-w-sm">
    <h1 class="text-lg font-medium text-zinc-200 mb-6">Create an account</h1>

    <form method="post" action="/register" class="space-y-3">
      <input type="hidden" name="_csrf" value="{{ csrf }}">

      <input
        type="email"
        name="email"
        placeholder="Email"
        required
        autocomplete="email"
        class="w-full px-3 py-2.5 text-[13px] rounded-lg border border-zinc-800/80
               bg-zinc-900/40 text-zinc-200 placeholder-zinc-600
               focus:outline-none focus:border-zinc-700 transition-colors"
      >

      <input
        type="password"
        name="password"
        placeholder="Password"
        required
        minlength="8"
        autocomplete="new-password"
        class="w-full px-3 py-2.5 text-[13px] rounded-lg border border-zinc-800/80
               bg-zinc-900/40 text-zinc-200 placeholder-zinc-600
               focus:outline-none focus:border-zinc-700 transition-colors"
      >

      <input
        type="password"
        name="password_confirm"
        placeholder="Confirm password"
        required
        minlength="8"
        autocomplete="new-password"
        class="w-full px-3 py-2.5 text-[13px] rounded-lg border border-zinc-800/80
               bg-zinc-900/40 text-zinc-200 placeholder-zinc-600
               focus:outline-none focus:border-zinc-700 transition-colors"
      >

      <button type="submit"
        class="w-full px-4 py-2.5 text-[13px] rounded-lg border border-zinc-800/80
               bg-zinc-900/40 text-zinc-400 hover:text-zinc-200 hover:border-zinc-700
               transition-colors cursor-pointer"
      >Create account</button>
    </form>

    <p class="mt-4 text-[12px] text-zinc-600 text-center">
      Already have an account? <a href="/login" class="text-zinc-400 hover:text-zinc-200 transition-colors">Sign in</a>
    </p>
  </div>
</main>
{% endblock %}
"##
}

fn login_template() -> &'static str {
    r##"{% extends "layouts/app.html" %}
{% block title %}Sign in{% endblock %}
{% block content %}
<main class="min-h-screen flex justify-center px-4 pt-16 pb-12 sm:pt-24">
  <div class="w-full max-w-sm">
    <h1 class="text-lg font-medium text-zinc-200 mb-6">Sign in</h1>

    <form method="post" action="/login" class="space-y-3">
      <input type="hidden" name="_csrf" value="{{ csrf }}">

      <input
        type="email"
        name="email"
        placeholder="Email"
        required
        autocomplete="email"
        class="w-full px-3 py-2.5 text-[13px] rounded-lg border border-zinc-800/80
               bg-zinc-900/40 text-zinc-200 placeholder-zinc-600
               focus:outline-none focus:border-zinc-700 transition-colors"
      >

      <input
        type="password"
        name="password"
        placeholder="Password"
        required
        autocomplete="current-password"
        class="w-full px-3 py-2.5 text-[13px] rounded-lg border border-zinc-800/80
               bg-zinc-900/40 text-zinc-200 placeholder-zinc-600
               focus:outline-none focus:border-zinc-700 transition-colors"
      >

      <button type="submit"
        class="w-full px-4 py-2.5 text-[13px] rounded-lg border border-zinc-800/80
               bg-zinc-900/40 text-zinc-400 hover:text-zinc-200 hover:border-zinc-700
               transition-colors cursor-pointer"
      >Sign in</button>
    </form>

    <div class="mt-4 flex items-center justify-between text-[12px] text-zinc-600">
      <a href="/forgot-password" class="hover:text-zinc-400 transition-colors">Forgot password?</a>
      <a href="/register" class="text-zinc-400 hover:text-zinc-200 transition-colors">Create account</a>
    </div>
  </div>
</main>
{% endblock %}
"##
}

fn forgot_password_template() -> &'static str {
    r##"{% extends "layouts/app.html" %}
{% block title %}Forgot password{% endblock %}
{% block content %}
<main class="min-h-screen flex justify-center px-4 pt-16 pb-12 sm:pt-24">
  <div class="w-full max-w-sm">
    <h1 class="text-lg font-medium text-zinc-200 mb-6">Reset your password</h1>
    <p class="text-[13px] text-zinc-500 mb-4">
      Enter your email and we'll send you a link to reset your password.
    </p>

    <form method="post" action="/forgot-password" class="space-y-3">
      <input type="hidden" name="_csrf" value="{{ csrf }}">

      <input
        type="email"
        name="email"
        placeholder="Email"
        required
        autocomplete="email"
        class="w-full px-3 py-2.5 text-[13px] rounded-lg border border-zinc-800/80
               bg-zinc-900/40 text-zinc-200 placeholder-zinc-600
               focus:outline-none focus:border-zinc-700 transition-colors"
      >

      <button type="submit"
        class="w-full px-4 py-2.5 text-[13px] rounded-lg border border-zinc-800/80
               bg-zinc-900/40 text-zinc-400 hover:text-zinc-200 hover:border-zinc-700
               transition-colors cursor-pointer"
      >Send reset link</button>
    </form>

    <p class="mt-4 text-[12px] text-zinc-600 text-center">
      <a href="/login" class="text-zinc-400 hover:text-zinc-200 transition-colors">Back to sign in</a>
    </p>
  </div>
</main>
{% endblock %}
"##
}

fn reset_password_template() -> &'static str {
    r##"{% extends "layouts/app.html" %}
{% block title %}Reset password{% endblock %}
{% block content %}
<main class="min-h-screen flex justify-center px-4 pt-16 pb-12 sm:pt-24">
  <div class="w-full max-w-sm">
    <h1 class="text-lg font-medium text-zinc-200 mb-6">Set a new password</h1>

    <form method="post" action="/reset-password" class="space-y-3">
      <input type="hidden" name="_csrf" value="{{ csrf }}">
      <input type="hidden" name="token" value="{{ token }}">

      <input
        type="password"
        name="password"
        placeholder="New password"
        required
        minlength="8"
        autocomplete="new-password"
        class="w-full px-3 py-2.5 text-[13px] rounded-lg border border-zinc-800/80
               bg-zinc-900/40 text-zinc-200 placeholder-zinc-600
               focus:outline-none focus:border-zinc-700 transition-colors"
      >

      <input
        type="password"
        name="password_confirm"
        placeholder="Confirm new password"
        required
        minlength="8"
        autocomplete="new-password"
        class="w-full px-3 py-2.5 text-[13px] rounded-lg border border-zinc-800/80
               bg-zinc-900/40 text-zinc-200 placeholder-zinc-600
               focus:outline-none focus:border-zinc-700 transition-colors"
      >

      <button type="submit"
        class="w-full px-4 py-2.5 text-[13px] rounded-lg border border-zinc-800/80
               bg-zinc-900/40 text-zinc-400 hover:text-zinc-200 hover:border-zinc-700
               transition-colors cursor-pointer"
      >Reset password</button>
    </form>
  </div>
</main>
{% endblock %}
"##
}

// --- Output hints ---

fn print_route_hints() {
    println!(
        "\n  {} Add auth routes to src/main.rs:",
        style("next:").cyan().bold()
    );
    println!("    .route(\"/register\", get(controllers::auth::register_page))");
    println!("    .route(\"/register\", post(controllers::auth::register))");
    println!("    .route(\"/login\", get(controllers::auth::login_page))");
    println!("    .route(\"/login\", post(controllers::auth::login))");
    println!("    .route(\"/logout\", post(controllers::auth::logout))");
    println!("    .route(\"/forgot-password\", get(controllers::auth::forgot_password_page))");
    println!("    .route(\"/forgot-password\", post(controllers::auth::forgot_password))");
    println!("    .route(\"/reset-password\", get(controllers::auth::reset_password_page))");
    println!("    .route(\"/reset-password\", post(controllers::auth::reset_password))");
    println!(
        "\n  {} forgot-password and reset-password are stubs.",
        style("note:").yellow().bold()
    );
    println!("  Implement token generation and email sending before deploying.");
}

/// Patches a freshly-scaffolded project to wire in auth routes and models.
///
/// Called by `blixt new --auth` after both `new::run()` and `generate_auth_in()`
/// have created their files. Rewrites `src/main.rs` and `src/controllers/mod.rs`.
pub fn patch_new_project(project: &Path) -> Result<(), String> {
    let main_rs = project.join("src/main.rs");
    let content =
        fs::read_to_string(&main_rs).map_err(|e| format!("Failed to read main.rs: {e}"))?;

    let anchor = "get(controllers::api::status_fragment))";
    if !content.contains(anchor) {
        return Err(
            "Cannot patch main.rs: expected route anchor not found. Add auth routes manually."
                .into(),
        );
    }

    let patched = content
        .replace("mod controllers;", "mod controllers;\nmod models;")
        .replace(
            "        .route(\"/fragments/status\", get(controllers::api::status_fragment))",
            "        .route(\"/fragments/status\", get(controllers::api::status_fragment))\n\
        // Auth\n\
        .route(\"/register\", get(controllers::auth::register_page))\n\
        .route(\"/register\", post(controllers::auth::register))\n\
        .route(\"/login\", get(controllers::auth::login_page))\n\
        .route(\"/login\", post(controllers::auth::login))\n\
        .route(\"/logout\", post(controllers::auth::logout))\n\
        .route(\"/forgot-password\", get(controllers::auth::forgot_password_page))\n\
        .route(\"/forgot-password\", post(controllers::auth::forgot_password))\n\
        .route(\"/reset-password\", get(controllers::auth::reset_password_page))\n\
        .route(\"/reset-password\", post(controllers::auth::reset_password))",
        );

    fs::write(&main_rs, patched).map_err(|e| format!("Failed to patch main.rs: {e}"))?;
    Ok(())
}

fn write_template(base: &Path, relative: &str, content: &str) -> Result<(), String> {
    let path = base.join("templates").join(relative);
    if let Some(parent) = path.parent() {
        ensure_dir_exists(parent)?;
    }
    write_file(&path, content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tempfile::TempDir;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_db_url(url: &str, f: impl FnOnce()) {
        let _guard = ENV_LOCK.lock().expect("env lock poisoned");
        let prev = std::env::var("DATABASE_URL").ok();
        // SAFETY: protected by ENV_LOCK mutex
        unsafe { std::env::set_var("DATABASE_URL", url) };
        f();
        unsafe {
            match prev {
                Some(v) => std::env::set_var("DATABASE_URL", v),
                None => std::env::remove_var("DATABASE_URL"),
            }
        }
    }

    #[test]
    fn auth_scaffold_creates_all_expected_files() {
        let tmp = TempDir::new().expect("temp dir");
        let base = tmp.path();

        with_db_url("postgres://localhost/test", || {
            let result = generate_auth_in(base);
            assert!(
                result.is_ok(),
                "generate_auth_in failed: {:?}",
                result.err()
            );
        });

        assert!(base.join("src/models/user.rs").exists());
        assert!(base.join("src/models/session.rs").exists());
        assert!(base.join("src/controllers/auth.rs").exists());
        assert!(base.join("templates/pages/auth/register.html").exists());
        assert!(base.join("templates/pages/auth/login.html").exists());
        assert!(
            base.join("templates/pages/auth/forgot_password.html")
                .exists()
        );
        assert!(
            base.join("templates/pages/auth/reset_password.html")
                .exists()
        );
    }

    #[test]
    fn auth_creates_two_migrations() {
        let tmp = TempDir::new().expect("temp dir");
        let base = tmp.path();

        with_db_url("postgres://localhost/test", || {
            generate_auth_in(base).expect("generate failed");
        });

        let entries: Vec<_> = fs::read_dir(base.join("migrations"))
            .expect("migrations dir")
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(entries.len(), 2, "expected 2 migrations");

        let names: Vec<String> = entries
            .iter()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().any(|n| n.contains("create_users")));
        assert!(names.iter().any(|n| n.contains("create_sessions")));
    }

    #[test]
    fn user_model_has_key_methods() {
        let tmp = TempDir::new().expect("temp dir");
        let base = tmp.path();

        with_db_url("postgres://localhost/test", || {
            generate_auth_in(base).expect("generate failed");
        });

        let model = fs::read_to_string(base.join("src/models/user.rs")).expect("user model");
        assert!(model.contains("find_by_email"));
        assert!(model.contains("verify_credentials"));
        assert!(model.contains("validate_registration"));
        assert!(model.contains("hash_password"));
        assert!(model.contains("update_password"));
    }

    #[test]
    fn auth_controller_has_all_handlers() {
        let tmp = TempDir::new().expect("temp dir");
        let base = tmp.path();

        with_db_url("postgres://localhost/test", || {
            generate_auth_in(base).expect("generate failed");
        });

        let controller =
            fs::read_to_string(base.join("src/controllers/auth.rs")).expect("controller");
        assert!(controller.contains("pub async fn register_page("));
        assert!(controller.contains("pub async fn register("));
        assert!(controller.contains("pub async fn login_page("));
        assert!(controller.contains("pub async fn login("));
        assert!(controller.contains("pub async fn logout("));
        assert!(controller.contains("pub async fn forgot_password_page("));
        assert!(controller.contains("pub async fn forgot_password("));
        assert!(controller.contains("pub async fn reset_password_page("));
        assert!(controller.contains("pub async fn reset_password("));
    }

    #[test]
    fn templates_use_csrf_and_forms() {
        let tmp = TempDir::new().expect("temp dir");
        let base = tmp.path();

        with_db_url("postgres://localhost/test", || {
            generate_auth_in(base).expect("generate failed");
        });

        let register = fs::read_to_string(base.join("templates/pages/auth/register.html"))
            .expect("register template");
        assert!(register.contains("_csrf"));
        assert!(register.contains("method=\"post\""));
        assert!(register.contains("action=\"/register\""));
        assert!(register.contains("password_confirm"));

        let login = fs::read_to_string(base.join("templates/pages/auth/login.html"))
            .expect("login template");
        assert!(login.contains("_csrf"));
        assert!(login.contains("action=\"/login\""));
        assert!(login.contains("/forgot-password"));

        let forgot = fs::read_to_string(base.join("templates/pages/auth/forgot_password.html"))
            .expect("forgot template");
        assert!(forgot.contains("action=\"/forgot-password\""));

        let reset = fs::read_to_string(base.join("templates/pages/auth/reset_password.html"))
            .expect("reset template");
        assert!(reset.contains("action=\"/reset-password\""));
        assert!(reset.contains("name=\"token\""));
    }

    #[test]
    fn mod_files_updated() {
        let tmp = TempDir::new().expect("temp dir");
        let base = tmp.path();

        with_db_url("postgres://localhost/test", || {
            generate_auth_in(base).expect("generate failed");
        });

        let models = fs::read_to_string(base.join("src/models/mod.rs")).expect("models mod");
        assert!(models.contains("pub mod user;"));
        assert!(models.contains("pub mod session;"));

        let controllers =
            fs::read_to_string(base.join("src/controllers/mod.rs")).expect("controllers mod");
        assert!(controllers.contains("pub mod auth;"));
    }

    #[test]
    fn postgres_migration_uses_correct_types() {
        let tmp = TempDir::new().expect("temp dir");
        let base = tmp.path();

        with_db_url("postgres://localhost/test", || {
            generate_auth_in(base).expect("generate failed");
        });

        let entries: Vec<_> = fs::read_dir(base.join("migrations"))
            .expect("migrations dir")
            .filter_map(|e| e.ok())
            .collect();

        let users_migration = entries
            .iter()
            .find(|e| e.file_name().to_string_lossy().contains("create_users"))
            .expect("users migration");
        let sql = fs::read_to_string(users_migration.path()).expect("read migration");
        assert!(sql.contains("BIGSERIAL PRIMARY KEY"));
        assert!(sql.contains("TIMESTAMPTZ"));
    }

    #[test]
    fn sqlite_migration_uses_correct_types() {
        let tmp = TempDir::new().expect("temp dir");
        let base = tmp.path();

        with_db_url("sqlite://test.db", || {
            generate_auth_in(base).expect("generate failed");
        });

        let entries: Vec<_> = fs::read_dir(base.join("migrations"))
            .expect("migrations dir")
            .filter_map(|e| e.ok())
            .collect();

        let users_migration = entries
            .iter()
            .find(|e| e.file_name().to_string_lossy().contains("create_users"))
            .expect("users migration");
        let sql = fs::read_to_string(users_migration.path()).expect("read migration");
        assert!(sql.contains("INTEGER PRIMARY KEY AUTOINCREMENT"));
        assert!(sql.contains("DATETIME"));
    }

    #[test]
    fn duplicate_auth_generation_returns_error() {
        let tmp = TempDir::new().expect("temp dir");
        let base = tmp.path();

        with_db_url("postgres://localhost/test", || {
            generate_auth_in(base).expect("first generation");
            let result = generate_auth_in(base);
            assert!(result.is_err());
            assert!(result.unwrap_err().contains("already exists"));
        });
    }
}
