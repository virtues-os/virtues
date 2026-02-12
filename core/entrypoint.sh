#!/bin/sh
set -e

# ---------------------------------------------------------------------------
# SSH setup: generate persistent keys on first boot (stored in /data volume)
# ---------------------------------------------------------------------------
SSH_DIR="/data/ssh"
mkdir -p "$SSH_DIR"

# Host keys (persist across restarts so clients don't get host-key warnings)
if [ ! -f "$SSH_DIR/ssh_host_ed25519_key" ]; then
    ssh-keygen -t ed25519 -f "$SSH_DIR/ssh_host_ed25519_key" -N "" -q
    echo "[entrypoint] Generated SSH host key"
fi

# Internal key pair for backend -> localhost SSH bridge
if [ ! -f "$SSH_DIR/internal_key" ]; then
    ssh-keygen -t ed25519 -f "$SSH_DIR/internal_key" -N "" -C "virtues-internal" -q
    echo "[entrypoint] Generated internal SSH key pair"
fi

# Ensure the virtues user (which runs the server) can read the internal private key
chown virtues:virtues "$SSH_DIR/internal_key"
chmod 600 "$SSH_DIR/internal_key"

# Set up authorized_keys for the virtues user
mkdir -p /home/virtues/.ssh
cp "$SSH_DIR/internal_key.pub" /home/virtues/.ssh/authorized_keys
chown -R virtues:virtues /home/virtues/.ssh
chmod 700 /home/virtues/.ssh
chmod 600 /home/virtues/.ssh/authorized_keys

# Start sshd with persistent host key (runs as root in background)
/usr/sbin/sshd -o "HostKey=$SSH_DIR/ssh_host_ed25519_key"

echo "[entrypoint] sshd started"

# ---------------------------------------------------------------------------
# Drop to virtues user and run the application
# ---------------------------------------------------------------------------
exec su -s /bin/sh virtues -c "virtues migrate && virtues server"
