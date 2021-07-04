# About
An Rust library for creating HTTP image servers that can:
* accept image uploads via multi-part forms at `POST /upload`
* serve existing uploads at `GET /uploads/{filename}.{extension}`.
  * substitute `{extension}` with `png`, `jpeg`, or `webp` for dynamic encoding.
  * use query string parameter `w={width}` and `h={height}` for dynamic resizing
  * use query string parameter `w={width}` and `h={height}` for dynamic resizing
  * use query string parameter `stretch={boolean}` to determine whether resizing
  should affect aspect ratio. Defaults to `false`, which preserves aspect ratio.
  * use query string parameter `sampling={method}` to specify which algorithm to
  use for resizing. Options are `triangle`, `catmullrom`, `gaussian`, `lanczos3`, and `nearest`. Defaults to `nearest`.

# Usage

```toml
# Cargo.toml
[dependencies]
imogen = { git = "github.com/craigfay/imogen.git", ref = "1.0.0" }
```

```rust
// main.rs
use imogen::ImageServer;

fn main() {
    ImageServer::listen(8080);
}
```
