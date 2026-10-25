//! Everything that could play sound: installed desktop apps and Steam games, and the channel
//! the user picked for each one (saved in `~/.config/beacn-utility/app-routing.json`).
//!
//! Pipeweaver remembers routes per exact (process binary, stream name) pair, which only works
//! once an app has played something. This catalog lets a route be picked before that: when a
//! stream appears that matches a catalogued app, the page turns the choice into a regular
//! Pipeweaver route for that exact pair (see `MixerPage::enforce_app_routes`).

use log::{debug, warn};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Process names shared by many unrelated apps, so they say nothing about which app it is
const GENERIC_PROCESSES: &[&str] = &[
    "wine",
    "wine64",
    "wine-preloader",
    "wine64-preloader",
    "wineserver",
    "proton",
    "python",
    "python3",
    "java",
    "sh",
    "bash",
    "env",
    "node",
    "electron",
    "mono",
    "gamescope",
    "flatpak",
    "bwrap",
    "pipewire",
    "pw-cat",
    "pw-play",
    "paplay",
    "aplay",
];

/// Steam "apps" that are tools rather than games
const STEAM_TOOLS: &[&str] = &[
    "proton",
    "steam linux runtime",
    "steamworks common redistributables",
    "steam runtime",
    "steamvr",
];

/// Folders inside a game's install dir that hold installers, not the game
const SKIP_DIRS: &[&str] = &[
    "redist",
    "_commonredist",
    "commonredist",
    "directx",
    "vcredist",
    "dotnet",
    "easyanticheat",
    "battleye",
    "installers",
    "installer",
    "__installer",
    "support",
    "prereq",
    "shadercache",
];

#[derive(Debug, Clone)]
pub enum Icon {
    Svg(PathBuf),
    Raster(PathBuf),
}

#[derive(Debug, Clone)]
pub struct CatalogApp {
    /// `desktop:<desktop file id>` or `steam:<appid>`
    pub key: String,
    pub name: String,
    pub steam: bool,
    /// Shown under the name: the command, or where a Steam game lives
    pub detail: String,
    pub icon: Option<Icon>,
    /// Lowercase executable names (desktop Exec, Steam .exe and Linux binaries)
    pub binaries: Vec<String>,
    /// Lowercase names a stream might carry (app name, window class)
    pub names: Vec<String>,
    /// Lowercase ids: Flatpak app id, desktop id, `steam:<appid>`
    pub ids: Vec<String>,
    /// Lowercase text the search box matches against
    pub search: String,
}

#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub apps: Vec<CatalogApp>,
    by_id: HashMap<String, usize>,
    by_binary: HashMap<String, usize>,
    by_name: HashMap<String, usize>,
}

/// What we know about a running stream beyond Pipeweaver's (process, name)
#[derive(Debug, Clone, Default)]
pub struct StreamIds {
    /// Lowercase: Flatpak/portal app id, `steam:<SteamAppId>`
    pub ids: Vec<String>,
}

impl Catalog {
    fn index(apps: Vec<CatalogApp>) -> Self {
        let mut by_id = HashMap::new();
        let mut by_binary = HashMap::new();
        let mut by_name = HashMap::new();
        // Steam games first, so a game's exe wins over a desktop entry with the same name
        let mut order: Vec<usize> = (0..apps.len()).collect();
        order.sort_by_key(|i| !apps[*i].steam);
        for i in order {
            let app = &apps[i];
            for id in &app.ids {
                by_id.entry(id.clone()).or_insert(i);
            }
            for b in &app.binaries {
                if !GENERIC_PROCESSES.contains(&b.as_str()) {
                    by_binary.entry(b.clone()).or_insert(i);
                }
            }
            for n in &app.names {
                by_name.entry(n.clone()).or_insert(i);
            }
        }
        Self {
            apps,
            by_id,
            by_binary,
            by_name,
        }
    }

