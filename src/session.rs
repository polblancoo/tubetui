//! The YouTube login: cookies exported from a browser by yt-dlp, and the
//! request signature YouTube's own web client uses.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use crate::i18n::Lang;

const ORIGIN: &str = "https://www.youtube.com";
/// Reading cookies from the browser is slow, so the export is reused for a day.
const COOKIES_MAX_AGE: Duration = Duration::from_secs(24 * 3600);

/// Chromium-family browsers: yt-dlp name and config dir relative to `$HOME`.
/// The first dir of each is where yt-dlp looks by itself; profiles in the
/// others (snap, flatpak) are passed to it by path.
#[cfg(not(any(windows, target_os = "macos")))]
const CHROMIUM: &[(&str, &[&str])] = &[
    (
        "brave",
        &[".config/BraveSoftware/Brave-Browser", ".var/app/com.brave.Browser/config/BraveSoftware/Brave-Browser"],
    ),
    ("chrome", &[".config/google-chrome", ".var/app/com.google.Chrome/config/google-chrome"]),
    (
        "chromium",
        &[".config/chromium", "snap/chromium/common/chromium", ".var/app/org.chromium.Chromium/config/chromium"],
    ),
    ("vivaldi", &[".config/vivaldi", ".var/app/com.vivaldi.Vivaldi/config/vivaldi"]),
    ("edge", &[".config/microsoft-edge", ".var/app/com.microsoft.Edge/config/microsoft-edge"]),
    ("opera", &[".config/opera", ".var/app/com.opera.Opera/config/opera"]),
];

#[cfg(windows)]
const CHROMIUM: &[(&str, &[&str])] = &[
    ("brave", &["AppData/Local/BraveSoftware/Brave-Browser/User Data"]),
    ("chrome", &["AppData/Local/Google/Chrome/User Data"]),
    ("chromium", &["AppData/Local/Chromium/User Data"]),
    ("vivaldi", &["AppData/Local/Vivaldi/User Data"]),
    ("edge", &["AppData/Local/Microsoft/Edge/User Data"]),
    ("opera", &["AppData/Roaming/Opera Software/Opera Stable"]),
];

#[cfg(target_os = "macos")]
const CHROMIUM: &[(&str, &[&str])] = &[
    ("brave", &["Library/Application Support/BraveSoftware/Brave-Browser"]),
    ("chrome", &["Library/Application Support/Google/Chrome"]),
    ("chromium", &["Library/Application Support/Chromium"]),
    ("vivaldi", &["Library/Application Support/Vivaldi"]),
    ("edge", &["Library/Application Support/Microsoft Edge"]),
    ("opera", &["Library/Application Support/com.operasoftware.Opera"]),
];

/// Firefox profile folders: the classic one (yt-dlp's own), the newer XDG
/// one, snap (Ubuntu's default) and flatpak.
#[cfg(not(any(windows, target_os = "macos")))]
const FIREFOX: &[&str] = &[
    ".mozilla/firefox",
    ".config/mozilla/firefox",
    "snap/firefox/common/.mozilla/firefox",
    ".var/app/org.mozilla.firefox/.mozilla/firefox",
];

#[cfg(windows)]
const FIREFOX: &[&str] = &["AppData/Roaming/Mozilla/Firefox/Profiles"];

#[cfg(target_os = "macos")]
const FIREFOX: &[&str] = &["Library/Application Support/Firefox/Profiles"];

/// Browser profiles with cookies under `home`, as yt-dlp
/// `--cookies-from-browser` specs (`brave+KEYRING:Default`, `firefox:abcd.default`,
/// `firefox:/home/me/snap/firefox/common/.mozilla/firefox/abcd.default`).
pub fn scan_browsers(home: &Path, keyring: &str) -> Vec<String> {
    scan_browsers_used(home, keyring).into_iter().map(|(spec, _)| spec).collect()
}

