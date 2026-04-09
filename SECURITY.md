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
