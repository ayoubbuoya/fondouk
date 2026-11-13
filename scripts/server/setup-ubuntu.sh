#!/usr/bin/env bash
#
# setup-ubuntu.sh — one-time install of everything Fondouk (and the other
# hackathon apps) need on a fresh Ubuntu 26.04 x86_64 server, without Docker.
#
# What it installs:
#   base build tools, 4 GB swap, ufw + fail2ban + unattended-upgrades,
#   PostgreSQL (PGDG repo), Node.js LTS (NodeSource) + npm + pnpm + pm2,
#   Nginx (Ubuntu repo), Certbot (snap), Rust (rustup, stable) + sqlx-cli.
#
# What it does NOT do (that is the later "deploy" step, it needs the domain):
#   create the DB user/database, clone the repo, build, write systemd units,
#   write Nginx site configs, or request TLS certificates.
#
# Usage (as the normal login user, e.g. "ubuntu", NOT as root):
#   chmod +x setup-ubuntu.sh && ./setup-ubuntu.sh
#
# Safe to run again: every step checks whether its work is already done.

set -euo pipefail

# ---- Settings you may change -------------------------------------------------
PG_VERSION="18"        # PostgreSQL major version to install
SWAP_SIZE_GB="4"       # swap file size; safety net for Rust / Next.js builds
TIMEZONE="UTC"         # server clock; UTC keeps logs and the DB unambiguous
# ------------------------------------------------------------------------------

# Non-interactive apt: no debconf questions, and needrestart restarts
# services automatically instead of opening a dialog mid-script.
APT="sudo DEBIAN_FRONTEND=noninteractive NEEDRESTART_MODE=a apt-get -y -q"

log() { printf '\n\033[1;34m==> %s\033[0m\n' "$*"; }

# rustup installs Rust into the current user's home (~/.cargo). Running as
# root would put it in /root and the app user could not use it.
if [[ "${EUID}" -eq 0 ]]; then
  echo "Run this as your normal user (it uses sudo when needed), not as root." >&2
  exit 1
fi

if [[ "$(uname -m)" != "x86_64" ]]; then
  echo "Warning: expected x86_64, found $(uname -m). Continuing anyway." >&2
fi

# -----------------------------------------------------------------------------
log "1/9  System update + base packages"
$APT update
$APT upgrade
# build-essential/pkg-config/cmake/clang: compile Rust crates with C parts
#   (e.g. the TLS library used by reqwest).
# libssl-dev: in case any crate links OpenSSL.
$APT install \
  build-essential pkg-config libssl-dev cmake clang \
  git curl wget ca-certificates gnupg lsb-release unzip jq htop \
  ufw fail2ban unattended-upgrades snapd
sudo timedatectl set-timezone "${TIMEZONE}"

# -----------------------------------------------------------------------------
log "2/9  Swap file (${SWAP_SIZE_GB} GB)"
if ! swapon --show | grep -q '/swapfile'; then
  sudo fallocate -l "${SWAP_SIZE_GB}G" /swapfile
  sudo chmod 600 /swapfile
  sudo mkswap /swapfile
  sudo swapon /swapfile
  grep -q '^/swapfile' /etc/fstab || echo '/swapfile none swap sw 0 0' | sudo tee -a /etc/fstab
  # Use swap only under real memory pressure, not for normal caching.
  echo 'vm.swappiness=10' | sudo tee /etc/sysctl.d/99-swappiness.conf
  sudo sysctl --system >/dev/null
else
  echo "Swap already active, skipping."
fi

# -----------------------------------------------------------------------------
log "3/9  Firewall (ufw) + fail2ban + automatic security updates"
# Second layer behind the AWS security group. SSH is allowed BEFORE enabling,
# so the current session is not cut off.
sudo ufw allow OpenSSH
sudo ufw allow 80/tcp
sudo ufw allow 443/tcp
sudo ufw --force enable
# fail2ban's default Ubuntu config already protects sshd.
sudo systemctl enable --now fail2ban
# Install security updates automatically every day.
sudo dpkg-reconfigure -f noninteractive unattended-upgrades

# -----------------------------------------------------------------------------
log "4/9  PostgreSQL ${PG_VERSION} (official PGDG apt repo)"
if ! dpkg -s "postgresql-${PG_VERSION}" >/dev/null 2>&1; then
  $APT install postgresql-common
  # Official helper from postgresql-common: adds apt.postgresql.org for this
  # Ubuntu release, so we get the latest minor version of PG ${PG_VERSION}.
  if ! sudo /usr/share/postgresql-common/pgdg/apt.postgresql.org.sh -y; then
    echo "PGDG repo not available for this Ubuntu release; using Ubuntu's own packages." >&2
  fi
  $APT update
  $APT install "postgresql-${PG_VERSION}" "postgresql-client-${PG_VERSION}"
fi
sudo systemctl enable --now postgresql
# Ubuntu's default config listens on localhost only — keep it that way.

# -----------------------------------------------------------------------------
log "5/9  Node.js LTS (NodeSource) + npm, pnpm, pm2"
if ! command -v node >/dev/null 2>&1; then
  # Official NodeSource setup script: adds their apt repo for the current LTS.
  curl -fsSL https://deb.nodesource.com/setup_lts.x | sudo -E bash -
  $APT install nodejs
fi
# Global CLIs. pnpm is installed with npm because recent Node versions no
# longer ship corepack.
sudo npm install -g npm@latest pnpm@latest pm2@latest
# Make pm2 start its saved apps after a reboot (systemd unit for this user).
sudo env PATH="$PATH:/usr/bin" pm2 startup systemd -u "$USER" --hp "$HOME" >/dev/null

# -----------------------------------------------------------------------------
log "6/9  Nginx"
$APT install nginx
sudo systemctl enable --now nginx

# -----------------------------------------------------------------------------
log "7/9  Certbot (snap — the install method recommended by Certbot)"
if ! command -v certbot >/dev/null 2>&1; then
  sudo snap install core
  sudo snap install --classic certbot
  sudo ln -sf /snap/bin/certbot /usr/bin/certbot
fi
# The snap installs a systemd timer that renews certificates automatically.

# -----------------------------------------------------------------------------
log "8/9  Rust (rustup, stable toolchain)"
if ! command -v rustup >/dev/null 2>&1 && [[ ! -x "$HOME/.cargo/bin/rustup" ]]; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain stable
fi
# shellcheck disable=SC1091
source "$HOME/.cargo/env"
rustup update stable
rustup default stable

# -----------------------------------------------------------------------------
log "9/9  sqlx-cli (runs Fondouk's migrations; takes a few minutes to compile)"
# Postgres only + rustls: smaller build, no OpenSSL needed.
if ! command -v sqlx >/dev/null 2>&1; then
  cargo install sqlx-cli --locked --no-default-features --features postgres,rustls
fi

# -----------------------------------------------------------------------------
log "Done. Installed versions:"
echo "PostgreSQL : $(psql --version)"
echo "Node.js    : $(node --version)"
echo "npm        : $(npm --version)"
echo "pnpm       : $(pnpm --version)"
echo "pm2        : $(pm2 --version)"
echo "Nginx      : $(nginx -v 2>&1)"
echo "Certbot    : $(certbot --version 2>&1)"
echo "Rust       : $(rustc --version)"
echo "Cargo      : $(cargo --version)"
echo "sqlx-cli   : $(sqlx --version)"
echo
swapon --show
echo
echo "Open a NEW shell (or run: source ~/.cargo/env) so cargo is on your PATH."