    /// The catalogued app a stream belongs to, strongest evidence first:
    /// 1. an id read from the process (Flatpak app id, SteamAppId)
    /// 2. the process binary (unless it's a generic one like wine64-preloader or python3)
    /// 3. the stream's name, for generic or Windows processes (Proton games name their
    ///    stream after the game or its .exe) and for apps whose binary we don't know
    pub fn find(&self, process: &str, name: &str, extra: Option<&StreamIds>) -> Option<usize> {
        if let Some(extra) = extra {
            for id in &extra.ids {
                if let Some(i) = self.by_id.get(id) {
                    return Some(*i);
                }
            }
        }
        let process = process.to_lowercase();
        let name = name.to_lowercase();
        let generic = GENERIC_PROCESSES.contains(&process.as_str()) || process.ends_with(".exe");
        if !generic && let Some(i) = self.by_binary.get(&process) {
            return Some(*i);
        }
        if process.ends_with(".exe")
            && let Some(i) = self.by_binary.get(&process)
        {
            return Some(*i);
        }
        if name.ends_with(".exe")
            && let Some(i) = self.by_binary.get(&name)
        {
            return Some(*i);
        }
        if let Some(i) = self.by_name.get(&name)
            && (generic || self.apps[*i].binaries.is_empty())
        {
            return Some(*i);
        }
        None
    }
}

/// Scans the system. Slow-ish (reads a few hundred small files), so run it off the UI thread.
pub fn scan() -> Catalog {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let icons = IconIndex::build(&home);
    let mut apps = vec![];
    let mut steam_icons: HashMap<String, String> = HashMap::new();
    apps.extend(scan_desktop(&home, &icons, &mut steam_icons));
    apps.extend(scan_steam(&home, &icons, &steam_icons));
    apps.sort_by_key(|a| a.name.to_lowercase());
    log::info!(
        "App catalog: {} apps ({} Steam games)",
        apps.len(),
        apps.iter().filter(|a| a.steam).count()
    );
    Catalog::index(apps)
}

////////////////////////////////////////////////////////////////////////////////////////////
// Desktop entries

fn data_dirs(home: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![];
    match std::env::var_os("XDG_DATA_HOME") {
        Some(d) if !d.is_empty() => dirs.push(PathBuf::from(d)),
        _ => dirs.push(home.join(".local/share")),
    }
    dirs.push(home.join(".local/share/flatpak/exports/share"));
    dirs.push(PathBuf::from("/var/lib/flatpak/exports/share"));
    let system = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    for d in system.split(':').filter(|d| !d.is_empty()) {
        dirs.push(PathBuf::from(d));
    }
    if !dirs.iter().any(|d| d == Path::new("/usr/share")) {
        dirs.push("/usr/share".into());
    }
    let mut seen = HashSet::new();
    dirs.retain(|d| seen.insert(fs::canonicalize(d).unwrap_or_else(|_| d.clone())));
    dirs
}

fn desktop_files(dir: &Path, prefix: &str, out: &mut Vec<(String, PathBuf)>, depth: u8) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() && depth < 3 {
            desktop_files(&path, &format!("{prefix}{file_name}-"), out, depth + 1);
        } else if let Some(stem) = file_name.strip_suffix(".desktop") {
            out.push((format!("{prefix}{stem}"), path));
        }
    }
}

fn parse_desktop(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut in_entry = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            map.entry(k.trim().to_string())
                .or_insert_with(|| v.trim().to_string());
        }
    }
    map
}

