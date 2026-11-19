//! Verified downloads: only URLs written in `gate.lock`, over HTTPS, checked against their
//! sha256 before they enter the cache.
//!
//! A download goes to a temporary file in the cache while it is hashed, and is renamed into place
//! only when the checksum matches; a mismatch removes that temporary file and nothing else. Every
//! redirect hop is checked like the first URL.
//!
//! Every request is bounded ([`Bounds`]): resolving and connecting, and each read of the headers
//! and the body. A server that stops sending fails the download with an error that names the URL
//! and says to try again; nothing is retried.
//!
//! GitHub: `https://github.com` and its API, and optionally one GitHub Enterprise named by both
//! `$GATE_GITHUB_URL` and `$GATE_GITHUB_API_URL`. The core and the shared plugins of
//! `ElsiKora/Gate` always come from github.com; every other repository comes from the Enterprise
//! when one is named. A GitHub on this machine (a test fixture) stands in for github.com too.
//!
//! Credentials: a download is first made without one. When GitHub refuses a release asset URL
//! (`<github>/<owner>/<repo>/releases/download/<tag>/<asset>`) the asset may be private: gate then
//! takes a credential for that GitHub, finds the same asset of the same release through its API,
//! and downloads it from the API. `$GATE_GITHUB_TOKEN` and `$GITHUB_TOKEN` belong to the
//! Enterprise when one is named, else to github.com; `gh auth token --hostname <host>` is asked
//! for either. A credential is sent only to its own API, under `/repos/<owner>/<repo>/releases/`,
//! never to a redirect target, and never appears in output, logs or errors.

use std::fmt;
use std::io::{Read, Write as _};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::cache::{Cache, Sha256Sum, Temporary};
use crate::error::{Error, Result};
use crate::lock::{CORE_REPOSITORY, Pin};

/// The variables a credential is taken from, in order, before `gh auth token`.
pub const TOKEN_VARIABLES: [&str; 2] = ["GATE_GITHUB_TOKEN", "GITHUB_TOKEN"];

/// The variable that names the API of a GitHub Enterprise, with [`WEB_URL_VARIABLE`].
pub const API_URL_VARIABLE: &str = "GATE_GITHUB_API_URL";

/// The variable that names the web address of a GitHub Enterprise, with [`API_URL_VARIABLE`].
pub const WEB_URL_VARIABLE: &str = "GATE_GITHUB_URL";

/// github.com.
pub const GITHUB_WEB_URL: &str = "https://github.com";

/// github.com's API.
pub const GITHUB_API_URL: &str = "https://api.github.com";

const API_VERSION: &str = "2022-11-28";

const MAX_REDIRECTS: usize = 10;

/// The most a download may be: far above any plugin or core archive.
const MAX_DOWNLOAD: u64 = 1024 * 1024 * 1024;

/// The most one download may take, every redirect hop and its body included: the outer bound on a
/// download that trickles. Each request of the download gets what is left of it ([`within`]).
const GLOBAL_TIMEOUT: Duration = Duration::from_mins(15);

/// How long a request waits for the network before it fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    /// Resolving the host, then connecting, TLS included: each within this.
    pub connect: Duration,
    /// Each read, of the response headers and of the body: a server that sends nothing for this
    /// long fails the request.
    pub read: Duration,
}

impl Bounds {
    /// 30 s to connect, 30 s for each read.
    pub const DEFAULT: Self = Self {
        connect: Duration::from_secs(30),
        read: Duration::from_secs(30),
    };
}

/// Checks that gate fetches `url`: `https`, or plain `http` to this machine only (`127.0.0.1`,
/// `[::1]`, `localhost`), with no user information, quote, backslash, space or control
/// character.
///
/// # Errors
/// Returns what is wrong with the URL.
pub fn check_url(url: &str) -> std::result::Result<(), String> {
    if url.chars().any(|character| {
        character.is_control() || character.is_whitespace() || "\"\\".contains(character)
    }) {
        return Err(format!(
            "{url:?} is not a URL gate fetches: it has a space, a quote, a backslash or a control character"
        ));
    }
    let not_https = || {
        format!(
            "{url:?} is not an https URL; gate downloads over https only (plain http only to this machine)"
        )
    };
    let (scheme, rest) = url.split_once("://").ok_or_else(not_https)?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty() {
        return Err(format!("{url:?} has no host"));
    }
    if authority.contains('@') {
        return Err(format!(
            "{url:?} names a user before its host (`@`); gate fetches no URL with user information"
        ));
    }
    match scheme {
        "https" => Ok(()),
        "http" if is_loopback(authority) => Ok(()),
        _ => Err(not_https()),
    }
}

