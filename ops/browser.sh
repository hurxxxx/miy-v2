#!/usr/bin/env bash
set -euo pipefail
rm -rf /root/.pki/nssdb
mkdir -p /root/.pki/nssdb
certutil -N --empty-password -d sql:/root/.pki/nssdb
certutil -A -d sql:/root/.pki/nssdb -n miy-local -t 'C,,' -i /miy-ca.crt
cp /miy-ca.crt /usr/local/share/ca-certificates/miy-local.crt
update-ca-certificates >/dev/null
gateway=$(getent ahostsv4 host.docker.internal | head -1 | awk '{print $1}')
browser=$(find /ms-playwright -name chrome -type f | head -1)
"$browser" --headless --no-sandbox --disable-dev-shm-usage --remote-debugging-port=9223 --user-data-dir=/tmp/miy-browser --host-resolver-rules="MAP localhost $gateway" >/tmp/chromium.log 2>&1 &
exec socat TCP-LISTEN:9222,fork,reuseaddr TCP:127.0.0.1:9223
