# Show available commands.
default:
    @just --list

# Install JavaScript dependencies.
i:
    bun install

# Run the Tauri app in development mode.
dev:
   bun i && bun run tauri dev

# Create a production Tauri build.
build:
    bun run tauri build

# Format JavaScript, Svelte, JSON, TOML, and Rust sources.
fmt:
    bunx oxfmt . '!**/.agents/**' '!**/.claude/**' '!**/.crush/**' '!**/.pi/**'
    cargo fmt --manifest-path src-tauri/Cargo.toml

# Lint JavaScript and TypeScript sources.
lint:
    bunx oxlint src

# Remove generated dependencies and build artifacts.
clean:
    rm -rf node_modules build .svelte-kit package src-tauri/target src-tauri/gen/schemas