/// Whether `authority` (`host` or `host:port`) is this machine.
fn is_loopback(authority: &str) -> bool {
    let (host, port) = if let Some(rest) = authority.strip_prefix("[::1]") {
        ("[::1]", rest)
    } else {
        match authority.split_once(':') {
            Some((host, _)) => (host, &authority[host.len()..]),
            None => (authority, ""),
        }
    };
    let port_ok = port.is_empty()
        || port
            .strip_prefix(':')
            .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()));
    port_ok && ["127.0.0.1", "localhost", "[::1]"].contains(&host)
}

/// A credential. Its value is never formatted.
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    /// A credential with this value.
    #[must_use]
    pub fn new(value: String) -> Self {
        Self(value)
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Secret(<redacted>)")
    }
}

/// One GitHub: its web address and its API, without a trailing `/`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Endpoint {
    /// The web address, such as `https://github.com`.
    pub web_url: String,
    /// The API, such as `https://api.github.com`.
    pub api_url: String,
}

impl Endpoint {
    /// github.com.
    #[must_use]
    pub fn github() -> Self {
        Self {
            web_url: GITHUB_WEB_URL.to_owned(),
            api_url: GITHUB_API_URL.to_owned(),
        }
    }

    fn host(&self) -> &str {
        self.web_url
            .split_once("://")
            .map_or(self.web_url.as_str(), |(_, host)| host)
    }

    /// A GitHub on this machine: a test fixture.
    fn local(&self) -> bool {
        self.web_url.starts_with("http://") && is_loopback(self.host())
    }
}

/// Where downloads go and where credentials come from.
#[derive(Clone, Debug)]
pub struct FetchConfig {
    /// github.com.
    pub github: Endpoint,
    /// The GitHub Enterprise `$GATE_GITHUB_URL` and `$GATE_GITHUB_API_URL` name.
    pub enterprise: Option<Endpoint>,
    /// The credential variables in order, with their values when set: for the Enterprise when
    /// one is named, else for github.com.
    pub tokens: Vec<(String, Option<Secret>)>,
    /// The `gh` command to ask for a token after the variables; `None` never asks.
    pub gh: Option<PathBuf>,
}

impl FetchConfig {
    /// The configuration the environment gives.
    ///
    /// # Errors
    /// Fails when only one of `$GATE_GITHUB_URL` and `$GATE_GITHUB_API_URL` is set, or either is
    /// not a URL gate fetches.
    pub fn from_env() -> Result<Self> {
        let variable = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
        let trim = |value: String| value.trim_end_matches('/').to_owned();
        let enterprise = match (variable(WEB_URL_VARIABLE), variable(API_URL_VARIABLE)) {
            (None, None) => None,
            (Some(web_url), Some(api_url)) => {
                let endpoint = Endpoint {
                    web_url: trim(web_url),
                    api_url: trim(api_url),
                };
                for (name, url) in [
                    (WEB_URL_VARIABLE, &endpoint.web_url),
                    (API_URL_VARIABLE, &endpoint.api_url),
                ] {
                    check_url(url)
                        .map_err(|message| Error::environment(format!("{name}: {message}")))?;
                }
                Some(endpoint)
            }
            (Some(_), None) | (None, Some(_)) => {
                return Err(Error::environment(format!(
                    "{WEB_URL_VARIABLE} and {API_URL_VARIABLE} name a GitHub Enterprise together, and only one is set; set both (for example https://github.example and https://github.example/api/v3), or neither for github.com"
                )));
            }
        };
        Ok(Self {
            github: Endpoint::github(),
            enterprise,
            tokens: TOKEN_VARIABLES
                .iter()
                .map(|name| ((*name).to_owned(), variable(name).map(Secret::new)))
                .collect(),
            gh: Some(PathBuf::from("gh")),
        })
    }
}

