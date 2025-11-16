use log::{error, info, trace, warn};
use quick_xml::{events::Event, Reader};
use reqwest::{Client, Url};
use std::env;
use std::fs;
use std::net::IpAddr;
use std::path::Path;
use std::time::Duration;
use tokio::time::sleep;

struct Config {
    domain: String,
    password: String,
    hosts: Vec<String>,
    interval_secs: u64,
    ip_providers: Vec<String>,
}

impl Config {
    fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        let domain = env::var("NC_DOMAIN").expect("NC_DOMAIN env var missing");
        let password = env::var("NC_PASSWORD").expect("NC_PASSWORD env var missing");
        let hosts_raw = env::var("NC_HOSTS").expect("NC_HOSTS env var missing");

        let interval_secs: u64 = env::var("NC_INTERVAL_SECONDS")
            .unwrap_or_else(|_| "300".to_string())
            .parse()
            .expect("NC_INTERVAL_SECONDS must be an integer");

        let ip_providers: Vec<String> = env::var("NC_IP_PROVIDERS")
            .unwrap_or_else(|_| {
                "https://ifconfig.me/ip,https://ipv4.icanhazip.com,https://api.ipify.org"
                    .to_string()
            })
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let hosts: Vec<String> = hosts_raw
            .split(',')
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
            .collect();

        Ok(Config {
            domain,
            password,
            hosts,
            interval_secs,
            ip_providers,
        })
    }
}

fn looks_like_ipv4(s: &str) -> bool {
    s.parse::<IpAddr>()
        .map(|ip| ip.is_ipv4())
        .unwrap_or(false)
}

async fn get_current_ip(
    client: &Client,
    providers: &[String],
) -> Result<String, Box<dyn std::error::Error>> {
    for p in providers {
        if p.is_empty() {
            continue;
        }
        info!("Trying IP provider: {}", p);

        match client.get(p).send().await {
            Ok(resp) if resp.status().is_success() => {
                let text = resp.text().await?;
                let ip = text.trim();

                if looks_like_ipv4(ip) {
                    return Ok(ip.to_string());
                } else {
                    let preview = &text[..text.len().min(80)];
                    warn!("Provider {} returned non-IPv4: {:?}", p, preview);
                }
            }
            Ok(resp) => {
                warn!("Provider {} returned status {}", p, resp.status());
            }
            Err(e) => {
                warn!("Provider {} failed: {}", p, e);
            }
        }
    }
    Err("All IP providers failed or returned invalid IPv4".into())
}

/// Parse Namecheap's XML DDNS response.
/// Returns Ok(()) if ErrCount == 0.
/// Returns Err(message) if ErrCount > 0 or XML is malformed.
fn parse_namecheap_response(xml: &str) -> Result<(), String> {
    let mut reader = Reader::from_str(xml);
    // quick-xml 0.36: configure trimming via config_mut()
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut current_tag: Option<String> = None;

    let mut err_count: Option<u32> = None;
    let mut errors: Vec<String> = Vec::new();
    let mut descriptions: Vec<String> = Vec::new();
    let mut response_strings: Vec<String> = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                current_tag = Some(String::from_utf8_lossy(e.name().as_ref()).to_string());
            }
            Ok(Event::Text(e)) => {
                if let Some(tag) = &current_tag {
                    let text = e.unescape().unwrap_or_default().trim().to_string();
                    if text.is_empty() {
                        buf.clear();
                        continue;
                    }

                    match tag.as_str() {
                        "ErrCount" => {
                            if let Ok(n) = text.parse::<u32>() {
                                err_count = Some(n);
                            }
                        }
                        // <Err1>, <Err2>, ...
                        t if t.starts_with("Err") => {
                            errors.push(text);
                        }
                        "Description" => {
                            descriptions.push(text);
                        }
                        "ResponseString" => {
                            response_strings.push(text);
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::End(_)) => {
                current_tag = None;
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(format!("Failed to parse Namecheap XML: {e}"));
            }
            _ => {}
        }

        buf.clear();
    }

    let count = err_count.unwrap_or(0);

    // Success path: ErrCount == 0 (or missing)
    if count == 0 {
        return Ok(());
    }

    // Error path: ErrCount > 0 – collect any messages we can
    let mut messages = Vec::new();
    messages.extend(errors);
    messages.extend(descriptions);
    messages.extend(response_strings);

    if messages.is_empty() {
        Err(format!(
            "Namecheap reported ErrCount={} but no error messages were found",
            count
        ))
    } else {
        Err(messages.join("; "))
    }
}

