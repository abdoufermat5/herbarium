MANIFEST := --manifest-path desktop/Cargo.toml
VAULT ?=

.PHONY: help install dev build check lint test ci e2e demo mcp icons release clean

help: ## List targets
	@grep -E '^[a-z-]+:.*## ' $(MAKEFILE_LIST) | awk -F':.*## ' '{printf "  %-10s %s\n", $$1, $$2}'

install: ## Install frontend dependencies
	pnpm install

dev: ## Run the desktop app with hot reload
	pnpm tauri dev

build: ## Build the .deb and .AppImage bundles
	pnpm tauri build

check: ## Type-check the frontend
	pnpm check

lint: ## Clippy over the whole Rust workspace, warnings as errors
	cargo clippy $(MANIFEST) --workspace --all-targets -- -D warnings

test: ## Rust tests (core, MCP server, app)
	cargo test $(MANIFEST) --workspace

ci: check lint test ## Run what CI runs
	pnpm build

e2e: ## Run the end-to-end suites (CLI, extension, desktop app)
	pnpm test:e2e

demo: ## Record docs/demo.gif (needs Xvfb, tauri-driver, WebKitWebDriver, xdotool, ffmpeg)
	HERBARIUM_REBUILD=1 node e2e/tools/demo.mjs docs/demo.gif

mcp: ## Run the MCP server on stdio (VAULT=path to override the app's vault)
	cargo run $(MANIFEST) --quiet -- mcp $(if $(VAULT),--vault $(VAULT))

icons: ## Render the app, tray and extension icons from docs/brand/*.svg
	pnpm icons

release: ## Cut a release: make release BUMP=patch|minor|major|x.y.z
	pnpm release $(BUMP)

clean: ## Remove build output
	cargo clean $(MANIFEST)
	rm -rf dist
