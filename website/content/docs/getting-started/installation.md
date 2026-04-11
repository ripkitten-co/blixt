+++
title = "Installation"
weight = 1
description = "Install Blixt and create your first project"
+++

## Prerequisites

Blixt requires **Rust 1.85+** with the `cargo` package manager. If you don't have Rust installed, grab it from [rustup.rs](https://rustup.rs):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

You also need a database. Blixt supports **PostgreSQL** and **SQLite**:

- **PostgreSQL** -- recommended for production. Install via your package manager or [postgresql.org](https://www.postgresql.org/download/).
- **SQLite** -- zero setup, great for prototyping. Already available on most systems.

## Install the CLI

Install `blixt-cli` from the repository:

```bash
cargo install --git https://github.com/ripkitten-co/blixt blixt-cli
```

Verify the installation:

```bash
blixt --help
```

You should see the available commands:

```
Lightning-fast Rust web framework

Usage: blixt <COMMAND>

Commands:
  new       Create a new Blixt project
  dev       Start the development server
  build     Build for production
  generate  Generate scaffolding
  db        Run database migrations
```

## Create a project

Generate a new project with `blixt new`. You can specify the database backend with `--db`, or Blixt will prompt you interactively:

```bash
# PostgreSQL (default)
blixt new my_app --db postgres

# SQLite
blixt new my_app --db sqlite
```

This downloads [Datastar](https://data-star.dev) (verified by SHA-256 checksum), compiles your Tailwind CSS, and scaffolds the full project.

## What gets generated

```
my_app/
  Cargo.toml
  .env.example
  .gitignore
  src/
    main.rs
    controllers/
      mod.rs
      home.rs
      api.rs
  templates/
    layouts/
      app.html
    pages/
      home.html
    fragments/
      time.html
      status.html
    components/
    emails/
  static/
    css/
      app.css
      output.css
    js/
      datastar.js
    logo.svg
  migrations/
```

Key pieces:

- **`src/main.rs`** -- application entry point with route registration
- **`src/controllers/`** -- request handlers, one file per resource
- **`templates/`** -- Askama HTML templates organized by type (layouts, pages, fragments, components, emails)
- **`static/`** -- CSS, JavaScript, and other assets served at `/static/`
- **`migrations/`** -- SQL migration files, run in timestamp order

## Configure the environment

Copy the example env file and edit it:

```bash
cd my_app
cp .env.example .env
```

For PostgreSQL, update `DATABASE_URL` with your connection string:

```
BLIXT_ENV=development
HOST=127.0.0.1
PORT=3000
DATABASE_URL=postgres://localhost/my_app
JWT_SECRET=change-me-to-a-random-secret-at-least-32-chars
```

For SQLite, the default works out of the box:

```
DATABASE_URL=sqlite://data.db
```

## Run the dev server

Start the development server:

```bash
blixt dev
```

This does three things simultaneously:

1. Compiles and runs your application with `cargo run`
2. Watches `src/` and `templates/` for changes and auto-restarts the server
3. Runs Tailwind CSS in watch mode for hot CSS reloading

Open [http://localhost:3000](http://localhost:3000) in your browser. You should see the Blixt welcome page with a reactive counter (client-side Datastar signals) and SSE fragment demos.

## Project dependencies

The generated `Cargo.toml` includes these core dependencies:

```toml
[dependencies]
askama = "0.15"
axum = { version = "0.8", features = ["macros"] }
blixt = { git = "https://github.com/ripkitten-co/blixt" }
chrono = { version = "0.4", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
tracing = "0.1"
```

For SQLite projects, the `blixt` dependency uses `default-features = false` with the `sqlite` feature flag:

```toml
blixt = { git = "https://github.com/ripkitten-co/blixt", default-features = false, features = ["sqlite"] }
```
