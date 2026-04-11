+++
title = "Templates"
weight = 2
description = "Render HTML with Askama templates, template inheritance, and the render! macro."
+++

# Templates

Blixt uses [Askama](https://docs.rs/askama) for compile-time checked HTML templates. Templates are type-safe Rust structs that map to HTML files -- if a template variable is missing or the wrong type, the compiler catches it.

## Defining a template

Derive `Template` on a struct and point it at an HTML file:

```rust
use blixt::prelude::*;

#[derive(Template)]
#[template(path = "pages/home.html")]
struct HomePage {
    title: String,
    posts: Vec<Post>,
}
```

The `path` is relative to your project's `templates/` directory. Fields on the struct become variables in the template.

## Template file locations

Generated Blixt projects organize templates into subdirectories by purpose:

```
templates/
  layouts/       # Base layouts with shared HTML structure
  pages/         # Full page templates that extend layouts
  fragments/     # Partial HTML for Datastar SSE updates
  components/    # Reusable UI components
  emails/        # Email body templates
```

This is a convention, not a requirement -- Askama resolves any path relative to `templates/`.

## Template inheritance

Layouts define a base HTML structure with named blocks. Pages extend a layout and fill in those blocks.

**Layout** (`templates/layouts/app.html`):

```html
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <meta name="description" content="{% block description %}A Blixt application{% endblock %}">
    <title>{% block title %}Blixt App{% endblock %}</title>
    <link rel="stylesheet" href="/static/css/output.css" data-blixt-css>
</head>
<body>
    {% block content %}{% endblock %}
    <script type="module" src="/static/js/datastar.js"></script>
</body>
</html>
```

**Page** (`templates/pages/home.html`):

```html
{% extends "layouts/app.html" %}
{% block title %}Home{% endblock %}
{% block description %}Welcome to the app{% endblock %}
{% block content %}
<main>
  <h1>{{ greeting }}</h1>
</main>
{% endblock %}
```

Blocks not overridden keep their default content from the layout.

## The render! macro

`render!` takes a template struct, calls `.render()`, and wraps the result in `Html` for Axum. It converts rendering errors into `Error::Internal`.

```rust
async fn index() -> Result<impl IntoResponse> {
    render!(HomePage {
        title: "Welcome".to_string(),
        posts: vec![],
    })
}
```

This expands to roughly:

```rust
let html = template.render().map_err(|e| Error::Internal(e.to_string()))?;
Ok(Html(html))
```

Without the macro you would write:

```rust
async fn index() -> Result<impl IntoResponse> {
    let page = HomePage { title: "Welcome".to_string(), posts: vec![] };
    let html = page.render().map_err(|e| Error::Internal(e.to_string()))?;
    Ok(Html(html))
}
```

## Including fragments

Use `{% include %}` to embed one template inside another. This is how list and item fragments compose:

**List fragment** (`templates/fragments/todo_list.html`):

```html
<div id="todo-list">
{% for todo in page.items %}
  {% include "fragments/todo_item.html" %}
{% endfor %}
</div>
```

**Page including the fragment** (`templates/pages/home.html`):

```html
{% extends "layouts/app.html" %}
{% block content %}
<main>
  {% include "fragments/todo_list.html" %}
</main>
{% endblock %}
```

Included templates have access to all variables in the including template's scope. The `todo` variable from the `{% for %}` loop is available inside `todo_item.html`.

## Template syntax

Askama uses `{{ }}` for expressions and `{% %}` for control flow:

```html
<!-- Variables -->
<h1>{{ title }}</h1>

<!-- Conditionals -->
{% if page.items.is_empty() %}
  <p>No items yet.</p>
{% else %}
  <p>{{ page.total }} items found.</p>
{% endif %}

<!-- Loops -->
{% for item in items %}
  <li>{{ item.name }}</li>
{% endfor %}

<!-- Method calls -->
<span>Page {{ page.page }} / {{ page.total_pages }}</span>

<!-- Expressions -->
{% if page.total == 1 %}item{% else %}items{% endif %}
```

## Auto-escaping

Askama escapes HTML by default in `.html` templates. Given a struct field containing `<script>alert('xss')</script>`, the output will be:

```
&lt;script&gt;alert(&#x27;xss&#x27;)&lt;/script&gt;
```

This prevents XSS without any extra work. To render raw HTML (only for trusted content), use the `|safe` filter:

```html
{{ trusted_html|safe }}
```

## Fragments for Datastar

Fragments are partial HTML templates used with Datastar SSE responses. Define them as standalone template structs:

```rust
#[derive(Template)]
#[template(path = "fragments/todo_list.html")]
struct TodoListFragment {
    page: Paginated<Todo>,
}
```

Return them as SSE patches using `SseFragment` or `SseResponse`:

```rust
async fn update(State(ctx): State<AppContext>) -> Result<impl IntoResponse> {
    let page = fetch_page(&ctx.db, 1).await?;
    SseFragment::new(TodoListFragment { page })
}
```

The fragment HTML replaces the matching DOM element (by `id` attribute) on the client, with no full page reload.
