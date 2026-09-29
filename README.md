# Black Betty (BB)

A high-performance, modular desktop AI agent workspace designed for autonomous coding, architectural analysis, and developer tooling integration. 

Black Betty pairs an event-driven **Rust** backend with a lightweight, responsive **Godot 4** GUI to deliver an out-of-the-box development environment.

---

## Architecture Overview

                  +-----------------------------+
                  |   Godot 4 Frontend (GUI)    |
                  |  - Chat & Event Log Monitor |
                  |  - Workspace File Tree      |
                  +--------------+--------------+
                                 |
                         IPC / JSON-RPC Bus
                                 |
                  +--------------v--------------+
                  |      Rust Core Backend      |
                  |  - Recon: Context Slicing   |
                  |  - Forge: Patch Synthesis   |
                  |  - Tool & Process Execution |
                  +--------------+--------------+
                                 |
         +-----------------------+-----------------------+
         |                                               |
+------------v------------+                     +------------v------------+
|   AI Engine Integrations|                     |    Integrated Tooling   |
| - Gemini API            |                     | - Lite XL (Code Editor) |
| - Local LLM Providers   |                     | - RLL (Debugger/Monitor)|
+-------------------------+                     | - Betty Companion       |
                                                +-------------------------+

### Key Highlights
- **Two-Stage Context Slicing**: Utilizes a targeted Recon phase to identify and extract precise code slices before synthesizing patches in the Forge pipeline.
- **Portable "Out-of-the-Box" Tooling**: Ships with self-contained, pre-configured portable tools located in `tools/`:
  - **Lite XL**: Lightweight, fast code editor integrated into the workflow.
  - **RLL (Reg Life Log)**: Dedicated diagnostic debugger and process inspector.
  - **Betty Companion**: Web companion assistant.
- **Robust Event Bus**: Non-blocking asynchronous message bus orchestrating state between the Rust backend and Godot frontend.

---

## Directory Structure

```text
BB/
├── betty_backend/        # Rust core backend (engine, IPC, AI pipelines)
├── betty_frontend/       # Godot 4 desktop user interface
├── tools/                # Portable out-of-the-box workspace binaries
│   ├── betty_companion/  # Companion utility
│   ├── lite-xl/          # Portable code editor
│   └── RLLTool/          # Diagnostic logging & debugging suite
├── architecture.yaml     # System architecture specifications
└── .gitignore            # Clean environment & artifact filters

Getting Started
Prerequisites
Rust toolchain (latest stable)

Godot Engine 4.x (Standard 64-bit)

Running Locally
Start the Backend:

Bash
cd betty_backend
cargo run --release
Launch the Frontend:
Open betty_frontend/project.godot inside Godot Engine 4 and run the project (F5), or execute the compiled frontend binary.

Configure:
Open the Settings Dialog in the UI, select your preferred language (English / Russian), paste your Gemini API key, and you're good to go!

License
MIT License. Free to use, adapt, and build upon.