/// Splits an Exec line into arguments (handles quotes, drops %f-style field codes)
fn exec_args(exec: &str) -> Vec<String> {
    let mut args = vec![];
    let mut cur = String::new();
    let mut quote = None;
    let mut chars = exec.chars().peekable();
    let mut has = false;
    while let Some(c) = chars.next() {
        match (quote, c) {
            (None, '"' | '\'') => {
                quote = Some(c);
                has = true;
            }
            (Some(q), c) if c == q => quote = None,
            (Some('"'), '\\') => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            (None, c) if c.is_whitespace() => {
                if has {
                    args.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            (_, c) => {
                cur.push(c);
                has = true;
            }
        }
    }
    if has {
        args.push(cur);
    }
    args.retain(|a| !(a.len() == 2 && a.starts_with('%')));
    args
}

fn basename(s: &str) -> String {
    Path::new(s)
        .file_name()
        .map(|f| f.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

fn scan_desktop(
    home: &Path,
    icons: &IconIndex,
    steam_icons: &mut HashMap<String, String>,
) -> Vec<CatalogApp> {
    let mut files = vec![];
    for dir in data_dirs(home) {
        desktop_files(&dir.join("applications"), "", &mut files, 0);
    }
    let mut seen = HashSet::new();
    let mut apps = vec![];
    for (id, path) in files {
        // The first one found wins (user entries override system ones)
        if !seen.insert(id.clone()) {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let entry = parse_desktop(&text);
        let yes = |k: &str| entry.get(k).is_some_and(|v| v.eq_ignore_ascii_case("true"));
        if entry.get("Type").map(String::as_str) != Some("Application")
            || yes("NoDisplay")
            || yes("Hidden")
        {
            continue;
        }
        let Some(name) = entry.get("Name").filter(|n| !n.is_empty()).cloned() else {
            continue;
        };
        let exec = entry.get("Exec").cloned().unwrap_or_default();

        // Steam's own game shortcuts: fold into the Steam entry (just borrow the icon)
        if let Some(pos) = exec.find("steam://rungameid/") {
            let appid: String = exec[pos + 18..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if let Some(icon) = entry.get("Icon") {
                steam_icons.insert(appid, icon.clone());
            }
            continue;
        }

        let mut binaries = vec![];
        let mut ids = vec![id.to_lowercase()];
        let mut flatpak = entry.get("X-Flatpak").cloned();
        let mut args = exec_args(&exec).into_iter().peekable();
        // Skip `env VAR=value ...` prefixes
        while let Some(a) = args.peek() {
            if basename(a) == "env" || (a.contains('=') && !a.starts_with('-')) {
                args.next();
            } else {
                break;
            }
        }
        let args: Vec<String> = args.collect();
        if let Some(first) = args.first() {
            let bin = basename(first);
            if bin == "flatpak" {
                if let Some(run) = args.iter().position(|a| a == "run") {
                    for a in &args[run + 1..] {
                        if let Some(cmd) = a.strip_prefix("--command=") {
                            binaries.push(basename(cmd));
                        } else if !a.starts_with('-') {
                            flatpak.get_or_insert_with(|| a.clone());
                            break;
                        }
                    }
                }
            } else if !bin.is_empty() {
                binaries.push(bin);
            }
        }
        if let Some(try_exec) = entry.get("TryExec") {
            binaries.push(basename(try_exec));
        }
        if let Some(flatpak) = &flatpak {
            ids.push(flatpak.to_lowercase());
            // Flatpak apps usually run a binary named like the last part of their id
            if let Some(last) = flatpak.rsplit('.').next() {
                binaries.push(last.to_lowercase());
            }
        }
        let mut names = vec![name.to_lowercase()];
        if let Some(class) = entry.get("StartupWMClass") {
            names.push(class.to_lowercase());
        }
        binaries.retain(|b| !b.is_empty());
        binaries.dedup();
        let detail = match &flatpak {
            Some(f) => format!("Flatpak · {f}"),
            None => binaries.first().cloned().unwrap_or_else(|| id.clone()),
        };
        let icon = entry.get("Icon").and_then(|i| icons.find(i));
        let search = format!("{} {} {}", name, detail, binaries.join(" ")).to_lowercase();
        apps.push(CatalogApp {
            key: format!("desktop:{id}"),
            name,
            steam: false,
            detail,
            icon,
            binaries,
            names,
            ids,
            search,
        });
    }
    apps
}

////////////////////////////////////////////////////////////////////////////////////////////
// Steam

/// Reads `"key" "value"` pairs from a VDF/ACF file, ignoring nesting (enough for what we need)
fn vdf_pairs(text: &str) -> Vec<(String, String)> {
    let mut out = vec![];
    for line in text.lines() {
        let parts: Vec<&str> = line.split('"').collect();
        // "key" "value" splits into ["", key, ws, value, ""]
        if parts.len() >= 5 {
            out.push((parts[1].to_string(), parts[3].replace("\\\\", "\\")));
        }
    }
    out
}

fn steam_roots(home: &Path) -> Vec<PathBuf> {
    let mut roots = vec![];
    let mut seen = HashSet::new();
    for r in [
        home.join(".local/share/Steam"),
        home.join(".steam/steam"),
        home.join(".steam/root"),
        home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
        home.join(".var/app/com.valvesoftware.Steam/data/Steam"),
    ] {
        if let Ok(c) = fs::canonicalize(&r)
            && c.join("steamapps").is_dir()
            && seen.insert(c.clone())
        {
            roots.push(c);
        }
    }
    roots
}

fn scan_steam(
    home: &Path,
    icons: &IconIndex,
    steam_icons: &HashMap<String, String>,
) -> Vec<CatalogApp> {
    let roots = steam_roots(home);
    let mut libraries = vec![];
    let mut seen = HashSet::new();
    for root in &roots {
        let mut add = |p: PathBuf| {
            if let Ok(c) = fs::canonicalize(&p)
                && seen.insert(c.clone())
            {
                libraries.push(c);
            }
        };
        add(root.clone());
        if let Ok(text) = fs::read_to_string(root.join("steamapps/libraryfolders.vdf")) {
            for (k, v) in vdf_pairs(&text) {
                if k == "path" {
                    add(PathBuf::from(v));
                }
            }
        }
    }

    let mut apps = vec![];
    let mut seen_ids = HashSet::new();
    for lib in libraries {
        let Ok(entries) = fs::read_dir(lib.join("steamapps")) else {
            continue;
        };
        for entry in entries.flatten() {
            let file = entry.file_name().to_string_lossy().to_string();
            if !(file.starts_with("appmanifest_") && file.ends_with(".acf")) {
                continue;
            }
            let Ok(text) = fs::read_to_string(entry.path()) else {
                continue;
            };
            let pairs: HashMap<String, String> = vdf_pairs(&text)
                .into_iter()
                .rev() // first occurrence wins (top-level keys come first)
                .collect();
            let (Some(appid), Some(name)) = (pairs.get("appid"), pairs.get("name")) else {
                continue;
            };
            let lower = name.to_lowercase();
            if STEAM_TOOLS.iter().any(|t| lower.starts_with(t))
                || lower.contains("redistributable")
                || appid == "228980"
                || !seen_ids.insert(appid.clone())
            {
                continue;
            }
            let install = pairs
                .get("installdir")
                .map(|d| lib.join("steamapps/common").join(d));
            let mut binaries = vec![];
            if let Some(dir) = &install {
                executables(dir, &mut binaries);
            }
            binaries.sort();
            binaries.dedup();
            binaries.truncate(96);
            debug!("App catalog: Steam {appid} {name}: {}", binaries.join(", "));

            let icon = steam_icons
                .get(appid)
                .and_then(|i| icons.find(i))
                .or_else(|| icons.find(&format!("steam_icon_{appid}")))
                .or_else(|| steam_cache_icon(&roots, appid));
            let detail = match &install {
                Some(d) => format!("Steam game · {}", d.display()),
                None => "Steam game".into(),
            };
            let search = format!("{name} steam {}", binaries.join(" ")).to_lowercase();
            apps.push(CatalogApp {
                key: format!("steam:{appid}"),
                name: name.clone(),
                steam: true,
                detail,
                icon,
                names: vec![lower],
                ids: vec![format!("steam:{appid}")],
                binaries,
                search,
            });
        }
    }
    apps
}

/// Collects .exe names and native (ELF) executables a few levels into a game's folder.
/// Breadth first, so the game's own exe (usually near the top) is found before a big asset
/// folder uses up the budget.
fn executables(root: &Path, out: &mut Vec<String>) {
    let mut budget: u32 = 20000;
    let mut queue = std::collections::VecDeque::from([(root.to_path_buf(), 0u8)]);
    while let Some((dir, depth)) = queue.pop_front() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if budget == 0 {
                return;
            }
            budget -= 1;
            let name = entry.file_name().to_string_lossy().to_lowercase();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                if depth < 4 && !SKIP_DIRS.iter().any(|s| name.contains(s)) {
                    queue.push_back((entry.path(), depth + 1));
                }
            } else if kind.is_file() {
                if name.ends_with(".exe") {
                    let junk = name.starts_with("unins")
                        || name.contains("crashhandler")
                        || name.contains("crashreport")
                        || name.contains("setup")
                        || name.contains("redist")
                        || name.contains("prereq");
                    if !junk {
                        out.push(name);
                    }
                } else if ((depth <= 2 && !name.contains('.')) || name.ends_with(".x86_64"))
                    && is_elf_executable(&entry.path())
                {
                    out.push(name);
                }
            }
        }
    }
}

fn is_elf_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    if meta.permissions().mode() & 0o111 == 0 {
        return false;
    }
    let mut magic = [0u8; 4];
    fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut magic))
        .is_ok()
        && magic == *b"\x7fELF"
}

