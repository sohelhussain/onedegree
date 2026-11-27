.PHONY: help db-up db-down db-logs dev dev-api dev-api-neon dev-web stop build check clean

# Default target
help:
	@echo "========================================================================"
	@echo " 1degree (1degree.fit) — Development Automation Commands"
	@echo "========================================================================"
	@echo "  make db-up         Start PostgreSQL 16 container via Docker Compose"
	@echo "  make db-down       Stop PostgreSQL container"
	@echo "  make db-logs       View live PostgreSQL container logs"
	@echo "  make dev           Start both API and Web servers (background)"
	@echo "  make stop          Stop both API and Web servers"
	@echo "  make dev-api       Run Rust Axum API with local Docker PostgreSQL"
	@echo "  make dev-api-neon  Run Rust Axum API with Neon PostgreSQL"
	@echo "  make dev-web       Run Next.js 16 Web App (http://localhost:3000)"
	@echo "  make check         Verify compilation of both api and web"
	@echo "  make build         Build release artifacts for both api and web"
	@echo "  make clean         Remove target and node_modules artifacts"
	@echo "========================================================================"

# Database Commands
db-up:
	docker compose up -d postgres

db-down:
	docker compose down

db-logs:
	docker compose logs -f postgres

# Development Commands
dev: db-up
	@echo "Starting 1degree API (port 8080) and Web (port 3000)..."
	@echo "✓ API  → http://localhost:8080"
	@echo "✓ Web  → http://localhost:3000"
	@echo "✓ Docs → http://localhost:8080/v1/swagger-ui/"
	@echo "Press Ctrl+C or run 'make stop' to stop both servers."
	@set -m; \
	api_pid=""; web_pid=""; \
	cleanup() { \
		trap - INT TERM; \
		echo ""; \
		echo "Stopping API and Web servers..."; \
		if [ -n "$$api_pid" ]; then kill -TERM -$$api_pid 2>/dev/null || kill -TERM $$api_pid 2>/dev/null || true; fi; \
		if [ -n "$$web_pid" ]; then kill -TERM -$$web_pid 2>/dev/null || kill -TERM $$web_pid 2>/dev/null || true; fi; \
		wait 2>/dev/null || true; \
		exit 0; \
	}; \
	trap cleanup INT TERM; \
	(cd api && exec cargo run) & \
	api_pid=$$!; \
	(cd web && exec pnpm dev) & \
	web_pid=$$!; \
	wait

stop:
	@stopped=""; \
	for port in 8080 3000; do \
		pids=$$(lsof -ti tcp:$$port -sTCP:LISTEN 2>/dev/null || true); \
		if [ -n "$$pids" ]; then \
			echo "Stopping listener on port $$port..."; \
			kill $$pids 2>/dev/null || true; \
			waited=0; \
			while [ $$waited -lt 15 ]; do \
				rem=$$(lsof -ti tcp:$$port -sTCP:LISTEN 2>/dev/null || true); \
				if [ -z "$$rem" ]; then break; fi; \
				sleep 0.2; \
				waited=$$((waited + 1)); \
			done; \
			rem=$$(lsof -ti tcp:$$port -sTCP:LISTEN 2>/dev/null || true); \
			if [ -n "$$rem" ]; then \
				echo "Listener on port $$port still active after 3s; sending SIGKILL..."; \
				kill -9 $$rem 2>/dev/null || true; \
			fi; \
			echo "✓ Stopped server on port $$port."; \
			stopped="$$stopped $$port"; \
		fi; \
	done; \
	if [ -z "$$stopped" ]; then \
		echo "No listeners running on ports 8080 or 3000."; \
	fi

dev-api:
	cd api && cargo run

dev-api-neon:
	@echo "Starting 1degree API connected to Neon PostgreSQL..."
	@set -e; \
	NEON_URL=$$(grep -E '^DATABASE_URL_NEON=' api/.env 2>/dev/null | head -n1 | cut -d'=' -f2-); \
	if [ -z "$$NEON_URL" ]; then \
		NEON_URL=$$(grep -E '^DATABASE_URL_NEON=' .env 2>/dev/null | head -n1 | cut -d'=' -f2-); \
	fi; \
	if [ -z "$$NEON_URL" ]; then \
		echo "Error: DATABASE_URL_NEON not found in api/.env or .env"; \
		exit 1; \
	fi; \
	DATABASE_URL="$$NEON_URL" cargo run --manifest-path api/Cargo.toml

dev-web:
	cd web && pnpm dev

# Verification & Build Commands
check:
	cd api && cargo check
	cd web && pnpm build

build:
	cd api && cargo build --release
	cd web && pnpm build

clean:
	cd api && cargo clean
	rm -rf web/.next
