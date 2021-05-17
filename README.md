# About
A prototype file server built with Rust. Each section below describes how to demo a piece of functionality.

# Upload Files
* Start the Rust server: `cargo run`
* Use NodeJS to serve **index.html**: `npx serve`
* Visit **index.html**, probably at http://localhost:5000
* Choose a file, click submit, and it should appear in `./uploads`

# Re-encode .png files as .webp
* Start the Rust server: `cargo run`
* Visit [rust.png](http://localhost:8080/rust.png)
  * The onscreen file has been served statically from the project root
* Visit [rust.webp](http://localhost:8080/rust.png)
  * The onscreen file has been re-encoded and served, reducing the size by 50%