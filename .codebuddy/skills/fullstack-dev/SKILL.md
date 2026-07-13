---
name: fullstack-dev
description: |
  Full-stack backend architecture and frontend-backend integration guide.
  TRIGGER when: building a full-stack app, creating REST API with frontend, scaffolding backend service,
  building todo app, building CRUD app, building real-time app, building chat app,
  Express + React, Next.js API, Node.js backend, Python backend, Go backend,
  designing service layers, implementing error handling, managing config/auth,
  setting up API clients, implementing auth flows, handling file uploads,
  adding real-time features (SSE/WebSocket), hardening for production.
  DO NOT TRIGGER when: pure frontend UI work, pure CSS/styling, database schema only.
description_zh: "全栈应用架构与开发指南"
description_en: "Full-stack architecture guide (REST API, Express, React, Next.js)"
license: MIT
metadata:
  category: full-stack
  version: "1.0.0"
  sources:
    - The Twelve-Factor App (12factor.net)
    - Clean Architecture (Robert C. Martin)
    - Domain-Driven Design (Eric Evans)
    - Patterns of Enterprise Application Architecture (Martin Fowler)
    - Martin Fowler (Testing Pyramid, Contract Tests)
    - Google SRE Handbook (Release Engineering)
    - ThoughtWorks Technology Radar
display_name: "fullstack-dev"
display_name_en: "fullstack-dev"
visibility: "public"
---

# Full-Stack Development Practices

## MANDATORY WORKFLOW — Follow These Steps In Order

**When this skill is triggered, you MUST follow this workflow before writing any code.**

### Step 0: Gather Requirements

Before scaffolding anything, ask the user to clarify (or infer from context):

1. **Stack**: Language/framework for backend and frontend (e.g., Express + React, Django + Vue, Go + HTMX)
2. **Service type**: API-only, full-stack monolith, or microservice?
3. **Database**: SQL (PostgreSQL, SQLite, MySQL) or NoSQL (MongoDB, Redis)?
4. **Integration**: REST, GraphQL, tRPC, or gRPC?
5. **Real-time**: Needed? If yes — SSE, WebSocket, or polling?
6. **Auth**: Needed? If yes — JWT, session, OAuth, or third-party (Clerk, Auth.js)?

If the user has already specified these in their request, skip asking and proceed.

### Step 1: Architectural Decisions

Based on requirements, make and state these decisions before coding:

