# onedegree (onedegree.fit)

A plane that tilts just 1 degree ends up hundreds of miles from its original course. **1degree** is a modern personal transformation and wellness platform built on the philosophy that small, non-negotiable daily micro-actions compound into massive personal transformations over 365 days.

---

## Repository Structure

```
onedegree/
├── api/                  # High-performance Rust backend (Axum, sqlx, PostgreSQL 16)
├── web/                  # Modern web client (Next.js 15, React 19, Tailwind CSS, shadcn/ui)
├── docs/                 # Architecture, schemas, and implementation plan documentation
├── Makefile              # Unified development automation commands
└── docker-compose.yml     # Local services container orchestration (PostgreSQL 16)
```

---

## Getting Started

### Prerequisites

- **Docker** and **Docker Compose**
- **Rust & Cargo** (1.75+)
- **Node.js** (v20+) and **pnpm** (v9+)

### Local Development Setup

1. **Set up local environment files**:
   ```sh
   cp .env.example .env
   cp api/.env.example api/.env
   cp web/.env.example web/.env.local
   ```
   Adjust any custom configuration in `api/.env` and `web/.env.local` if needed.

2. **Start the PostgreSQL 16 container**:
   ```sh
   make db-up
   ```

3. **Run the API service (Rust Axum)**:
   ```sh
   make dev-api
   ```
   API runs on `http://localhost:8080` (API base: `http://localhost:8080/v1`).

4. **Run the Web application (Next.js 15)**:
   ```sh
   make dev-web
   ```
   Access the web application at [http://localhost:3000](http://localhost:3000).

---

## Required Environment Variables

### Backend (`api/.env`)
- `PORT`: HTTP listening port (e.g. `8080`).
- `HOST`: Host interface to bind (e.g. `0.0.0.0`).
- `DATABASE_URL`: Connection URL for PostgreSQL.
- `LOCAL_DATABASE_URL`: Local PostgreSQL connection URL for testing and migrations.
- `DATABASE_URL_NEON`: (Optional) Connection URL for Neon serverless PostgreSQL.
- `JWT_SECRET`: Secret key used for signing JWT tokens (min 32 bytes).
- `JWT_ACCESS_EXPIRATION_SECS`: Expiration window for access tokens (e.g. `900` for 15m).
- `JWT_REFRESH_EXPIRATION_SECS`: Expiration window for refresh tokens (e.g. `2592000` for 30d).
- `ADMIN_EMAILS`: Comma-separated list of administrative email addresses.
- `GOOGLE_CLIENT_ID`: Google OAuth client ID for token verification.

### Frontend (`web/.env.local`)
- `NEXT_PUBLIC_API_URL`: Backend API endpoint URL (e.g. `http://localhost:8080/v1`).
- `NEXT_PUBLIC_GOOGLE_CLIENT_ID`: Public Google OAuth client ID for web authentication.

---

## Security Policy

> **Important**: Secrets, API keys, private keys, production credentials, and `.env` files are **never** committed to version control. Only template `.env.example` files containing non-sensitive placeholder values are tracked in this repository.
