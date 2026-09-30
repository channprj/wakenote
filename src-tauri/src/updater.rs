//! GitHub release discovery and verified, local-DMG updates. No caller-supplied
//! download URLs or shell source cross the IPC boundary.
use std::{
    cmp::Ordering,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering as AtomicOrdering},
    },
    time::{Duration, Instant},
};

use reqwest::blocking::{Client, Response};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

const API: &str = "https://api.github.com/repos/channprj/wakenote/releases/latest";
const RELEASES: &str = "https://github.com/channprj/wakenote/releases";
const MAX_METADATA: u64 = 2 * 1024 * 1024;
const MAX_DMG: u64 = 512 * 1024 * 1024;
const CHECK_CACHE_TTL: Duration = Duration::from_secs(60);
const INSTALL_SCRIPT: &str = include_str!("../scripts/self-update.sh");

#[derive(Default)]
pub struct RestartGate(Mutex<(usize, bool)>);
pub static RESTART_GATE: RestartGate = RestartGate(Mutex::new((0, false)));

pub struct ActivityGuard<'a>(&'a RestartGate);
pub struct RestartGuard<'a>(&'a RestartGate);
impl RestartGate {
    pub fn is_busy(&self) -> Result<bool, String> {
        let gate = self
            .0
            .lock()
            .map_err(|_| "Update safety lock unavailable")?;
        Ok(gate.0 != 0 || gate.1)
    }
    /// Hold while starting work, or throughout work without a runtime record.
    pub fn activity(&self) -> Result<ActivityGuard<'_>, String> {
        let mut gate = self
            .0
            .lock()
            .map_err(|_| "Update safety lock unavailable")?;
        if gate.1 {
            return Err("WakeNote is restarting to install an update.".into());
        }
        gate.0 += 1;
        Ok(ActivityGuard(self))
    }
    pub fn restart(&self) -> Result<RestartGuard<'_>, String> {
        let mut gate = self
            .0
            .lock()
            .map_err(|_| "Update safety lock unavailable")?;
        if gate.1 || gate.0 != 0 {
            return Err(
                "Wait for the current operation to finish, then try the update again.".into(),
            );
        }
        gate.1 = true;
        Ok(RestartGuard(self))
    }
}
impl Drop for ActivityGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut gate) = self.0.0.lock() {
            gate.0 -= 1;
        }
    }
}
impl Drop for RestartGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut gate) = self.0.0.lock() {
            gate.1 = false;
        }
    }
}

static DOWNLOADING: AtomicBool = AtomicBool::new(false);
pub struct DownloadGuard;
impl DownloadGuard {
    pub fn acquire() -> Result<Self, String> {
        DOWNLOADING
            .compare_exchange(false, true, AtomicOrdering::AcqRel, AtomicOrdering::Acquire)
            .map_err(|_| "An update is already in progress.".to_string())?;
        Ok(Self)
    }
}
impl Drop for DownloadGuard {
    fn drop(&mut self) {
        DOWNLOADING.store(false, AtomicOrdering::Release);
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub status: &'static str,
    pub current_version: String,
    pub latest_version: Option<String>,
    pub release_url: String,
    pub notes: String,
    pub can_install: bool,
    pub install_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: Option<u64>,
    digest: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct ReleaseManifest {
    version: String,
    arch: String,
    artifact: String,
    sha256: String,
}

fn release_from_manifest(bytes: &[u8]) -> Result<Release, String> {
    let manifest: ReleaseManifest = serde_json::from_slice(bytes)
        .map_err(|_| "GitHub returned invalid release information.")?;
    version(&manifest.version)?;
    if manifest.version.starts_with('v') || !matches!(manifest.arch.as_str(), "aarch64" | "x64") {
        return Err("The release installer metadata is invalid.".into());
    }
    let tag = format!("v{}", manifest.version);
    let release = Release {
        tag_name: tag.clone(),
        draft: false,
        prerelease: false,
        body: None,
        assets: vec![Asset {
            browser_download_url: format!("{RELEASES}/download/{tag}/{}", manifest.artifact),
            name: manifest.artifact,
            // Existing published manifests have no size. Streaming still
            // enforces MAX_DMG and SHA-256; HTTP length adds a check if present.
            size: None,
            digest: Some(format!("sha256:{}", manifest.sha256)),
        }],
    };
    asset_for(&release, &manifest.arch)?;
    Ok(release)
}

#[derive(Default)]
struct ReleaseCache {
    entry: Option<(Instant, Option<Release>)>,
}

impl ReleaseCache {
    fn get_or_fetch(
        &mut self,
        now: Instant,
        fetch: impl FnOnce() -> Result<Option<Release>, String>,
    ) -> Result<Option<Release>, String> {
        if let Some((checked_at, release)) = &self.entry
            && now.saturating_duration_since(*checked_at) < CHECK_CACHE_TTL
        {
            return Ok(release.clone());
        }
        let release = fetch()?;
        self.entry = Some((now, release.clone()));
        Ok(release)
    }
}

static RELEASE_CACHE: Mutex<ReleaseCache> = Mutex::new(ReleaseCache { entry: None });

fn version(raw: &str) -> Result<(u64, u64, u64), String> {
    let core = raw.strip_prefix('v').unwrap_or(raw);
    let parts: Vec<_> = core.split('.').collect();
    if parts.len() != 3
        || parts.iter().any(|p| {
            p.is_empty()
                || !p.bytes().all(|c| c.is_ascii_digit())
                || (p.len() > 1 && p.starts_with('0'))
        })
    {
        return Err("The release version is invalid.".into());
    }
    let number = |s: &str| {
        s.parse::<u64>()
            .map_err(|_| "The release version is invalid.".to_string())
    };
    Ok((number(parts[0])?, number(parts[1])?, number(parts[2])?))
}

fn architecture() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "aarch64",
        "x86_64" => "x64",
        _ => "unsupported",
    }
}

