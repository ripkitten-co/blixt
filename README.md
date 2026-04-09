<p align="center">
  <img alt="Blixt" src="crates/blixt-cli/logo.svg" width="80">
</p>

<h3 align="center">Blixt</h3>
<p align="center">Lightning-fast Rust web framework.<br>Compile-time safety. Zero JavaScript build steps.</p>

<p align="center">
  <a href="https://opensource.org/licenses/MIT"><img src="https://img.shields.io/badge/license-MIT-blue?logo=opensourceinitiative&logoColor=white" alt="License"></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/rust-2024_edition-orange?logo=rust&logoColor=white" alt="Rust"></a>
  <a href="#"><img src="https://img.shields.io/badge/lighthouse-100%2F100%2F100%2F100-brightgreen?logo=lighthouse&logoColor=white" alt="Lighthouse"></a>
  <a href="#"><img src="https://img.shields.io/badge/TTFB-1ms-amber?logo=speedtest&logoColor=white" alt="TTFB"></a>
</p>

---

## Stack

| | |
|---|---|
| **Axum** | HTTP server (~700k req/s) |
| **Askama** | Compile-time HTML templates |
| **SQLx** | Compile-time checked SQL queries |
| **Datastar** | SSE-based client interactivity (no JS framework) |
| **Tailwind v4** | Auto-downloaded, zero Node.js |

## Quick start

```bash
cargo install blixt-cli
blixt new my_app
cd my_app
blixt dev
```

## CLI

```
blixt new <name>              Create a new project
blixt dev                     Dev server with file watching + Tailwind HMR
blixt build                   Production build (single binary)
blixt generate controller     Generate controller + templates
blixt generate model          Generate model + migration
blixt generate scaffold       Generate full CRUD
blixt db migrate              Run migrations
blixt db rollback             Rollback last migration
```

## License

MIT