#[derive(Debug)]
struct Credential {
    secret: Secret,
    /// Where it came from, for messages: a variable's name or `gh auth token`.
    from: String,
}

/// An asset of a GitHub release, as the API lists it.
#[derive(Debug, Deserialize)]
struct ReleaseAsset {
    name: String,
    /// The API URL that serves the asset's bytes with a credential.
    url: String,
    browser_download_url: String,
    /// `sha256:<hex>`, when GitHub has computed it.
    #[serde(default)]
    digest: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Release {
    #[serde(default)]
    assets: Vec<ReleaseAsset>,
}

/// A release asset URL on a GitHub's web address.
struct GithubAsset<'a> {
    endpoint: Which,
    owner: &'a str,
    repo: &'a str,
    tag: &'a str,
    name: &'a str,
}

/// Which GitHub.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Which {
    Github,
    Enterprise,
}

/// Downloads into the cache, verified.
#[derive(Debug)]
pub struct Fetcher {
    config: FetchConfig,
    cache: Cache,
    /// Follows no redirect: every hop is checked here.
    agent: ureq::Agent,
    bounds: Bounds,
    credentials: [OnceLock<Option<Credential>>; 2],
}

impl Fetcher {
    /// A fetcher that fills `cache`, with the [`Bounds::DEFAULT`] bounds.
    #[must_use]
    pub fn new(config: FetchConfig, cache: Cache) -> Self {
        Self::bounded(config, cache, Bounds::DEFAULT)
    }

    /// A fetcher that fills `cache`, each request within `bounds`.
    #[must_use]
    pub fn bounded(config: FetchConfig, cache: Cache, bounds: Bounds) -> Self {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .user_agent(format!("gate/{}", env!("CARGO_PKG_VERSION")))
            .timeout_resolve(Some(bounds.connect))
            .timeout_connect(Some(bounds.connect))
            .timeout_per_read(Some(bounds.read))
            .timeout_global(Some(GLOBAL_TIMEOUT))
            .build()
            .new_agent();
        Self {
            config,
            cache,
            agent,
            bounds,
            credentials: [OnceLock::new(), OnceLock::new()],
        }
    }

    /// A fetcher configured by the environment, filling the cache the environment names.
    ///
    /// # Errors
    /// Fails when there is no cache directory, or the GitHub variables are wrong.
    pub fn from_env() -> Result<Self> {
        Ok(Self::new(FetchConfig::from_env()?, Cache::from_env()?))
    }

    /// The cache it fills.
    #[must_use]
    pub fn cache(&self) -> &Cache {
        &self.cache
    }

    /// The file of a pinned download: from the cache when the entry there still hashes to the
    /// pin, else downloaded, verified and cached first. `asset` names it in messages.
    ///
    /// # Errors
    /// Fails closed when the download fails, a credential is missing or refused, or the bytes do
    /// not hash to the pin; nothing enters the cache then.
    pub fn fetch(&self, asset: &str, pin: &Pin) -> Result<PathBuf> {
        if let Some(path) = self.cache.get(&pin.sha256)? {
            return Ok(path);
        }
        let (_, path) = self.download(asset, &pin.url, Some(&pin.sha256))?;
        Ok(path)
    }

    /// [`Fetcher::fetch`] for a pin that `pinned_by` (such as `gate.toml [tools.fmt]`) holds, not
    /// `gate.lock`: a mismatch says to pin the new sha256 there.
    ///
    /// # Errors
    /// Fails like [`Fetcher::fetch`].
    pub fn fetch_pinned_by(&self, asset: &str, pin: &Pin, pinned_by: &str) -> Result<PathBuf> {
        if let Some(path) = self.cache.get(&pin.sha256)? {
            return Ok(path);
        }
        let url = &pin.url;
        let (_, path) = self.download_checked(asset, url, Some(&pin.sha256), |expected, actual| {
            format!(
                "checksum mismatch for {asset} from {url}: {pinned_by} pins sha256 {expected}, the download has sha256 {actual}; nothing entered the cache. Do not use the download; if the release was replaced on purpose, pin the new sha256 in {pinned_by}"
            )
        })?;
        Ok(path)
    }

