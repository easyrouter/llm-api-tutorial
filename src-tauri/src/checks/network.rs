//! Network reachability pre-checks (all through `net::probe`, i.e. the shared client and
//! `mirrors.probe_timeout_ms`; *reachable* = any HTTP response, even 401/404).
//!
//! - `network_npm`     : every npm registry in the preset. `official` reachable → Pass
//!                       `network.ok`; only a mirror → Warn `network.mirror_only`; none →
//!                       Fail `network.unreachable`.
//! - `network_gateway` : origin of `gateway.base_url` (scheme + host [+ port], path `/`) —
//!                       Pass `network.ok` / Fail `network.unreachable`.
//! - `network_github`  : `cc_switch.releases_api` — Pass `network.ok` / **Warn**
//!                       `network.unreachable` (the download page or an intranet mirror can
//!                       still be used).
//!
//! `params.target` is the probed host (or `GitHub`); details list one line per probe.

use std::time::Duration;

use futures_util::future::join_all;

use crate::models::{CcSwitchSpec, GatewayPreset, MirrorEntry, Mirrors, ProbeResult};
use crate::net;

use super::Verdict;

/// Probe timeout used when the preset value is `0`.
const FALLBACK_PROBE_TIMEOUT: Duration = Duration::from_secs(4);

/// npm registry probed when the preset lists none.
const DEFAULT_NPM_REGISTRY: &str = "https://registry.npmjs.org/";

/// Target label of the GitHub probe.
pub const GITHUB_TARGET: &str = "GitHub";

/// Probes all npm registries concurrently.
pub async fn check_npm(client: &reqwest::Client, mirrors: &Mirrors) -> Verdict {
    let entries = registries(mirrors);
    let timeout = probe_timeout(mirrors);
    let probes = join_all(
        entries
            .iter()
            .map(|m| net::probe(client, &m.id, &m.url, timeout)),
    )
    .await;
    npm_verdict(&entries, &probes)
}

/// Pure decision for the npm registries check (see module docs).
pub fn npm_verdict(entries: &[MirrorEntry], probes: &[ProbeResult]) -> Verdict {
    let official = entries.iter().find(|e| e.id == "official");
    let reachable = |entry: &MirrorEntry| probes.iter().any(|p| p.url == entry.url && p.reachable);
    let official_host = official.map(|e| host_of(&e.url));
    let verdict = match official {
        Some(o) if reachable(o) => Verdict::pass("network.ok").param("target", host_of(&o.url)),
        _ => match entries.iter().find(|e| reachable(e)) {
            Some(mirror) => {
                Verdict::warn("network.mirror_only").param("target", host_of(&mirror.url))
            }
            None => Verdict::fail("network.unreachable").param(
                "target",
                official_host.unwrap_or_else(|| host_of(DEFAULT_NPM_REGISTRY)),
            ),
        },
    };
    probes.iter().fold(verdict, |v, p| v.detail(describe(p)))
}

/// Probes the gateway origin.
pub async fn check_gateway(
    client: &reqwest::Client,
    gateway: &GatewayPreset,
    mirrors: &Mirrors,
) -> Verdict {
    let Some(origin) = origin_of(&gateway.base_url) else {
        return Verdict::fail("network.unreachable")
            .param("target", crate::redact::redact_secrets(&gateway.base_url))
            .detail("invalid gateway URL");
    };
    let probe = net::probe(client, "gateway", &origin, probe_timeout(mirrors)).await;
    let target = host_of(&origin);
    let verdict = if probe.reachable {
        Verdict::pass("network.ok")
    } else {
        Verdict::fail("network.unreachable")
    };
    verdict.param("target", target).detail(describe(&probe))
}

/// Probes the GitHub releases API (Warn when unreachable — installs can fall back).
pub async fn check_github(
    client: &reqwest::Client,
    cc_switch: &CcSwitchSpec,
    mirrors: &Mirrors,
) -> Verdict {
    let probe = net::probe(
        client,
        "github",
        &cc_switch.releases_api,
        probe_timeout(mirrors),
    )
    .await;
    let verdict = if probe.reachable {
        Verdict::pass("network.ok")
    } else {
        Verdict::warn("network.unreachable")
    };
    let verdict = verdict
        .param("target", GITHUB_TARGET)
        .detail(describe(&probe));
    if !probe.reachable && !cc_switch.intranet_mirror.trim().is_empty() {
        verdict.detail(format!(
            "intranet mirror: {}",
            cc_switch.intranet_mirror.trim()
        ))
    } else {
        verdict
    }
}

