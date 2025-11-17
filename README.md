# namecheap-ddns

A tiny, fast, multi-platform Namecheap Dynamic DNS updater written in Rust.

- 🚀 Lightweight (distroless image)
- 🧩 Runs on amd64, arm64, and armv7 (Raspberry Pi)
- 🌍 Multiple IPv4 detection providers with failover
- 🔁 Only updates DNS when your IP actually changes
- 📝 Configurable logging (compact, raw, JSON)
- 🐳 Minimal Docker footprint
- 🔒 Written in safe Rust

---

## Features

- Supports multiple hosts: `@,www,api`
- Retrieves your public IPv4 from multiple fallback providers
- Parses Namecheap XML responses and reports errors properly
- Local caching (`/data/last_ip`) to avoid unnecessary DNS updates
- Fully static, runs on any platform
- Clean Docker logs using `LOG_STYLE`

---

# Usage

## Required environment variables

| Variable | Required | Example | Description |
|---------|----------|---------|-------------|
| `NC_DOMAIN` | Yes | `example.com` | Your Namecheap domain |
| `NC_PASSWORD` | Yes | `abcd1234` | Your DDNS password from Namecheap |
| `NC_HOSTS` | Yes | `@,www,api` | Comma-separated list of hosts |
| `NC_INTERVAL_SECONDS` | No | `300` | Update interval (default 300s) |
| `NC_IP_PROVIDERS` | No | Custom list | Override IPv4 detection sources |
| `LOG_STYLE` | No | `compact` | Log formatting style |
| `RUST_LOG` | No | `debug` | Log level |

---

# Docker Example

```bash
docker run \
  --name namecheap-ddns \
  --restart=always \
  -e NC_DOMAIN="example.com" \
  -e NC_PASSWORD="your-ddns-password" \
  -e NC_HOSTS="@,*" \
  -e LOG_STYLE="compact" \
  -v ddns-data:/data \
  elob/namecheap-ddns:latest
```

---

# Logging Configuration

Logging can be customized using two environment variables:

## LOG_STYLE (format)

Supported values:

| Style | Description | Example output |
|-------|-------------|----------------|
| `default` | Full env_logger format | `[2025-01-01T12:00:00Z INFO namecheap_ddns] Message` |
| `compact` | Short & Docker-friendly | `[INFO] Message` |
| `raw` | Message only | `Message` |
| `json` | Structured for log collectors | `{"level":"INFO","msg":"Message"}` |

### Example

```bash
-e LOG_STYLE=compact
```

## RUST_LOG (level)

Standard Rust filtering:

```bash
RUST_LOG=info
RUST_LOG=warn
RUST_LOG=debug
RUST_LOG=trace
```

### Example

```bash
-e RUST_LOG=debug
```

---

# Volume (IP cache)

The container stores the last known IP here:

```
/data/last_ip
```

To persist between restarts:

```bash
docker run --rm \
  -v namecheap_data:/data \
  ...
```

---

# Docker Compose Example

```yaml
services:
  ddns:
    image: elob/namecheap-ddns:latest
    restart: always
    environment:
      NC_DOMAIN: "example.com"
      NC_PASSWORD: "your-ddns-password"
      NC_HOSTS: "@,www"
      LOG_STYLE: "compact"
      RUST_LOG: "info"
    volumes:
      - ddns-data:/data

volumes:
  ddns-data:
```

---

# ✅ Release Workflow (using `cargo release`)

This project uses [`cargo-release`](https://github.com/crate-ci/cargo-release) to automate versioning, tagging, and preparing releases.

## 1. Install cargo-release

```bash
cargo install cargo-release
```

## 2. Perform a release

Examples:

```bash
# cargo release patch
# cargo release minor
# cargo release major

# Specific version
cargo release 1.2.0
```

This will:

- Update version in `Cargo.toml`
- Commit the change
- Create a git tag like `v1.2.0`
- Push commit + tag
- Start next dev cycle (`1.2.1-alpha.0`)

## 3. Build & push multi-arch Docker images

```bash
VERSION=$(cargo pkgid | sed 's/.*#//')

docker buildx build \
  --platform linux/amd64,linux/arm64,linux/arm/v7 \
  -t elob/namecheap-ddns:latest \
  -t elob/namecheap-ddns:${VERSION} \
  --push .
```

## 4. Verify the release

```bash
docker pull elob/namecheap-ddns:${VERSION}
docker pull elob/namecheap-ddns:latest
```

---

# Example Outputs

### Success

```
[INFO] IPv4 detected: 81.234.220.45
[INFO] Updated host=@ successfully
```

### Namecheap error (bad domain/password)

```
[ERROR] Namecheap update failed for host=@: Domain name not found
```

### Debug XML (trace)

```
[TRACE] <interface-response>...</interface-response>
```

---

# License

MIT
