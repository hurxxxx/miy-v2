# 빈 서버에 처음 설치하기

**Ubuntu 24.04 LTS x86_64 전용 서버에 이 저장소를 clone한 뒤, PostgreSQL·Keycloak·Core·worker·포털·앱 프레임과 첫 앱까지 구성하는 절차다.** 기존 서버의 DB·사용자·앱을 옮기는 절차가 아니다. `v2.dwdcc.kr`의 파일·인증서·DB·CLI 게시물을 복사할 필요가 없다. §13~15는 이 안내로 설치한 서버의 최소 수동 관리 절차다.

명령은 새 서버의 **sudo 가능한 일반 사용자 한 명**으로 Bash에서 순서대로 실행한다. 이 사용자가 Core·worker의 서비스 계정이 된다. sudo/Docker 권한을 가진 계정으로 공개 요청을 받는 Core도 실행하는 개발 단계의 단순화이며, 서비스 권한을 분리한 구성은 아니다. root로 Cargo/pnpm을 실행하지 않는다. 이 저장소를 현재 운영하는 호스트에서는 아래 명령을 실행하지 않는다. 단계가 실패하면 그 원인을 해결한 뒤 이어가며, 볼륨을 지워 다시 시작하지 않는다.

**검증 상태:** 이 문서 전체를 빈 Ubuntu 서버에서 설치→로그인→첫 앱→재부팅까지 실행한 결과는 아직 없다. 이번 보완에 추가한 갱신·백업/복원 명령도 `NOT_RUN`이며 사용자가 별도 VM에서 인수한다. 이전의 구문·격리 설정 검사는 전체 설치 성공을 뜻하지 않는다. 최초 설치 뒤의 최소 운영 절차도 §12~15에 함께 둔다.

