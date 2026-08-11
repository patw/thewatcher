# Deploying TheWatcher

How to run TheWatcher as a background service on Linux, macOS, and Windows.

## Quick reference

| Platform | Manager | Section |
|----------|---------|---------|
| Linux (systemd) | systemd | [systemd](#systemd) |
| Linux (Alpine, Gentoo, Artix) | OpenRC | [OpenRC](#openrc) |
| Linux (runit, s6, etc.) | runit | [runit](#runit) |
| macOS | launchd | [launchd](#launchd) |
| Windows | sc.exe / NSSM | [Windows](#windows) |

All examples assume the binary is at `/usr/local/bin/thewatcher` (or
`C:\Program Files\TheWatcher\thewatcher.exe` on Windows). Adjust paths
to match your installation.

---

## systemd

Most Linux distributions — Debian, Ubuntu, Fedora, RHEL, Arch, openSUSE.

Create `/etc/systemd/system/thewatcher.service`:

```ini
[Unit]
Description=TheWatcher system metrics collector
Documentation=https://github.com/patw/thewatcher
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
# Run as an unprivileged user — no root needed
User=thewatcher
Group=thewatcher
ExecStart=/usr/local/bin/thewatcher --listen 127.0.0.1 --port 8080

# Sandboxing (optional but recommended)
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=read-only
ReadWritePaths=/var/lib/thewatcher
PrivateTmp=yes
RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6 AF_NETLINK

# Restart on failure, with backoff
Restart=on-failure
RestartSec=15

# Log to journal
StandardOutput=journal
StandardError=journal
SyslogIdentifier=thewatcher

[Install]
WantedBy=multi-user.target
```

Set it up:

```bash
# Create the user
sudo useradd --system --no-create-home --home-dir /var/lib/thewatcher thewatcher

# Create the data directory
sudo mkdir -p /var/lib/thewatcher
sudo chown thewatcher:thewatcher /var/lib/thewatcher

# Install and start
sudo cp thewatcher /usr/local/bin/
sudo chmod +x /usr/local/bin/thewatcher
sudo systemctl daemon-reload
sudo systemctl enable --now thewatcher

# Check it's running
sudo systemctl status thewatcher
curl http://127.0.0.1:8080/api/health
```

### systemd with SSH tunnel

To make the dashboard available remotely without exposing the port, add a
second unit that maintains an SSH tunnel.

Create `/etc/systemd/system/thewatcher-tunnel.service`:

```ini
[Unit]
Description=TheWatcher SSH tunnel
After=network-online.target

[Service]
Type=simple
User=thewatcher
ExecStart=/usr/bin/ssh -N -L 8080:127.0.0.1:8080 -o ExitOnForwardFailure=yes -o ServerAliveInterval=60 -o ServerAliveCountMax=3 your-workstation.example.com
Restart=always
RestartSec=30

[Install]
WantedBy=multi-user.target
```

### Log rotation

TheWatcher writes to stdout (captured by journald). For long-running
deployments you may want to limit journal growth:

```ini
# /etc/systemd/journald.conf.d/thewatcher.conf
[Journal]
MaxUse=500M
```

Then `sudo systemctl restart systemd-journald`.

---

## OpenRC

Alpine Linux, Gentoo, Artix, and other non-systemd distributions.

Create `/etc/init.d/thewatcher`:

```sh
#!/sbin/openrc-run

name="thewatcher"
description="TheWatcher system metrics collector"
command="/usr/local/bin/thewatcher"
command_args="--listen 127.0.0.1 --port 8080 --data-dir /var/lib/thewatcher"
command_user="thewatcher:thewatcher"
pidfile="/run/${RC_SVCNAME}.pid"
command_background=true
output_log="/var/log/thewatcher.log"
error_log="/var/log/thewatcher.log"

depend() {
    need net
    after firewall
}
```

Set it up:

```bash
# Create the user
sudo adduser -S -D -h /var/lib/thewatcher thewatcher

# Create the data directory
sudo mkdir -p /var/lib/thewatcher
sudo chown thewatcher:thewatcher /var/lib/thewatcher

# Install and start
sudo cp thewatcher /usr/local/bin/
sudo chmod +x /usr/local/bin/thewatcher
sudo chmod +x /etc/init.d/thewatcher
sudo rc-update add thewatcher default
sudo rc-service thewatcher start

# Check
sudo rc-service thewatcher status
curl http://127.0.0.1:8080/api/health
```

### Log rotation (OpenRC)

```bash
# /etc/logrotate.d/thewatcher
/var/log/thewatcher.log {
    daily
    rotate 7
    compress
    delaycompress
    missingok
    notifempty
    copytruncate
}
```

---

## runit

Void Linux, and available on most distributions as an alternative init.

Create the service directory:

```bash
sudo mkdir -p /etc/sv/thewatcher/log
```

`/etc/sv/thewatcher/run`:

```sh
#!/bin/sh
exec 2>&1
exec chpst -u thewatcher:thewatcher /usr/local/bin/thewatcher \
    --listen 127.0.0.1 --port 8080 --data-dir /var/lib/thewatcher
```

`/etc/sv/thewatcher/log/run`:

```sh
#!/bin/sh
exec svlogd -tt /var/log/thewatcher
```

Set it up:

```bash
sudo chmod +x /etc/sv/thewatcher/run /etc/sv/thewatcher/log/run
sudo mkdir -p /var/log/thewatcher
sudo chown thewatcher:thewatcher /var/log/thewatcher
sudo ln -s /etc/sv/thewatcher /var/service/
```

Check:

```bash
sudo sv status thewatcher
curl http://127.0.0.1:8080/api/health
```

---

## launchd

macOS.

Create `~/Library/LaunchAgents/com.github.patw.thewatcher.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.github.patw.thewatcher</string>

    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/thewatcher</string>
        <string>--listen</string>
        <string>127.0.0.1</string>
        <string>--port</string>
        <string>8080</string>
        <string>--data-dir</string>
        <string>/Users/thewatcher/Library/Application Support/TheWatcher</string>
    </array>

    <key>RunAtLoad</key>
    <true/>

    <key>KeepAlive</key>
    <true/>

    <key>StandardOutPath</key>
    <string>/Users/thewatcher/Library/Logs/thewatcher.log</string>

    <key>StandardErrorPath</key>
    <string>/Users/thewatcher/Library/Logs/thewatcher.log</string>
</dict>
</plist>
```

Set it up:

```bash
# Copy binary
sudo cp thewatcher /usr/local/bin/

# Create log directory
mkdir -p ~/Library/Logs

# Load and start
launchctl load ~/Library/LaunchAgents/com.github.patw.thewatcher.plist

# Check
launchctl list | grep thewatcher
curl http://127.0.0.1:8080/api/health
```

To stop:

```bash
launchctl unload ~/Library/LaunchAgents/com.github.patw.thewatcher.plist
```

---

## Windows

### Option 1: sc.exe (built-in)

From an **Administrator** PowerShell:

```powershell
# Create the service
New-Service -Name "TheWatcher" `
    -BinaryPathName '"C:\Program Files\TheWatcher\thewatcher.exe" --listen 127.0.0.1 --port 8080 --data-dir "C:\ProgramData\TheWatcher"' `
    -DisplayName "TheWatcher System Metrics" `
    -Description "Self-hosted system metrics collector" `
    -StartupType Automatic

# Create data directory
New-Item -ItemType Directory -Path "C:\ProgramData\TheWatcher" -Force

# Start
Start-Service TheWatcher

# Check
Get-Service TheWatcher
curl http://127.0.0.1:8080/api/health
```

### Option 2: NSSM (Non-Sucking Service Manager)

[NSSM](https://nssm.cc/) gives you automatic restarts, log rotation, and a
cleaner management experience than raw sc.exe.

```powershell
# Install NSSM (via chocolatey or manual download)
choco install nssm

# Create the service
nssm install TheWatcher "C:\Program Files\TheWatcher\thewatcher.exe"

# Configure
nssm set TheWatcher AppParameters "--listen 127.0.0.1 --port 8080 --data-dir C:\ProgramData\TheWatcher"
nssm set TheWatcher DisplayName "TheWatcher System Metrics"
nssm set TheWatcher Description "Self-hosted system metrics collector"
nssm set TheWatcher Start SERVICE_AUTO_START
nssm set TheWatcher AppStdout "C:\ProgramData\TheWatcher\thewatcher.log"
nssm set TheWatcher AppStderr "C:\ProgramData\TheWatcher\thewatcher.log"
nssm set TheWatcher AppRotateFiles 1
nssm set TheWatcher AppRotateBytes 10485760

# Start
nssm start TheWatcher

# Check
nssm status TheWatcher
curl http://127.0.0.1:8080/api/health
```

---

## Common patterns

### Remote access via SSH tunnel (server side)

Run a persistent SSH tunnel from the server to your workstation so you
don't expose the HTTP port:

```bash
ssh -N -L 8080:127.0.0.1:8080 -o ExitOnForwardFailure=yes -o ServerAliveInterval=60 your-workstation.example.com
```

See the [systemd tunnel unit](#systemd-with-ssh-tunnel) above for a
persistent version.

### Remote access via SSH tunnel (workstation side)

If you'd rather initiate from your workstation:

```bash
ssh -L 8080:127.0.0.1:8080 admin@server
# Then open http://127.0.0.1:8080
```

### Reverse proxy with nginx

If you must expose TheWatcher beyond the loopback interface, put it behind
a reverse proxy with TLS:

```nginx
server {
    listen 443 ssl;
    server_name metrics.example.com;

    ssl_certificate     /etc/letsencrypt/live/metrics.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/metrics.example.com/privkey.pem;

    # Restrict to your management network
    allow 192.168.10.0/24;
    allow 10.0.0.0/8;
    deny all;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }
}
```

### Systemd sandbox hardening

The [systemd unit](#systemd) above includes basic sandbox options. For
stricter confinement, add:

```ini
# Read-only everywhere except the data directory
ReadOnlyPaths=/
ReadWritePaths=/var/lib/thewatcher
InaccessiblePaths=/boot /root /etc/ssh

# No device access
PrivateDevices=yes

# Isolated /tmp
PrivateTmp=yes

# Network-only, no IPC
RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6 AF_NETLINK
```

---

## Upgrading

1. Download the new binary for your platform from
   [GitHub Releases](https://github.com/patw/thewatcher/releases).
2. Stop the service.
3. Replace the binary.
4. Start the service.

TheWatcher uses MooFile for storage with deterministic document IDs,
so rollup and retention are safe across restarts and upgrades. No
database migrations needed.

```bash
# Typical upgrade on systemd:
sudo systemctl stop thewatcher
sudo cp thewatcher-linux-x86_64 /usr/local/bin/thewatcher
sudo chmod +x /usr/local/bin/thewatcher
sudo systemctl start thewatcher
sudo systemctl status thewatcher
```
