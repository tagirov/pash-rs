//! pash-rs - simple password manager.
//!
//! A dependency-free Rust port of pash. Passwords are stored as
//! gpg-encrypted files under PASH_DIR, one file per entry.

use std::env;
use std::fs;
use std::io::{self, BufRead, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{exit, Command, Stdio};

const USAGE: &str = "\
pash-rs 1.0.0 - simple password manager.

=> [a]dd  [name] - Create a new password entry.
=> [c]opy [name] - Copy entry to the clipboard.
=> [d]el  [name] - Delete a password entry.
=> [l]ist        - List all entries.
=> [s]how [name] - Show password for an entry.
=> [t]ree        - List all entries in a tree.

Omitting [name] for copy/del/show opens fzf.

Using a key pair:  export PASH_KEYID=XXXXXXXX
Password length:   export PASH_LENGTH=50
Password pattern:  export PASH_PATTERN=_A-Z-a-z-0-9
Store location:    export PASH_DIR=~/.local/share/pash
Clipboard tool:    export PASH_CLIP='wl-copy'
Fuzzy finder:      export PASH_FZF='fzf'
";

fn die(msg: &str) -> ! {
    eprintln!("error: {msg}.");
    exit(1);
}

// -- Terminal helpers -------------------------------------------------------

fn stty(args: &[&str]) {
    let _ = Command::new("stty")
        .args(args)
        .stdin(Stdio::inherit())
        .status();
}

/// Ask a yes/no question, reading a single raw byte so the
/// user doesn't need to press Return.
fn yn(prompt: &str) -> bool {
    print!("{prompt} [y/n]: ");
    let _ = io::stdout().flush();

    stty(&["-icanon"]);
    let mut byte = [0u8; 1];
    let n = io::stdin().read(&mut byte).unwrap_or(0);
    stty(&["icanon"]);

    println!();
    n == 1 && matches!(byte[0], b'y' | b'Y')
}

/// Read a line with terminal echo disabled.
fn sread(prompt: &str) -> String {
    print!("{prompt}: ");
    let _ = io::stdout().flush();

    stty(&["-echo"]);
    let mut line = String::new();
    let _ = io::stdin().lock().read_line(&mut line);
    stty(&["echo"]);

    println!();
    line.trim_end_matches('\n').to_string()
}

// -- Password generation ----------------------------------------------------

/// Expand a tr-style character set ("_A-Z-a-z-0-9") into a
/// membership table over all byte values.
fn expand_pattern(pattern: &str) -> [bool; 256] {
    let bytes = pattern.as_bytes();
    let mut set = [false; 256];
    let mut i = 0;

    while i < bytes.len() {
        // "c1-c2" forms a range; a '-' anywhere else is literal.
        if i + 2 < bytes.len() && bytes[i + 1] == b'-' && bytes[i] <= bytes[i + 2] {
            for b in bytes[i]..=bytes[i + 2] {
                set[b as usize] = true;
            }
            i += 3;
        } else {
            set[bytes[i] as usize] = true;
            i += 1;
        }
    }

    set
}

/// Generate a password by rejection-sampling '/dev/urandom'
/// against the character set, exactly like `tr -dc SET | dd`.
///
/// Regarding usage of '/dev/urandom' instead of '/dev/random'.
/// See: https://www.2uo.de/myths-about-urandom
fn gen_password(length: usize, set: &[bool; 256]) -> String {
    let mut urandom = fs::File::open("/dev/urandom")
        .unwrap_or_else(|_| die("Couldn't open /dev/urandom"));
    let mut pass = Vec::with_capacity(length);
    let mut buf = [0u8; 256];

    while pass.len() < length {
        urandom
            .read_exact(&mut buf)
            .unwrap_or_else(|_| die("Couldn't read /dev/urandom"));

        for &b in &buf {
            if set[b as usize] {
                pass.push(b);
                if pass.len() == length {
                    break;
                }
            }
        }
    }

    String::from_utf8(pass).unwrap_or_else(|_| die("Failed to generate a password"))
}

// -- Commands ---------------------------------------------------------------

fn pw_add(gpg: &str, name: &str) {
    let pass = if yn("Generate a password?") {
        let length = env::var("PASH_LENGTH")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(50);
        let pattern = env::var("PASH_PATTERN").unwrap_or_else(|_| "_A-Z-a-z-0-9".into());
        gen_password(length, &expand_pattern(&pattern))
    } else {
        let pass = sread("Enter password");
        let pass2 = sread("Enter password (again)");

        if pass != pass2 {
            die("Passwords do not match");
        }
        pass
    };

    if pass.is_empty() {
        die("Failed to generate a password");
    }

    if let Some(category) = Path::new(name).parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(category)
            .unwrap_or_else(|_| die(&format!("Couldn't create category '{}'", category.display())));
    }

    let mut cmd = Command::new(gpg);
    match env::var("PASH_KEYID") {
        Ok(keyid) if !keyid.is_empty() => {
            cmd.args(["--trust-model", "always", "-aer", &keyid]);
        }
        _ => {
            cmd.arg("-c");
        }
    }

    let file = format!("{name}.gpg");
    let mut child = cmd
        .args(["-o", &file])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap_or_else(|_| die("Failed to run gpg"));

    // The password is piped straight into gpg's stdin, so it
    // never appears on a command line or in '/proc'.
    if let Some(mut stdin) = child.stdin.take() {
        let _ = writeln!(stdin, "{pass}");
    }

    let status = child.wait().unwrap_or_else(|_| die("Failed to run gpg"));
    if !status.success() {
        exit(1);
    }

    let _ = fs::set_permissions(&file, fs::Permissions::from_mode(0o600));
    println!("Saved '{name}' to the store.");
}

