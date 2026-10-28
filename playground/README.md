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

The playground is a static client-side editor. It validates OWS definitions as
they change. Workflow execution and database events are not connected yet.

The Pages workflow publishes the project site at `/ows-rs/`. In the repository
settings, set **Pages → Build and deployment → Source** to **GitHub Actions**.