/// [`scan_browsers`], each with when the browser last wrote its cookies (unix
/// time): the one in use shows today.
pub fn scan_browsers_used(home: &Path, keyring: &str) -> Vec<(String, Option<i64>)> {
    let keyring = if keyring.is_empty() { String::new() } else { format!("+{keyring}") };
    let mut found = Vec::new();
    for (name, dirs) in CHROMIUM {
        for (i, dir) in dirs.iter().enumerate() {
            for profile in subdirs(&home.join(dir)) {
                let is_profile = profile == "Default" || profile.starts_with("Profile ");
                let base = home.join(dir).join(&profile);
                if is_profile && (base.join("Cookies").exists() || base.join("Network/Cookies").exists()) {
                    let used = last_written(&[&base.join("Cookies"), &base.join("Network/Cookies")]);
                    let profile = if i == 0 { profile } else { base.display().to_string() };
                    found.push((format!("{name}{keyring}:{profile}"), used));
                }
            }
        }
    }
    for (i, dir) in FIREFOX.iter().enumerate() {
        for profile in subdirs(&home.join(dir)) {
            let base = home.join(dir).join(&profile);
            if base.join("cookies.sqlite").exists() {
                let used = last_written(&[&base.join("cookies.sqlite")]);
                let profile = if i == 0 { profile } else { base.display().to_string() };
                found.push((format!("firefox:{profile}"), used));
            }
        }
    }
    found
}

/// The keyring to use: `configured` if set, else the one detected on this machine.
pub fn resolve_keyring(configured: &str) -> String {
    if configured.is_empty() { detect_keyring() } else { configured.to_string() }
}

/// The keyring Chromium browsers keep their cookie key in, as yt-dlp's
/// `+KEYRING`. yt-dlp guesses it from the desktop name and gets it wrong on
/// plain window managers (fluxbox, i3, openbox…): it falls back to plain text
/// and can't decrypt the cookies. Empty when nothing was found.
#[cfg(not(any(windows, target_os = "macos")))]
pub fn detect_keyring() -> String {
    let running = running_programs();
    let is_running = |name: &str| running.iter().any(|p| p == name);
    // KWallet 6 also answers as a Secret Service, so it goes first.
    if is_running("kwalletd6") {
        "KWALLET6".into()
    } else if is_running("kwalletd5") {
        "KWALLET5".into()
    } else if is_running("gnome-keyring-d") || is_running("keepassxc") || secret_service_available() {
        "GNOMEKEYRING".into()
    } else {
        String::new()
    }
}

#[cfg(any(windows, target_os = "macos"))]
pub fn detect_keyring() -> String {
    String::new()
}

/// Process names (`/proc/*/comm`, cut to 15 bytes) of everything running.
#[cfg(not(any(windows, target_os = "macos")))]
fn running_programs() -> Vec<String> {
    std::fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| std::fs::read_to_string(e.path().join("comm")).ok())
        .map(|comm| comm.trim_end().to_string())
        .collect()
}

/// Whether the session bus has, or can start, a Secret Service (gnome-keyring
/// is often started on demand).
#[cfg(not(any(windows, target_os = "macos")))]
fn secret_service_available() -> bool {
    Command::new("dbus-send")
        .args([
            "--session",
            "--print-reply",
            "--dest=org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus.ListActivatableNames",
        ])
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("\"org.freedesktop.secrets\""))
}

/// `spec` with `+keyring` added when it is a Chromium browser without one:
/// `brave:Default` → `brave+GNOMEKEYRING:Default`. Firefox needs no keyring.
pub fn with_keyring(spec: &str, keyring: &str) -> String {
    let (browser, profile) = match spec.split_once(':') {
        Some((browser, profile)) => (browser, Some(profile)),
        None => (spec, None),
    };
    if keyring.is_empty() || browser.contains('+') || !CHROMIUM.iter().any(|(name, _)| *name == browser) {
        return spec.to_string();
    }
    match profile {
        Some(profile) => format!("{browser}+{keyring}:{profile}"),
        None => format!("{browser}+{keyring}"),
    }
}