    /// Downloads `url` into the cache and returns its checksum and file. With `expected`, a
    /// download that hashes differently fails and is removed.
    ///
    /// # Errors
    /// Fails like [`Fetcher::fetch`].
    pub fn download(
        &self,
        asset: &str,
        url: &str,
        expected: Option<&Sha256Sum>,
    ) -> Result<(Sha256Sum, PathBuf)> {
        self.download_checked(asset, url, expected, |expected, actual| {
            format!(
                "checksum mismatch for {asset} from {url}: gate.lock pins sha256 {expected}, the download has sha256 {actual}; nothing entered the cache. Do not use the download; if the release was replaced on purpose, pin it again with gate lock"
            )
        })
    }

    fn download_checked(
        &self,
        asset: &str,
        url: &str,
        expected: Option<&Sha256Sum>,
        mismatch: impl FnOnce(&Sha256Sum, &Sha256Sum) -> String,
    ) -> Result<(Sha256Sum, PathBuf)> {
        check_url(url).map_err(|message| Error::environment(format!("{asset}: {message}")))?;
        let mut temporary = self.cache.temporary()?;
        let actual = self.download_into(asset, url, &mut temporary)?;
        if let Some(expected) = expected
            && expected != &actual
        {
            drop(temporary);
            return Err(Error::environment(mismatch(expected, &actual)));
        }
        let path = self.cache.insert(temporary, &actual)?;
        Ok((actual, path))
    }

    /// Pins the asset `asset` of the release `tag` of `owner/repo`: finds it through the GitHub
    /// API, downloads it into the cache and hashes it. When the release publishes a digest, the
    /// download must match it before it enters the cache.
    ///
    /// # Errors
    /// Fails when the release or the asset does not exist, a credential is missing or refused, a
    /// download fails, or the download does not match the published digest.
    pub fn pin_release_asset(
        &self,
        owner: &str,
        repo: &str,
        tag: &str,
        asset: &str,
    ) -> Result<Pin> {
        let which = self.for_repository(owner, repo);
        let release = self.release(which, owner, repo, tag, Instant::now() + GLOBAL_TIMEOUT)?;
        let Some(found) = release.assets.iter().find(|item| item.name == asset) else {
            let names: Vec<&str> = release
                .assets
                .iter()
                .map(|item| item.name.as_str())
                .collect();
            return Err(Error::environment(format!(
                "release {tag} of {owner}/{repo} has no asset {asset} (it has: {}); check the name and the version",
                if names.is_empty() {
                    "none".to_owned()
                } else {
                    names.join(", ")
                }
            )));
        };
        let expected_url = format!(
            "{}/{owner}/{repo}/releases/download/{tag}/{asset}",
            self.endpoint(which).web_url
        );
        if !found
            .browser_download_url
            .eq_ignore_ascii_case(&expected_url)
        {
            return Err(Error::environment(format!(
                "release {tag} of {owner}/{repo} lists {asset} at {}, not at {expected_url}; nothing was pinned. Check {WEB_URL_VARIABLE} and {API_URL_VARIABLE}",
                found.browser_download_url
            )));
        }
        let url = found.browser_download_url.clone();
        let published = match found
            .digest
            .as_deref()
            .and_then(|digest| digest.strip_prefix("sha256:"))
        {
            Some(digest) => Some(Sha256Sum::parse(digest).map_err(|message| {
                Error::environment(format!(
                    "{asset} of release {tag} of {owner}/{repo}: the published digest {message}"
                ))
            })?),
            None => None,
        };
        let (actual, _) =
            self.download_checked(asset, &url, published.as_ref(), |published, actual| {
                format!(
                    "{asset} of release {tag} of {owner}/{repo}: the release publishes sha256 {published}, the download from {url} has sha256 {actual}; nothing was pinned or cached, do not trust the download"
                )
            })?;
        Ok(Pin {
            url,
            sha256: actual,
        })
    }

    fn endpoint(&self, which: Which) -> &Endpoint {
        match (which, &self.config.enterprise) {
            (Which::Enterprise, Some(enterprise)) => enterprise,
            _ => &self.config.github,
        }
    }

