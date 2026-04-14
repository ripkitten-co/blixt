# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability, please report it responsibly.

**Do not open a public issue.**

Open a [private security advisory](https://github.com/ripkitten-co/blixt/security/advisories/new) on GitHub.

You should receive a response within 48 hours. We will work with you to understand and address the issue before any public disclosure.

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.x     | Latest only |

## Security Design

Blixt is designed with security defaults:

- **SQL injection**: Prevented at compile time by SQLx parameterized queries
- **XSS**: Prevented at compile time by Askama auto-escaping
- **CSRF**: Double-submit cookie middleware enabled by default
- **Auth**: Deny-by-default route protection via AuthUser extractor
- **Passwords**: argon2id hashing with secure defaults
- **JWT**: Algorithm pinned to HS256, `none` algorithm rejected
- **Headers**: CSP, HSTS, X-Frame-Options, and more set by default
- **Rate limiting**: Token bucket per IP on auth endpoints
- **Memory safety**: Rust prevents buffer overflows and use-after-free

## Dependency Advisory Policy

Blixt runs `cargo audit` on every push, pull request, and weekly schedule.
Each advisory that surfaces is evaluated with one of three outcomes:

1. **Upgrade available, fix immediately** — bump the dependency in the next PR
2. **No upgrade path, not exploited in our code** — document in `.cargo/audit.toml` with
   justification and blocker; re-review on every release
3. **No upgrade path, actually exploitable** — treat as a critical issue, file a
   private advisory, release a workaround

All accepted advisories live in [`.cargo/audit.toml`](./.cargo/audit.toml)
with inline comments explaining *why* each is accepted and *what would unblock*
removing it. This file is the single source of truth for both local audits
and CI.

### Currently accepted advisories

| Advisory | Crate | Why it doesn't affect Blixt |
|----------|-------|-----------------------------|
| [RUSTSEC-2026-0097](https://rustsec.org/advisories/RUSTSEC-2026-0097) | rand 0.8.x | Requires a custom `log` logger calling `rand::rng()` during reseeding. Blixt uses `tracing` and never calls `rand::rng()` from any logging path. |
| [RUSTSEC-2023-0071](https://rustsec.org/advisories/RUSTSEC-2023-0071) | rsa 0.9.x | Marvin Attack requires RSA key operations. Blixt pins JWT to HS256 (HMAC) and uses Postgres/SQLite only. The `rsa` crate is transitively linked but never exercised. |

Both are blocked on upstream major version releases from `jsonwebtoken`,
`sqlx-postgres`, and related crates that pin the older `rand 0.8.x` line.
