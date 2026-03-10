# Repository Guidelines

## Project Structure & Module Organization
This repository is documentation-first. The current source of truth lives in:

- `AGENTS.md`: contributor instructions for this repository.
- `general_info/Business_Requirements_Document.md`: business rules, operations, costs, and phased scope.
- `general_info/Flow_Design_Diagram.mermaid`: end-to-end conversation flow.
- `Software_Design_Document.md`: target Rust architecture, data model, state machine, and webhook contract.
- `Implementation_&_Deployment_Document.md`: phased build plan and acceptance criteria.

There is no `src/` or `tests/` directory yet. When implementation starts, follow the planned layout in `Software_Design_Document.md`: `src/routes/`, `src/whatsapp/`, `src/bot/`, `src/db/`, plus `migrations/`.

## Build, Test, and Development Commands
There is no active build pipeline in this workspace yet. Use these commands for document work and planned Rust development:

- `git status --short --branch`: confirm working tree state before editing.
- `rg --files`: list repository files quickly.
- `sed -n '1,120p' general_info/Business_Requirements_Document.md`: inspect a document section before editing.
- `cargo check`: verify the Rust project compiles once scaffolding exists.
- `cargo test`: run unit and integration tests once tests are added.
- `cargo fmt && cargo clippy --all-targets --all-features`: format and lint Rust code before review.

## Coding Style & Naming Conventions
Keep Markdown concise, task-oriented, and aligned with the existing Spanish project documents. Use clear section headings and short paragraphs. For Mermaid, prefer readable node labels and grouped flow sections.

For planned Rust code, follow standard conventions: 4-space indentation, `snake_case` for files/functions/modules, `PascalCase` for structs/enums, and small focused modules such as `pricing.rs` or `state_machine.rs`. Avoid `unwrap()` in production code; the implementation guide explicitly requires structured error handling.

## Testing Guidelines
No automated test suite exists yet. For documentation changes, verify internal consistency across the business, design, and implementation documents. For Rust code, add `cargo test` coverage for webhook parsing, pricing rules, and state transitions. Name tests by behavior, for example `parses_button_reply_payload` or `applies_liquor_pair_discount`.

## Commit & Pull Request Guidelines
The current history is minimal and uses a plain descriptive subject (`setting AGENTS.MD and secuencial phase of the project`). Keep future commit subjects short, imperative, and specific. Prefer patterns like `docs: refine implementation phases` or `feat: scaffold webhook routes` over vague multi-topic messages.

Pull requests should include:

- a short summary of the change,
- affected documents or modules,
- linked issue or requirement section when available,
- screenshots or rendered Mermaid output when a flow diagram changes.