    /// The GitHub `owner/repo` lives on: github.com for the shared repository, else the
    /// Enterprise when one is named. A local Enterprise (a fixture) stands in for github.com.
    fn for_repository(&self, owner: &str, repo: &str) -> Which {
        match &self.config.enterprise {
            Some(enterprise) if enterprise.local() || (owner, repo) != CORE_REPOSITORY => {
                Which::Enterprise
            }
            _ => Which::Github,
        }
    }

    fn release(
        &self,
        which: Which,
        owner: &str,
        repo: &str,
        tag: &str,
        deadline: Instant,
    ) -> Result<Release> {
        let api_url = &self.endpoint(which).api_url;
        let url = format!("{api_url}/repos/{owner}/{repo}/releases/tags/{tag}");
        let what = format!("release {tag} of {owner}/{repo}");
        check_url(&url).map_err(Error::environment)?;
        let credential = self.credential(which);
        let mut request = self
            .agent
            .get(&url)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", API_VERSION);
        if let Some(credential) = credential {
            request = request.header("Authorization", bearer(&credential.secret));
        }
        let response = within(request, deadline)
            .call()
            .map_err(|error| transport(&what, &url, &error, self.bounds))?;
        let status = response.status().as_u16();
        let with = credential.map_or_else(
            || "without a credential".to_owned(),
            |credential| format!("with the credential from {}", credential.from),
        );
        match status {
            200 => {
                let text = response
                    .into_body()
                    .with_config()
                    .limit(64 * 1024 * 1024)
                    .read_to_string()
                    .map_err(|error| transport(&what, &url, &error, self.bounds))?;
                serde_json::from_str(&text).map_err(|error| {
                    Error::environment(format!(
                        "{url} did not answer with a GitHub release ({error}); check {API_URL_VARIABLE}"
                    ))
                })
            }
            301 | 302 | 307 | 308 => Err(Error::environment(format!(
                "{owner}/{repo} has moved: {url} redirects to {}; update the source in gate.toml to the new owner/repository, then run gate lock",
                location(&response).unwrap_or("an address GitHub did not give")
            ))),
            403 if rate_limited(&response) => Err(Error::environment(format!(
                "GitHub's API rate limit {with} is used up ({url}, HTTP 403); wait for it to reset, or set {} to a token to get a higher limit",
                TOKEN_VARIABLES[0]
            ))),
            401 | 403 | 404 => Err(match credential {
                None => Error::environment(format!(
                    "{what} was not found without a credential ({url}, HTTP {status}); check the version, and if {owner}/{repo} is private set {} (or {}) to a token that can read it, or sign in with gh auth login",
                    TOKEN_VARIABLES[0], TOKEN_VARIABLES[1]
                )),
                Some(credential) => Error::environment(format!(
                    "{what} was not found with the credential from {} ({url}, HTTP {status}); check the version, and set {} to a token that can read {owner}/{repo}",
                    credential.from, TOKEN_VARIABLES[0]
                )),
            }),
            _ => Err(Error::environment(format!(
                "{what}: {url} answered HTTP {status}; try again later"
            ))),
        }
    }

    /// GETs `url` without a credential, following redirects and checking every hop.
    fn get_anonymous(
        &self,
        asset: &str,
        url: &str,
        deadline: Instant,
    ) -> Result<(String, ureq::http::Response<ureq::Body>)> {
        let mut current = url.to_owned();
        for _ in 0..=MAX_REDIRECTS {
            check_url(&current).map_err(|message| {
                Error::environment(format!("{asset}: {url} leads to {message}"))
            })?;
            let response = within(self.agent.get(&current), deadline)
                .call()
                .map_err(|error| transport(asset, &current, &error, self.bounds))?;
            if !matches!(response.status().as_u16(), 301 | 302 | 303 | 307 | 308) {
                return Ok((current, response));
            }
            let next = location(&response)
                .and_then(|location| resolve(&current, location))
                .ok_or_else(|| {
                    Error::environment(format!(
                        "cannot download {asset}: {current} redirects without a usable Location; check the URL in gate.lock"
                    ))
                })?;
            current = next;
        }
        Err(Error::environment(format!(
            "cannot download {asset} from {url}: more than {MAX_REDIRECTS} redirects; check the URL in gate.lock"
        )))
    }

