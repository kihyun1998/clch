# The dashboard is a React + Vite SPA embedded in the binary

The dashboard UI is a React single-page app built with Vite, and its build output is embedded in the Rust binary, so users install one file and the Rust server provides the API. Contributors who touch the UI need a Node toolchain; a release must build the UI before the crate is packaged. Next.js was considered and rejected: embedding it allows only its static export, which drops the server features that would justify it.

## Considered Options

- Plain HTML and JavaScript with no build step. Rejected in favour of React's component model for the dashboard.
- Next.js with static export. Rejected for the reason above.
