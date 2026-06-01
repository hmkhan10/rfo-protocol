-- 1. Enable UUID Extension
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- 2. SITES TABLE (Stores the authoritative state of verified RFO nodes)
CREATE TABLE sites (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    site_id VARCHAR(64) UNIQUE NOT NULL,
    domain_url VARCHAR(255) NOT NULL,
    quality_score INT NOT NULL CHECK (quality_score BETWEEN 0 AND 100),
    coordinates JSONB NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- 3. HANDSHAKE LOGS TABLE (Tracks transaction history and keeps network telemetry)
CREATE TABLE handshake_logs (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    site_id VARCHAR(64) REFERENCES sites(site_id) ON DELETE CASCADE,
    request_timestamp TIMESTAMP WITH TIME ZONE NOT NULL,
    nonce VARCHAR(64) NOT NULL,
    processing_time_ms INT NOT NULL,
    client_ip VARCHAR(45) NOT NULL,
    status_code INT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- Optimized performance indexes
CREATE INDEX idx_sites_quality ON sites(quality_score DESC);
CREATE INDEX idx_sites_coordinates ON sites USING gin (coordinates);
CREATE INDEX idx_handshake_telemetry ON handshake_logs(site_id, request_timestamp DESC);