fn steam_cache_icon(roots: &[PathBuf], appid: &str) -> Option<Icon> {
    for root in roots {
        let cache = root.join("appcache/librarycache");
        let old = cache.join(format!("{appid}_icon.jpg"));
        if old.is_file() {
            return Some(Icon::Raster(old));
        }
        // Newer clients: librarycache/<appid>/<hash>.jpg is the small icon
        if let Ok(entries) = fs::read_dir(cache.join(appid)) {
            let mut best: Option<(u64, PathBuf)> = None;
            for e in entries.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                if n.ends_with(".jpg")
                    && n.len() == 44
                    && let Ok(m) = e.metadata()
                    && best.as_ref().is_none_or(|(s, _)| m.len() < *s)
                {
                    best = Some((m.len(), e.path()));
                }
            }
            if let Some((_, p)) = best {
                return Some(Icon::Raster(p));
            }
        }
    }
    None
}

////////////////////////////////////////////////////////////////////////////////////////////
// Icons

/// Icon name -> file, from the hicolor/breeze themes and pixmaps (first match wins)
struct IconIndex(HashMap<String, PathBuf>);

impl IconIndex {
    fn build(home: &Path) -> Self {
        let mut map = HashMap::new();
        let mut bases = vec![home.join(".icons")];
        bases.extend(data_dirs(home).into_iter().map(|d| d.join("icons")));
        let sizes = [
            "scalable", "48x48", "64x64", "128x128", "96x96", "256x256", "32x32", "512x512",
            "24x24",
        ];
        for theme in ["hicolor", "breeze", "breeze-dark", "Adwaita"] {
            for base in &bases {
                let theme_dir = base.join(theme);
                if !theme_dir.is_dir() {
                    continue;
                }
                for size in sizes {
                    Self::add_dir(&mut map, &theme_dir.join(size).join("apps"));
                }
                // breeze lays out apps/<size>/
                for size in ["48", "64", "32"] {
                    Self::add_dir(&mut map, &theme_dir.join("apps").join(size));
                }
            }
        }
        Self::add_dir(&mut map, Path::new("/usr/share/pixmaps"));
        Self(map)
    }