/// A spec as people read it: `brave · Default`, `firefox (snap) · abcd.default`.
pub fn label(spec: &str) -> String {
    let (browser, profile) = spec.split_once(':').unwrap_or((spec, ""));
    let browser = browser.split('+').next().unwrap_or(browser);
    let place = if profile.contains("/snap/") {
        " (snap)"
    } else if profile.contains("/.var/app/") {
        " (flatpak)"
    } else {
        ""
    };
    let profile = profile.rsplit('/').next().unwrap_or(profile);
    if profile.is_empty() { format!("{browser}{place}") } else { format!("{browser}{place} · {profile}") }
}

/// The newest change to these databases or their journals (`-wal`, `-journal`).
fn last_written(databases: &[&Path]) -> Option<i64> {
    databases
        .iter()
        .flat_map(|db| ["", "-wal", "-journal"].map(|suffix| sibling_name(db, suffix)))
        .filter_map(|path| std::fs::metadata(path).and_then(|m| m.modified()).ok())
        .filter_map(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .max()
}

fn sibling_name(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn subdirs(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Cookie export for `spec`, one file per browser profile.
pub fn cookies_path(cache_dir: &Path, spec: &str) -> PathBuf {
    let safe: String = spec.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    cache_dir.join(format!("cookies-{safe}.txt"))
}

/// `jar` with `tag` before its extension, in the same folder.
fn sibling(jar: &Path, tag: &str) -> PathBuf {
    let stem = jar.file_stem().unwrap_or_default().to_string_lossy();
    jar.with_file_name(format!("{stem}.{tag}.txt"))
}

/// A private copy of the login for one yt-dlp run. yt-dlp rewrites its
/// `--cookies` file when it ends, and on the shared one whoever read it at
/// that moment got half a login. Removed when dropped.
pub struct JarCopy(PathBuf);

impl JarCopy {
    pub fn new(jar: &Path) -> std::io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let copy = sibling(jar, &format!("run-{}-{n}", std::process::id()));
        copy_private(jar, &copy)?;
        Ok(Self(copy))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for JarCopy {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// mpv's copy of the login, renewed at every start: its yt-dlp rewrites it
/// while playing. The shared jar if the copy can't be made.
pub fn player_jar(jar: &Path) -> PathBuf {
    let (copy, partial) = (sibling(jar, "mpv"), sibling(jar, &format!("mpv-{}", std::process::id())));
    match copy_private(jar, &partial).and_then(|()| std::fs::rename(&partial, &copy)) {
        Ok(()) => copy,
        Err(_) => {
            let _ = std::fs::remove_file(&partial);
            jar.to_path_buf()
        }
    }
}

/// Exports the browser's cookies to `path` unless a fresh export exists.
/// One export at a time, and `path` only ever changes whole: other threads
/// read it meanwhile.
pub fn refresh_cookies(spec: &str, path: &Path, force: bool, lang: Lang) -> Result<(), String> {
    static EXPORT: Mutex<()> = Mutex::new(());
    let _one_at_a_time = EXPORT.lock().unwrap_or_else(PoisonError::into_inner);
    if !force && is_fresh(path, COOKIES_MAX_AGE) {
        // An export from an older version: trim it and lock it down.
        let _ = harden(path);
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        private_dir(dir).map_err(|e| e.to_string())?;
    }
    // A new file: yt-dlp also loads an existing jar, and its stale login
    // would win over the browser's.
    let fresh = sibling(path, "new");
    let _ = std::fs::remove_file(&fresh);
    // Any authenticated request makes yt-dlp write the jar; `-I 0` downloads nothing.
    let output = Command::new(crate::doctor::program("yt-dlp"))
        .args(["-q", "--skip-download", "--flat-playlist", "-I", "0", "--cookies-from-browser", spec, "--cookies"])
        .arg(&fresh)
        .arg(":ytsubs")
        .output()
        .map_err(|e| format!("{}: {e}", lang.pick("no se pudo ejecutar yt-dlp", "couldn't run yt-dlp")))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    // yt-dlp exports every site's cookies: keep the Google ones, owner-only.
    let _ = harden(&fresh);
    let jar = std::fs::read_to_string(&fresh).ok();
    let error = cookie_error(spec, &stderr, lang)
        .or_else(|| jar.as_deref().is_some_and(|text| !has_login(text)).then(|| no_login(spec, lang)));
    if error.is_none() && jar.is_some() {
        return std::fs::rename(&fresh, path).map_err(|e| e.to_string());
    }
    // No login read: dropping the old one too reads the browser again next time.
    let _ = std::fs::remove_file(&fresh);
    let _ = std::fs::remove_file(path);
    if let Some(error) = error {
        Err(error)
    } else {
        let detail = stderr.lines().last().unwrap_or(lang.pick("sin detalle", "no details"));
        Err(format!("{} {spec}: {detail}", lang.pick("no se pudo leer la sesión de", "couldn't read the login of")))
    }
}

/// Creates `dir` (and its parents) and makes `dir` itself owner-only: the
/// cookie exports live there.
fn private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Writes `text` to `path` readable by its owner only, from the first byte.
fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    #[cfg(unix)]
    {
        // `mode` only applies to a new file.
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(text.as_bytes())
}

/// A copy of `from` at `to` that only its owner can read, whatever `from` allows.
fn copy_private(from: &Path, to: &Path) -> std::io::Result<()> {
    write_private(to, &std::fs::read_to_string(from)?)
}

/// Cuts the cookie file at `path` down to the Google ones and makes it
/// owner-only. Rewritten whole through a temporary file, since other threads
/// read it meanwhile.
fn harden(path: &Path) -> std::io::Result<()> {
    let text = std::fs::read_to_string(path)?;
    let kept = keep_google_cookies(&text);
    let private = {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::metadata(path)?.permissions().mode() & 0o077 == 0
        }
        #[cfg(not(unix))]
        true
    };
    if kept == text && private {
        return Ok(());
    }
    let partial = sibling(path, &format!("trim-{}", std::process::id()));
    let result = write_private(&partial, &kept).and_then(|()| std::fs::rename(&partial, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    result
}

/// Whether cookies for `domain` (Netscape style, with or without a leading
/// dot) belong to YouTube or Google.
fn is_google_domain(domain: &str) -> bool {
    const SITES: &[&str] = &[
        "youtube.com",
        "youtu.be",
        "google.com",
        "googlevideo.com",
        "googleusercontent.com",
        "googleapis.com",
        "ytimg.com",
        "gstatic.com",
    ];
    let domain = domain.trim_start_matches('.').to_ascii_lowercase();
    let under = |site: &str| domain == site || domain.strip_suffix(site).is_some_and(|rest| rest.ends_with('.'));
    if SITES.iter().any(|site| under(site)) {
        return true;
    }
    // google.de, google.co.uk, google.com.br, and their subdomains.
    let labels: Vec<&str> = domain.split('.').collect();
    let tail_ok = |tail: &[&str]| match tail {
        [tld] => tld.len() == 2,
        [second, tld] => matches!(*second, "com" | "co") && tld.len() == 2,
        _ => false,
    };
    labels.iter().position(|l| *l == "google").is_some_and(|i| tail_ok(&labels[i + 1..]))
}

/// The lines of a Netscape cookie file worth keeping: comments, blank lines
/// and the YouTube / Google cookies (`#HttpOnly_` ones included).
pub fn keep_google_cookies(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let cookie = line.strip_prefix("#HttpOnly_").unwrap_or(line);
        let keep = if cookie.starts_with('#') || cookie.trim().is_empty() {
            true
        } else {
            let fields: Vec<&str> = cookie.split('\t').collect();
            fields.len() == 7 && is_google_domain(fields[0])
        };
        if keep {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Whether a Netscape cookie file carries a YouTube login.
pub fn has_login(text: &str) -> bool {
    authorization(&parse_cookies(text), 0).is_some()
}

/// The browser has cookies but none from a YouTube login.
fn no_login(spec: &str, lang: Lang) -> String {
    let (label, browser) = (label(spec), browser_name(spec));
    match lang {
        Lang::Es => format!(
            "{label} no tiene sesión de YouTube: entrá a youtube.com en {browser}, iniciá sesión y navegá un poco; \
             después volvé y usá Configuración → Releer la sesión"
        ),
        Lang::En => format!(
            "{label} has no YouTube login: open youtube.com in {browser}, log in and browse a little; \
             then come back and use Settings → Read the login again"
        ),
    }
}

/// What went wrong reading the browser, from yt-dlp's messages. Most are only
/// warnings: yt-dlp still writes a jar, just without the login.
fn cookie_error(spec: &str, stderr: &str, lang: Lang) -> Option<String> {
    let (label, browser) = (label(spec), browser_name(spec));
    let error = if stderr.contains("secretstorage not available") {
        lang.pick(
            "a yt-dlp le falta el módulo secretstorage para abrir el llavero: \
             Configuración → Revisar el sistema te da el comando para instalarlo",
            "yt-dlp lacks the secretstorage module to open the keyring: \
             Settings → Check the system gives the command to install it",
        )
        .to_string()
    } else if stderr.contains("kwallet-query command not found") {
        lang.pick(
            "falta kwallet-query para abrir KWallet: instalalo (viene con el paquete kwallet de tu distribución)",
            "kwallet-query is missing to open KWallet: install it (it comes with your distribution's kwallet package)",
        )
        .to_string()
    } else if stderr.contains("failed to read from keyring") || stderr.contains("kwallet-query failed") {
        match lang {
            Lang::Es => format!(
                "no encontré la clave de {browser} en el llavero: desbloquealo, abrí {browser} una vez \
                 y volvé a leer la sesión"
            ),
            Lang::En => {
                format!("{browser}'s key isn't in the keyring: unlock it, open {browser} once and read the login again")
            }
        }
    } else if stderr.contains("Could not copy Chrome cookie database") {
        match lang {
            Lang::Es => format!(
                "no pude copiar las cookies de {browser}: cerralo y volvé a leer la sesión, o usá Firefox o un cookies.txt"
            ),
            Lang::En => format!(
                "couldn't copy {browser}'s cookies: close it and read the login again, or use Firefox or a cookies.txt"
            ),
        }
    } else if stderr.contains("cannot decrypt") {
        match lang {
            Lang::Es => format!(
                "yt-dlp no pudo descifrar las cookies de {label}: poné keyring = \"GNOMEKEYRING\" o \"KWALLET6\" \
                 en config.toml y volvé a elegir el navegador"
            ),
            Lang::En => format!(
                "yt-dlp couldn't decrypt {label}'s cookies: set keyring = \"GNOMEKEYRING\" or \"KWALLET6\" \
                 in config.toml and pick the browser again"
            ),
        }
    } else {
        return None;
    };
    Some(error)
}

/// `brave+GNOMEKEYRING:Default` → `brave`.
fn browser_name(spec: &str) -> &str {
    spec.split([':', '+']).next().unwrap_or(spec)
}

pub fn is_fresh(path: &Path, max_age: Duration) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .is_ok_and(|t| SystemTime::now().duration_since(t).is_ok_and(|age| age < max_age))
}

/// youtube.com cookies from a Netscape cookie file (`#HttpOnly_` lines included).
pub fn parse_cookies(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let line = line.strip_prefix("#HttpOnly_").unwrap_or(line);
            if line.starts_with('#') {
                return None;
            }
            let fields: Vec<&str> = line.split('\t').collect();
            match fields.as_slice() {
                [domain, _, _, _, _, name, value] if domain.ends_with("youtube.com") => {
                    Some((name.to_string(), value.to_string()))
                }
                _ => None,
            }
        })
        .collect()
}

/// `Authorization` header for YouTube's internal API, or `None` when the
/// cookies carry no session.
pub fn authorization(cookies: &HashMap<String, String>, timestamp: u64) -> Option<String> {
    let third_party = cookies.get("__Secure-3PAPISID");
    let sapisid = cookies.get("SAPISID").or(third_party)?;
    let mut header = format!("SAPISIDHASH {}", sapisidhash(timestamp, sapisid));
    if let Some(value) = third_party {
        header.push_str(&format!(" SAPISID3PHASH {}", sapisidhash(timestamp, value)));
    }
    Some(header)
}

fn sapisidhash(timestamp: u64, sapisid: &str) -> String {
    let digest = sha1_smol::Sha1::from(format!("{timestamp} {sapisid} {ORIGIN}")).digest();
    format!("{timestamp}_{digest}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::temp_dir;

    #[test]
    fn sapisidhash_matches_youtube_web_client() {
        // Reference computed with Python's hashlib, as in ytv.
        assert_eq!(sapisidhash(1_700_000_000, "abcSAPISID"), "1700000000_b4d59c573190b5106be035dd4b3e304f7edd9c46");
    }

    #[test]
    fn authorization_needs_a_sapisid() {
        assert_eq!(authorization(&HashMap::new(), 1), None);
        let only_3p = HashMap::from([("__Secure-3PAPISID".to_string(), "abcSAPISID".to_string())]);
        let header = authorization(&only_3p, 1_700_000_000).unwrap();
        assert_eq!(
            header,
            "SAPISIDHASH 1700000000_b4d59c573190b5106be035dd4b3e304f7edd9c46 \
             SAPISID3PHASH 1700000000_b4d59c573190b5106be035dd4b3e304f7edd9c46"
        );
    }

    #[test]
    fn parses_youtube_cookies_only() {
        let text = "# Netscape HTTP Cookie File\n\
            .youtube.com\tTRUE\t/\tTRUE\t0\tSAPISID\tabc\n\
            #HttpOnly_.youtube.com\tTRUE\t/\tTRUE\t0\tSID\tsecret\n\
            .google.com\tTRUE\t/\tTRUE\t0\tNID\tnope\n\
            broken line\n";
        let cookies = parse_cookies(text);
        assert_eq!(cookies.len(), 2);
        assert_eq!(cookies["SAPISID"], "abc");
        assert_eq!(cookies["SID"], "secret");
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    #[test]
    fn scan_finds_chromium_and_firefox_profiles() {
        let home = temp_dir("scan");
        let touch = |p: &str| {
            let path = home.join(p);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "").unwrap();
        };
        touch(".config/BraveSoftware/Brave-Browser/Default/Cookies");
        touch(".config/BraveSoftware/Brave-Browser/Profile 1/Network/Cookies");
        touch(".config/BraveSoftware/Brave-Browser/System Profile/Cookies");
        touch(".config/google-chrome/Default/Preferences");
        touch(".mozilla/firefox/ab12.default-release/cookies.sqlite");
        touch("snap/firefox/common/.mozilla/firefox/cd34.default/cookies.sqlite");
        touch(".var/app/com.google.Chrome/config/google-chrome/Default/Cookies");
        let snap = home.join("snap/firefox/common/.mozilla/firefox/cd34.default").display().to_string();
        let flatpak = home.join(".var/app/com.google.Chrome/config/google-chrome/Default").display().to_string();
        assert_eq!(
            scan_browsers(&home, "GNOMEKEYRING"),
            [
                "brave+GNOMEKEYRING:Default".to_string(),
                "brave+GNOMEKEYRING:Profile 1".into(),
                format!("chrome+GNOMEKEYRING:{flatpak}"),
                "firefox:ab12.default-release".into(),
                format!("firefox:{snap}"),
            ]
        );
        assert_eq!(scan_browsers(&home, "")[0], "brave:Default");
        let used = scan_browsers_used(&home, "");
        assert!(used.iter().all(|(_, at)| at.is_some_and(|t| t > 1_700_000_000)), "{used:?}");
        assert_eq!(label(&format!("firefox:{snap}")), "firefox (snap) · cd34.default");
        assert_eq!(label(&format!("chrome+GNOMEKEYRING:{flatpak}")), "chrome (flatpak) · Default");
        assert_eq!(label("brave+GNOMEKEYRING:Profile 1"), "brave · Profile 1");
        assert_eq!(label("firefox"), "firefox");
    }

    #[test]
    fn adds_the_keyring_to_chromium_specs_only() {
        assert_eq!(with_keyring("brave:Default", "GNOMEKEYRING"), "brave+GNOMEKEYRING:Default");
        assert_eq!(with_keyring("chrome", "KWALLET6"), "chrome+KWALLET6");
        assert_eq!(with_keyring("brave+BASICTEXT:Default", "GNOMEKEYRING"), "brave+BASICTEXT:Default");
        assert_eq!(with_keyring("firefox:ab12.default", "GNOMEKEYRING"), "firefox:ab12.default");
        assert_eq!(with_keyring("brave:Default", ""), "brave:Default");
        assert_eq!(resolve_keyring("KWALLET5"), "KWALLET5");
    }

    #[test]
    fn explains_why_the_browser_could_not_be_read() {
        let spec = "brave+GNOMEKEYRING:Default";
        let error = |stderr: &str| cookie_error(spec, stderr, Lang::Es).unwrap_or_default();
        assert!(error("WARNING: cannot decrypt v11 cookies: no key found\n").contains("brave · Default"));
        let missing = "ERROR: secretstorage not available No module named 'secretstorage'\n\
                       WARNING: cannot decrypt v11 cookies: no key found\n";
        assert!(error(missing).contains("secretstorage"), "the cause wins over its effect");
        assert!(error("ERROR: kwallet-query command not found. KWallet…").contains("kwallet-query"));
        assert!(error("ERROR: failed to read from keyring\n").contains("clave de brave"));
        assert!(error("ERROR: Could not copy Chrome cookie database. See …").contains("cerralo"));
        assert_eq!(cookie_error(spec, "WARNING: something else\n", Lang::Es), None);
        let english = cookie_error(spec, "ERROR: failed to read from keyring\n", Lang::En).unwrap();
        assert!(english.contains("brave's key isn't in the keyring"), "{english}");
    }

    #[test]
    fn a_jar_without_youtube_login_says_how_to_get_one() {
        assert!(has_login(".youtube.com\tTRUE\t/\tTRUE\t0\tSAPISID\tabc\n"));
        assert!(!has_login(".youtube.com\tTRUE\t/\tTRUE\t0\tYSC\tabc\n"));
        let message = no_login("firefox:ab12.default", Lang::Es);
        assert!(message.contains("firefox · ab12.default no tiene sesión"), "{message}");
        assert!(message.contains("entrá a youtube.com en firefox"), "{message}");
        assert!(message.contains("Releer la sesión"), "{message}");
        let english = no_login("firefox:ab12.default", Lang::En);
        assert!(english.contains("has no YouTube login") && english.contains("Read the login again"), "{english}");
    }

    #[test]
    fn cookies_path_is_per_profile_and_filename_safe() {
        let p = cookies_path(Path::new("/c"), "brave+GNOMEKEYRING:Profile 1");
        assert_eq!(p, Path::new("/c/cookies-brave_GNOMEKEYRING_Profile_1.txt"));
    }

    #[test]
    fn freshness() {
        let path = temp_dir("fresh").join("f");
        assert!(!is_fresh(&path, Duration::from_secs(60)));
        std::fs::write(&path, "").unwrap();
        assert!(is_fresh(&path, Duration::from_secs(60)));
        assert!(!is_fresh(&path, Duration::ZERO));
    }

    #[test]
    fn yt_dlp_gets_its_own_copy_of_the_login() {
        let jar = temp_dir("jar").join("cookies-brave.txt");
        std::fs::write(&jar, "login").unwrap();
        let copy = JarCopy::new(&jar).unwrap();
        let (a, b) = (copy.path().to_path_buf(), JarCopy::new(&jar).unwrap().path().to_path_buf());
        assert_ne!(a, b, "one per run");
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "login");
        drop(copy);
        assert!(!a.exists() && !b.exists(), "removed after the run");
        let mpv = player_jar(&jar);
        assert_ne!(mpv, jar);
        assert_eq!(std::fs::read_to_string(&mpv).unwrap(), "login");
    }

    const EXPORT: &str = "# Netscape HTTP Cookie File\n# This file is generated by yt-dlp.  Do not edit.\n\n\
        .youtube.com\tTRUE\t/\tTRUE\t0\tSAPISID\tabc\n\
        #HttpOnly_.youtube.com\tTRUE\t/\tTRUE\t0\tSID\tsecret\n\
        accounts.google.com\tFALSE\t/\tTRUE\t0\tLSID\tl\n\
        #HttpOnly_.google.com\tTRUE\t/\tTRUE\t0\tNID\tn\n\
        .google.co.uk\tTRUE\t/\tTRUE\t0\tNID\tuk\n\
        .rr1---sn.googlevideo.com\tTRUE\t/\tTRUE\t0\tx\ty\n\
        youtu.be\tFALSE\t/\tTRUE\t0\tz\tz\n\
        .github.com\tTRUE\t/\tTRUE\t0\tuser_session\tnope\n\
        #HttpOnly_.bank.example\tTRUE\t/\tTRUE\t0\tsession\tnope\n\
        .notyoutube.com\tTRUE\t/\tTRUE\t0\ta\tnope\n\
        .google.evil.com\tTRUE\t/\tTRUE\t0\ta\tnope\n\
        .youtube.com.evil.org\tTRUE\t/\tTRUE\t0\ta\tnope\n\
        broken line\n";

    #[test]
    fn the_export_keeps_google_cookies_only() {
        let kept = keep_google_cookies(EXPORT);
        assert!(kept.starts_with("# Netscape HTTP Cookie File\n# This file is generated"), "{kept}");
        for domain in [
            ".youtube.com",
            "#HttpOnly_.youtube.com",
            "accounts.google.com",
            "#HttpOnly_.google.com",
            ".google.co.uk",
            ".rr1---sn.googlevideo.com",
            "youtu.be",
        ] {
            assert!(kept.contains(&format!("{domain}\t")), "{domain} kept");
        }
        for domain in ["github", "bank.example", "notyoutube", "google.evil", "evil.org", "broken"] {
            assert!(!kept.contains(domain), "{domain} dropped");
        }
        assert!(has_login(&kept), "the login survives");
        assert_eq!(keep_google_cookies(&kept), kept, "trimming twice changes nothing");
    }

    #[test]
    fn google_domains() {
        for yes in
            ["google.com", ".google.com", "www.google.de", ".google.com.br", "i.ytimg.com", "lh3.googleusercontent.com"]
        {
            assert!(is_google_domain(yes), "{yes}");
        }
        for no in ["googl.com", "mygoogle.com", "google.com.evil.org", "google.example.com", "ytimg.com.evil.org", ""] {
            assert!(!is_google_domain(no), "{no}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn cookie_files_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        let dir = temp_dir("private").join("cache");
        private_dir(&dir).unwrap();
        assert_eq!(mode(&dir), 0o700);
        let jar = dir.join("cookies-brave.txt");
        std::fs::write(&jar, EXPORT).unwrap();
        std::fs::set_permissions(&jar, std::fs::Permissions::from_mode(0o644)).unwrap();
        // An export from an older version is trimmed and locked down in place.
        harden(&jar).unwrap();
        assert_eq!(mode(&jar), 0o600);
        assert_eq!(std::fs::read_to_string(&jar).unwrap(), keep_google_cookies(EXPORT));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "no temporary file left");
        // The copies for yt-dlp and mpv are owner-only even from a lax source.
        std::fs::set_permissions(&jar, std::fs::Permissions::from_mode(0o644)).unwrap();
        let copy = JarCopy::new(&jar).unwrap();
        assert_eq!(mode(copy.path()), 0o600);
        assert_eq!(mode(&player_jar(&jar)), 0o600);
    }
}
