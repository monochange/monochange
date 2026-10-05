#!/usr/bin/env bash
set -euo pipefail

DEPLOY_USER="${DEPLOY_USER:-deploy}"
AUTHORIZED_KEYS_SOURCE="${AUTHORIZED_KEYS_SOURCE:-/root/.ssh/authorized_keys}"
SSH_PORT="${SSH_PORT:-22}"

if [ "$(id -u)" -ne 0 ]; then
	echo "harden-droplet.sh must run as root" >&2
	exit 1
fi

apt-get update
apt-get install -y \
	ca-certificates \
	curl \
	fail2ban \
	gnupg \
	sqlite3 \
	ufw \
	unattended-upgrades

install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc
chmod a+r /etc/apt/keyrings/docker.asc
. /etc/os-release
echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/ubuntu ${VERSION_CODENAME} stable" >/etc/apt/sources.list.d/docker.list
apt-get update
apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin

if ! id "${DEPLOY_USER}" >/dev/null 2>&1; then
	adduser --disabled-password --gecos "" "${DEPLOY_USER}"
fi
usermod -aG docker "${DEPLOY_USER}"

install -d -m 700 -o "${DEPLOY_USER}" -g "${DEPLOY_USER}" "/home/${DEPLOY_USER}/.ssh"
if [ -s "${AUTHORIZED_KEYS_SOURCE}" ]; then
	install -m 600 -o "${DEPLOY_USER}" -g "${DEPLOY_USER}" "${AUTHORIZED_KEYS_SOURCE}" "/home/${DEPLOY_USER}/.ssh/authorized_keys"
else
	touch "/home/${DEPLOY_USER}/.ssh/authorized_keys"
	chown "${DEPLOY_USER}:${DEPLOY_USER}" "/home/${DEPLOY_USER}/.ssh/authorized_keys"
	chmod 600 "/home/${DEPLOY_USER}/.ssh/authorized_keys"
fi

cat >/etc/ssh/sshd_config.d/99-monochange-hardening.conf <<EOF
Port ${SSH_PORT}
PubkeyAuthentication yes
PasswordAuthentication no
KbdInteractiveAuthentication no
ChallengeResponseAuthentication no
PermitRootLogin prohibit-password
X11Forwarding no
AllowUsers ${DEPLOY_USER} root
EOF
sshd -t
# Ubuntu 24.04 can activate SSH through ssh.socket while ssh.service is stopped.
systemctl start ssh
systemctl reload ssh

ufw default deny incoming
ufw default allow outgoing
ufw allow "${SSH_PORT}/tcp"
ufw allow 80/tcp
ufw allow 443/tcp
ufw --force enable

cat >/etc/fail2ban/jail.d/monochange-sshd.conf <<EOF
[sshd]
enabled = true
port = ${SSH_PORT}
maxretry = 5
findtime = 10m
bantime = 1h
EOF
systemctl enable --now fail2ban
systemctl restart fail2ban

dpkg-reconfigure -f noninteractive unattended-upgrades
systemctl enable --now unattended-upgrades

install -d -m 755 /opt/monochange/data /opt/monochange/backups /opt/monochange/caddy/data /opt/monochange/caddy/config
chown "${DEPLOY_USER}:${DEPLOY_USER}" /opt/monochange
chown -R "${DEPLOY_USER}:${DEPLOY_USER}" /opt/monochange/data /opt/monochange/backups /opt/monochange/caddy
install -d -m 700 /opt/monochange/secrets
chown root:root /opt/monochange/secrets

cat >/usr/local/bin/monochange-deploy <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
cd /opt/monochange
IMAGE="${1:?usage: monochange-deploy monochange-app:<commit-sha>}"
if [[ ! "$IMAGE" =~ ^monochange-app:[0-9a-f]{40}$ ]]; then
	echo 'expected monochange-app:<40-character commit SHA>' >&2
	exit 1
fi
docker image inspect "$IMAGE" >/dev/null
# Keep the selected local image across subsequent Compose invocations.
printf 'MONOCHANGE_IMAGE=%s\n' "$IMAGE" >.env
docker compose up -d --pull never
docker compose ps
curl --fail --silent --show-error --retry 20 --retry-all-errors \
	--retry-delay 1 --retry-max-time 60 --max-time 5 http://127.0.0.1:3000/health
echo
EOF
chmod 0755 /usr/local/bin/monochange-deploy

cat >/usr/local/bin/monochange-sqlite-backup <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
DB=/opt/monochange/data/monochange_app.sqlite3
BACKUP_DIR=/opt/monochange/backups
STAMP=$(date -u +%Y%m%dT%H%M%SZ)
mkdir -p "$BACKUP_DIR"
if [ -f "$DB" ]; then
	sqlite3 "$DB" ".backup $BACKUP_DIR/monochange_app-$STAMP.sqlite3"
fi
find "$BACKUP_DIR" -name "monochange_app-*.sqlite3" -mtime +14 -delete
EOF
chmod 0755 /usr/local/bin/monochange-sqlite-backup

cat >/etc/cron.daily/monochange-sqlite-backup <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
/usr/local/bin/monochange-sqlite-backup
EOF
chmod 0755 /etc/cron.daily/monochange-sqlite-backup

echo "Droplet hardening complete. Use user '${DEPLOY_USER}' for SSH/deploy access."