/// `scheme://host[:port]/` of `url` (`None` when it does not parse or has no host).
pub fn origin_of(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url.trim()).ok()?;
    let host = parsed.host_str()?;
    let scheme = parsed.scheme();
    Some(match parsed.port() {
        Some(port) => format!("{scheme}://{host}:{port}/"),
        None => format!("{scheme}://{host}/"),
    })
}

/// Host part of `url` (falls back to the trimmed input when it does not parse).
pub fn host_of(url: &str) -> String {
    reqwest::Url::parse(url.trim())
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_else(|| url.trim().to_owned())
}

/// One detail line per probe: `<id>: <url> → <status> (<latency> ms)` or `→ unreachable (<error>)`.
pub fn describe(probe: &ProbeResult) -> String {
    match (probe.reachable, probe.http_status, probe.latency_ms) {
        (true, Some(status), Some(ms)) => {
            format!("{}: {} → HTTP {status} ({ms} ms)", probe.id, probe.url)
        }
        (true, _, _) => format!("{}: {} → reachable", probe.id, probe.url),
        (false, _, _) => format!(
            "{}: {} → unreachable ({})",
            probe.id,
            probe.url,
            probe.error.as_deref().unwrap_or("error")
        ),
    }
}

fn registries(mirrors: &Mirrors) -> Vec<MirrorEntry> {
    if mirrors.npm_registries.is_empty() {
        vec![MirrorEntry {
            id: "official".to_owned(),
            url: DEFAULT_NPM_REGISTRY.to_owned(),
            download_page: String::new(),
        }]
    } else {
        mirrors.npm_registries.clone()
    }
}