fn pw_del(name: &str) {
    if !yn(&format!("Delete pass file '{name}'?")) {
        return;
    }

    let _ = fs::remove_file(format!("{name}.gpg"));

    // Remove empty parent directories of the entry. It's fine
    // if this fails as it means another entry also lives in
    // the same directory.
    let mut dir = Path::new(name).parent();
    while let Some(d) = dir {
        if d.as_os_str().is_empty() || fs::remove_dir(d).is_err() {
            break;
        }
        dir = d.parent();
    }
}

fn pw_show(gpg: &str, name: &str) {
    let status = Command::new(gpg)
        .args(["-dq", &format!("{name}.gpg")])
        .status()
        .unwrap_or_else(|_| die("Failed to run gpg"));

    if !status.success() {
        exit(1);
    }
}

fn pw_copy(gpg: &str, name: &str) {
    let output = Command::new(gpg)
        .args(["-dq", &format!("{name}.gpg")])
        .stderr(Stdio::inherit())
        .output()
        .unwrap_or_else(|_| die("Failed to run gpg"));

    if !output.status.success() {
        exit(1);
    }

    // Pick a clipboard tool based on the running session
    // unless the user has already chosen one.
    let clip = env::var("PASH_CLIP").unwrap_or_else(|_| {
        if env::var("WAYLAND_DISPLAY").is_ok() {
            "wl-copy".into()
        } else {
            "xclip -sel c".into()
        }
    });

    let mut parts = clip.split_whitespace();
    let program = parts.next().unwrap_or_else(|| die("PASH_CLIP is empty"));
    let mut child = Command::new(program)
        .args(parts)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap_or_else(|_| die(&format!("Failed to run clipboard tool '{clip}'")));

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(&output.stdout);
    }
    let _ = child.wait();
}

fn pw_list(dir: &Path, base: &Path, entries: &mut Vec<String>) {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            pw_list(&path, base, entries);
        } else if path.extension().is_some_and(|e| e == "gpg") {
            let rel = path.strip_prefix(base).unwrap_or(&path);
            entries.push(rel.with_extension("").to_string_lossy().into_owned());
        }
    }
}

/// Pick an entry interactively by piping the store listing
/// through fzf (or the command set in PASH_FZF).
fn pw_pick() -> String {
    let mut entries = Vec::new();
    pw_list(Path::new("."), Path::new("."), &mut entries);
    if entries.is_empty() {
        die("No entries in the store");
    }
    entries.sort();

    let fzf = env::var("PASH_FZF").unwrap_or_else(|_| "fzf".into());
    let mut parts = fzf.split_whitespace();
    let program = parts.next().unwrap_or_else(|| die("PASH_FZF is empty"));
    let mut child = Command::new(program)
        .args(parts)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap_or_else(|_| die("Failed to run fzf (set PASH_FZF to change the picker)"));

    if let Some(mut stdin) = child.stdin.take() {
        for entry in &entries {
            let _ = writeln!(stdin, "{entry}");
        }
    }

    let output = child
        .wait_with_output()
        .unwrap_or_else(|_| die("Failed to run fzf"));

    // A non-zero status means the user cancelled or nothing
    // matched; that's not an error worth a message.
    if !output.status.success() {
        exit(1);
    }

    let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if name.is_empty() {
        exit(1);
    }
    name
}

