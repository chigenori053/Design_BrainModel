# Rust UI PoC

This is the **Presentation Layer** (External Client) for the Design Brain Model.

## Role
- **UI Display**: Rendering the state provided by HybridVM.
- **Human Input**: Capturing user intent and forwarding it to HybridVM.
- **Dumb Client**: Contains NO decision logic, NO state management.

## Connection
It connects to the `design_brain_model` via the Interface Layer (API).
It does **not** import Python code directly.

## Usage
```bash
cargo run
```