fn asset_for(release: &Release, arch: &str) -> Result<Asset, String> {
    let latest = release
        .tag_name
        .strip_prefix('v')
        .unwrap_or(&release.tag_name);
    version(latest)?;
    let name = format!("WakeNote_{latest}_{arch}.dmg");
    let candidates: Vec<_> = release.assets.iter().filter(|a| a.name == name).collect();
    if candidates.len() != 1 {
        return Err("This release has no installer for this Mac's architecture.".into());
    }
    let asset = candidates[0];
    let expected_url = format!("{RELEASES}/download/{}/{name}", release.tag_name);
    if asset.browser_download_url != expected_url
        || asset.size.is_some_and(|size| size == 0 || size > MAX_DMG)
    {
        return Err("The release installer metadata is invalid.".into());
    }
    asset_digest(asset)?;
    Ok(asset.clone())
}

fn asset_digest(asset: &Asset) -> Result<&str, String> {
    let hash = asset
        .digest
        .as_deref()
        .and_then(|d| d.strip_prefix("sha256:"))
        .ok_or("This release has no verified SHA-256 digest. Install it manually from GitHub.")?;
    if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("The release checksum is invalid.".into());
    }
    Ok(hash)
}

fn allowed_download_url(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none_or(|p| p == 443)
        && matches!(
            url.host_str(),
            Some("github.com" | "release-assets.githubusercontent.com" | "api.github.com")
        )
}

