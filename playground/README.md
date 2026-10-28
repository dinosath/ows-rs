# OWS Workflow Playground

This Dioxus web app hosts the reusable workflow editor from
`crates/ows-workflow-editor`. GitHub Pages deployment is handled by
`.github/workflows/playground-pages.yml`.

## Run locally

Install the matching Dioxus CLI once if it is not already installed:

```sh
cargo install dioxus-cli --version 0.7.10 --locked
```

Then, from the repository root, start the web app:

```sh
dx serve --package ows-playground --platform web
```

Open the local address printed by `dx` in your browser. Local serving uses the
root path; the Pages build uses the repository subpath.

The playground is a static client-side editor. It validates definitions as
they change, imports JSON or YAML from a file or URL, and includes an Examples
menu with the workflows in `examples/workflows` plus `download-novel.yaml`.
Use **Input** to enter the JSON value passed to a workflow.

Execution runs in the browser with the `ows-expressions` engine. HTTP calls are
sent directly from the browser and require the target server to allow CORS.
The sample `listen` task reads an emitted in-memory event or returns `null`,
`wait` completes immediately, and the `run` sample supports `echo`; the browser
cannot launch arbitrary system commands. The playground does not upload
workflow definitions to a backend.

The Pages workflow publishes the project site at `/ows-rs/`. In the repository
settings, set **Pages → Build and deployment → Source** to **GitHub Actions**.