| Decision | Options | Reference |
|----------|---------|-----------|
| Project structure | Feature-first (recommended) vs layer-first | [Section 1](#1-project-structure--layering-critical) |
| API client approach | Typed fetch / React Query / tRPC / OpenAPI codegen | [Section 5](#5-api-client-patterns-medium) |
| Auth strategy | JWT + refresh / session / third-party | [Section 6](#6-authentication--middleware-high) |
| Real-time method | Polling / SSE / WebSocket | [Section 11](#11-real-time-patterns-medium) |
| Error handling | Typed error hierarchy + global handler | [Section 3](#3-error-handling--resilience-high) |

Briefly explain each choice (1 sentence per decision).

### Step 2: Scaffold with Checklist

Use the appropriate checklist below. Ensure ALL checked items are implemented — do not skip any.

### Step 3: Implement Following Patterns

Write code following the patterns in this document. Reference specific sections as you implement each part.

### Step 4: Test & Verify

After implementation, run these checks before claiming completion:

1. **Build check**: Ensure both backend and frontend compile without errors
2. **Start & smoke test**: Start the server, verify key endpoints return expected responses
3. **Integration check**: Verify frontend can connect to backend (CORS, API base URL, auth flow)
4. **Real-time check** (if applicable): Open two browser tabs, verify changes sync

If any check fails, fix the issue before proceeding.

### Step 5: Handoff Summary

Provide a brief summary to the user:

- **What was built**: List of implemented features and endpoints
- **How to run**: Exact commands to start backend and frontend
- **What's missing / next steps**: Any deferred items, known limitations, or recommended improvements
- **Key files**: List the most important files the user should know about

---

## Scope

**USE this skill when:**
- Building a full-stack application (backend + frontend)
- Scaffolding a new backend service or API
- Designing service layers and module boundaries
- Implementing database access, caching, or background jobs
- Writing error handling, logging, or configuration management
- Reviewing backend code for architectural issues
- Hardening for production
- Setting up API clients, auth flows, file uploads, or real-time features

**NOT for:**
- Pure frontend/UI concerns (use your frontend framework's docs)
- Pure database schema design without backend context

---

## Core Principles (7 Iron Rules)

```
1. ✅ Organize by FEATURE, not by technical layer
2. ✅ Controllers never contain business logic
3. ✅ Services never import HTTP request/response types
4. ✅ All config from env vars, validated at startup, fail fast
5. ✅ Every error is typed, logged, and returns consistent format
6. ✅ All input validated at the boundary — trust nothing from client
7. ✅ Structured JSON logging with request ID — not console.log
```

---

## 1. Project Structure & Layering (CRITICAL)

### Feature-First Organization

```
✅ Feature-first                    ❌ Layer-first
src/                                src/
  orders/                             controllers/
    order.controller.ts                 order.controller.ts
    order.service.ts                    user.controller.ts
    order.repository.ts               services/
    order.dto.ts                        order.service.ts
    order.test.ts                       user.service.ts
  users/                              repositories/
    user.controller.ts                  ...
    user.service.ts
  shared/
    database/
    middleware/
```

### Three-Layer Architecture

```
Controller (HTTP) → Service (Business Logic) → Repository (Data Access)
```

| Layer | Responsibility | ❌ Never |
|-------|---------------|---------|
| Controller | Parse request, validate, call service, format response | Business logic, DB queries |
| Service | Business rules, orchestration, transaction mgmt | HTTP types (req/res), direct DB |
| Repository | Database queries, external API calls | Business logic, HTTP types |

---

## 2. Configuration & Environment (CRITICAL)

### Rules

```
✅ All config via environment variables (Twelve-Factor)
✅ Validate required vars at startup — fail fast
✅ Type-cast at config layer, not at usage sites
✅ Commit .env.example with dummy values

❌ Never hardcode secrets, URLs, or credentials
❌ Never commit .env files
❌ Never scatter process.env / os.environ throughout code
```

---

## 3. Error Handling & Resilience (HIGH)

### Typed Error Hierarchy

```typescript
// Base (TypeScript)
class AppError extends Error {
  constructor(
    message: string,
    public readonly code: string,
    public readonly statusCode: number,
    public readonly isOperational: boolean = true,
  ) { super(message); }
}
class NotFoundError extends AppError {
  constructor(resource: string, id: string) {
    super(`${resource} not found: ${id}`, 'NOT_FOUND', 404);
  }
}
class ValidationError extends AppError {
  constructor(public readonly errors: FieldError[]) {
    super('Validation failed', 'VALIDATION_ERROR', 422);
  }
}
```

### Rules

```
✅ Typed, domain-specific error classes
✅ Global error handler catches everything
✅ Operational errors → structured response
✅ Programming errors → log + generic 500
✅ Retry transient failures with exponential backoff

❌ Never catch and ignore errors silently
❌ Never return stack traces to client
❌ Never throw generic Error('something')
```

---

## 4. Database Access Patterns (HIGH)

### Migrations Always

```
✅ Schema changes via migrations, never manual SQL
✅ Migrations must be reversible
✅ Review migration SQL before production
❌ Never modify production schema manually
```

### N+1 Prevention

```typescript
// ❌ N+1: 1 query + N queries
const orders = await db.order.findMany();
for (const o of orders) { o.items = await db.item.findMany({ where: { orderId: o.id } }); }

// ✅ Single JOIN query
const orders = await db.order.findMany({ include: { items: true } });
```

---

## 5. API Client Patterns (MEDIUM)

The "glue layer" between frontend and backend.

| Approach | When | Type Safety | Effort |
|----------|------|-------------|--------|
| Typed fetch wrapper | Simple apps, small teams | Manual types | Low |
| React Query + fetch | React apps, server state | Manual types | Medium |
| tRPC | Same team, TypeScript both sides | Automatic | Low |
| OpenAPI generated | Public API, multi-consumer | Automatic | Medium |

---

## 6. Authentication & Middleware (HIGH)

### Standard Middleware Order

```
Request → 1.RequestID → 2.Logging → 3.CORS → 4.RateLimit → 5.BodyParse
       → 6.Auth → 7.Authz → 8.Validation → 9.Handler → 10.ErrorHandler → Response
```

### JWT Rules

```
✅ Short expiry access token (15min) + refresh token (server-stored)
✅ Minimal claims: userId, roles (not entire user object)
✅ Rotate signing keys periodically

❌ Never store tokens in localStorage (XSS risk)
❌ Never pass tokens in URL query params
```

---

## 7. Logging & Observability (MEDIUM-HIGH)

### Rules

```
✅ Request ID in every log entry (propagated via middleware)
✅ Log at layer boundaries (request in, response out, external call)
❌ Never log passwords, tokens, PII, or secrets
❌ Never use console.log in production code
```

---

## 8. Background Jobs & Async (MEDIUM)

### Rules

```
✅ All jobs must be IDEMPOTENT (same job running twice = same result)
✅ Failed jobs → retry (max 3) → dead letter queue → alert
✅ Workers run as SEPARATE processes (not threads in API server)

❌ Never put long-running tasks in request handlers
❌ Never assume job runs exactly once
```

---

## 9. Caching Patterns (MEDIUM)

### Rules

```
✅ ALWAYS set TTL — never cache without expiry
✅ Invalidate on write (delete cache key after update)
✅ Use cache for reads, never for authoritative state

❌ Never cache without TTL (stale data is worse than slow data)
```

---

## 10. File Upload Patterns (MEDIUM)

| Method | File Size | Server Load | Complexity |
|--------|-----------|-------------|------------|
| Presigned URL | Any (recommended > 5MB) | None (direct to storage) | Medium |
| Multipart | < 10MB | High (streams through server) | Low |

---

## 11. Real-Time Patterns (MEDIUM)

| Method | Direction | Complexity | When |
|--------|-----------|------------|------|
| Polling | Client → Server | Low | Simple status checks, < 10 clients |
| SSE | Server → Client | Medium | Notifications, feeds, AI streaming |
| WebSocket | Bidirectional | High | Chat, collaboration, gaming |

**SSE (Server-Sent Events) — Recommended for AI streaming:**

```typescript
// Backend (Express):
app.get('/api/events', authenticate, (req, res) => {
  res.writeHead(200, {
    'Content-Type': 'text/event-stream',
    'Cache-Control': 'no-cache',
    Connection: 'keep-alive',
  });
  const send = (event: string, data: unknown) => {
    res.write(`event: ${event}\ndata: ${JSON.stringify(data)}\n\n`);
  };
  const unsubscribe = eventBus.subscribe(req.user.id, (event) => {
    send(event.type, event.payload);
  });
  req.on('close', () => unsubscribe());
});

// Frontend:
function useServerEvents(userId: string) {
  useEffect(() => {
    const source = new EventSource(`/api/events?userId=${userId}`);
    source.addEventListener('notification', (e) => {
      showToast(JSON.parse(e.data).message);
    });
    source.onerror = () => { source.close(); setTimeout(() => /* reconnect */, 3000); };
    return () => source.close();
  }, [userId]);
}
```

---

## 12. Cross-Boundary Error Handling (MEDIUM)

### Rules

```
✅ Map every API error code to a human-readable message
✅ Show field-level validation errors next to form inputs
✅ Auto-retry on 5xx (max 3, with backoff), never on 4xx
✅ Redirect to login on 401 (after refresh attempt fails)
✅ Show "offline" banner when fetch fails with TypeError

❌ Never show raw API error messages to users ("NullPointerException")
❌ Never silently swallow errors (show toast or log)
❌ Never retry 4xx errors (client is wrong, retrying won't help)
```

---

## 13. Production Hardening (MEDIUM)

### Security Checklist

```
✅ CORS: explicit origins (never '*' in production)
✅ Security headers (helmet / equivalent)
✅ Rate limiting on public endpoints
✅ Input validation on ALL endpoints (trust nothing)
✅ HTTPS enforced
❌ Never expose internal errors to clients
```

---

## Anti-Patterns

| # | ❌ Don't | ✅ Do Instead |
|---|---------|--------------|
| 1 | Business logic in routes/controllers | Move to service layer |
| 2 | `process.env` scattered everywhere | Centralized typed config |
| 3 | `console.log` for logging | Structured JSON logger |
| 4 | Generic `Error('oops')` | Typed error hierarchy |
| 5 | Direct DB calls in controllers | Repository pattern |
| 6 | No input validation | Validate at boundary (Zod/Pydantic) |
| 7 | Catching errors silently | Log + rethrow or return error |
| 8 | No health check endpoints | `/health` + `/ready` |
| 9 | Hardcoded config/secrets | Environment variables |
| 10 | No graceful shutdown | Handle SIGTERM properly |
| 11 | Hardcode API URL in frontend | Environment variable |
| 12 | Store JWT in localStorage | Memory + httpOnly refresh cookie |
| 13 | Show raw API errors to users | Map to human-readable messages |
| 14 | Retry 4xx errors | Only retry 5xx (server failures) |
| 15 | Skip loading states | Skeleton/spinner while fetching |
| 16 | Upload large files through API server | Presigned URL → direct to S3 |
| 17 | Poll for real-time data | SSE or WebSocket |
| 18 | Duplicate types frontend + backend | Shared types, tRPC, or OpenAPI codegen |