    fn download_into(
        &self,
        asset: &str,
        url: &str,
        temporary: &mut Temporary,
    ) -> Result<Sha256Sum> {
        let deadline = Instant::now() + GLOBAL_TIMEOUT;
        // Every 200 answer streams into the temporary file under the same asset name, URL and bounds.
        let save = |response: ureq::http::Response<ureq::Body>, temporary: &mut Temporary| {
            stream(
                asset,
                url,
                response.into_body().into_reader(),
                temporary,
                self.bounds,
            )
        };
        let (_, response) = self.get_anonymous(asset, url, deadline)?;
        let status = response.status().as_u16();
        if status == 200 {
            return save(response, temporary);
        }
        let refused = matches!(status, 401 | 403 | 404);
        let Some(github) = refused.then(|| self.github_asset(url)).flatten() else {
            return Err(Error::environment(format!(
                "cannot download {asset} from {url}: HTTP {status}; check the URL in gate.lock, or run gate lock again"
            )));
        };

        let Some(credential) = self.credential(github.endpoint) else {
            return Err(Error::environment(format!(
                "{asset} at {url} was refused without a credential (HTTP {status}); if {}/{} is private, set {} (or {}) to a token that can read its releases, or sign in with gh auth login",
                github.owner, github.repo, TOKEN_VARIABLES[0], TOKEN_VARIABLES[1]
            )));
        };
        let release = self.release(
            github.endpoint,
            github.owner,
            github.repo,
            github.tag,
            deadline,
        )?;
        let Some(found) = release.assets.iter().find(|item| item.name == github.name) else {
            return Err(Error::environment(format!(
                "release {} of {}/{} has no asset {}; check the URL in gate.lock, or run gate lock again",
                github.tag, github.owner, github.repo, github.name
            )));
        };
        // The credential goes to this repository's release assets on its own API only.
        let prefix = format!(
            "{}/repos/{}/{}/releases/assets/",
            self.endpoint(github.endpoint).api_url,
            github.owner,
            github.repo
        );
        let on_api = found
            .url
            .strip_prefix(&prefix)
            .is_some_and(|id| !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit()));
        if !on_api {
            return Err(Error::environment(format!(
                "release {} of {}/{} lists {} at {}, which is not an asset of that repository on {}; no credential was sent",
                github.tag,
                github.owner,
                github.repo,
                github.name,
                found.url,
                self.endpoint(github.endpoint).api_url
            )));
        }