    fn add_dir(map: &mut HashMap<String, PathBuf>, dir: &Path) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let file = entry.file_name().to_string_lossy().to_string();
            if let Some((stem, ext)) = file.rsplit_once('.')
                && matches!(ext, "svg" | "png" | "jpg")
            {
                map.entry(stem.to_string()).or_insert_with(|| entry.path());
            }
        }
    }

    fn find(&self, name: &str) -> Option<Icon> {
        let path = if name.starts_with('/') {
            let p = PathBuf::from(name);
            p.is_file().then_some(p)?
        } else {
            self.0.get(name)?.clone()
        };
        let svg = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("svg"));
        Some(if svg {
            Icon::Svg(path)
        } else {
            Icon::Raster(path)
        })
    }
}

/// Loads a raster icon scaled down to `size` px (keeps memory low with hundreds of apps)
pub fn load_raster(path: &Path, size: u32) -> Option<(u32, u32, Vec<u8>)> {
    let img = image::open(path).ok()?;
    let img = img.thumbnail(size, size).into_rgba8();
    Some((img.width(), img.height(), img.into_raw()))
}

////////////////////////////////////////////////////////////////////////////////////////////
// Live stream properties

/// Reads ids Pipeweaver doesn't report (Flatpak app id, SteamAppId) for the given nodes,
/// from `pw-dump` and the owning process's environment.
pub fn probe(nodes: &[u32]) -> Vec<(u32, StreamIds)> {
    let wanted: HashSet<u32> = nodes.iter().copied().collect();
    let output = match std::process::Command::new("pw-dump").output() {
        Ok(o) if o.status.success() => o.stdout,
        Ok(_) | Err(_) => {
            debug!("App catalog: pw-dump unavailable");
            return nodes.iter().map(|n| (*n, StreamIds::default())).collect();
        }
    };
    let Ok(objects) = serde_json::from_slice::<Vec<serde_json::Value>>(&output) else {
        warn!("App catalog: couldn't read pw-dump output");
        return vec![];
    };
    let mut found = HashMap::new();
    for obj in objects {
        let Some(id) = obj.get("id").and_then(|i| i.as_u64()).map(|i| i as u32) else {
            continue;
        };
        if !wanted.contains(&id) {
            continue;
        }
        let props = &obj["info"]["props"];
        let prop = |k: &str| props.get(k).and_then(|v| v.as_str()).map(str::to_string);
        let mut ids = vec![];
        for k in [
            "pipewire.access.portal.app_id",
            "application.id",
            "flatpak.app-id",
        ] {
            if let Some(v) = prop(k).filter(|v| !v.is_empty()) {
                ids.push(v.to_lowercase());
            }
        }
        let pid = props
            .get("application.process.id")
            .and_then(|v| {
                v.as_str()
                    .map(str::to_string)
                    .or_else(|| Some(v.to_string()))
            })
            .and_then(|v| v.trim_matches('"').parse::<u32>().ok());
        if let Some(pid) = pid
            && let Ok(env) = fs::read(format!("/proc/{pid}/environ"))
        {
            for var in env.split(|b| *b == 0) {
                let var = String::from_utf8_lossy(var);
                let Some((k, v)) = var.split_once('=') else {
                    continue;
                };
                if matches!(k, "SteamAppId" | "SteamGameId" | "STEAM_COMPAT_APP_ID")
                    && !v.is_empty()
                    && v != "0"
                    && v.chars().all(|c| c.is_ascii_digit())
                {
                    ids.insert(0, format!("steam:{v}"));
                } else if k == "FLATPAK_ID" && !v.is_empty() {
                    ids.push(v.to_lowercase());
                }
            }
        }
        let mut seen = HashSet::new();
        ids.retain(|i| seen.insert(i.clone()));
        found.insert(id, StreamIds { ids });
    }
    nodes
        .iter()
        .map(|n| (*n, found.remove(n).unwrap_or_default()))
        .collect()
}

