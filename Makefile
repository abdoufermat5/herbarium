MANIFEST := --manifest-path src-tauri/Cargo.toml
VAULT ?=

.PHONY: help install dev build check lint test ci mcp release clean

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

mcp: ## Run the MCP server on stdio (VAULT=path to override the app's vault)
	cargo run $(MANIFEST) --quiet -- mcp $(if $(VAULT),--vault $(VAULT))

release: ## Cut a release: make release BUMP=patch|minor|major|x.y.z
	pnpm release $(BUMP)

clean: ## Remove build output
	cargo clean $(MANIFEST)
	rm -rf dist