        let request = self
            .agent
            .get(&found.url)
            .header("Accept", "application/octet-stream")
            .header("X-GitHub-Api-Version", API_VERSION)
            .header("Authorization", bearer(&credential.secret));
        let response = within(request, deadline)
            .call()
            .map_err(|error| transport(asset, &found.url, &error, self.bounds))?;
        match response.status().as_u16() {
            200 => save(response, temporary),
            301 | 302 | 303 | 307 | 308 => {
                let target = location(&response)
                    .and_then(|location| resolve(&found.url, location))
                    .ok_or_else(|| {
                        Error::environment(format!(
                            "cannot download {asset}: GitHub redirected without a usable Location; try again later"
                        ))
                    })?;
                // The redirect target is signed; the credential stays with the API.
                let (_, response) = self.get_anonymous(asset, &target, deadline)?;
                match response.status().as_u16() {
                    200 => save(response, temporary),
                    status => Err(Error::environment(format!(
                        "cannot download {asset} from {url}: the storage GitHub redirected to answered HTTP {status}; try again later"
                    ))),
                }
            }
            status @ (401 | 403 | 404) => Err(Error::environment(format!(
                "{asset} at {} was refused with the credential from {} (HTTP {status}); set {} to a token that can read the releases of {}/{}",
                found.url, credential.from, TOKEN_VARIABLES[0], github.owner, github.repo
            ))),
            status => Err(Error::environment(format!(
                "cannot download {asset} from {url}: the GitHub API answered HTTP {status}; try again later"
            ))),
        }
    }

    fn github_asset<'a>(&self, url: &'a str) -> Option<GithubAsset<'a>> {
        let candidates = [
            (Which::Enterprise, self.config.enterprise.as_ref()),
            (Which::Github, Some(&self.config.github)),
        ];
        candidates.into_iter().find_map(|(endpoint, web)| {
            let rest = url.strip_prefix(web?.web_url.as_str())?.strip_prefix('/')?;
            let parts: Vec<&str> = rest.split('/').collect();
            match parts.as_slice() {
                [owner, repo, "releases", "download", tag, name]
                    if [owner, repo, tag, name].iter().all(|part| !part.is_empty()) =>
                {
                    Some(GithubAsset {
                        endpoint,
                        owner,
                        repo,
                        tag,
                        name,
                    })
                }
                _ => None,
            }
        })
    }

    /// The credential for one GitHub: the variables belong to the Enterprise when one is named,
    /// else to github.com; `gh auth token` is asked for the GitHub's own host.
    fn credential(&self, which: Which) -> Option<&Credential> {
        let index = match which {
            Which::Github => 0,
            Which::Enterprise => 1,
        };
        self.credentials[index]
            .get_or_init(|| {
                let variables_here = match which {
                    Which::Enterprise => true,
                    Which::Github => self.config.enterprise.is_none(),
                };
                if variables_here {
                    for (name, value) in &self.config.tokens {
                        if let Some(secret) = value {
                            return Some(Credential {
                                secret: secret.clone(),
                                from: format!("${name}"),
                            });
                        }
                    }
                }
                let gh = self.config.gh.as_ref()?;
                let host = self.endpoint(which).host().to_owned();
                let output = Command::new(gh)
                    .args(["auth", "token", "--hostname", &host])
                    .stdin(Stdio::null())
                    .stderr(Stdio::null())
                    .output()
                    .ok()?;
                let token = String::from_utf8(output.stdout).ok()?.trim().to_owned();
                (output.status.success() && !token.is_empty()).then(|| Credential {
                    secret: Secret::new(token),
                    from: "gh auth token".to_owned(),
                })
            })
            .as_ref()
    }
}

fn bearer(secret: &Secret) -> String {
    format!("Bearer {}", secret.0)
}

fn location(response: &ureq::http::Response<ureq::Body>) -> Option<&str> {
    response
        .headers()
        .get("location")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
}

fn rate_limited(response: &ureq::http::Response<ureq::Body>) -> bool {
    response
        .headers()
        .get("x-ratelimit-remaining")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.trim() == "0")
}

/// The URL a `Location` header names, relative to `base`.
fn resolve(base: &str, location: &str) -> Option<String> {
    if location.contains("://") {
        return Some(location.to_owned());
    }
    let (scheme, rest) = base.split_once("://")?;
    if let Some(network) = location.strip_prefix("//") {
        return Some(format!("{scheme}://{network}"));
    }
    let authority = rest.split(['/', '?', '#']).next()?;
    if location.starts_with('/') {
        return Some(format!("{scheme}://{authority}{location}"));
    }
    let path = rest.split(['?', '#']).next()?;
    let directory = path.rfind('/').map_or(authority, |index| &path[..index]);
    Some(format!("{scheme}://{directory}/{location}"))
}

fn transport(what: &str, url: &str, error: &ureq::Error, bounds: Bounds) -> Error {
    if let ureq::Error::Timeout(timeout) = error {
        return timed_out(what, url, *timeout, bounds);
    }
    Error::environment(format!(
        "cannot download {what} from {url}: {error}; check the network (and HTTPS_PROXY, if one is needed)"
    ))
}

/// `request` bounded by the time left before `deadline`: every request of one download, each
/// redirect hop and the private asset's lookup included, shares one [`GLOBAL_TIMEOUT`]. Past the
/// deadline the request fails at once as timed out.
fn within<B>(request: ureq::RequestBuilder<B>, deadline: Instant) -> ureq::RequestBuilder<B> {
    request
        .config()
        .timeout_global(Some(deadline.saturating_duration_since(Instant::now())))
        .build()
}

