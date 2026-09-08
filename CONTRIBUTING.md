# Contributing

Use Rust 1.85 or newer. Keep Vault behavior portable across Windows, macOS and
Linux; do not add an operating-system or Agent-host requirement to the core.

Before opening a pull request, run the smallest relevant tests and `cargo fmt
--all -- --check`. Changes to public CLI, MCP, HTTP, Skill, configuration or
file-format behavior must update their owning reference document.

Do not include personal Vault content, source objects, credentials, local
paths or tokens in commits, issues or pull requests. Security-sensitive
reports follow [SECURITY.md](SECURITY.md).
