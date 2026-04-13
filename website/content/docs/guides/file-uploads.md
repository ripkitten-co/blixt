+++
title = "File Uploads"
weight = 16
description = "Handle multipart file uploads with validation, CSRF protection, and Storage integration."
+++

# File Uploads

Blixt provides `MultipartForm` and `UploadedFile` for handling file uploads
with built-in CSRF validation, size/type checking, and Storage integration.

## Basic upload

```rust
use blixt::prelude::*;

async fn upload_avatar(
    mut form: MultipartForm,
    State(ctx): State<AppContext>,
) -> Result<impl IntoResponse> {
    while let Some(field) = form.next_field().await? {
        if field.name() == Some("avatar") {
            let file = UploadedFile::from_field(field).await?;
            file.validate()
                .max_size(5 * 1024 * 1024)
                .allowed_types(&["image/jpeg", "image/png", "image/webp"])
                .save(&ctx.storage, "avatars/user-42.jpg")
                .await?;
        }
    }
    Ok(Redirect::to("/profile").with_flash(Flash::success("Avatar updated")))
}
```

## The HTML form

Upload forms must use `enctype="multipart/form-data"` and include a `_csrf`
hidden field as the first field:

```html
<form method="post" action="/upload" enctype="multipart/form-data">
    <input type="hidden" name="_csrf" value="{{ csrf }}">
    <input type="file" name="avatar" accept="image/*">
    <button type="submit">Upload</button>
</form>
```

The `_csrf` field must come before file fields in the form. `MultipartForm`
reads it first and validates against the CSRF cookie before yielding other
fields. Alternatively, send the token as an `x-csrf-token` header.

## UploadedFile

Created from a multipart field:

```rust
let file = UploadedFile::from_field(field).await?;

file.filename()     // Option<&str> — original filename
file.content_type() // Option<&str> — MIME type
file.size()         // usize — bytes
file.data()         // &[u8] — raw bytes
```

## Validation

Chain validators before saving:

```rust
file.validate()
    .max_size(10 * 1024 * 1024)                    // 10MB limit
    .allowed_types(&["image/jpeg", "image/png"])    // MIME whitelist
    .save(&ctx.storage, "uploads/photo.jpg")        // validate + save
    .await?;
```

On failure, returns `Error::BadRequest` with a message like
`"File too large (max 10MB)"` or `"File type image/gif not allowed"`.

Use `.finish()` instead of `.save()` to validate without saving:

```rust
let validated = file.validate()
    .max_size(1024 * 1024)
    .allowed_types(&["application/pdf"])
    .finish()?;
// validated is an UploadedFile — do something custom with it
let bytes = validated.into_bytes();
```

## Saving without validation

Skip the validation chain and save directly:

```rust
let file = UploadedFile::from_field(field).await?;
file.save(&ctx.storage, "documents/report.pdf").await?;
```

## Multiple files

Process multiple file fields in one form:

```rust
async fn upload_gallery(
    mut form: MultipartForm,
    State(ctx): State<AppContext>,
) -> Result<impl IntoResponse> {
    let mut count = 0;
    while let Some(field) = form.next_field().await? {
        if field.name() == Some("photos") {
            let file = UploadedFile::from_field(field).await?;
            let path = format!("gallery/{count}.jpg");
            file.validate()
                .max_size(5 * 1024 * 1024)
                .allowed_types(&["image/jpeg", "image/png"])
                .save(&ctx.storage, &path)
                .await?;
            count += 1;
        }
    }
    Ok(Redirect::to("/gallery"))
}
```

## Security

- CSRF validated automatically by `MultipartForm`
- Always validate file type with `allowed_types()` — don't trust the filename extension
- Always validate file size with `max_size()` — prevent denial-of-service
- Files without a content type are treated as `application/octet-stream`
