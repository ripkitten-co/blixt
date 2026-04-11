+++
title = "Validation"
weight = 9
description = "Fluent, type-safe input validation with per-field error collection and 422 responses."
+++

Blixt provides a `Validator` with a fluent builder API for validating user
input. Each field type returns its own validator struct, so string rules cannot
be called on integer fields and vice versa -- misuse is caught at compile time.

## Basic usage

Create a `Validator`, add rules for each field, then call `.check()`:

```rust
use blixt::prelude::*;
use blixt::validate::Validator;

async fn create_post(form: Form<PostForm>) -> Result<impl IntoResponse> {
    let data = form.into_inner();

    let mut v = Validator::new();
    v.str_field(&data.title, "title").not_empty().max_length(255);
    v.str_field(&data.body, "body").not_empty();
    v.i64_field(data.priority, "priority").range(1, 5);
    v.check()?;

    // validation passed, proceed with the insert
    Ok(Redirect::to("/posts"))
}
```

If any rule fails, `v.check()` returns `Err(Error::Validation(...))`, which
Blixt converts to a `422 Unprocessable Entity` response with a JSON body
containing per-field error messages.

## String field rules

`v.str_field(value, name)` returns a `StrFieldValidator` with these chainable
methods:

### `.not_empty()`

Requires the string to be non-empty after trimming. Whitespace-only strings
are rejected.

```rust
v.str_field(&title, "title").not_empty();
```

### `.max_length(max)`

Requires the string length to be at most `max` bytes.

```rust
v.str_field(&title, "title").max_length(255);
```

### `.min_length(min)`

Requires the string length to be at least `min` bytes.

```rust
v.str_field(&password, "password").min_length(8);
```

### `.pattern(regex, message)`

Requires the string to match a regex pattern. The `message` is appended to the
field name in the error.

```rust
use blixt::validate::EMAIL_PATTERN;

v.str_field(&email, "email")
    .pattern(EMAIL_PATTERN, "must be a valid email address");
```

If the regex itself is invalid, an error is added for the field rather than
panicking.

## Integer field rules

`v.i64_field(value, name)` returns an `I64FieldValidator` with these chainable
methods:

### `.range(min, max)`

Requires the value to be within an inclusive range.

```rust
v.i64_field(priority, "priority").range(1, 5);
```

### `.positive()`

Requires the value to be greater than zero.

```rust
v.i64_field(quantity, "quantity").positive();
```

## Type safety

String rules and integer rules live on separate types. This means the compiler
catches invalid combinations:

```rust
// This compiles:
v.str_field(&title, "title").not_empty().max_length(255);
v.i64_field(count, "count").range(0, 100).positive();

// This does NOT compile:
// v.i64_field(count, "count").not_empty();  // no method `not_empty` on I64FieldValidator
// v.str_field(&title, "title").range(1, 5); // no method `range` on StrFieldValidator
```

## Built-in patterns

The `blixt::validate` module exports commonly used regex patterns:

| Constant              | Pattern                                          | Matches                        |
|-----------------------|--------------------------------------------------|--------------------------------|
| `EMAIL_PATTERN`       | `^[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}$` | Email addresses          |
| `SLUG_PATTERN`        | `^[a-z0-9]+(?:-[a-z0-9]+)*$`                    | URL-safe slugs (`my-post`)     |
| `ALPHANUMERIC_PATTERN`| `^[a-zA-Z0-9]+$`                                | Alphanumeric strings           |

```rust
use blixt::validate::{Validator, EMAIL_PATTERN, SLUG_PATTERN};

let mut v = Validator::new();
v.str_field(&email, "email")
    .not_empty()
    .pattern(EMAIL_PATTERN, "must be a valid email address");
v.str_field(&slug, "slug")
    .not_empty()
    .pattern(SLUG_PATTERN, "must be a valid slug (lowercase, hyphens only)");
v.check()?;
```

## Error output

When validation fails, `v.check()` returns `Error::Validation(ValidationErrors)`.
The `ValidationErrors` struct contains a `HashMap<String, Vec<String>>` mapping
field names to lists of error messages.

The HTTP response is `422 Unprocessable Entity` with a JSON body:

```json
{
    "errors": {
        "title": ["title must not be empty"],
        "priority": ["priority must be between 1 and 5"]
    }
}
```

Error messages reference the field name but never include the submitted value,
preventing accidental data leaks in responses.

## Chaining rules

Multiple rules on the same field are evaluated independently. All failures are
collected:

```rust
let mut v = Validator::new();
v.str_field("ab", "username")
    .not_empty()
    .min_length(3)
    .max_length(20)
    .pattern(ALPHANUMERIC_PATTERN, "must contain only letters and numbers");
v.check()?;
// Error: username must be at least 3 characters
```

Multiple fields can fail independently. All errors are returned together:

```rust
let mut v = Validator::new();
v.str_field("", "title").not_empty();
v.i64_field(0, "priority").range(1, 5);
let err = v.check().unwrap_err();
// Both "title" and "priority" appear in the error response
```

## Full example: validated form submission

```rust
use blixt::prelude::*;
use blixt::validate::{Validator, EMAIL_PATTERN};

#[derive(Deserialize)]
struct RegisterForm {
    username: String,
    email: String,
    password: String,
}

async fn register(form: Form<RegisterForm>) -> Result<impl IntoResponse> {
    let data = form.into_inner();

    let mut v = Validator::new();
    v.str_field(&data.username, "username")
        .not_empty()
        .min_length(3)
        .max_length(30);
    v.str_field(&data.email, "email")
        .not_empty()
        .pattern(EMAIL_PATTERN, "must be a valid email address");
    v.str_field(&data.password, "password")
        .not_empty()
        .min_length(12);
    v.check()?;

    // All fields are valid, proceed with registration
    Ok(Redirect::to("/login")
        .with_flash(Flash::success("Account created")))
}
```