fn pw_tree(dir: &Path, prefix: &str) {
    let mut entries: Vec<_> = match fs::read_dir(dir) {
        Ok(read_dir) => read_dir
            .flatten()
            .filter(|e| {
                let name = e.file_name();
                let name = name.to_string_lossy();
                !name.starts_with('.') && (e.path().is_dir() || name.ends_with(".gpg"))
            })
            .collect(),
        Err(_) => return,
    };
    entries.sort_by_key(|e| e.file_name());

    let count = entries.len();
    for (i, entry) in entries.iter().enumerate() {
        let last = i + 1 == count;
        let name = entry.file_name().to_string_lossy().into_owned();
        let name = name.strip_suffix(".gpg").unwrap_or(&name);

        println!("{prefix}{}{name}", if last { "└── " } else { "├── " });

        if entry.path().is_dir() {
            let child_prefix = format!("{prefix}{}", if last { "    " } else { "│   " });
            pw_tree(&entry.path(), &child_prefix);
        }
    }
}

// -- Entry point --------------------------------------------------------------

fn find_gpg() -> &'static str {
    // Look for both 'gpg' and 'gpg2',
    // preferring 'gpg2' if it is available.
    for gpg in ["gpg2", "gpg"] {
        let found = Command::new(gpg)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if found {
            return gpg;
        }
    }

    die("GPG not found");
}

fn check_name(name: &str) {
    if name.is_empty() {
        die("Missing [name] argument");
    }
    if name.starts_with('/') {
        die("Entry name can't start with '/'");
    }
    if name.ends_with('/') {
        die("Entry name can't end with '/'");
    }
    if name.split('/').any(|c| c.is_empty() || c == "." || c == "..") {
        die("Entry name contains an invalid path component");
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    let Some(command) = args.first().and_then(|a| a.chars().next()) else {
        print!("{USAGE}");
        return;
    };
    let mut name = args.get(1).cloned().unwrap_or_default();

    let gpg = find_gpg();

    let store = env::var("PASH_DIR").unwrap_or_else(|_| {
        let data = env::var("XDG_DATA_HOME").unwrap_or_else(|_| {
            let home = env::var("HOME").unwrap_or_else(|_| die("HOME not set"));
            format!("{home}/.local/share")
        });
        format!("{data}/pash")
    });

    fs::create_dir_all(&store).unwrap_or_else(|_| die("Couldn't create password directory"));
    let _ = fs::set_permissions(&store, fs::Permissions::from_mode(0o700));
    env::set_current_dir(&store).unwrap_or_else(|_| die("Can't access password directory"));

    if matches!(command, 'c' | 'd' | 's') && name.is_empty() {
        name = pw_pick();
    }

    if matches!(command, 'a' | 'c' | 'd' | 's') {
        check_name(&name);

        let exists = Path::new(&format!("{name}.gpg")).is_file();
        match command {
            'a' if exists => die(&format!("Pass file '{name}' already exists")),
            'c' | 'd' | 's' if !exists => die(&format!("Pass file '{name}' doesn't exist")),
            _ => {}
        }
    }

    // Set 'GPG_TTY' to the current 'TTY' if it
    // is unset. Fixes a somewhat rare 'gpg' issue.
    if env::var("GPG_TTY").is_err()
        && let Ok(out) = Command::new("tty").stdin(Stdio::inherit()).output()
        && out.status.success()
    {
        // SAFETY: the process is single-threaded at this point;
        // no other thread can observe the environment.
        unsafe {
            env::set_var("GPG_TTY", String::from_utf8_lossy(&out.stdout).trim());
        }
    }

    match command {
        'a' => pw_add(gpg, &name),
        'c' => pw_copy(gpg, &name),
        'd' => pw_del(&name),
        's' => pw_show(gpg, &name),
        'l' => {
            let mut entries = Vec::new();
            pw_list(Path::new("."), Path::new("."), &mut entries);
            entries.sort();
            for entry in entries {
                println!("{entry}");
            }
        }
        't' => {
            println!(".");
            pw_tree(Path::new("."), "");
        }
        _ => print!("{USAGE}"),
    }
}