/// The one error of a request that ran out of time, whichever phase it was in. A connection
/// that is not made in time carries the launcher's hint about the network and the proxy.
fn timed_out(what: &str, url: &str, timeout: ureq::Timeout, bounds: Bounds) -> Error {
    let seconds = |limit: Duration| limit.as_secs_f64();
    let reason = match timeout {
        ureq::Timeout::Resolve | ureq::Timeout::Connect => format!(
            "no connection within {} s; check the network (and HTTPS_PROXY, if one is needed) and try again",
            seconds(bounds.connect)
        ),
        ureq::Timeout::Global | ureq::Timeout::PerCall => format!(
            "not finished within {} minutes; try again",
            GLOBAL_TIMEOUT.as_secs() / 60
        ),
        _ => format!("no data for {} s; try again", seconds(bounds.read)),
    };
    Error::environment(format!("cannot download {what} from {url}: {reason}"))
}

fn stream(
    asset: &str,
    url: &str,
    reader: impl Read,
    temporary: &mut Temporary,
    bounds: Bounds,
) -> Result<Sha256Sum> {
    let failed = |error: std::io::Error| {
        if let Some(ureq::Error::Timeout(timeout)) = error
            .get_ref()
            .and_then(|inner| inner.downcast_ref::<ureq::Error>())
        {
            return timed_out(asset, url, *timeout, bounds);
        }
        Error::environment(format!(
            "cannot download {asset} from {url}: {error}; try again"
        ))
    };
    let mut reader = reader.take(MAX_DOWNLOAD + 1);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut total: u64 = 0;
    loop {
        let read = reader.read(&mut buffer).map_err(failed)?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > MAX_DOWNLOAD {
            return Err(Error::environment(format!(
                "cannot download {asset} from {url}: it is larger than {} MiB, more than any plugin or core; check the URL in gate.lock",
                MAX_DOWNLOAD / 1024 / 1024
            )));
        }
        hasher.update(&buffer[..read]);
        temporary
            .file()
            .write_all(&buffer[..read])
            .map_err(|error| {
                Error::environment(format!(
                    "cannot write {}: {error}",
                    temporary.path().display()
                ))
            })?;
    }
    Ok(Sha256Sum::from_digest(&hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_and_local_http_are_fetched() {
        assert!(check_url("https://github.com/o/r/releases/download/t/a.wasm").is_ok());
        assert!(check_url("http://127.0.0.1:8080/a.wasm").is_ok());
        assert!(check_url("http://localhost/a.wasm").is_ok());
        assert!(check_url("http://[::1]:9/a.wasm").is_ok());
        assert!(check_url("http://example.com/a.wasm").is_err());
        assert!(check_url("http://127.0.0.1.example.com/a.wasm").is_err());
        assert!(check_url("http://localhost.example/a.wasm").is_err());
        assert!(check_url("http://127.0.0.1@evil.example/a.wasm").is_err());
        assert!(check_url("http://localhost:1@evil.example/x").is_err());
        assert!(check_url("http://localhost@evil.example/x").is_err());
        assert!(check_url("https://user@host.example/x").is_err());
        assert!(check_url("http://localhost:80x/a").is_err());
        assert!(check_url("http://[::1]evil/a").is_err());
        assert!(check_url("ftp://example.com/a.wasm").is_err());
        assert!(check_url("https://").is_err());
        assert!(check_url("https://example.com/a b").is_err());
        assert!(check_url("https://example.com/\"").is_err());
    }

    #[test]
    fn a_location_resolves_against_the_url_it_answers() {
        let base = "https://a.example/x/y/z.wasm?q=1";
        assert_eq!(
            resolve(base, "https://b.example/s").as_deref(),
            Some("https://b.example/s")
        );
        assert_eq!(
            resolve(base, "//c.example/s").as_deref(),
            Some("https://c.example/s")
        );
        assert_eq!(
            resolve(base, "/s?t=1").as_deref(),
            Some("https://a.example/s?t=1")
        );
        assert_eq!(
            resolve(base, "w.wasm").as_deref(),
            Some("https://a.example/x/y/w.wasm")
        );
    }

    #[test]
    fn a_secret_is_never_formatted() {
        let secret = Secret::new("value-that-must-not-show".to_owned());
        assert_eq!(format!("{secret:?}"), "Secret(<redacted>)");
    }
}
