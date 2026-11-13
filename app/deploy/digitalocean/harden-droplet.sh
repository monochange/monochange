#!/usr/bin/env bash
set -euo pipefail

DEPLOY_USER="${DEPLOY_USER:-deploy}"
AUTHORIZED_KEYS_SOURCE="${AUTHORIZED_KEYS_SOURCE:-/root/.ssh/authorized_keys}"
SSH_PORT="${SSH_PORT:-22}"

if [ "$(id -u)" -ne 0 ]; then
	echo "harden-droplet.sh must run as root" >&2
	exit 1
fi

# Helper scripts are copied next to this one and installed from there.
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
for sibling in monochange-deploy.sh ci-deploy.sh monochange-sqlite-backup.sh mitigate-cve-2026-98432.sh; do
	if [ ! -f "${SCRIPT_DIR}/${sibling}" ]; then
		echo "missing ${SCRIPT_DIR}/${sibling}; copy it next to harden-droplet.sh" >&2
		exit 1
	fi
done

apt-get update
apt-get install -y \
	ca-certificates \
	curl \
	fail2ban \
	gnupg \
	jq \
	nftables \
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

# Reboot automatically when a security upgrade needs a new kernel or core
# library. apt-daily-upgrade runs at 06:00 UTC with up to 60 minutes of random
# delay, so 07:30 UTC leaves the upgrade time to finish.
cat >/etc/apt/apt.conf.d/51unattended-upgrades-auto-reboot <<'EOF'
Unattended-Upgrade::Automatic-Reboot "true";
Unattended-Upgrade::Automatic-Reboot-Time "07:30";
Unattended-Upgrade::Automatic-Reboot-WithUsers "true";
EOF
chmod 0644 /etc/apt/apt.conf.d/51unattended-upgrades-auto-reboot

install -d -m 755 /opt/monochange/data /opt/monochange/backups /opt/monochange/caddy/data /opt/monochange/caddy/config
chown "${DEPLOY_USER}:${DEPLOY_USER}" /opt/monochange
chown -R "${DEPLOY_USER}:${DEPLOY_USER}" /opt/monochange/data /opt/monochange/backups /opt/monochange/caddy
install -d -m 700 /opt/monochange/secrets
chown root:root /opt/monochange/secrets

install -o root -g root -m 0755 "${SCRIPT_DIR}/monochange-deploy.sh" /usr/local/bin/monochange-deploy
# Forced command for the GitHub Actions deployment key. It does nothing until
# that key is added to the deploy user's authorized_keys (see OPERATIONS.md).
install -o root -g root -m 0755 "${SCRIPT_DIR}/ci-deploy.sh" /usr/local/bin/monochange-ci-deploy

install -o root -g root -m 0755 "${SCRIPT_DIR}/monochange-sqlite-backup.sh" /usr/local/bin/monochange-sqlite-backup

cat >/etc/cron.daily/monochange-sqlite-backup <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
/usr/local/bin/monochange-sqlite-backup --daily
EOF
chmod 0755 /etc/cron.daily/monochange-sqlite-backup

MITIGATION_SCRIPT="${SCRIPT_DIR}/mitigate-cve-2026-98432.sh"
if [ ! -x "${MITIGATION_SCRIPT}" ]; then
	echo "missing ${MITIGATION_SCRIPT}; copy it next to harden-droplet.sh" >&2
	exit 1
fi
"${MITIGATION_SCRIPT}"

echo "Droplet hardening complete. Use user '${DEPLOY_USER}' for SSH/deploy access."