fn probe_timeout(mirrors: &Mirrors) -> Duration {
    if mirrors.probe_timeout_ms == 0 {
        FALLBACK_PROBE_TIMEOUT
    } else {
        Duration::from_millis(mirrors.probe_timeout_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CheckStatus;

    fn entry(id: &str, url: &str) -> MirrorEntry {
        MirrorEntry {
            id: id.into(),
            url: url.into(),
            download_page: String::new(),
        }
    }

    fn probe(url: &str, reachable: bool) -> ProbeResult {
        ProbeResult {
            id: "x".into(),
            url: url.into(),
            reachable,
            latency_ms: reachable.then_some(42),
            http_status: reachable.then_some(200),
            error: (!reachable).then(|| "connect: refused".to_owned()),
        }
    }

    #[test]
    fn npm_verdict_table() {
        let entries = [
            entry("official", "https://registry.npmjs.org/"),
            entry("npmmirror", "https://registry.npmmirror.com/"),
        ];
        let cases: &[(&str, bool, bool, CheckStatus, &str, &str)] = &[
            (
                "both",
                true,
                true,
                CheckStatus::Pass,
                "network.ok",
                "registry.npmjs.org",
            ),
            (
                "official only",
                true,
                false,
                CheckStatus::Pass,
                "network.ok",
                "registry.npmjs.org",
            ),
            (
                "mirror only",
                false,
                true,
                CheckStatus::Warn,
                "network.mirror_only",
                "registry.npmmirror.com",
            ),
            (
                "none",
                false,
                false,
                CheckStatus::Fail,
                "network.unreachable",
                "registry.npmjs.org",
            ),
        ];
        for (name, official, mirror, status, code, target) in cases {
            let probes = [
                probe("https://registry.npmjs.org/", *official),
                probe("https://registry.npmmirror.com/", *mirror),
            ];
            let v = npm_verdict(&entries, &probes);
            assert_eq!(v.status, *status, "case {name}");
            assert_eq!(v.code, *code, "case {name}");
            assert_eq!(
                v.params.get("target").map(String::as_str),
                Some(*target),
                "case {name}"
            );
            assert_eq!(v.details.len(), 2, "case {name}");
        }
    }

    #[test]
    fn npm_verdict_without_official_entry() {
        let entries = [entry("corp", "https://npm.corp.example/")];
        let ok = npm_verdict(&entries, &[probe("https://npm.corp.example/", true)]);
        assert_eq!(ok.code, "network.mirror_only");
        let none = npm_verdict(&entries, &[probe("https://npm.corp.example/", false)]);
        assert_eq!(none.code, "network.unreachable");
        assert_eq!(
            none.params.get("target").map(String::as_str),
            Some("registry.npmjs.org")
        );
    }

    #[test]
    fn origin_and_host_extraction() {
        assert_eq!(
            origin_of("https://gateway.example.com/v1").as_deref(),
            Some("https://gateway.example.com/")
        );
        assert_eq!(
            origin_of(" http://10.0.0.1:8080/api/v1/ ").as_deref(),
            Some("http://10.0.0.1:8080/")
        );
        assert_eq!(
            origin_of("https://gw.example.com:443/v1").as_deref(),
            Some("https://gw.example.com/"),
            "default port is dropped"
        );
        assert_eq!(origin_of("not a url"), None);
        assert_eq!(origin_of("mailto:x@y"), None);
        assert_eq!(host_of("https://api.github.com/repos/x"), "api.github.com");
        assert_eq!(host_of("garbage"), "garbage");
    }

    #[test]
    fn describe_lines() {
        let ok = ProbeResult {
            id: "official".into(),
            url: "https://r/".into(),
            reachable: true,
            latency_ms: Some(12),
            http_status: Some(404),
            error: None,
        };
        assert_eq!(describe(&ok), "official: https://r/ → HTTP 404 (12 ms)");
        assert_eq!(
            describe(&probe("https://r/", false)),
            "x: https://r/ → unreachable (connect: refused)"
        );
    }

    #[test]
    fn defaults_for_empty_preset() {
        let empty = Mirrors {
            npm_registries: vec![],
            node_dist: vec![],
            probe_timeout_ms: 0,
        };
        assert_eq!(probe_timeout(&empty), FALLBACK_PROBE_TIMEOUT);
        let list = registries(&empty);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "official");
        let set = Mirrors {
            probe_timeout_ms: 1500,
            ..empty
        };
        assert_eq!(probe_timeout(&set), Duration::from_millis(1500));
    }

    #[tokio::test]
    async fn gateway_check_flags_invalid_url_and_closed_port() {
        let client = net::build_client().expect("client");
        let mirrors = Mirrors {
            npm_registries: vec![],
            node_dist: vec![],
            probe_timeout_ms: 2000,
        };
        let bad = GatewayPreset {
            base_url: "not a url".into(),
            protocol: crate::models::Protocol::Responses,
            preset_provider_name: String::new(),
            default_model: String::new(),
            default_reasoning_effort: String::new(),
        };
        let v = check_gateway(&client, &bad, &mirrors).await;
        assert_eq!(v.code, "network.unreachable");
        assert_eq!(v.status, CheckStatus::Fail);

        let closed = GatewayPreset {
            base_url: "http://127.0.0.1:9/v1".into(),
            ..bad
        };
        let v = check_gateway(&client, &closed, &mirrors).await;
        assert_eq!(v.code, "network.unreachable");
        assert_eq!(
            v.params.get("target").map(String::as_str),
            Some("127.0.0.1")
        );
        assert!(v.details[0].contains("unreachable"), "{v:?}");
    }

    #[tokio::test]
    async fn github_check_is_a_warning_when_unreachable() {
        let client = net::build_client().expect("client");
        let mirrors = Mirrors {
            npm_registries: vec![],
            node_dist: vec![],
            probe_timeout_ms: 2000,
        };
        let spec = CcSwitchSpec {
            github_repo: String::new(),
            releases_api: "http://127.0.0.1:9/releases".into(),
            download_page: String::new(),
            intranet_mirror: "https://intranet.example/cc".into(),
            data_dir: "~/.cc-switch".into(),
        };
        let v = check_github(&client, &spec, &mirrors).await;
        assert_eq!(v.code, "network.unreachable");
        assert_eq!(v.status, CheckStatus::Warn);
        assert_eq!(v.params.get("target").map(String::as_str), Some("GitHub"));
        assert!(v.details.iter().any(|d| d.contains("intranet mirror")));
    }
}
