# RFO Deployment Guide

**Production deployment, configuration, and security hardening**

---

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Quick Start (Docker)](#quick-start-docker)
3. [Manual Deployment](#manual-deployment)
4. [Environment Variables](#environment-variables)
5. [Security Hardening](#security-hardening)
6. [Production Checklist](#production-checklist)
7. [Monitoring](#monitoring)
8. [Troubleshooting](#troubleshooting)

---

## Prerequisites

| Requirement | Version | Notes |
|-------------|---------|-------|
| Docker | 24.0+ | For containerized deployment |
| Docker Compose | 2.20+ | For multi-service orchestration |
| PostgreSQL | 16+ | Required database |
| Rust | 1.82+ | For building from source |
| OpenSSL | 3.0+ | For HMAC operations |

---

## Quick Start (Docker)

### 1. Clone and configure

```bash
git clone https://github.com/hmkhan10/rfo-protocol.git
cd rfo-protocol

cp .env.example .env
```

### 2. Generate secrets

```bash
# Generate a strong secret key
openssl rand -hex 32

# Generate API keys for your agents
openssl rand -hex 16
openssl rand -hex 16
```

### 3. Edit `.env`

```bash
RFO_SECRET_KEY=<your-generated-secret>
DATABASE_URL=postgres://rfo:securepassword@postgres:5432/rfo_protocol
RFO_API_KEYS=agent_alpha:<key1>,agent_beta:<key2>
RFO_CORS_ORIGINS=https://your-app.com
RUST_LOG=info
```

### 4. Start the stack

```bash
docker compose up -d
```

### 5. Verify

```bash
# Health check
curl http://localhost:3000/rfo/health

# List endpoints
curl http://localhost:3000/rfo/capabilities
```

---

## Manual Deployment

### 1. Start PostgreSQL

```bash
docker run -d --name rfo-postgres \
  -e POSTGRES_USER=rfo \
  -e POSTGRES_PASSWORD=yourpassword \
  -e POSTGRES_DB=rfo_protocol \
  -p 5432:5432 \
  -v rfo-data:/var/lib/postgresql/data \
  postgres:16-alpine
```

### 2. Build

```bash
cargo build --release
```

### 3. Configure

```bash
export RFO_SECRET_KEY=$(openssl rand -hex 32)
export DATABASE_URL="postgres://rfo:yourpassword@localhost/rfo_protocol"
export RFO_API_KEYS="agent_alpha:$(openssl rand -hex 16)"
export RUST_LOG=info
```

### 4. Run

```bash
./target/release/rfo-core
```

Server starts on `http://0.0.0.0:3000`.

---

## Environment Variables

### Required

| Variable | Description | Example |
|----------|-------------|---------|
| `RFO_SECRET_KEY` | Secret for HMAC operations (64 hex chars) | `a1b2c3d4...` |
| `DATABASE_URL` | PostgreSQL connection string | `postgres://rfo:pass@localhost/rfo_protocol` |

### Optional

| Variable | Default | Description |
|----------|---------|-------------|
| `RFO_API_KEYS` | none | API keys: `name:key,name:key` |
| `RFO_CORS_ORIGINS` | `*` | Allowed CORS origins |
| `RFO_DDOSS_MAX_PER_IP` | `100` | Max requests/min per IP |
| `RFO_DDOSS_MAX_GLOBAL` | `1000` | Max requests/min global |
| `RUST_LOG` | `info` | Log level (trace/debug/info/warn/error) |
| `RFO_PORT` | `3000` | Server listen port |
| `RFO_HOST` | `0.0.0.0` | Server listen address |

### API Key Format

```bash
# Format: name:key,name:key
RFO_API_KEYS=agent_alpha:abc123def456,agent_beta:789ghi012jkl
```

Keys are SHA-256 hashed before storage. Plaintext is never stored.

---

## Security Hardening

### 1. Strong Secrets

```bash
# Generate a 64-character hex secret
RFO_SECRET_KEY=$(openssl rand -hex 32)

# Generate API keys (32-character hex each)
AGENT_KEY=$(openssl rand -hex 16)
```

### 2. CORS Configuration

```bash
# Restrict to your domain
RFO_CORS_ORIGINS=https://your-domain.com

# Multiple origins (comma-separated)
RFO_CORS_ORIGINS=https://app.com,https://console.app.com
```

### 3. Rate Limiting

```bash
# Conservative limits for production
RFO_DDOSS_MAX_PER_IP=50
RFO_DDOSS_MAX_GLOBAL=500
```

### 4. TLS Termination

Use a reverse proxy (Nginx, Caddy, or cloud load balancer):

```nginx
# Nginx example
server {
    listen 443 ssl;
    server_name rfo.example.com;

    ssl_certificate /etc/letsencrypt/live/rfo.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/rfo.example.com/privkey.pem;

    location / {
        proxy_pass http://localhost:3000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # WebSocket support
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
    }
}
```

### 5. Firewall Rules

```bash
# Allow only HTTP/HTTPS
ufw allow 80/tcp
ufw allow 443/tcp
ufw deny 3000/tcp  # Block direct access to RFO port
```

### 6. Database Security

```bash
# Use a dedicated user with limited privileges
CREATE USER rfo WITH PASSWORD 'securepassword';
GRANT ALL PRIVILEGES ON DATABASE rfo_protocol TO rfo;
GRANT ALL ON ALL TABLES IN SCHEMA public TO rfo;
GRANT ALL ON ALL SEQUENCES IN SCHEMA public TO rfo;
```

---

## Production Checklist

### Pre-Deployment

- [ ] Strong `RFO_SECRET_KEY` (64 hex chars)
- [ ] Secure `DATABASE_URL` (not default password)
- [ ] API keys generated and distributed
- [ ] CORS origins restricted
- [ ] Rate limits configured
- [ ] TLS certificate installed
- [ ] Firewall rules applied

### Deployment

- [ ] Docker Compose up with production config
- [ ] Health check returns 200 OK
- [ ] All migrations applied automatically
- [ ] API key authentication works
- [ ] WebSocket connection works
- [ ] Rate limiting active (test with rapid requests)

### Post-Deployment

- [ ] Monitoring alerts configured
- [ ] Log aggregation setup (e.g., ELK, Loki)
- [ ] Backup schedule for PostgreSQL
- [ ] Backup verification tested
- [ ] Incident response plan documented

---

## Monitoring

### Health Endpoint

```bash
curl http://localhost:3000/rfo/health
```

Response:
```json
{
  "status": "ok",
  "version": "1.0.0",
  "uptime": "3h 42m",
  "requests": 1542
}
```

### Telemetry Dashboard

```bash
curl -H "X-API-Key: your-key" http://localhost:3000/rfo/telemetry
```

Response:
```json
{
  "total_requests": 1542,
  "cache_hits": 890,
  "cache_misses": 652,
  "cache_hit_rate": 57.7,
  "total_errors": 12,
  "avg_processing_ms": 145,
  "top_domains": ["example.com", "docs.rs"],
  "quality_trends": {
    "example.com": { "avg": 78, "samples": 42 }
  }
}
```

### Docker Logs

```bash
# Follow logs
docker compose logs -f rfo-engine

# Last 100 lines
docker compose logs --tail 100 rfo-engine
```

### Database Monitoring

```sql
-- Recent handshakes
SELECT domain, quality_score, processing_time_ms, created_at
FROM handshake_logs
ORDER BY created_at DESC
LIMIT 10;

-- Error rate
SELECT
    DATE_TRUNC('hour', created_at) AS hour,
    COUNT(*) FILTER (WHERE NOT success) AS errors,
    COUNT(*) AS total,
    ROUND(100.0 * COUNT(*) FILTER (WHERE NOT success) / COUNT(*), 2) AS error_rate
FROM handshake_logs
GROUP BY hour
ORDER BY hour DESC;

-- Audit events
SELECT event_type, severity, COUNT(*)
FROM audit_logs
WHERE created_at > NOW() - INTERVAL '24 hours'
GROUP BY event_type, severity
ORDER BY COUNT(*) DESC;
```

---

## Troubleshooting

### Connection Refused

```bash
# Check if PostgreSQL is running
docker compose ps

# Check logs
docker compose logs postgres

# Verify connection
psql "postgres://rfo:password@localhost/rfo_protocol" -c "SELECT 1;"
```

### Authentication Failed

```bash
# Verify API key format in .env
echo $RFO_API_KEYS

# Test with curl
curl -H "X-API-Key: your-key" http://localhost:3000/rfo/sites
```

### Rate Limited

```bash
# Check current limits
curl -v http://localhost:3000/rfo/health 2>&1 | grep -i "rate"

# Increase limits temporarily
RFO_DDOSS_MAX_PER_IP=200 RFO_DDOSS_MAX_GLOBAL=2000 docker compose up -d
```

### Slow Responses

```bash
# Check cache hit rate
curl -H "X-API-Key: key" http://localhost:3000/rfo/telemetry | jq .cache_hit_rate

# Low hit rate? Increase TTL in source code or restart more frequently
```

### Database Full

```bash
# Check database size
docker compose exec postgres psql -U rfo -d rfo_protocol -c "
SELECT pg_size_pretty(pg_database_size('rfo_protocol'));
"

# Vacuum old data
docker compose exec postgres psql -U rfo -d rfo_protocol -c "
DELETE FROM handshake_logs WHERE created_at < NOW() - INTERVAL '30 days';
DELETE FROM audit_logs WHERE created_at < NOW() - INTERVAL '90 days';
VACUUM;
"
```

### WebSocket Not Working

```bash
# Test with wscat
npx wscat -c ws://localhost:3000/rfo/ws

# Send subscribe message
> {"type":"subscribe","payload":{"domains":["example.com"]}}
```

---

## Docker Compose Reference

### Services

| Service | Image | Port | Purpose |
|---------|-------|------|---------|
| `postgres` | `postgres:16-alpine` | 5432 | Database |
| `rfo-engine` | Built from source | 3000 | RFO Engine |

### Volumes

| Volume | Purpose |
|--------|---------|
| `rfo-pgdata` | PostgreSQL data persistence |
| `rfo-logs` | Application logs (optional) |

### Networks

| Network | Purpose |
|---------|---------|
| `rfo-network` | Internal communication between services |

---

## Scaling

### Horizontal Scaling

```yaml
# docker-compose.yml
services:
  rfo-engine:
    deploy:
      replicas: 3
```

### Connection Pooling (PgBouncer)

For high-throughput deployments:

```yaml
services:
  pgbouncer:
    image: edoburu/pgbouncer
    environment:
      DATABASE_URL: postgres://rfo:password@postgres:5432/rfo_protocol
      MAX_CLIENT_CONN: 1000
      DEFAULT_POOL_SIZE: 20
    ports:
      - "6432:5432"
```

### Load Balancer

```nginx
upstream rfo_backend {
    server rfo-engine-1:3000;
    server rfo-engine-2:3000;
    server rfo-engine-3:3000;
}

server {
    listen 443 ssl;
    location / {
        proxy_pass http://rfo_backend;
    }
}
```
