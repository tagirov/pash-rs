<h1 align="center">pash-rs</h1>

<p align="center">A dependency-free Rust port of <a href="https://github.com/dylanaraps/pash">pash</a> with <a href="https://github.com/junegunn/fzf">fzf</a> integration</p>

## Install

#### Linux/MacOS

```bash
cargo install --git https://github.com/tagirov/pash-rs
```

The binary will be installed to `$HOME/.cargo/bin/pash`. Make sure this
path is added to your $PATH environment variable to use `pash` command
globally.

#### Nix

Install with a single command (any system with Nix, flakes enabled):

```bash
nix profile add github:tagirov/pash-rs
```

Run without installing:

```bash
nix run github:tagirov/pash-rs
```

Or add to your NixOS/Home Manager configuration as a flake input:

```nix
{
  inputs.pash-rs.url = "github:tagirov/pash-rs";
}
```

```nix
environment.systemPackages = [ inputs.pash-rs.packages.${pkgs.system}.default ];
```

Runtime dependencies: `gpg` (or `gpg2`), `fzf` for interactive picking,
and a clipboard tool (`wl-copy`/`xclip`).

## Usage

```
=> [a]dd  [name] - Create a new password entry.
=> [c]opy [name] - Copy entry to the clipboard.
=> [d]el  [name] - Delete a password entry.
=> [l]ist        - List all entries.
=> [s]how [name] - Show password for an entry.
=> [t]ree        - List all entries in a tree.
```

Omitting `[name]` for `copy`/`del`/`show` opens fzf to pick an entry interactively:

```sh
pash c            # fuzzy-pick an entry, copy it to the clipboard
pash c gmail      # show a specific entry
```

Passwords are stored as
gpg-encrypted files under `PASH_DIR`, one file per entry, with support
for hierarchical categories. Fully compatible with an existing pash
store.

## Configuration

Everything is configured through environment variables:

| Variable       | Purpose                        | Default                |
|----------------|--------------------------------|------------------------|
| `PASH_DIR`     | Password store location        | `~/.local/share/pash`  |
| `PASH_KEYID`   | GPG key id (asymmetric mode)   | unset → symmetric `-c` |
| `PASH_LENGTH`  | Generated password length      | `50`                   |
| `PASH_PATTERN` | Character set for generation   | `_A-Z-a-z-0-9`         |
| `PASH_CLIP`    | Clipboard tool                 | `wl-copy` / `xclip`    |
| `PASH_FZF`     | Fuzzy finder command           | `fzf`                  |