**현재 사설 CA의 범위:** 서버·브라우저·CLI와 기본 앱 기동은 아래 경로로 구성한다. 다만 사설 CA를 배포 앱 컨테이너와 SDK의 HTTP 클라이언트에 전달하는 경로는 아직 없으므로, 앱 백엔드가 호출하는 디렉터리·공통 파일·앱 간 API까지 사용하려면 보완이 필요하다. 이 공통 기능을 바로 사용할 서버는 **§3 B의 도메인 + 공개 CA 인증서**로 구성한다. 사설 CA 인수를 완료한 것으로 간주하거나 검증을 끄지 않는다. 이 한계는 [PLAN의 새 서버 인수](PLAN.md#검증과-반영-대기)에서 추적한다.

## 1. 준비할 것과 설치 결과

| 항목 | 이 안내의 기준 |
| --- | --- |
| 서버 | Ubuntu 24.04 LTS, x86_64, systemd·cgroup v2, 고정 IPv4. 작은 개발 설치의 시작 예산으로 4 vCPU·RAM 16GiB·디스크 100GiB를 권장하며 지원 규모 보장은 아님 |
| 빌드 여유 | Docker 저장 장치의 여유 **15GiB 이상이면서 15% 이상**. 소스·Rust target·Docker 빌드 캐시·DB는 계속 늘어남. Keycloak 컨테이너는 2GiB로 제한 |
| 네트워크 | 서버와 사용자 PC에서 서로 같은 공개 주소에 접근 가능. apt·GitHub·Rust·Node/npm·Docker Hub·quay.io·mcr.microsoft.com 등의 다운로드 가능 |
| 도구 | Git, Python 3.12, OpenSSL, Nginx, Docker Engine/CLI 28 이상과 Compose/Buildx, Node 22, 저장소가 고정한 Rust·pnpm |
| 소스 | `/opt/miy-v2`. 서비스 wrapper·컴포넌트 원본·정적 UI가 이 경로를 계속 사용하므로 설치 후 삭제하거나 옮기지 않음 |
| 비공개 상태 | `/srv/miy`(0700): 설정·비밀·실행 파일·프레임 런타임·앱 산출물/파일·선택적 CLI 게시물 |
| 데이터베이스 | Compose의 `miy-v2-validation_pgdata` 볼륨. 한 PostgreSQL 안의 `miy_core`, `miy_identity`와 이후 만들어질 앱/환경별 DB |
| 서비스 | Docker의 PostgreSQL·Keycloak, systemd의 `miy-v2-serve`·`miy-v2-worker`, Nginx |

이 안내는 호스트당 플랫폼 하나를 설치한다. Compose 프로젝트명·DB 컨테이너명·앱 네트워크명을 그대로 사용한다. 이름의 `validation`은 기존 기본 명칭이며 일회용 DB라는 뜻이 아니다. `docker compose -p ...`로 이름만 바꾸면 worker의 기본 DB 컨테이너와 달라진다.

업무 앱의 외부 통신은 플랫폼이 차단하지 않는다. 다른 시스템과 분리한 전용 호스트에서 시작한다. 공개 접근 범위는 사내망/VPN 등 설치자가 정하며, 외부에 열었다고 사내 신뢰 조건을 만족하는 것은 아니다. Docker가 공개한 포트는 UFW 규칙을 우회할 수 있으므로 DB·Keycloak·Core는 아래처럼 loopback으로만 공개한다. [Docker 설치 전 네트워크 조건](https://docs.docker.com/engine/install/ubuntu/)

## 2. OS 도구와 소스

아직 clone하지 않았다면:

```bash
sudo apt-get update
sudo apt-get install -y git ca-certificates curl python3 openssl
sudo install -d -o "$(id -un)" -g "$(id -gn)" -m 0755 /opt/miy-v2
git clone https://github.com/hurxxxx/miy-v2.git /opt/miy-v2
```

비공개 저장소라면 본인 GitHub의 Git 인증을 먼저 설정한다. 토큰을 clone URL에 넣지 않는다. 이미 다른 경로에 clone했다면 아래 `MIY_SETUP_ROOT`에 그 **절대 경로**를 넣고 Nginx가 상위 디렉터리를 통과할 수 있게 한다. 기본 경로를 쓰는 편이 쉽다.

대화형 SSH 셸 자체에 `set -e`를 설정하지 않는다. 먼저 아래 한 줄로 **설치용 자식 Bash**를 연 뒤 다음 블록을 실행한다. 명령 실패 시 자식 셸만 끝나고 SSH의 부모 셸로 돌아온다. 부모 셸까지 종료하지 말고 오류를 해결한 후 설치용 Bash를 다시 연다.

```bash
bash --noprofile --norc
```

```bash
set -euo pipefail
umask 077
export MIY_SETUP_ROOT=/opt/miy-v2
export MIY_STATE_DIR=/srv/miy
export MIY_SETUP_USER="$(id -un)"
cd "$MIY_SETUP_ROOT"
MIY_SETUP_COMMIT="$(git rev-parse HEAD)"
git status --short
git log -1 --oneline
sudo apt-get install -y build-essential pkg-config libssl-dev clang cmake \
  gnupg xz-utils unzip jq nginx
sudo install -d -o "$MIY_SETUP_USER" -g "$(id -gn)" -m 0700 "$MIY_STATE_DIR"
test ! -e "$MIY_STATE_DIR/credentials.json"
test "$(uname -m)" = x86_64
stat -fc %T /sys/fs/cgroup
timedatectl status
```

`cgroup2fs`와 정상 시각 동기화를 확인한다. 시작 시 작업 트리가 깨끗해야 하며 설치 중 다른 소스로 바꾸지 않는다. `credentials.json`이 이미 있다면 새 설치가 아니거나 중간에 다시 접속한 것이므로 이 절을 처음부터 덮어쓰지 말고 마지막 성공 단계부터 재개한다.

### Docker

빈 Ubuntu에 공식 저장소를 추가한다. 이미 다른 Docker가 설치돼 있다면 이 문서의 빈 서버 전제가 맞는지 먼저 확인한다. [공식 Ubuntu 설치 절차](https://docs.docker.com/engine/install/ubuntu/)

```bash
sudo install -d -m 0755 /etc/apt/keyrings
sudo curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc
sudo chmod 0644 /etc/apt/keyrings/docker.asc
sudo tee /etc/apt/sources.list.d/docker.sources >/dev/null <<'EOF'
Types: deb
URIs: https://download.docker.com/linux/ubuntu
Suites: noble
Components: stable
Architectures: amd64
Signed-By: /etc/apt/keyrings/docker.asc
EOF
sudo apt-get update
sudo apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
sudo systemctl enable --now docker
sudo usermod -aG docker "$MIY_SETUP_USER"
```

여기서 **SSH를 끊고 같은 사용자로 다시 접속**해 Docker 그룹을 적용한다. Docker 그룹은 호스트 관리 권한에 준하므로 이 서버 운영 계정에만 부여한다. 다시 자식 Bash를 연 뒤 변수를 지정한다. §5에서 설치 입력을 저장한 이후의 재접속은 아래 세 변수만 지정하는 대신 **§5의 재개 블록**을 사용한다.

```bash
bash --noprofile --norc
```

```bash
set -euo pipefail
umask 077
export MIY_SETUP_ROOT=/opt/miy-v2
export MIY_STATE_DIR=/srv/miy
export MIY_SETUP_USER="$(id -un)"
cd "$MIY_SETUP_ROOT"
MIY_SETUP_COMMIT="$(git rev-parse HEAD)"
docker version
docker compose version
docker buildx version
docker info --format 'cgroup={{.CgroupVersion}} root={{.DockerRootDir}}'
df -h / /var/lib/docker "$MIY_STATE_DIR"
```

`sudo` 없이 Docker가 동작하고 Engine·CLI가 모두 28 이상인지 확인한다. 다른 Docker data root를 사용한다면 `df`도 해당 경로로 확인한다. 서버 빌드·worker는 이 사용자의 Docker 권한을 사용한다.

### Rust·Node·pnpm

Rust는 [공식 rustup](https://rustup.rs/)으로 설치하고 저장소의 `rust-toolchain.toml`을 사용한다. Node는 [공식 22.x 배포 파일](https://nodejs.org/download/release/latest-v22.x/)과 해당 SHA-256을 사용한다. 임시 디렉터리에는 공개 설치 파일만 둔다.

```bash
MIY_SETUP_TOOLS="$(mktemp -d)"
curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs -o "$MIY_SETUP_TOOLS/rustup.sh"
sh "$MIY_SETUP_TOOLS/rustup.sh" -y --profile minimal --default-toolchain none
source "$HOME/.cargo/env"
MIY_SETUP_RUST="$(python3 -c 'import tomllib; print(tomllib.load(open("rust-toolchain.toml", "rb"))["toolchain"]["channel"])')"
rustup toolchain install "$MIY_SETUP_RUST" --profile minimal --component rustfmt --component clippy
rustup show
rustc --version
cargo --version

curl -fsSL https://nodejs.org/download/release/latest-v22.x/SHASUMS256.txt \
  -o "$MIY_SETUP_TOOLS/SHASUMS256.txt"
MIY_SETUP_NODE_ARCHIVE="$(awk '$2 ~ /^node-v22\.[0-9]+\.[0-9]+-linux-x64\.tar\.xz$/ {print $2}' "$MIY_SETUP_TOOLS/SHASUMS256.txt")"
test -n "$MIY_SETUP_NODE_ARCHIVE"
curl -fsSL "https://nodejs.org/download/release/latest-v22.x/$MIY_SETUP_NODE_ARCHIVE" \
  -o "$MIY_SETUP_TOOLS/$MIY_SETUP_NODE_ARCHIVE"
awk -v name="$MIY_SETUP_NODE_ARCHIVE" '$2 == name' "$MIY_SETUP_TOOLS/SHASUMS256.txt" \
  > "$MIY_SETUP_TOOLS/node.sha256"
(cd "$MIY_SETUP_TOOLS" && sha256sum -c node.sha256)
sudo tar -xJf "$MIY_SETUP_TOOLS/$MIY_SETUP_NODE_ARCHIVE" -C /opt
MIY_SETUP_NODE_DIR="/opt/${MIY_SETUP_NODE_ARCHIVE%.tar.xz}"
for MIY_SETUP_TOOL in node npm npx; do
  sudo ln -sfn "$MIY_SETUP_NODE_DIR/bin/$MIY_SETUP_TOOL" "/usr/local/bin/$MIY_SETUP_TOOL"
done
MIY_SETUP_PNPM="$(python3 -c 'import json; print(json.load(open("package.json"))["packageManager"])')"
sudo /usr/local/bin/npm install --global --prefix /usr/local "$MIY_SETUP_PNPM"
node --version
pnpm --version
```

현재 소스는 Rust 1.99.1·pnpm 10.34.5다. 설치 소스의 고정 버전이 기준이며 Node는 22.x여야 한다. Rust/pnpm 빌드를 위한 서비스 계정의 홈과 도구 경로를 설치 도중 바꾸지 않는다.

## 3. 공개 주소 선택

아래 **A 또는 B 하나만** 실행한다. 모든 주소는 사용자 PC뿐 아니라 서버 자신에서도 접근 가능해야 한다. **앱 컨테이너에서도 업무 포털 주소로 HTTPS 연결**이 가능해야 한다. 앱 백엔드는 dev 설치라도 이 공개 포털 주소로 디렉터리·파일·앱 간 API를 호출한다. 포트·이름은 인증서 발급 전에 정한다. 내부 포트는 다음 절의 기본값을 사용한다.

### A. 도메인 없이 고정 IP 하나

`192.168.10.20`을 실제 서버 IP로 바꾼다. IP만으로 사용할 수 있으며 내부 DNS가 필요 없다.

```bash
export MIY_SETUP_IP=192.168.10.20
export MIY_SETUP_PORTAL="https://$MIY_SETUP_IP:8443"
export MIY_SETUP_CONSOLE="https://$MIY_SETUP_IP:9443"
export MIY_SETUP_DEV="https://$MIY_SETUP_IP:9444"
export MIY_SETUP_FRAMES="https://$MIY_SETUP_IP:{port}"
export MIY_SETUP_FRAME_PORTS=20000-20039
export MIY_SETUP_SAN="IP:$MIY_SETUP_IP"
```

방화벽에서 사용자망→서버의 TCP **8443·9443·9444·20000~20039**와 관리자의 SSH 포트를 허용한다. 앱 포트 하나가 설치 하나를 담당하며 dev/prod는 별도 설치다. 이 예시 풀은 최대 40개 설치용 주소이며 앱 실행 용량 보장은 아니다. 다른 서비스가 이 포트들을 사용하면 안 된다. 포트를 나누면 DOM 출처는 분리되지만 같은 호스트의 쿠키 저장소는 분리되지 않는다.

### B. 도메인과 와일드카드 DNS

`miy.example.com`을 본인이 관리하는 실제 도메인으로 바꾼다. DNS에 `portal`, `console`, `dev`, `*.miy.example.com`을 서버 IP로 연결한다. 서버 내부에서도 같은 주소로 Nginx에 연결돼야 한다. 잘못된 AAAA 레코드는 제거하거나 실제 IPv6 연결을 별도로 구성한다.

```bash
export MIY_SETUP_DOMAIN=miy.example.com
export MIY_SETUP_PORTAL="https://portal.$MIY_SETUP_DOMAIN"
export MIY_SETUP_CONSOLE="https://console.$MIY_SETUP_DOMAIN"
export MIY_SETUP_DEV="https://dev.$MIY_SETUP_DOMAIN"
export MIY_SETUP_FRAMES="https://app-{key}.$MIY_SETUP_DOMAIN"
export MIY_SETUP_FRAME_PORTS=''
export MIY_SETUP_SAN="DNS:*.$MIY_SETUP_DOMAIN"
```

사용자망→서버 TCP **443**과 관리자의 SSH 포트를 허용한다. `*.miy.example.com` 인증서는 위 세 플랫폼 호스트와 `app-<키>.miy.example.com`을 함께 포함한다. `*.example.com`은 이 이름들을 포함하지 않는다. 이 절의 같은 등록 도메인 배치는 앱의 상위 도메인 쿠키 간섭까지 막지는 않는다.

두 방식 모두 외부에 **15432(PostgreSQL)·18181(Keycloak)·18080~18083(Core)**를 열지 않는다. 출발지를 제한하는 방화벽은 사용자망 외에 **이 플랫폼이 생성한 앱 Docker bridge 대역→공개 포털 HTTPS 포트**도 허용한다. 임의의 모든 사설 대역을 허용하지 말고 실제 네트워크 대역을 확인한다. 설치마다 생성되는 bridge에도 규칙이 적용되어야 한다.

공용 IP/NAT 환경에서는 서버와 **앱 컨테이너 양쪽**에서 공개 주소로 되돌아오는 경로가 필요하다. NAT loopback 또는 컨테이너에도 적용되는 내부 DNS로 해결한다. 호스트 `/etc/hosts`만 고쳐서는 컨테이너 DNS가 바뀌지 않는다. SDK는 환경의 HTTP proxy를 사용하지 않으므로 proxy 설정만으로 해결하지 않는다. §9에서 실제 앱의 공통 API 호출을 별도로 확인한다. 앱 포트·와일드카드 DNS 누락은 포털 로그인 성공만으로 발견되지 않는다.

## 4. TLS 인증서와 단말 신뢰

```bash
install -d -m 0700 "$MIY_STATE_DIR/tls"
export MIY_SETUP_CERT="$MIY_STATE_DIR/tls/server-chain.pem"
export MIY_SETUP_KEY="$MIY_STATE_DIR/tls/server.key"
```

### 이미 발급받은 인증서가 있다면

CA/기관에서 받은 서버 인증서(leaf 뒤에 intermediate를 붙인 PEM)와 암호화되지 않은 서비스용 키를 위 두 경로로 **안전한 파일 전송**으로 넣는다. 회사 CA라면 root 인증서를 `root-ca.crt`로 넣고 `MIY_SETUP_CA`를 지정한다. 공개 CA면 빈 값이다.

```bash
# 회사 CA일 때
export MIY_SETUP_CA="$MIY_STATE_DIR/tls/root-ca.crt"
# 공개 CA일 때는 위 줄 대신: export MIY_SETUP_CA=''
chmod 0600 "$MIY_SETUP_KEY"
chmod 0644 "$MIY_SETUP_CERT"
```

공개 와일드카드 인증서는 해당 DNS 제공자의 DNS-01 등으로 발급받을 수 있다. 발급 전부터 설치를 진행하려면 다음 사설 CA 경로를 사용한다.

### 인증서도 없는 사내 설치라면

다음은 앞에서 고른 IP SAN 또는 와일드카드 SAN을 가진 사설 인증서를 만든다. 이미 반입한 키에 덮어쓰지 않는다. CA 개인키는 이 서버의 정상 실행에 필요 없으며 발급 후 별도 안전한 보관소로 옮겨 관리한다. 사용자에게 전달할 것은 **root-ca.crt만**이다.

```bash
test ! -e "$MIY_SETUP_KEY"
export MIY_SETUP_CA="$MIY_STATE_DIR/tls/root-ca.crt"
(
  umask 077
  cd "$MIY_STATE_DIR/tls"
  openssl req -x509 -newkey rsa:3072 -nodes -sha256 -days 3650 \
    -keyout root-ca.key -out root-ca.crt -subj '/CN=MIY private installation CA' \
    -addext 'basicConstraints=critical,CA:TRUE' \
    -addext 'keyUsage=critical,keyCertSign,cRLSign'
  openssl req -new -newkey rsa:2048 -nodes -sha256 \
    -keyout server.key -out server.csr -subj '/CN=MIY server'
  cat > server.ext <<EOF
subjectAltName=$MIY_SETUP_SAN
basicConstraints=critical,CA:FALSE
keyUsage=critical,digitalSignature,keyEncipherment
extendedKeyUsage=serverAuth
subjectKeyIdentifier=hash
authorityKeyIdentifier=keyid,issuer
EOF
  openssl x509 -req -in server.csr -CA root-ca.crt -CAkey root-ca.key \
    -CAcreateserial -out server-chain.pem -days 365 -sha256 -extfile server.ext
  chmod 0644 root-ca.crt server-chain.pem
)
```

사설 CA면 서버 OS에도 신뢰를 등록한다. [Ubuntu CA 신뢰 절차](https://documentation.ubuntu.com/server/how-to/security/install-a-root-ca-certificate-in-the-trust-store/)

```bash
MIY_SETUP_CA_ARGS=()
if [ -n "$MIY_SETUP_CA" ]; then
  sudo install -m 0644 "$MIY_SETUP_CA" /usr/local/share/ca-certificates/miy-root-ca.crt
  sudo update-ca-certificates
  MIY_SETUP_CA_ARGS=(--ca-file "$MIY_SETUP_CA")
fi
```

사용자 PC도 로그인 전에 신뢰를 설정한다. Ubuntu는 위 OS 절차를 해당 PC에서, Windows는 root-ca.crt를 현재 사용자 또는 컴퓨터의 **신뢰할 수 있는 루트 인증 기관**, macOS는 키체인 접근의 신뢰된 인증서로 등록한다. Firefox 등 별도 저장소를 쓰는 브라우저는 인증서 설정의 기관에도 반입한다. 루트 인증서 지문은 `openssl x509 -in root-ca.crt -noout -fingerprint -sha256`으로 전달본과 대조한다. 인증서 경고를 무시하거나 `curl -k`로 통과시키지 않는다. CLI의 CA 설정은 §9에서 별도로 한다.

## 5. 비공개 설정과 첫 관리자

```bash
cd "$MIY_SETUP_ROOT"
MIY_SETUP_BOOTSTRAP=(--profile server --state-dir "$MIY_STATE_DIR"
  --origin "$MIY_SETUP_PORTAL" --console-origin "$MIY_SETUP_CONSOLE"
  --dev-portal-origin "$MIY_SETUP_DEV" --app-frame-origin "$MIY_SETUP_FRAMES"
  --bind 127.0.0.1:18080 --console-bind 127.0.0.1:18081
  --dev-portal-bind 127.0.0.1:18082 --app-frame-bind 127.0.0.1:18083
  --postgres-port 15432 --identity-port 18181 --no-shared-development-login)
if [ -n "$MIY_SETUP_FRAME_PORTS" ]; then
  MIY_SETUP_BOOTSTRAP+=(--app-frame-ports "$MIY_SETUP_FRAME_PORTS")
fi
# 비밀값이 아닌 설치 입력만 저장한다. Bash declare가 값과 배열을 인용한다.
test ! -e "$MIY_STATE_DIR/setup-context.sh"
(
  umask 077
  set -o noclobber
  declare -p MIY_SETUP_ROOT MIY_STATE_DIR MIY_SETUP_USER MIY_SETUP_COMMIT \
    MIY_SETUP_PORTAL MIY_SETUP_CONSOLE MIY_SETUP_DEV MIY_SETUP_FRAMES \
    MIY_SETUP_FRAME_PORTS MIY_SETUP_SAN MIY_SETUP_CERT MIY_SETUP_KEY \
    MIY_SETUP_CA MIY_SETUP_CA_ARGS MIY_SETUP_BOOTSTRAP \
    > "$MIY_STATE_DIR/setup-context.sh"
)
python3 scripts/bootstrap.py "${MIY_SETUP_BOOTSTRAP[@]}" "${MIY_SETUP_CA_ARGS[@]}"
```

**중단 후 재개:** §5의 입력 저장까지 성공했다면 재접속·자식 셸 종료·재부팅 후 아래 두 블록을 순서대로 실행한다. 기본 경로를 바꿨다면 `source` 경로도 바꾼다. 이 파일은 본인이 생성한 0600 파일만 사용하고, 전달받은 셸 파일을 실행하지 않는다. 암호나 `runtime.env` 전체를 이 파일에 추가하지 않는다.

```bash
bash --noprofile --norc
```

```bash
set -euo pipefail
umask 077
source /srv/miy/setup-context.sh
source "$HOME/.cargo/env"
export PATH="$HOME/.local/bin:$PATH"
export MIY_BINARY="$MIY_STATE_DIR/bin/miy-server"
cd "$MIY_SETUP_ROOT"
test "$(git rev-parse HEAD)" = "$MIY_SETUP_COMMIT"
```

저장 전 중단했다면 §2의 도구/소스 변수와 §3~4의 실제 선택값을 다시 지정한다. 이미 만든 인증서를 새로 발급하거나 이미 초기화된 DB의 첫 암호 설정을 반복하지 않는다. 저장 이후에는 **마지막 성공 단계 다음부터** 재개한다. `setup-context.sh` 생성 블록은 최초 한 번만 실행한다.

처음 로그인할 두 관리자 암호를 **첫 DB/Keycloak 기동 전에 한 번만** 지정한다. `admin`은 miy 플랫폼 관리자, `bootstrap`은 Keycloak의 master realm 관리자다. 서로 다른 계정이며 서로 다른 암호를 쓴다. 아래 입력은 터미널에 표시되지 않는다. 초기 env 파일과의 호환을 위해 영문·숫자·`._-`로 된 16~128자리 암호를 사용하고 암호 관리자에 보관한다.

```bash
python3 - <<'PY'
import getpass, json, os, re
from pathlib import Path
path = Path(os.environ['MIY_STATE_DIR']) / 'credentials.json'
values = json.loads(path.read_text())
for key, label in [('admin', 'miy admin'), ('identity_admin', 'Keycloak bootstrap')]:
    first = getpass.getpass(label + ' password: ')
    second = getpass.getpass('Confirm: ')
    if first != second or not re.fullmatch(r'[A-Za-z0-9._-]{16,128}', first):
        raise SystemExit('Passwords differ or do not match the initial password rule; nothing saved.')
    values[key] = first
if values['admin'] == values['identity_admin']:
    raise SystemExit('Use different passwords; nothing saved.')
temporary = path.with_suffix('.new')
with open(temporary, 'x', opener=lambda p, flags: os.open(p, flags, 0o600)) as output:
    json.dump(values, output, indent=2)
temporary.replace(path)
print('Initial passwords saved privately; regenerate configuration before starting containers.')
PY
python3 scripts/bootstrap.py "${MIY_SETUP_BOOTSTRAP[@]}" "${MIY_SETUP_CA_ARGS[@]}"
```

마지막 bootstrap 재실행이 `compose.env`와 `realm.json`을 선택한 암호로 맞춘다. **컨테이너를 먼저 시작한 뒤 이 파일을 고쳐도 기존 계정 암호는 바뀌지 않는다.** 초기 realm import는 빈 DB에서만 적용된다. 이후 암호 변경은 Keycloak에서 한다. `credentials.json`·`runtime.env`·`compose.env`·`realm.json`·`init.sql`에는 비밀이 있으므로 출력·Git 추가·상담 첨부를 하지 않는다. `docker compose config`도 펼친 비밀을 출력하므로 쓰지 않는다.

bootstrap/seed는 고정 ID의 `admin`, `developer`, `alice`, `bob`을 만든다. 나머지 세 계정은 무작위 암호이며 공유 개발 로그인은 꺼져 있다. 설정 동기화가 developer ID를 참조하므로 삭제하지 않는다. §8의 첫 로그인 뒤 miy 콘솔에서 세 예시 계정을 **사용 중지**하고, 아래 계정 관리 절차로 Keycloak에서도 비활성화한다. 실제 직원은 별도로 등록한다.

## 6. PostgreSQL·Keycloak 초기화

```bash
cd "$MIY_SETUP_ROOT"
docker compose --env-file "$MIY_STATE_DIR/compose.env" up -d --wait --wait-timeout 120 postgres
# 첫 초기화의 임시 Unix socket 서버와 최종 TCP 준비를 구분한다.
MIY_SETUP_DATABASE_READY=0
for MIY_SETUP_TRY in $(seq 1 120); do
  if docker compose --env-file "$MIY_STATE_DIR/compose.env" exec -T postgres \
    pg_isready -h 127.0.0.1 -U postgres >/dev/null 2>&1; then
    MIY_SETUP_DATABASE_READY=1
    break
  fi
  sleep 1
done
test "$MIY_SETUP_DATABASE_READY" = 1
docker compose --env-file "$MIY_STATE_DIR/compose.env" up -d keycloak
# 문서만 반영된 소스에서도 재부팅 정책을 명시적으로 설정한다.
docker update --restart unless-stopped \
  "$(docker compose --env-file "$MIY_STATE_DIR/compose.env" ps -q postgres)" \
  "$(docker compose --env-file "$MIY_STATE_DIR/compose.env" ps -q keycloak)"

# Keycloak은 Compose의 started 상태와 별도로 HTTP 준비를 기다린다.
MIY_SETUP_IDENTITY_READY=0
for MIY_SETUP_TRY in $(seq 1 180); do
  if curl -fsS --max-time 3 \
    http://127.0.0.1:18181/identity/realms/miy/.well-known/openid-configuration >/dev/null 2>&1; then
    MIY_SETUP_IDENTITY_READY=1
    break
  fi
  sleep 1
done
test "$MIY_SETUP_IDENTITY_READY" = 1
python3 scripts/configure-identity.py
docker compose --env-file "$MIY_STATE_DIR/compose.env" ps
docker compose --env-file "$MIY_STATE_DIR/compose.env" exec -T postgres \
  psql -U postgres -d postgres -c "SELECT datname FROM pg_database WHERE datname IN ('miy_core','miy_identity') ORDER BY datname;"
```

출력에 `miy_core`·`miy_identity`가 있어야 한다. PostgreSQL 이미지는 첫 기동에 `init.sql`로 DB와 전용 역할을 만들고 Keycloak은 realm·사용자를 가져온다. 이 단계에서는 miy 테이블이 아직 없으며 다음 절에서 migration/seed를 한다. 서버에서는 Compose의 `proxy` 서비스(Caddy)를 시작하지 않는다. 외부 HTTPS는 Nginx가 담당한다.

위 `docker update`가 PostgreSQL·Keycloak에 `unless-stopped`를 적용해 호스트 재부팅 후 다시 시작하게 한다. Compose로 컨테이너를 재생성하면 이 정책을 다시 적용한다(해당 소스의 Compose가 같은 정책을 선언한 경우에는 중복 적용해도 된다). 수동 `docker stop`/`compose stop` 상태는 유지되므로 정상 사용 전 `up -d`로 다시 켠다. **`docker compose down -v`는 모든 DB를 삭제하므로 설치 문제 해결에 쓰지 않는다.**

## 7. Core·웹·프레임 빌드와 서비스 설치

새 서버에서만 아래 기본 포털 빌드를 실행한다. 기존 운영 호스트의 `web/platform/dist`는 이 절로 덮어쓰지 않는다.

```bash
cd "$MIY_SETUP_ROOT"
export CARGO_BUILD_JOBS=2
cargo build --release --locked -p miy
install -D -m 0755 target/release/miy "$MIY_STATE_DIR/bin/miy-server"
export MIY_BINARY="$MIY_STATE_DIR/bin/miy-server"
bash scripts/run-service.sh migrate
bash scripts/run-service.sh seed

pnpm install --frozen-lockfile
pnpm --filter @miy/platform build
pnpm --filter @miy/frame-runtime exec vite build \
  --outDir "$MIY_STATE_DIR/frame-runtime" --emptyOutDir
test -s web/platform/dist/index.html
test -s "$MIY_STATE_DIR/frame-runtime/index.html"
chmod o+x "$MIY_SETUP_ROOT" "$MIY_SETUP_ROOT/web" "$MIY_SETUP_ROOT/web/platform"
chmod -R a+rX web/platform/dist
sudo -u www-data test -r "$MIY_SETUP_ROOT/web/platform/dist/index.html"

python3 scripts/check-server-tls.py --state-dir "$MIY_STATE_DIR" \
  --certificate "$MIY_SETUP_CERT" --key "$MIY_SETUP_KEY" "${MIY_SETUP_CA_ARGS[@]}"
python3 scripts/render-server-config.py --state-dir "$MIY_STATE_DIR" \
  --binary "$MIY_BINARY" --user "$MIY_SETUP_USER" --service-prefix miy-v2 \
  --certificate "$MIY_SETUP_CERT" --key "$MIY_SETUP_KEY" "${MIY_SETUP_CA_ARGS[@]}"
```

`migrate`가 현재 소스의 모든 코어 SQL을 적용하고 `seed`가 첫 관리자와 공용 UI registry를 게시한다. PostgreSQL을 호스트에 별도로 설치하거나 DB/테이블을 수작업으로 만들 필요가 없다. 프레임 빌드의 outDir는 반드시 절대 경로다. `@miy/platform build`만으로 프레임 런타임이 생기지는 않는다. `/srv/miy`를 Nginx가 읽게 하려고 0755로 열지 않는다. Nginx는 포털 정적 파일만 직접 읽고 프레임은 Core를 경유한다.

렌더러는 TLS SAN에 세 플랫폼 주소와 앱 출처를 검사한 뒤 설정 파일만 생성한다. 아래는 **빈 전용 Nginx**에 처음 설치하는 명령이다.

```bash
sudo install -m 0644 "$MIY_STATE_DIR/nginx-miy.conf" /etc/nginx/conf.d/miy.conf
if [ -L /etc/nginx/sites-enabled/default ]; then
  sudo unlink /etc/nginx/sites-enabled/default
fi
sudo nginx -t
sudo systemctl enable --now nginx
sudo systemctl reload nginx

# Core가 시작 시 읽는 public issuer를 TLS 검증을 켠 채 먼저 확인한다.
curl -fsS "$MIY_SETUP_CONSOLE/identity/realms/miy/.well-known/openid-configuration" \
  | jq -e --arg issuer "$MIY_SETUP_CONSOLE/identity/realms/miy" '.issuer == $issuer'

for MIY_SETUP_SERVICE in serve worker; do
  sudo install -m 0644 "$MIY_STATE_DIR/miy-v2-$MIY_SETUP_SERVICE.service" /etc/systemd/system/
  sudo install -d -m 0755 "/etc/systemd/system/miy-v2-$MIY_SETUP_SERVICE.service.d"
  sudo tee "/etc/systemd/system/miy-v2-$MIY_SETUP_SERVICE.service.d/proxy.conf" >/dev/null <<'EOF'
[Unit]
Wants=nginx.service
After=nginx.service
EOF
done
sudo systemctl daemon-reload
sudo systemctl enable --now miy-v2-serve miy-v2-worker
```

core/worker는 기존 unit의 `After/Requires=docker.service`, `Restart=on-failure`와 위 Nginx 순서를 사용한다. 재부팅 때 DB/IdP의 준비가 늦으면 실패 후 다시 시작한다. 첫 설치에서는 반드시 위 discovery 확인까지 성공한 후 시작한다. `services.py`·`up.sh`와 systemd를 동시에 사용하지 않는다. 서비스가 `/srv/miy/bin/miy-server`를 사용하므로 이후 CLI 빌드가 서버 실행 파일을 덮지 않는다.

## 8. 기동 확인과 첫 로그인

```bash
cd "$MIY_SETUP_ROOT"
sudo systemctl is-enabled miy-v2-serve miy-v2-worker nginx docker
MIY_SETUP_CORE_READY=0
for MIY_SETUP_TRY in $(seq 1 90); do
  if curl -fsS --max-time 3 "$MIY_SETUP_PORTAL/api/health" >/dev/null 2>&1 \
    && curl -fsS --max-time 3 "$MIY_SETUP_CONSOLE/api/health" >/dev/null 2>&1 \
    && curl -fsS --max-time 3 "$MIY_SETUP_DEV/api/health" >/dev/null 2>&1; then
    MIY_SETUP_CORE_READY=1
    break
  fi
  sleep 1
done
test "$MIY_SETUP_CORE_READY" = 1
sudo systemctl is-active miy-v2-serve miy-v2-worker nginx docker
python3 scripts/check-server-tls.py --state-dir "$MIY_STATE_DIR" \
  --certificate "$MIY_SETUP_CERT" --key "$MIY_SETUP_KEY" "${MIY_SETUP_CA_ARGS[@]}" --connect
curl -fsS "$MIY_SETUP_PORTAL/api/v1/cli/install" \
  | jq '{serverUrl,cliVersion,sourceCommit,downloads:(.downloads|length)}'
docker compose --env-file "$MIY_STATE_DIR/compose.env" exec -T postgres \
  psql -U postgres -d miy_core -c 'SELECT version,success FROM _sqlx_migrations ORDER BY version;'
```

세 HTTPS `/api/health`가 성공하고 migration의 `success`가 모두 참이어야 한다. 현재 소스는 schema 23이며 앞으로는 checkout의 migration 목록이 기준이다. CLI 게시 전 `sourceCommit: null`, `downloads: 0`은 정상이다. **0.10.0 게시물을 0.11.0 서버에 복사하면 버전 불일치가 되므로 기존 서버에서 가져오지 않는다.**

브라우저에서 다음을 확인한다.

1. **관리 콘솔**(§3의 `MIY_SETUP_CONSOLE`)에서 로그인하고 §5의 `admin`과 플랫폼 관리자 암호를 입력한다. 로그인 복귀 후 플랫폼 관리가 보여야 한다.
2. **업무 포털**과 **dev 포털**을 각각 열어 로그인/복귀를 확인한다. 같은 Keycloak 인증을 사용하지만 세션은 영역별이다. 아직 앱이 없는 빈 목록은 정상이다. 비밀번호 없는 공유 개발 로그인 버튼이 없어야 한다.
3. 아래 절차로 실제 직원을 추가한 뒤 그 계정으로도 로그인한다. 개발자는 miy 콘솔에서 `앱 개발 허용`을 켜고, 관리자 지정은 필요한 사람만 한다. 앱 사용 권한은 별도로 부여한다.

**직원 추가·암호 재설정·계정 중지:** 공개 프록시는 Keycloak 로그인 경로만 허용하며 master 관리 화면/API는 차단한다. 다음 블록은 새 서버의 대화형 SSH에서 내부 API를 사용한다. `create`는 새 직원, `find`는 정확한 로그인명으로 UUID 조회, `reset-password`는 임시 암호 재설정, `disable`/`enable`은 UUID로 로그인 계정 중지/재개다. 공개 프록시 제한을 풀 필요가 없다.

생성·재설정한 암호는 **첫 로그인 때 변경하도록** 지정한다. 입력값은 화면에 표시하지 않으며 직원에게 별도 안전한 방법으로 전달한다. **퇴사·계정 회수는 먼저 miy 콘솔에서 사용자를 중지**하고 이어서 `disable`을 실행한다. Keycloak만 중지하거나 암호만 재설정하는 것은 이미 발급된 miy CLI·플랫폼 자격증명의 회수를 대신하지 않는다. `enable`도 miy의 사용 중지를 자동 해제하지 않는다.

```bash
python3 - <<'PY'
import getpass, ipaddress, json, os, sys, uuid
from pathlib import Path
from urllib import error, parse, request
state = Path(os.environ['MIY_STATE_DIR'])
settings = json.loads((state / 'server.json').read_text())
base = settings['identityAdminUrl']
endpoint = parse.urlsplit(base)
if (endpoint.scheme != 'http' or not ipaddress.ip_address(endpoint.hostname).is_loopback
        or endpoint.path != '/identity' or endpoint.port is None
        or endpoint.username or endpoint.password or endpoint.query or endpoint.fragment):
    raise SystemExit('Expected the configured internal loopback identity endpoint.')
# The here-document supplies Python source on stdin; read answers from the TTY.
try:
    sys.stdin = open('/dev/tty', encoding='utf-8')
except OSError:
    raise SystemExit('Run this step in an interactive SSH terminal.') from None
action = input('Action [create/find/reset-password/disable/enable]: ').strip()
if action not in {'create', 'find', 'reset-password', 'disable', 'enable'}:
    raise SystemExit('Unknown action; nothing changed.')
if action in {'create', 'find'}:
    username = input('Exact employee login name: ').strip()
    if not username:
        raise SystemExit('Login name required; nothing changed.')
else:
    user_id = str(uuid.UUID(input('Existing miy realm user UUID: ').strip()))
if action == 'create':
    first = input('First name: ').strip()
    last = input('Last name: ').strip()
    email = input('Email: ').strip()
    if not all([first, last, email]):
        raise SystemExit('Required fields missing; nothing changed.')
if action in {'create', 'reset-password'}:
    password = getpass.getpass('Temporary employee password (16+ characters): ')
    if (not 16 <= len(password) <= 1024 or any(c in password for c in '\r\n\0')
            or password != getpass.getpass('Confirm password: ')):
        raise SystemExit('Invalid or mismatched password; nothing changed.')
class NoRedirect(request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None
opener = request.build_opener(request.ProxyHandler({}), NoRedirect())
try:
    secrets = json.loads((state / 'credentials.json').read_text())
    body = parse.urlencode({'client_id': 'admin-cli', 'grant_type': 'password',
                           'username': 'bootstrap', 'password': secrets['identity_admin']}).encode()
    with opener.open(request.Request(base + '/realms/master/protocol/openid-connect/token', data=body), timeout=15) as response:
        token = json.load(response)['access_token']
    headers = {'Authorization': 'Bearer ' + token, 'Content-Type': 'application/json'}
    def api(path='', method='GET', value=None):
        call = request.Request(base + '/admin/realms/miy/users' + path,
            data=json.dumps(value).encode() if value is not None else None,
            method=method, headers=headers)
        return opener.open(call, timeout=15)
    if action == 'find':
        query = parse.urlencode({'username': username, 'exact': 'true'})
        with api('?' + query) as response:
            users = json.load(response)
        for user in users:
            print(json.dumps({k: user[k] for k in ('id', 'username', 'enabled')}))
        if not users:
            print('No exact matching user.')
    elif action == 'create':
        user = {'username': username, 'firstName': first, 'lastName': last, 'email': email,
                'enabled': True, 'credentials': [
                    {'type': 'password', 'value': password, 'temporary': True}]}
        with api(method='POST', value=user) as response:
            user_id = str(uuid.UUID(response.headers['Location'].rstrip('/').rsplit('/', 1)[-1]))
        print('Created identity user UUID:', user_id)
    else:
        # Confirm the target before a mutation; only the miy realm is addressed.
        with api('/' + user_id) as response:
            user = json.load(response)
        print('Target:', user_id, user['username'])
        if input('Type the exact login name to confirm: ').strip() != user['username']:
            raise SystemExit('Confirmation mismatch; nothing changed.')
        suffix = '/' + user_id
        if action == 'reset-password':
            suffix += '/reset-password'
            value = {'type': 'password', 'value': password, 'temporary': True}
        else:
            value = {'enabled': action == 'enable'}
        with api(suffix, method='PUT', value=value):
            pass
        print('Identity action completed:', action, user_id)
except error.HTTPError as failure:
    raise SystemExit(f'Identity HTTP {failure.code}; no token or response body printed. 409 means the user already exists.') from None
except (error.URLError, TimeoutError, ConnectionError):
    raise SystemExit('Identity response unavailable. A mutation may have completed; inspect the account before retrying. No credentials printed.') from None
PY
```

생성 후 miy 콘솔 **플랫폼 관리 → 사용자 계정**에서 출력된 **UUID**와 표시 이름을 등록한다. Keycloak에만 있으면 miy에 로그인할 수 없다. 첫 로그인 때 직원이 임시 암호를 자기 암호로 변경하는지 확인한다. 같은 이름의 409 오류나 응답 유실은 먼저 `find`로 UUID를 확인하며 기존 사용자를 덮어쓰거나 재생성하지 않는다. 예시 계정 `developer`·`alice`·`bob`도 `find`로 UUID를 찾은 뒤 두 시스템에서 중지한다.

플랫폼 로그인에는 `admin`, 내부 Keycloak 관리에는 `bootstrap`을 사용한다. 위 도구는 **miy realm만** 관리하며 master의 `bootstrap` 암호 변경/분실 복구 도구가 아니다. bootstrap 암호는 설치 시 암호 관리자에 보관한다. 변경이 필요하면 로컬 Keycloak 관리 작업과 비공개 `identity_admin` 값을 함께 갱신하고, `credentials.json`만 바꾸어 계정 암호가 바뀐 것으로 취급하지 않는다. 비밀값을 찾기 위해 설정 파일을 출력하지 않는다.

## 9. 같은 소스의 CLI와 첫 앱

전체 OS용 다운로드 릴리스가 없어도 첫 앱을 만들 수 있다. 우선 서버의 현재 일반 사용자에게 같은 소스의 Linux CLI를 설치한다. 이 검사는 서버와 같은 PC에서 개발하는 경로이며 실제 개발 PC/Windows 인수는 별개다.

```bash
cd "$MIY_SETUP_ROOT"
cargo build --release --locked --no-default-features -p miy \
  --target-dir "$MIY_STATE_DIR/cli-build"
bash scripts/install-cli.sh --from "$MIY_STATE_DIR/cli-build/release/miy" --no-input
export PATH="$HOME/.local/bin:$PATH"
miy --version
miy context add new-server --url "$MIY_SETUP_PORTAL" "${MIY_SETUP_CA_ARGS[@]}"
miy context use new-server
miy auth login --no-open
miy auth status
```

SSH에서는 CLI가 표시한 승인 주소를 사용자 PC 브라우저에서 연다. 관리자 `admin`으로 로그인해 요청 코드를 대조하고 승인한다. Unix 설치기에는 `--ca-file` 옵션이 없으므로 context에 지정한다. 이 CA 파일은 CLI가 계속 읽을 수 있는 절대 경로에 보관한다. 이미 `new-server` context가 있다면 다른 이름을 쓰고 그 이름을 선택한다.

파이프/비대화형 환경에서 로그인 결과가 `pending`이면 승인 뒤 출력된 login ID로 `miy auth wait LOGIN_ID`를 실행하고 `auth status`를 다시 확인한다.

첫 연결 검사는 관리자 계정 하나로 수행할 수 있다. 이후 앱 개발은 §8에서 추가한 개발 계정으로 진행한다.

```bash
mkdir -p "$HOME/miy-apps"
miy --context new-server app init "$HOME/miy-apps/setup-check" --id setup-check --name '설치 확인'
cd "$HOME/miy-apps/setup-check"
.miy-tools/miy component sync
.miy-tools/miy app register
miy registration list
```

방금 나온 **등록 요청 UUID**를 사용한다. 문자열 `REGISTRATION_UUID` 자체를 입력하지 않는다.

```bash
miy registration approve REGISTRATION_UUID
git init
# 아래 이름·이메일은 자신의 Git 작성자 정보로 바꾼다.
git config user.name '설치 담당자'
git config user.email 'operator@example.com'
git add .
git commit -m 'Initialize setup check application'
.miy-tools/miy app build --timeout 2400
.miy-tools/miy app deploy --environment dev --dry-run
.miy-tools/miy app deploy --environment dev
.miy-tools/miy app status
```

성공하면 dev 포털의 **`/environments/dev/apps/setup-check`**를 열어 앱 화면과 ping 응답을 확인한다. 이 단계에서 실제 앱 DB 생성·접속·migration·제한된 DB 역할·서버 Docker 빌드·worker·설치별 프레임 출처가 연결된다. 기본 템플릿은 업무 레코드 CRUD를 제공하지 않으므로 업무 데이터 저장/복원 검사는 별도 앱에서 한다. 브라우저의 앱 요청 호스트/포트가 §3의 앱 출처이고 콘솔 출처가 아닌지 확인한다. 플랫폼 세 출처의 health만으로 이 검사를 대체하지 않는다.

**공통 API까지 사용할 서버:** 기본 ping은 앱→Core HTTPS 호출을 하지 않는다. 따라서 디렉터리 조회나 파일 기능을 사용하는 시험 앱에서도 한 번 호출하고, 해당 설치의 컨테이너 DNS·공개 포털 포트 접근·TLS 신뢰를 확인한다. 앱 네트워크의 주소 대역은 `docker network ls --filter name=miy-runtime-` 후 해당 이름을 넣은 `docker network inspect --format '{{json .IPAM.Config}}' NETWORK_NAME`으로 확인할 수 있다. 컨테이너 전체 inspect에는 비밀 환경이 포함될 수 있으므로 공유하지 않는다. 사설 CA 배치는 앞서 밝힌 SDK 전달 미구현 때문에 이 인수를 통과한 것으로 기록하지 않는다.

등록 승인은 요청자에게 `develop@dev`를 부여한다. prod 사용/배포는 별도 권한이며 이 설치 검사는 자동으로 prod에 승격하지 않는다. 일반 사용자를 시험하려면 관리자 CLI에서 `miy user list`로 ID를 확인하고 `miy --app setup-check grant set --user USER_ID --permission use --environment dev` 후 그 계정으로 dev 포털에 접속한다.

## 10. 개발 PC용 CLI 다운로드 게시 — 선택

§9까지는 GitHub Actions/Release 권한 없이 가능하다. 여러 PC에 소스 빌드 없이 설치하거나 `app dev --runtime docker`의 DB tunnel helper를 제공하려면 **동일 소스의 전체 CLI aggregate**를 게시한다. 브라우저의 `/cli/install`은 그 후 설치 명령을 제공한다. Windows GUI EXE는 추가 선택이고 ZIP/PowerShell만으로도 설치할 수 있다.

GitHub CLI가 설치되고 해당 저장소 Actions 실행 권한이 있는 작업 PC에서 후보를 만든다. 이 선택 단계에는 Python 3.11 이상도 필요하다. 이 명령은 GitHub Release를 공개하지 않는다. 작업 PC의 실제 clone 경로와 **서버 설치 시 기록한 40자리 commit**을 넣는다. 현재 PC의 HEAD를 대신 사용하지 않는다.

```bash
cd /absolute/path/to/miy-v2
MIY_SETUP_COMMIT=SERVER_INSTALL_40_HEX_COMMIT
MIY_SETUP_VERSION="$(git show "$MIY_SETUP_COMMIT:crates/miy/Cargo.toml" \
  | python3 -c 'import sys,tomllib; print(tomllib.loads(sys.stdin.read())["package"]["version"])')"
gh workflow run cli-release.yml --repo hurxxxx/miy-v2 --ref main \
  -f candidate_commit="$MIY_SETUP_COMMIT" -f release_tag="v$MIY_SETUP_VERSION" -F publish=false
gh run list --repo hurxxxx/miy-v2 --workflow cli-release.yml --limit 10
```

실행 입력 commit이 일치하는 run의 모든 target·aggregate 성공을 확인하고 그 run ID를 넣는다. artifact가 만료되었다면 같은 commit으로 후보 CI를 다시 실행한다.

```bash
export MIY_SETUP_CLI_RUN_ID=RUN_ID
export MIY_SETUP_AGGREGATE="$HOME/miy-cli-candidate-$MIY_SETUP_COMMIT"
gh run watch "$MIY_SETUP_CLI_RUN_ID" --repo hurxxxx/miy-v2 --exit-status
gh run download "$MIY_SETUP_CLI_RUN_ID" --repo hurxxxx/miy-v2 \
  --name "miy-${MIY_SETUP_VERSION}-${MIY_SETUP_COMMIT}" --dir "$MIY_SETUP_AGGREGATE"
```

별도 PC에서 받았다면 디렉터리를 새 서버로 전송한다. 아래는 **새 서버에서** 수행하며 `MIY_SETUP_AGGREGATE`를 실제 전송 경로로 지정한다. 단일 Linux archive를 전체 묶음처럼 게시하지 않는다. 도구가 5개 OS/CPU CLI와 2개 helper의 출처·해시를 모두 확인한다.

```bash
cd "$MIY_SETUP_ROOT"
MIY_SETUP_AGGREGATE=/absolute/path/to/transferred-aggregate
python3 scripts/stage-cli-release.py --source "$MIY_SETUP_AGGREGATE" \
  --destination "$MIY_STATE_DIR/cli-downloads" --commit "$MIY_SETUP_COMMIT"
python3 scripts/check-cli-downloads.py --origin "$MIY_SETUP_PORTAL" \
  --expected-commit "$MIY_SETUP_COMMIT" "${MIY_SETUP_CA_ARGS[@]}"
```

같은 버전 서버의 빈 다운로드 디렉터리에 첫 게시하는 것이므로 서버 재시작은 필요 없다. 이후 다른 서버/CLI 버전으로 갱신할 때는 함께 반영해야 한다. Windows GUI 설치기·정식 Release의 절차는 [CLI 릴리스](CLI-RELEASE.md)에 있다. 다운로드를 게시하지 않는 별도 Linux/macOS 개발 PC는 같은 commit의 client-only Cargo 빌드와 `install-cli.sh --from`을 사용할 수 있다.

## 11. 재부팅과 인수 기록

서버 설치 완료는 다음을 확인한 상태다. 외부에서 실행한 결과를 기록하며, 이 안내의 작성/정적 검사를 새 서버 실기 PASS로 대신하지 않는다.

| 확인 | 완료 조건 |
| --- | --- |
| 출처·TLS | 사용자 PC에서 prod/dev/console 모두 인증서 경고 없이 접속, public issuer 일치 |
| 데이터베이스 | core/identity DB, 전체 core migration, 공용 UI seed 성공 |
| 계정 | admin 로그인, 신규 직원의 Keycloak UUID 등록과 로그인, 공유 개발 로그인 꺼짐 |
| 실행 | Core·worker active, 첫 앱 서버 빌드·dev 배포·프레임 화면/ping·앱 DB 접속 |
| 지속성 | 재부팅 후 DB·Keycloak·Nginx·Core·worker 복귀, 기존 계정·설치 상태·앱 DB 유지. 업무 레코드 보존은 별도 앱 인수 |
| 선택 게시 | 게시한 경우에만 CLI 5종/helper 검사와 실제 개발 PC 설치·연결 |

설치용 SSH 세션을 닫아도 systemd와 Docker가 동작하는지 먼저 확인한다. 그 뒤 설치 담당자가 새 서버를 재부팅하고 다시 접속해 **§5의 재개 블록**으로 변수와 배열을 복구한 뒤 확인한다.

```bash
sudo reboot
```

```bash
cd "$MIY_SETUP_ROOT"
docker compose --env-file "$MIY_STATE_DIR/compose.env" ps
sudo systemctl is-active docker nginx miy-v2-serve miy-v2-worker
```

§8의 세 HTTPS health·로그인과 §9의 기존 앱을 다시 확인한다. 설치 commit, OS/도구 버전, 공개 origin, 성공/실패 단계와 시간을 설치 기록에 남긴다. 암호·토큰·쿠키·DB 연결 문자열은 기록하지 않는다. 전체 복원·Windows 실기·최대 부하 등 추가 인수는 [PLAN](PLAN.md#검증과-반영-대기)에서 별도로 추적한다.

## 12. 문제가 생겼을 때

| 증상 | 확인/조치 |
| --- | --- |
| Docker permission denied | Docker 그룹 적용 후 다시 로그인했는지, systemd의 User가 같은 사용자인지 확인 |
| Keycloak 준비 시간 초과 | `docker compose --env-file "$MIY_STATE_DIR/compose.env" ps`와 keycloak/postgres의 최근 로그를 해당 서버에서 확인. DB 초기화 실패·메모리·포트 충돌 해결 후 다시 시작. 비밀 포함 가능 로그는 그대로 공유하지 않음 |
| `password authentication failed` / 첫 admin 로그인 실패 | 첫 import 이후 설정 암호만 바꿨는지 확인. DB를 삭제하지 말고 최초 암호·Keycloak 계정 상태를 복구. bootstrap 재실행은 DB 암호 변경 도구가 아님 |
| Nginx 403 / index가 없음 | `/opt`부터 dist까지 통과·읽기 권한과 포털 빌드 확인. private state의 권한을 열지 않음 |
| `issuer_mismatch`, TLS 오류, Core 재시작 반복 | 서버 자신에서 public console discovery 접근, DNS/NAT, CA·SAN·시각 확인. Nginx가 준비됐는지와 `sudo journalctl -u miy-v2-serve -n 50 --no-pager` 확인 |
| `frame_runtime_missing` / 앱만 열리지 않음 | `/srv/miy/frame-runtime/index.html`, 앱 와일드카드 DNS 또는 포트 풀 방화벽·SAN 확인. 포털 빌드와 프레임 빌드는 별개 |
| 사설 CA에서 앱의 디렉터리·파일·앱 간 API 실패 | 호스트/CLI CA 등록은 컨테이너 SDK의 신뢰를 바꾸지 않음. 현재 미구현 범위이며 공개 CA 배치 또는 앱 런타임/SDK CA 전달 보완이 필요 |
| `docker_storage_low` / 빌드 실패 | Docker data root 여유 15GiB·15%, Rust/Node 의존성 다운로드 경로, 해당 build job의 오류 확인. 활성 DB 볼륨·사용 중 이미지의 일괄 prune 금지 |
| `server_build_required` | 소스를 커밋하고 현재 CLI의 `app build`를 성공시킨 후 deploy. 로컬 Docker 이미지 업로드 경로는 없음 |
| CLI 다운로드가 없음 / 버전 불일치 | 미게시이면 §9 소스 CLI 사용 또는 §10 게시. 서버와 다른 버전의 index를 복사하지 않음 |
| 재부팅 후 서비스가 안 뜸 | Docker/Nginx enable, 컨테이너 restart 정책, unit/drop-in·절대 경로·CA파일·서비스 사용자 홈 확인. 수동 stop한 컨테이너는 up으로 재개 |

직원의 암호 재설정·계정 중지·UUID 조회는 §8의 같은 내부 API 블록을 사용한다. 인증서 만료는 §15, 데이터가 생긴 뒤의 백업·갱신은 아래 절을 따른다.

## 13. 중지 상태의 전체 백업과 복원 확인

**다음은 이 문서의 전용 호스트·기본 경로·동일 PostgreSQL 이미지에서 수행할 수동 절차이며 아직 실기 `NOT_RUN`이다.** `/srv/miy`만 복사하면 DB 백업이 되지 않는다. 반대로 DB만 복사하면 파일·암호화 키·릴리스 이미지가 빠진다. 자동 백업 제품이나 새 서버의 전체 복원 완료를 뜻하지 않으며, 사용자가 별도 VM에서 아래 절차를 인수한 뒤 실제 데이터를 보호하는 데 사용한다.

### 백업 전 중지

1. §5의 입력을 복구하고 다른 운영자·에이전트의 변경을 중단한다. 앱 개발자는 각 프로젝트에서 `miy app dev-stop`을 실행한다. 관리자 CLI로 다른 사용자의 개발 세션을 일괄 종료할 수 있다고 가정하지 않는다.
2. `miy app list`의 모든 페이지와 각 `miy --app APP_ID app status`에서 **현재 실행 중인 앱·환경**을 비공개 운영 기록에 저장한다. 목록에 다음 cursor가 있으면 끝까지 확인한다. 해당 항목마다 `miy --app APP_ID --yes app stop --environment dev` 또는 `prod`를 실행하고 작업 완료를 확인한다. 이미 중지된 앱은 나중에도 시작하지 않는다.
3. Nginx를 중지해 새 요청을 막고, 아래 모든 작업 수가 0인지 확인한다. 진행 중이면 무조건 DB를 복사하지 말고 완료/취소를 해결한다. 작업을 다시 조작하려면 Nginx를 재개한 뒤 1~3을 반복한다. 임의 SQL로 상태를 완료 처리하지 않는다.

```bash
cd "$MIY_SETUP_ROOT"
sudo systemctl stop nginx
docker compose --env-file "$MIY_STATE_DIR/compose.env" exec -T postgres \
  psql -U postgres -d miy_core -v ON_ERROR_STOP=1 <<'SQL'
SELECT 'runtime' AS kind, count(*) FROM runtime_jobs WHERE status IN ('queued','running')
UNION ALL SELECT 'build', count(*) FROM app_builds WHERE status IN ('receiving','queued','running')
UNION ALL SELECT 'database', count(*) FROM jobs WHERE status IN ('queued','running')
UNION ALL SELECT 'development', count(*) FROM app_dev_sessions WHERE status IN ('queued','preparing','ready')
UNION ALL SELECT 'image', count(*) FROM image_uploads WHERE status IN ('receiving','pending','loading')
UNION ALL SELECT 'file', count(*) FROM app_files WHERE status IN ('pending','uploading','cancelling');
SQL
```

위 SQL은 현 schema 23 기준이다. 다른 schema에서는 해당 버전의 작업·파일 테이블에 맞춘다. 파일 예약이 남아 있으면 취소나 만료 정리를 기다린다. `job list`만으로 서버 전체가 유휴라고 판단하지 않는다. 장시간 실행되는 앱 외부 연동·수동 DB 연결도 중지한다.

### 백업 만들기

전부 0이고 앱이 중지된 뒤 실행한다. 백업은 상태 디렉터리 밖에 만들며 비밀·개인정보를 포함한다. 터미널에 내용을 펼치거나 Git에 넣지 않는다. 별도 장치에 암호화해 보관하기 전까지 같은 서버의 사본만으로 장비 장애를 대비했다고 하지 않는다.

```bash
test -z "$(git status --porcelain)"
MIY_BACKUP="/srv/miy-backups/$(date -u +%Y%m%dT%H%M%SZ)"
sudo install -d -o "$MIY_SETUP_USER" -g "$(id -gn)" -m 0700 /srv/miy-backups
mkdir -m 0700 "$MIY_BACKUP"
MIY_BACKUP_PG="$(docker compose --env-file "$MIY_STATE_DIR/compose.env" ps -q postgres)"
MIY_BACKUP_KC="$(docker compose --env-file "$MIY_STATE_DIR/compose.env" ps -q keycloak)"
test -n "$MIY_BACKUP_PG"
test -n "$MIY_BACKUP_KC"
MIY_BACKUP_PG_IMAGE="$(docker inspect --format '{{.Image}}' "$MIY_BACKUP_PG")"
MIY_BACKUP_KC_IMAGE="$(docker inspect --format '{{.Image}}' "$MIY_BACKUP_KC")"
MIY_BACKUP_VOLUME="$(docker inspect --format '{{range .Mounts}}{{if eq .Destination "/var/lib/postgresql"}}{{.Name}}{{end}}{{end}}' "$MIY_BACKUP_PG")"
test -n "$MIY_BACKUP_VOLUME"
MIY_BACKUP_VOLUME_PATH="$(docker volume inspect --format '{{.Mountpoint}}' "$MIY_BACKUP_VOLUME")"
git rev-parse HEAD > "$MIY_BACKUP/source-commit.txt"
git bundle create "$MIY_BACKUP/source.bundle" HEAD
declare -p MIY_BACKUP_PG_IMAGE MIY_BACKUP_KC_IMAGE MIY_BACKUP_VOLUME \
  > "$MIY_BACKUP/database-context.sh"

sudo systemctl stop miy-v2-worker miy-v2-serve
docker compose --env-file "$MIY_STATE_DIR/compose.env" stop keycloak postgres
test "$(docker inspect --format '{{.State.Running}}' "$MIY_BACKUP_PG")" = false
sudo tar --numeric-owner -C "$MIY_BACKUP_VOLUME_PATH" -cpf "$MIY_BACKUP/postgres.tar" .
tar --exclude=./cli-build -C "$MIY_STATE_DIR" -cpf "$MIY_BACKUP/state.tar" .
tar -C "$MIY_SETUP_ROOT/web/platform" -cpf "$MIY_BACKUP/portal.tar" dist
sudo tar -C / -cpf "$MIY_BACKUP/services.tar" \
  etc/nginx etc/systemd/system/miy-v2-serve.service etc/systemd/system/miy-v2-worker.service \
  etc/systemd/system/miy-v2-serve.service.d etc/systemd/system/miy-v2-worker.service.d
# 전용 호스트의 태그 있는 이미지와 고정 DB/IdP 이미지. 실행 중 앱만 저장하지 않는다.
docker image ls --format '{{.Repository}}:{{.Tag}}' \
  | awk '!/<none>/' | sort -u > "$MIY_BACKUP/image-tags.txt"
mapfile -t MIY_BACKUP_IMAGES < "$MIY_BACKUP/image-tags.txt"
docker image save -o "$MIY_BACKUP/images.tar" \
  "$MIY_BACKUP_PG_IMAGE" "$MIY_BACKUP_KC_IMAGE" "${MIY_BACKUP_IMAGES[@]}"
sudo chown "$MIY_SETUP_USER:$(id -gn)" "$MIY_BACKUP/postgres.tar" "$MIY_BACKUP/services.tar"
chmod 0600 "$MIY_BACKUP"/*
(cd "$MIY_BACKUP" && sha256sum *.tar *.bundle *.txt database-context.sh > SHA256SUMS)
```

`state.tar`에는 기본 경로의 TLS와 프레임·공통 파일·앱 산출물이 들어 있다. **인증서/키/CA를 상태 밖에 두었다면 그 실제 파일도 별도로 보관**한다. CA 개인키는 설치 시 분리한 보관소에서 관리한다. 과거 릴리스가 참조하는 태그 없는 이미지·외부 저장소·앱 외부 DB가 있으면 이 백업만으로 충분하지 않다. 각 이미지 참조와 외부 데이터의 별도 보관을 포함한다. 서비스 중지 이후의 실패는 자동 재시작하지 않으므로 아래 재개 절차 또는 복구를 수행한다.

### 원본을 건드리지 않는 DB 복원 확인

다음은 **새 이름의 볼륨**에 백업한 전체 PostgreSQL 클러스터를 풀어 동일 이미지로 여는 절차다. 원본 DB에 덮어쓰지 않는다. host 포트·외부 네트워크·Core·Keycloak 연결이 없고 SQL은 socket으로만 실행한다. PostgreSQL 주요 버전을 바꾸는 방법이 아니다.

백업 이후 재접속했다면 §5의 입력을 복구하고 `MIY_BACKUP`을 실제 백업 절대 경로로 지정한 뒤, 본인이 만든 비공개 `$MIY_BACKUP/database-context.sh`를 `source`하여 이미지 변수를 복구한다. 파일 변조 여부와 백업 해시는 별도 VM 인수에서 확인한다.

```bash
MIY_RESTORE_NAME="miy-restore-$(date -u +%Y%m%dT%H%M%SZ)"
if docker volume inspect "$MIY_RESTORE_NAME" >/dev/null 2>&1 \
  || docker container inspect "$MIY_RESTORE_NAME" >/dev/null 2>&1; then
  echo 'Restore name already exists; choose a new name.' >&2
  exit 1
fi
docker volume create "$MIY_RESTORE_NAME" >/dev/null
MIY_RESTORE_PATH="$(docker volume inspect --format '{{.Mountpoint}}' "$MIY_RESTORE_NAME")"
sudo tar --numeric-owner -C "$MIY_RESTORE_PATH" -xpf "$MIY_BACKUP/postgres.tar"
docker run -d --name "$MIY_RESTORE_NAME" --network none --memory 1g --cpus 1 \
  -v "$MIY_RESTORE_NAME:/var/lib/postgresql" "$MIY_BACKUP_PG_IMAGE"
# 아래 준비 확인이 성공한 뒤 SQL을 실행한다.
docker exec "$MIY_RESTORE_NAME" pg_isready -U postgres
docker exec "$MIY_RESTORE_NAME" psql -U postgres -d postgres -c 'SELECT datname FROM pg_database ORDER BY datname;'
docker exec "$MIY_RESTORE_NAME" psql -U postgres -d miy_core \
  -c 'SELECT version,success FROM _sqlx_migrations ORDER BY version;'
```

기동 직후 `pg_isready`가 실패하면 잠시 후 다시 실행한다. 백업 시점과 core 사용자·설치·릴리스 수, identity 사용자 수, 각 앱 DB의 업무 레코드 수를 대조한다. 실제 업무 행·암호·역할 암호를 로그에 출력하지 않는다. 완료 후 **여기서 만든 이름만** `docker stop "$MIY_RESTORE_NAME"`, `docker rm "$MIY_RESTORE_NAME"`, `docker volume rm "$MIY_RESTORE_NAME"`으로 정리한다. 실패하면 증거와 복원 볼륨을 보존하고 성공으로 기록하지 않는다.

이 확인은 DB 사본이 열리는지 보는 것이며 **새 호스트의 파일·이미지·키·인증·앱까지 전체 복원**을 검증하지 않는다. 그 전체 인수는 별도 VM에서 같은 소스/경로를 준비하고 이미지 반입·상태/DB 복원 후 §8~11을 다시 수행한다. 실제 데이터 원복은 중지 후 변경된 데이터의 처리까지 정한 별도 작업이며, 원본 볼륨을 지워 시험하지 않는다.

### 서비스 재개

```bash
docker compose --env-file "$MIY_STATE_DIR/compose.env" up -d --wait --wait-timeout 120 postgres keycloak
sudo systemctl start nginx
```

§6의 Keycloak 준비와 §7의 공개 issuer 확인 뒤 `sudo systemctl start miy-v2-serve miy-v2-worker`를 실행하고 §8을 확인한다. 처음 저장한 목록의 앱·환경만 `miy --app APP_ID --yes app start --environment dev` 또는 `prod`로 재개한다. 백업 직전에 중지한 상태가 DB에 기록돼 있으므로 모든 앱이 저절로 재개된다고 가정하지 않는다.

## 14. 설치 후 소스 버전 갱신

`scripts/core-rollout.py`에는 기존 개발 호스트의 경로·컨테이너·서비스 가정이 있으므로 **이 서버의 범용 갱신기로 실행하지 않는다.** 아래는 주소·서비스 경로·PostgreSQL/Keycloak 이미지가 유지되는 개발 서버의 수동 갱신 순서다. DB 주요 버전, issuer, 상태 경로, 설정 형식이 바뀌는 릴리스는 그 변경에 맞는 별도 전환 절차가 필요하다. `git pull` 후 실행 파일만 바꾸는 방식은 사용하지 않는다.

1. 설치 시 기록한 이전 commit과 갱신할 **전체 commit SHA**를 정한다. Git 작업본이 깨끗한지 확인하고 `git fetch origin` 후 변경 문서·migration·도구 버전을 읽는다. 태그나 main이 나중에 이동해도 선택한 SHA는 바꾸지 않는다.
2. 별도 checkout에서 서버·포털·프레임을 먼저 빌드한다. 현재 제공 중인 dist·실행 파일을 빌드 대상으로 쓰지 않는다.

```bash
cd "$MIY_SETUP_ROOT"
test -z "$(git status --porcelain)"
git fetch origin
MIY_UPDATE_COMMIT=REPLACE_WITH_FULL_COMMIT_SHA
MIY_UPDATE_COMMIT="$(git rev-parse --verify "$MIY_UPDATE_COMMIT^{commit}")"
MIY_UPDATE_ROOT="$HOME/miy-candidate-$MIY_UPDATE_COMMIT"
git worktree add --detach "$MIY_UPDATE_ROOT" "$MIY_UPDATE_COMMIT"
(
  cd "$MIY_UPDATE_ROOT"
  export CARGO_BUILD_JOBS=2
  cargo build --release --locked -p miy
  pnpm install --frozen-lockfile
  pnpm --filter @miy/platform build
  pnpm --filter @miy/frame-runtime exec vite build \
    --outDir "$MIY_UPDATE_ROOT/frame-dist" --emptyOutDir
)
```

3. §13의 앱/세션/작업 중지·전체 백업·별도 DB 복원 확인을 수행한다. **이 단계에서는 마지막 서비스 재개를 보류**한다. 대상 migration이 있으면 실제 DB보다 먼저 복원 사본에 적용하여 schema와 보존 데이터를 확인한다. 복원 확인이 실패하면 실제 DB를 바꾸지 않는다.
4. 아래는 Nginx·Core·worker·앱이 중지된 상태에서 실행한다. 후보의 최소 검사를 마친 사실과 백업 위치를 기록한다. **migration 전에 PostgreSQL만 재개**한다. 새 포털과 프레임은 같은 후보에서 가져온다. 새 migration이 있으면 `migrate`, 이어 공용 UI registry용 `seed`를 수행한다. 기존 migration 파일은 수정하지 않는다.

```bash
cd "$MIY_SETUP_ROOT"
git switch --detach "$MIY_UPDATE_COMMIT"
install -m 0755 "$MIY_UPDATE_ROOT/target/release/miy" "$MIY_STATE_DIR/bin/miy-server"
mv web/platform/dist "$MIY_BACKUP/previous-portal-dist"
cp -a "$MIY_UPDATE_ROOT/web/platform/dist" web/platform/dist
mv "$MIY_STATE_DIR/frame-runtime" "$MIY_BACKUP/previous-frame-runtime"
cp -a "$MIY_UPDATE_ROOT/frame-dist" "$MIY_STATE_DIR/frame-runtime"
chmod -R a+rX web/platform/dist
docker compose --env-file "$MIY_STATE_DIR/compose.env" up -d --wait --wait-timeout 120 postgres
docker compose --env-file "$MIY_STATE_DIR/compose.env" exec -T postgres pg_isready -h 127.0.0.1 -U postgres
bash scripts/run-service.sh migrate
bash scripts/run-service.sh seed
```

5. §13의 재개는 DB/IdP→Nginx→issuer 확인→Core/worker 순서로 진행한다. §8의 health·로그인, 기존 앱과 레코드, §9의 CLI를 확인한다. CLI/번들 버전이 바뀌면 같은 후보의 CLI를 §9에서 다시 만들고, 게시하려면 §10에서 새 aggregate도 준비한다. 이전 다운로드를 새 서버 버전으로 취급하지 않는다.
6. 성공 후에만 `MIY_SETUP_COMMIT="$MIY_UPDATE_COMMIT"`으로 바꾸고, §5의 `declare -p`와 같은 변수 목록을 **새 임시 파일에** 써서 `setup-context.sh`와 교체한다. 이전 입력 파일은 백업 안에 있다. 설치 기록에는 이전/새 SHA, 백업 위치, migration, 확인 결과를 남긴다. 후보 checkout은 문제 확인이 끝날 때까지 보존한다.

**되돌릴 때:** DB나 registry를 변경하지 않았다면 이전 commit·실행 파일·포털/프레임·설정을 백업에서 함께 되돌린다. `migrate`/`seed` 후에는 바이너리만 되돌리지 않고, 서비스를 중지한 채 같은 시점의 DB·state·UI·설정을 한 묶음으로 복원한다. 재개 후 추가된 데이터를 임의로 버리지 않으며, 중지 후 추가 백업과 복구 방침을 먼저 정한다. 이 수동 순서는 미검증이며 무중단 갱신이나 자동 host updater의 완성을 뜻하지 않는다.

## 15. 인증서 만료 확인과 갱신

§4의 사설 CA 서버 인증서는 365일 후 만료된다. 최소 월 1회와 갱신 전에 확인한다. `-checkend`의 비정상 종료 코드는 갱신 시기를 나타내므로 셸이 끝나지 않도록 조건식에서 처리한다.

```bash
openssl x509 -in "$MIY_SETUP_CERT" -noout -dates
if openssl x509 -in "$MIY_SETUP_CERT" -noout -checkend 2592000; then
  echo 'Certificate has more than 30 days remaining.'
else
  echo 'Renew the certificate before it expires.'
fi
```

공개 CA는 발급 기관의 DNS-01/갱신 절차로 같은 SAN의 새 leaf+중간 인증서를 받는다. 사설 CA는 **기존 CA**로 서명한다. 루트 CA를 다시 만들면 모든 단말에 신뢰를 다시 배포해야 하므로 일반적인 leaf 갱신에서 §4의 CA 생성을 반복하지 않는다.

```bash
MIY_TLS_RENEW="$MIY_STATE_DIR/tls/renew-$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -m 0700 "$MIY_TLS_RENEW"
openssl req -new -newkey rsa:2048 -nodes -sha256 \
  -keyout "$MIY_TLS_RENEW/server.key" -out "$MIY_TLS_RENEW/server.csr" -subj '/CN=MIY server'
cat > "$MIY_TLS_RENEW/server.ext" <<EOF
subjectAltName=$MIY_SETUP_SAN
basicConstraints=critical,CA:FALSE
keyUsage=critical,digitalSignature,keyEncipherment
extendedKeyUsage=serverAuth
subjectKeyIdentifier=hash
authorityKeyIdentifier=keyid,issuer
EOF
```

사설 CA 서명은 CA 개인키를 보관하는 환경에서 수행하고 CSR과 위 ext만 전달한다. 아래 CA 경로는 실제 보관 위치로 바꾼다. 키를 채팅·Git·로그에 노출하지 않는다.

```bash
openssl x509 -req -in server.csr -CA /secure/ca/root-ca.crt -CAkey /secure/ca/root-ca.key \
  -set_serial "0x$(openssl rand -hex 16)" -out server-chain.pem \
  -days 365 -sha256 -extfile server.ext
```

발급된 `server-chain.pem`을 `$MIY_TLS_RENEW/server-chain.pem`으로 안전하게 전송한다. 새 인증서/키를 먼저 검사하고 성공한 경우에만 현재 경로를 교체한다. 아래는 같은 CA의 leaf 갱신용이며 기존 개인키와 인증서도 비공개로 보관한다. 교체 도중 어느 명령이 실패해도 이전 인증서와 키를 **둘 다** 복구한 뒤 Nginx를 확인한다.

```bash
python3 "$MIY_SETUP_ROOT/scripts/check-server-tls.py" --state-dir "$MIY_STATE_DIR" \
  --certificate "$MIY_TLS_RENEW/server-chain.pem" --key "$MIY_TLS_RENEW/server.key" \
  "${MIY_SETUP_CA_ARGS[@]}"
cp -p "$MIY_SETUP_CERT" "$MIY_TLS_RENEW/previous-chain.pem"
cp -p "$MIY_SETUP_KEY" "$MIY_TLS_RENEW/previous.key"
install -m 0644 "$MIY_TLS_RENEW/server-chain.pem" "$MIY_SETUP_CERT"
install -m 0600 "$MIY_TLS_RENEW/server.key" "$MIY_SETUP_KEY"
if sudo nginx -t && sudo systemctl reload nginx; then
  echo 'Certificate reloaded; verify the live endpoints next.'
else
  install -m 0644 "$MIY_TLS_RENEW/previous-chain.pem" "$MIY_SETUP_CERT"
  install -m 0600 "$MIY_TLS_RENEW/previous.key" "$MIY_SETUP_KEY"
  sudo nginx -t
  sudo systemctl reload nginx
  echo 'Renewal failed; previous certificate restored.' >&2
  exit 1
fi
python3 "$MIY_SETUP_ROOT/scripts/check-server-tls.py" --state-dir "$MIY_STATE_DIR" \
  --certificate "$MIY_SETUP_CERT" --key "$MIY_SETUP_KEY" "${MIY_SETUP_CA_ARGS[@]}" --connect
```

마지막 연결 확인이 실패한 경우에도 위의 이전 인증서/키를 복구하고 Nginx 검사·reload를 수행한다. 같은 CA라면 단말에 다시 등록할 필요는 없다. CA 자체·공개 origin·SAN 구성을 바꾸는 경우에는 이 leaf 교체 외에 OS/브라우저/CLI 신뢰와 issuer/redirect 변경을 별도로 확인한다. 자동 갱신은 이 수동 절차에 포함하지 않는다.