fn client() -> Result<Client, String> {
    Client::builder()
        .user_agent(concat!("WakeNote/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(180))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 || !allowed_download_url(attempt.url()) {
                attempt.error("Untrusted release redirect")
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|_| "Could not create the update client.".into())
}

fn read_limited(response: Response, maximum: u64) -> Result<Vec<u8>, String> {
    if response.content_length().is_some_and(|n| n > maximum) {
        return Err("The release response is too large.".into());
    }
    let mut bytes = Vec::new();
    response
        .take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read the release response.")?;
    if bytes.len() as u64 > maximum {
        return Err("The release response is too large.".into());
    }
    Ok(bytes)
}

fn fetch_release(client: &Client) -> Result<Option<Release>, String> {
    fetch_release_from(
        client,
        &format!("{RELEASES}/latest/download/release.json"),
        API,
    )
}

fn fetch_release_from(
    client: &Client,
    manifest_url: &str,
    api_url: &str,
) -> Result<Option<Release>, String> {
    // Published artifacts do not consume the shared, unauthenticated REST API
    // budget. The release script has shipped this manifest since 0.260929.0.
    let response = client
        .get(manifest_url)
        .timeout(Duration::from_secs(20))
        .send()
        .map_err(|_| "Could not reach GitHub. Check your connection and try again.")?;
    if response.status().is_success() {
        return release_from_manifest(&read_limited(response, MAX_METADATA)?).map(Some);
    }
    if response.status() != reqwest::StatusCode::NOT_FOUND {
        return Err("GitHub could not provide the release metadata. Try again later.".into());
    }
    // Compatibility for older/manual releases without a release.json asset.
    let response = client
        .get(api_url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .timeout(Duration::from_secs(20))
        .send()
        .map_err(|_| "Could not reach GitHub. Check your connection and try again.")?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if matches!(response.status().as_u16(), 403 | 429) {
        return Err("GitHub's update check limit was reached. Try again later.".into());
    }
    if !response.status().is_success() {
        return Err("GitHub could not provide the latest release. Try again later.".into());
    }
    let release: Release = serde_json::from_slice(&read_limited(response, MAX_METADATA)?)
        .map_err(|_| "GitHub returned invalid release information.")?;
    if release.draft || release.prerelease {
        return Err("The latest release is not a stable public release.".into());
    }
    version(&release.tag_name)?;
    Ok(Some(release))
}

fn info_for(current: &str, release: Option<&Release>, arch: &str) -> Result<UpdateInfo, String> {
    let mut info = UpdateInfo {
        status: "no_release",
        current_version: current.into(),
        latest_version: None,
        release_url: RELEASES.into(),
        notes: String::new(),
        can_install: false,
        install_reason: None,
    };
    let Some(release) = release else {
        return Ok(info);
    };
    let latest = release
        .tag_name
        .strip_prefix('v')
        .unwrap_or(&release.tag_name);
    info.status = match version(latest)?.cmp(&version(current)?) {
        Ordering::Greater => "available",
        Ordering::Equal => "up_to_date",
        Ordering::Less => "ahead",
    };
    info.latest_version = Some(latest.into());
    info.release_url = format!("{RELEASES}/tag/{}", release.tag_name);
    info.notes = release
        .body
        .as_deref()
        .unwrap_or_default()
        .chars()
        .take(12000)
        .collect();
    if info.status == "available" {
        match asset_for(release, arch) {
            Ok(_) => info.can_install = true,
            Err(reason) => info.install_reason = Some(reason),
        }
    }
    Ok(info)
}

pub fn check(current: &str) -> Result<UpdateInfo, String> {
    let release = RELEASE_CACHE
        .lock()
        .map_err(|_| "Could not check for updates.")?
        .get_or_fetch(Instant::now(), || fetch_release(&client()?))?;
    let mut info = info_for(current, release.as_ref(), architecture())?;
    if info.can_install
        && let Err(reason) = installed_bundle()
    {
        info.can_install = false;
        info.install_reason = Some(reason);
    }
    Ok(info)
}

pub fn open_release(release_version: Option<&str>) -> Result<(), String> {
    let url = if let Some(value) = release_version {
        version(value)?;
        format!(
            "{RELEASES}/tag/v{}",
            value.strip_prefix('v').unwrap_or(value)
        )
    } else {
        RELEASES.into()
    };
    Command::new("/usr/bin/open")
        .arg(url)
        .status()
        .map_err(|_| "Could not open the GitHub release page.".to_string())?
        .success()
        .then_some(())
        .ok_or("Could not open the GitHub release page.".into())
}

fn installed_bundle() -> Result<PathBuf, String> {
    if !cfg!(target_os = "macos") {
        return Err("Automatic installation is available on macOS.".into());
    }
    let exe = std::env::current_exe()
        .and_then(fs::canonicalize)
        .map_err(|_| "Could not locate the running app.")?;
    let bundle = exe
        .ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .ok_or("Run the installed WakeNote.app to install updates automatically.")?
        .to_path_buf();
    if bundle.starts_with("/Volumes")
        || bundle
            .components()
            .any(|c| c.as_os_str() == "AppTranslocation")
    {
        return Err("Move WakeNote to Applications and reopen it before updating.".into());
    }
    if plist_value(&bundle, "CFBundleIdentifier")? != "com.chann.wakenote"
        || plist_value(&bundle, "CFBundleExecutable")? != "wakenote"
    {
        return Err("The running app is not a WakeNote installation.".into());
    }
    Ok(bundle)
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    pub phase: &'static str,
    pub downloaded: u64,
    pub total: u64,
}

fn copy_verified(
    mut source: impl Read,
    destination: &mut impl Write,
    asset: &Asset,
    progress: &impl Fn(UpdateProgress),
) -> Result<(), String> {
    let mut digest = Sha256::new();
    let mut downloaded = 0u64;
    let mut last_report = 0u64;
    let mut buffer = [0; 65536];
    loop {
        let size = source
            .read(&mut buffer)
            .map_err(|_| "The update download was interrupted. Try again.")?;
        if size == 0 {
            break;
        }
        downloaded += size as u64;
        if asset.size.is_some_and(|size| downloaded > size) || downloaded > MAX_DMG {
            return Err("The installer size does not match the release.".into());
        }
        digest.update(&buffer[..size]);
        destination
            .write_all(&buffer[..size])
            .map_err(|_| "Could not save the update. Check available disk space.")?;
        if downloaded - last_report >= 1024 * 1024 || Some(downloaded) == asset.size {
            progress(UpdateProgress {
                phase: "downloading",
                downloaded,
                total: asset.size.unwrap_or(0),
            });
            last_report = downloaded;
        }
    }
    if asset.size.is_some_and(|size| downloaded != size)
        || !format!("{:x}", digest.finalize()).eq_ignore_ascii_case(asset_digest(asset)?)
    {
        return Err("The installer checksum does not match. The update was discarded.".into());
    }
    Ok(())
}

fn command_ok(program: &str, args: &[&std::ffi::OsStr]) -> Result<(), String> {
    let result = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|_| "Could not run the macOS update verifier.")?;
    if !result.status.success() {
        return Err("The downloaded app could not be verified. Your current app was kept.".into());
    }
    Ok(())
}

fn plist_value(bundle: &Path, key: &str) -> Result<String, String> {
    let output = Command::new("/usr/bin/plutil")
        .args(["-extract", key, "raw", "-o", "-"])
        .arg(bundle.join("Contents/Info.plist"))
        .output()
        .map_err(|_| "Could not read the app metadata.")?;
    if !output.status.success() {
        return Err("The installer app metadata is invalid.".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().into())
}

fn verify_bundle(bundle: &Path, expected: &str) -> Result<(), String> {
    if fs::symlink_metadata(bundle)
        .map_err(|_| "The installer contains no WakeNote.app.")?
        .file_type()
        .is_symlink()
        || plist_value(bundle, "CFBundleIdentifier")? != "com.chann.wakenote"
        || plist_value(bundle, "CFBundleShortVersionString")? != expected
        || plist_value(bundle, "CFBundleExecutable")? != "wakenote"
    {
        return Err("The installer app identity or version does not match the release.".into());
    }
    command_ok(
        "/usr/bin/codesign",
        &[
            "--verify".as_ref(),
            "--deep".as_ref(),
            "--strict".as_ref(),
            bundle.as_os_str(),
        ],
    )?;
    let arch = if architecture() == "aarch64" {
        "arm64"
    } else {
        "x86_64"
    };
    command_ok(
        "/usr/bin/lipo",
        &[
            bundle.join("Contents/MacOS/wakenote").as_os_str(),
            "-verify_arch".as_ref(),
            arch.as_ref(),
        ],
    )
}

struct MountedDmg(PathBuf);
impl Drop for MountedDmg {
    fn drop(&mut self) {
        let _ = Command::new("/usr/bin/hdiutil")
            .args(["detach", "-quiet", "-force"])
            .arg(&self.0)
            .output();
    }
}

pub struct PreparedUpdate {
    staging: TempDir,
    destination: PathBuf,
    version: String,
}

/// Download and fully validate/copy before asking the running app to exit.
pub fn prepare(
    current: &str,
    expected: &str,
    progress: impl Fn(UpdateProgress),
) -> Result<PreparedUpdate, String> {
    let destination = installed_bundle()?;
    prepare_for_destination(current, expected, destination, progress)
}

fn prepare_for_destination(
    current: &str,
    expected: &str,
    destination: PathBuf,
    progress: impl Fn(UpdateProgress),
) -> Result<PreparedUpdate, String> {
    let client = client()?;
    let release = fetch_release(&client)?.ok_or("No public release is available.")?;
    let latest = release
        .tag_name
        .strip_prefix('v')
        .unwrap_or(&release.tag_name);
    if latest != expected || version(latest)? <= version(current)? {
        return Err("The release changed or is not newer. Check for updates again.".into());
    }
    let mut asset = asset_for(&release, architecture())?;
    let parent = destination
        .parent()
        .ok_or("Could not locate the installation folder.")?;
    let staging = tempfile::Builder::new()
        .prefix(".wakenote-update-")
        .tempdir_in(parent)
        .map_err(|_| "The app folder is not writable. Install the update manually from GitHub.")?;
    let download = tempfile::Builder::new()
        .prefix("wakenote-download-")
        .tempdir()
        .map_err(|_| "Could not create an update folder.")?;
    let dmg = download.path().join("update.dmg");
    progress(UpdateProgress {
        phase: "downloading",
        downloaded: 0,
        total: asset.size.unwrap_or(0),
    });
    let response = client
        .get(&asset.browser_download_url)
        .send()
        .map_err(|_| "Could not download the update.")?;
    if !response.status().is_success() {
        return Err("The update download failed. Try again later.".into());
    }
    if let Some(length) = response.content_length() {
        if length == 0 || length > MAX_DMG || asset.size.is_some_and(|size| size != length) {
            return Err("The installer size does not match the release.".into());
        }
        asset.size = Some(length);
    }
    let mut file = File::create(&dmg).map_err(|_| "Could not save the update.")?;
    copy_verified(response, &mut file, &asset, &progress)?;
    file.sync_all().map_err(|_| "Could not save the update.")?;
    drop(file);
    progress(UpdateProgress {
        phase: "verifying",
        downloaded: asset.size.unwrap_or(0),
        total: asset.size.unwrap_or(0),
    });
    let mount = download.path().join("volume");
    fs::create_dir(&mount).map_err(|_| "Could not create the installer mount point.")?;
    let mounted = MountedDmg(mount.clone());
    command_ok(
        "/usr/bin/hdiutil",
        &[
            "attach".as_ref(),
            dmg.as_os_str(),
            "-nobrowse".as_ref(),
            "-readonly".as_ref(),
            "-mountpoint".as_ref(),
            mount.as_os_str(),
        ],
    )?;
    let source = mount.join("WakeNote.app");
    verify_bundle(&source, expected)?;
    let staged = staging.path().join("WakeNote.app");
    command_ok("/usr/bin/ditto", &[source.as_os_str(), staged.as_os_str()])?;
    verify_bundle(&staged, expected)?;
    drop(mounted);
    fs::write(staging.path().join("install.sh"), INSTALL_SCRIPT)
        .map_err(|_| "Could not prepare the update installer.")?;
    Ok(PreparedUpdate {
        staging,
        destination,
        version: expected.into(),
    })
}

impl PreparedUpdate {
    pub fn launch(self) -> Result<(), String> {
        Command::new("/bin/bash")
            .args(["--noprofile", "--norc"])
            .arg(self.staging.path().join("install.sh"))
            .arg(std::process::id().to_string())
            .arg(&self.destination)
            .arg(&self.version)
            .env_remove("BASH_ENV")
            .env_remove("ENV")
            .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "Could not start the installer. Your current app was kept.")?;
        // The detached helper owns cleanup only after it has been launched.
        let _ = self.staging.keep();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn serve_release_metadata() -> (String, std::thread::JoinHandle<String>) {
        use std::net::TcpListener;
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", server.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (mut connection, _) = server.accept().unwrap();
            connection
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = [0; 4096];
            let length = connection.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..length]).into_owned();
            let (status, body) = if request.starts_with("GET /manifest ") {
                (
                    "200 OK",
                    serde_json::json!({
                        "version": "0.261001.0", "arch": "aarch64",
                        "artifact": "WakeNote_0.261001.0_aarch64.dmg",
                        "sha256": format!("{:x}", Sha256::digest(b"test")),
                        "signing": "ad-hoc", "notarized": false
                    })
                    .to_string(),
                )
            } else {
                (
                    "403 Forbidden",
                    "{\"message\":\"API rate limit exceeded\"}".into(),
                )
            };
            write!(
                connection,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            request
        });
        (url, handle)
    }

    #[test]
    fn public_manifest_checks_and_installs_without_a_rest_api_budget() {
        let (url, server) = serve_release_metadata();
        let result = fetch_release_from(
            &Client::new(),
            &format!("{url}/manifest"),
            &format!("{url}/api"),
        );
        let request = server.join().unwrap();
        let release = result
            .expect("a public manifest must work with an exhausted API budget")
            .unwrap();
        assert!(request.starts_with("GET /manifest "));
        let info = info_for("0.260930.5", Some(&release), "aarch64").unwrap();
        assert_eq!(info.status, "available");
        assert!(info.can_install);
        let asset = asset_for(&release, "aarch64").unwrap();
        assert!(copy_verified(&b"test"[..], &mut Vec::new(), &asset, &|_| {}).is_ok());
        assert!(copy_verified(&b"bad!"[..], &mut Vec::new(), &asset, &|_| {}).is_err());
        assert!(copy_verified(&b"tes"[..], &mut Vec::new(), &asset, &|_| {}).is_err());
        assert!(
            !info_for("0.260930.5", Some(&release), "x64")
                .unwrap()
                .can_install
        );
    }

    #[test]
    fn public_manifests_reject_untrusted_installer_metadata() {
        let valid = serde_json::json!({
            "version": "0.261001.0", "arch": "aarch64",
            "artifact": "WakeNote_0.261001.0_aarch64.dmg",
            "sha256": "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
        });
        assert!(release_from_manifest(&serde_json::to_vec(&valid).unwrap()).is_ok());
        for (field, value) in [
            ("version", "0.261001.0-beta"),
            ("version", "v0.261001.0"),
            ("arch", "../../other"),
            ("artifact", "../WakeNote_0.261001.0_aarch64.dmg"),
            ("artifact", "https://evil.example/update.dmg"),
            ("artifact", "WakeNote_0.261001.0_x64.dmg"),
            ("sha256", "invalid"),
        ] {
            let mut malformed = valid.clone();
            malformed[field] = value.into();
            assert!(
                release_from_manifest(&serde_json::to_vec(&malformed).unwrap()).is_err(),
                "{field}: {value}"
            );
        }
    }

    #[test]
    fn repeated_checks_reuse_recent_metadata_but_expired_checks_fail_honestly() {
        let mut cache = ReleaseCache::default();
        let now = Instant::now();
        let first = cache
            .get_or_fetch(now, || Ok(Some(release("0.261001.0", "aarch64"))))
            .unwrap();
        let repeated = cache
            .get_or_fetch(now + Duration::from_secs(30), || {
                panic!("duplicate request")
            })
            .unwrap();
        assert_eq!(first.unwrap().tag_name, repeated.unwrap().tag_name);
        let expired = now + Duration::from_secs(61);
        assert_eq!(
            cache
                .get_or_fetch(expired, || Err("Offline".into()))
                .unwrap_err(),
            "Offline"
        );
        let fresh = cache
            .get_or_fetch(expired, || Ok(Some(release("0.261002.0", "aarch64"))))
            .unwrap();
        assert_eq!(fresh.unwrap().tag_name, "v0.261002.0");
    }

    #[test]
    #[ignore = "Downloads and verifies the current public macOS DMG; run explicitly when validating releases"]
    fn stages_real_public_release_without_changing_an_installed_app() {
        let release = fetch_release(&client().unwrap())
            .unwrap()
            .expect("published release");
        let expected = release.tag_name.strip_prefix('v').unwrap();
        let parent = tempfile::tempdir().unwrap();
        let destination = parent.path().join("WakeNote.app");
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("original"), b"untouched").unwrap();
        let mut phases = std::collections::HashSet::new();
        let phase_log = Mutex::new(&mut phases);
        let prepared = prepare_for_destination("0.0.0", expected, destination.clone(), |p| {
            phase_log.lock().unwrap().insert(p.phase);
        })
        .unwrap();
        assert_eq!(
            fs::read(destination.join("original")).unwrap(),
            b"untouched"
        );
        assert!(
            prepared
                .staging
                .path()
                .join("WakeNote.app/Contents/MacOS/wakenote")
                .is_file()
        );
        assert!(phases.contains("downloading") && phases.contains("verifying"));
        println!(
            "Verified public release {expected}: download, SHA-256, DMG mount, bundle identity, version, architecture, codesign and staging."
        );
    }

    fn release(version: &str, arch: &str) -> Release {
        let name = format!("WakeNote_{version}_{arch}.dmg");
        Release {
            tag_name: format!("v{version}"),
            draft: false,
            prerelease: false,
            body: Some("Notes".into()),
            assets: vec![Asset {
                browser_download_url: format!("{RELEASES}/download/v{version}/{name}"),
                name,
                size: Some(4),
                digest: Some(format!("sha256:{:x}", Sha256::digest(b"test"))),
            }],
        }
    }

    #[test]
    fn numeric_headatever_ordering_and_honest_current_state() {
        assert_eq!(
            info_for(
                "0.260930.9",
                Some(&release("0.260930.10", "aarch64")),
                "aarch64"
            )
            .unwrap()
            .status,
            "available"
        );
        assert_eq!(
            info_for(
                "0.260930.10",
                Some(&release("0.260930.10", "aarch64")),
                "aarch64"
            )
            .unwrap()
            .status,
            "up_to_date"
        );
        assert_eq!(
            info_for(
                "0.261001.0",
                Some(&release("0.260930.10", "aarch64")),
                "aarch64"
            )
            .unwrap()
            .status,
            "ahead"
        );
        assert_eq!(
            info_for("0.260930.0", None, "aarch64").unwrap().status,
            "no_release"
        );
        for invalid in [
            "",
            "vv1.2.3",
            "1.2",
            "1.2.3.4",
            "1.2.3-beta",
            "1.2.3/evil",
            "01.2.3",
            "1. 2.3",
            "18446744073709551616.1.1",
        ] {
            assert!(version(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn installer_requires_matching_architecture_origin_and_checksum() {
        let mut r = release("0.261001.0", "aarch64");
        assert!(asset_for(&r, "aarch64").is_ok());
        assert!(asset_for(&r, "x64").is_err());
        r.assets[0].browser_download_url = "https://evil.example/update.dmg".into();
        assert!(asset_for(&r, "aarch64").is_err());
        r = release("0.261001.0", "aarch64");
        r.assets[0].digest = None;
        assert!(asset_for(&r, "aarch64").is_err());
        r = release("0.261001.0", "aarch64");
        r.assets.push(r.assets[0].clone());
        assert!(asset_for(&r, "aarch64").is_err());
    }

    #[test]
    fn corrupt_truncated_and_oversized_downloads_never_pass() {
        let asset = release("0.261001.0", "aarch64").assets.remove(0);
        assert!(copy_verified(&b"test"[..], &mut Vec::new(), &asset, &|_| {}).is_ok());
        for bytes in [&b"bad!"[..], &b"tes"[..], &b"test extra"[..]] {
            assert!(copy_verified(bytes, &mut Vec::new(), &asset, &|_| {}).is_err());
        }
    }

    #[test]
    fn redirects_cannot_escape_https_github_asset_hosts() {
        for url in [
            "http://github.com/a",
            "https://github.com.evil.example/a",
            "https://user@github.com/a",
            "https://github.com:444/a",
            "https://127.0.0.1/a",
        ] {
            assert!(!allowed_download_url(&url.parse().unwrap()));
        }
        assert!(allowed_download_url(
            &"https://release-assets.githubusercontent.com/path?signature=test"
                .parse()
                .unwrap()
        ));
    }

    #[test]
    fn restart_gate_closes_start_races_and_recovers_after_failures() {
        let gate = RestartGate::default();
        let job = gate.activity().unwrap();
        assert!(gate.restart().is_err());
        drop(job);
        let restart = gate.restart().unwrap();
        assert!(gate.activity().is_err());
        assert!(gate.restart().is_err());
        drop(restart);
        assert!(gate.activity().is_ok());
    }
}