////////////////////////////////////////////////////////////////////////////////////////////
// Saved choices

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SavedRoute {
    /// The Pipeweaver channel (ULID)
    pub channel: String,
    /// For humans reading the file
    #[serde(default)]
    pub channel_name: String,
    #[serde(default)]
    pub app_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppRouting {
    #[serde(default)]
    pub version: u32,
    /// When the file was last read or written by us (to notice hand edits)
    #[serde(skip)]
    pub loaded_at: Option<std::time::SystemTime>,
    /// Catalog key -> route. No entry means "Default" (the default output, System).
    #[serde(default)]
    pub apps: BTreeMap<String, SavedRoute>,
}

fn modified(path: &Path) -> Option<std::time::SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn routing_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "beacn-utility")
        .map(|d| d.config_dir().join("app-routing.json"))
}

impl AppRouting {
    pub fn load() -> Self {
        let Some(path) = routing_path() else {
            return Self::default();
        };
        let mut routing: Self = match fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                warn!("Couldn't read {}: {e}", path.display());
                Self::default()
            }),
            Err(_) => Self::default(),
        };
        routing.loaded_at = modified(&path);
        routing
    }

    /// Re-reads the file if it was changed outside the app (hand edits, a script)
    pub fn reload_if_changed(&mut self) -> bool {
        let Some(path) = routing_path() else {
            return false;
        };
        let now = modified(&path);
        if now == self.loaded_at {
            return false;
        }
        *self = Self::load();
        true
    }

    pub fn save(&mut self) {
        let Some(path) = routing_path() else { return };
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let mut copy = self.clone();
        copy.version = 1;
        match serde_json::to_string_pretty(&copy) {
            Ok(text) => {
                let tmp = path.with_extension("json.tmp");
                if let Err(e) = fs::write(&tmp, text).and_then(|_| fs::rename(&tmp, &path)) {
                    warn!("Couldn't save {}: {e}", path.display());
                }
                self.loaded_at = modified(&path);
            }
            Err(e) => warn!("Couldn't encode app routing: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_parsing() {
        assert_eq!(
            exec_args(r#"env FOO=1 "/opt/My App/app" --x %U"#),
            vec!["env", "FOO=1", "/opt/My App/app", "--x"]
        );
    }

    #[test]
    fn vdf() {
        let p = vdf_pairs(
            "\"AppState\"\n{\n\t\"appid\"\t\t\"1091500\"\n\t\"name\"\t\t\"Cyberpunk 2077\"\n}",
        );
        assert_eq!(p[0], ("appid".into(), "1091500".into()));
        assert_eq!(p[1].1, "Cyberpunk 2077");
    }

    #[test]
    fn matching() {
        let app = |key: &str, name: &str, steam, bins: &[&str], ids: &[&str]| CatalogApp {
            key: key.into(),
            name: name.into(),
            steam,
            detail: String::new(),
            icon: None,
            binaries: bins.iter().map(|s| s.to_string()).collect(),
            names: vec![name.to_lowercase()],
            ids: ids.iter().map(|s| s.to_string()).collect(),
            search: String::new(),
        };
        let c = Catalog::index(vec![
            app("desktop:chromium", "Chromium", false, &["chromium"], &[]),
            app(
                "desktop:com.discordapp.Discord",
                "Discord",
                false,
                &["discord"],
                &["com.discordapp.discord"],
            ),
            app(
                "steam:1",
                "Schedule I",
                true,
                &["schedule i.exe"],
                &["steam:1"],
            ),
        ]);
        // Discord's Chromium-named stream belongs to Discord, not Chromium
        assert_eq!(c.find("Discord", "Chromium", None), Some(1));
        assert_eq!(c.find("chromium", "Chromium", None), Some(0));
        assert_eq!(c.find("wine64-preloader", "Schedule I.exe", None), Some(2));
        assert_eq!(c.find("wine64-preloader", "Schedule I", None), Some(2));
        assert_eq!(c.find("Schedule I.exe", "x", None), Some(2));
        let ids = StreamIds {
            ids: vec!["steam:1".into()],
        };
        assert_eq!(c.find("wine64-preloader", "whatever", Some(&ids)), Some(2));
        assert_eq!(c.find("unknown", "Chromium", None), None);
    }
}
