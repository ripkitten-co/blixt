+++
title = "CLI Reference"
weight = 1
description = "Complete reference for the blixt command-line tool: project scaffolding, code generation, dev server, builds, and database migrations."
+++

# CLI Reference

The `blixt` CLI scaffolds projects, generates code, runs the development server, and manages database migrations.

```
blixt <COMMAND>
```

## blixt new

Create a new Blixt project.

```
blixt new <NAME> [--db <BACKEND>] [--auth]
```

| Argument | Required | Description |
|----------|----------|-------------|
| `NAME` | yes | Project name (used as directory name and crate name) |
| `--db <BACKEND>` | no | Database backend: `postgres` (alias `pg`) or `sqlite`. Prompts interactively if omitted. |
| `--auth` | no | Include authentication scaffold (users, sessions, login/register/password reset) |

The command creates a full project directory with:

- `Cargo.toml` configured with the chosen database feature
- `src/` with controllers, models, and route setup
- `templates/` with layouts, pages, fragments, components, and email directories
- `migrations/` directory
- `static/` with CSS and Datastar JS
- `.env` file with development defaults

**Examples:**

```bash
blixt new my_app --db postgres
blixt new blog --db sqlite
blixt new my_app --auth       # includes auth scaffold with routes pre-wired
blixt new my_app              # interactive database prompt
```

If no TTY is detected (e.g. CI), the `--db` flag is required.

## blixt dev

Start the development server with file watching and Tailwind CSS hot reload.

```
blixt dev
```

This command:

1. Verifies you are in a Blixt project directory (checks for `Cargo.toml`)
2. Downloads the Tailwind CSS binary if not already present
3. Starts Tailwind in watch mode for live CSS recompilation
4. Starts `cargo run` for the application
5. Watches for file changes in `src/` and `templates/` and restarts the application automatically

When the application exits with a non-zero status, the dev server waits for file changes before restarting. Press `Ctrl+C` to shut down.

## blixt build

Build the project for production deployment.

```
blixt build
```

This command:

1. Verifies you are in a Blixt project directory
2. Compiles Tailwind CSS with `--minify`
3. Runs `cargo build --release`
4. Reports the output binary path and file size

## blixt generate

Generate scaffolding code. All generators validate the name argument against Rust reserved keywords and naming rules.

### blixt generate controller

Generate a controller with Askama template views.

```
blixt generate controller <NAME>
```

Creates:

- `src/controllers/<name>.rs` with index and show handler functions
- `templates/pages/<name>/index.html` template
- `templates/pages/<name>/show.html` template
- Updates `src/controllers/mod.rs` to include the new module

After generation, the CLI prints a route hint showing how to wire the controller into your router.

**Example:**

```bash
blixt generate controller posts
```

### blixt generate model

Generate a model with a database migration.

```
blixt generate model <NAME> [FIELDS...]
```

Fields use `name:type` format. Supported types:

| Type | Aliases | Rust type | PostgreSQL | SQLite |
|------|---------|-----------|------------|--------|
| `string` | `text` | `String` | `TEXT NOT NULL` | `TEXT NOT NULL` |
| `int` | `integer` | `i64` | `BIGINT NOT NULL DEFAULT 0` | `INTEGER NOT NULL DEFAULT 0` |
| `bool` | `boolean` | `bool` | `BOOLEAN NOT NULL DEFAULT FALSE` | `BOOLEAN NOT NULL DEFAULT 0` |
| `float` | | `f64` | `DOUBLE PRECISION NOT NULL DEFAULT 0` | `REAL NOT NULL DEFAULT 0` |

Creates:

- `src/models/<name>.rs` with a struct deriving `FromRow` and `Serialize`
- A timestamped SQL migration file in `migrations/`
- Updates `src/models/mod.rs` to include the new module

The `id`, `created_at`, and `updated_at` fields are generated automatically and cannot be specified manually. SQL reserved words and Rust keywords are rejected as field names.

**Examples:**

```bash
blixt generate model post title:string body:text published:bool
blixt generate model product name:string price:float active:boolean
```

### blixt generate scaffold

Generate a full CRUD scaffold: controller, model, views, and list fragment.

```
blixt generate scaffold <NAME> [FIELDS...]
```

Combines controller and model generation, then adds a Datastar-ready list fragment template for streaming updates. Field syntax is the same as `blixt generate model`.

If no fields are specified, a single `name:string` field is created by default.

**Examples:**

```bash
blixt generate scaffold task title:string priority:int completed:bool
blixt generate scaffold item                        # defaults to name:string
```

### blixt generate auth

Generate a complete authentication scaffold.

```
blixt generate auth
```

Creates:

- `migrations/*_create_users.sql` with email, password_hash, role, reset token columns
- `migrations/*_create_sessions.sql` with token_hash and expiry
- `src/models/user.rs` with registration, login, password reset methods
- `src/models/session.rs` with create, revoke, cleanup methods
- `src/controllers/auth.rs` with 9 handlers (register, login, logout, forgot/reset password)
- `templates/pages/auth/` with register, login, forgot password, reset password pages
- `templates/emails/reset_password.html` for password reset emails
- Updates `src/models/mod.rs` and `src/controllers/mod.rs`

After generation, the CLI prints the routes to add to your `src/main.rs`.

Password reset emails are sent via the `Mailer` when SMTP is configured. Without SMTP variables, reset links are logged to stdout for local development.

**Example:**

```bash
blixt generate auth
```

## blixt db

Database migration commands. All subcommands read `DATABASE_URL` from the environment (or `.env` file) and connect using SQLx's `AnyPool`, which supports both PostgreSQL and SQLite URLs.

### blixt db migrate

Run all pending migrations.

```
blixt db migrate
```

Applies every unapplied migration from the `./migrations` directory in order. Reports the number of migrations applied.

### blixt db rollback

Revert the most recently applied migration.

```
blixt db rollback
```

Rolls back exactly one migration. Run multiple times to revert further.

### blixt db status

Display a table of all migrations and their applied/pending status.

```
blixt db status
```

Output shows each migration's version number, status (`applied` or `pending`), and description.

## Field type reference

When specifying fields for `generate model` and `generate scaffold`, use these types:

```
title:string      # or title:text
count:int         # or count:integer
active:bool       # or active:boolean
score:float
```

**Validation rules:**

- Field names must be valid Rust identifiers in `snake_case`
- Reserved names (`id`, `created_at`, `updated_at`) are rejected
- SQL reserved words (`select`, `order`, `table`, `index`, etc.) are rejected
- Rust keywords (`type`, `fn`, `struct`, etc.) are rejected
- Duplicate field names within a single command are rejected