async fn update_namecheap(
    client: &Client,
    host: &str,
    domain: &str,
    password: &str,
    ip: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut url = Url::parse("https://dynamicdns.park-your-domain.com/update")?;
    url.query_pairs_mut()
        .append_pair("host", host)
        .append_pair("domain", domain)
        .append_pair("password", password)
        .append_pair("ip", ip);

    let resp = client.get(url).send().await?;
    let status = resp.status();
    let body = resp.text().await?;

    match parse_namecheap_response(&body) {
        Ok(()) => {
            let preview = &body[..body.len().min(160)];
            info!(
                "Namecheap DDNS update succeeded: host={}, status={} {}, body_preview={:?}",
                host,
                status.as_u16(),
                status.canonical_reason().unwrap_or(""),
                preview,
            );
            trace!("Namecheap full XML response: {}", body);
        }
        Err(parse_err) => {
            error!(
                "Namecheap DDNS update FAILED for host={}: {} (status={} {}) \
                 → This usually means wrong domain/host/password.",
                host,
                parse_err,
                status.as_u16(),
                status.canonical_reason().unwrap_or(""),
            );
            // Only dump full XML at trace level so normal logs stay clean
            trace!("Namecheap full XML error response: {}", body);
        }
    }

    Ok(())
}

fn init_logging() {
    use env_logger::Env;
    use std::io::Write;

    let style = std::env::var("LOG_STYLE")
        .unwrap_or_else(|_| "default".to_string())
        .to_lowercase();

    let mut builder = env_logger::Builder::from_env(Env::default().default_filter_or("info"));

    match style.as_str() {
        "compact" => {
            builder
                .format_timestamp_secs()
                .format(|buf, record| {
                    writeln!(buf, "[{}] {}", record.level(), record.args())
                });
        }

        "raw" => {
            builder.format(|buf, record| {
                writeln!(buf, "{}", record.args())
            });
        }

        "json" => {
            builder.format(|buf, record| {
                let ts = buf.timestamp();
                writeln!(
                    buf,
                    "{{\"ts\":\"{}\",\"level\":\"{}\",\"msg\":\"{}\"}}",
                    ts,
                    record.level(),
                    record.args()
                )
            });
        }

        "default" | _ => {
            // normal env_logger formatting
        }
    }

    builder.init();
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();

    let config = Config::from_env()?;

    info!(
        "Starting namecheap-ddns: domain={}, hosts={:?}, interval={}s",
        config.domain, config.hosts, config.interval_secs
    );

    let client = Client::builder()
        .user_agent("namecheap-ddns-rust/0.1")
        .timeout(Duration::from_secs(10))
        .build()?;

    let cache_path = "/data/last_ip";

    loop {
        let current_ip = match get_current_ip(&client, &config.ip_providers).await {
            Ok(ip) => ip,
            Err(e) => {
                warn!("Failed to detect IP: {}", e);
                sleep(Duration::from_secs(config.interval_secs)).await;
                continue;
            }
        };

        info!("Current IPv4: {}", current_ip);

        let last_ip = if Path::new(cache_path).exists() {
            fs::read_to_string(cache_path).unwrap_or_default()
        } else {
            String::new()
        };
        let last_ip = last_ip.trim();

        if last_ip == current_ip {
            info!("IP unchanged, skipping updates.");
        } else {
            for host in &config.hosts {
                if let Err(e) = update_namecheap(
                    &client,
                    host,
                    &config.domain,
                    &config.password,
                    &current_ip,
                )
                .await
                {
                    error!("Error updating host {}: {}", host, e);
                }
            }

            if let Err(e) = fs::write(cache_path, &current_ip) {
                warn!("Failed to write cache: {}", e);
            }
        }

        sleep(Duration::from_secs(config.interval_secs)).await;
    }
}