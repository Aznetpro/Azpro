#!/bin/bash
# Azpro Installer - Version v3

REPO_URL="https://github.com/Aznetpro/Azpro.git"
REPO_BRANCH="main"
CMD_NAME="azpro"
TOTAL_STEPS=7

CURRENT_STEP=0
BUILD_DIR="/tmp/Mpro_build"
CARGO_TARGET_DIR="/var/cache/azpro/cargo-target"
CPU_COUNT="$(nproc 2>/dev/null || echo 2)"
MEMORY_MB="$(awk '/MemTotal/ {print int($2/1024)}' /proc/meminfo 2>/dev/null || echo 2048)"
if [ "$MEMORY_MB" -lt 2500 ]; then
    BUILD_JOBS=1
elif [ "$MEMORY_MB" -lt 5000 ]; then
    BUILD_JOBS=2
else
    BUILD_JOBS="$CPU_COUNT"
    [ "$BUILD_JOBS" -gt 4 ] && BUILD_JOBS=4
fi
ACTIVE_AZPRO_SERVICES=()
SERVICES_STOPPED=0

# --- Cores e Estilos ---
GREEN="\033[0;32m"
BLUE="\033[0;34m"
RED="\033[0;31m"
NC="\033[0m"
BOLD="\033[1m"

log_info() {
    echo -e "${BLUE}${BOLD}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}${BOLD}[SUCESSO]${NC} $1"
}

log_error() {
    echo -e "${RED}${BOLD}[ERRO]${NC} $1"
    exit 1
}

restore_active_services() {
    if [ "$SERVICES_STOPPED" -eq 1 ]; then
        for SERVICE in "${ACTIVE_AZPRO_SERVICES[@]}"; do
            systemctl start "$SERVICE" >/dev/null 2>&1 || echo "[AVISO] Não foi possível restaurar ${SERVICE}."
        done
        SERVICES_STOPPED=0
    fi
}
trap restore_active_services EXIT

show_progress() {
    CURRENT_STEP=$((CURRENT_STEP + 1))
    PERCENT=$((CURRENT_STEP * 100 / TOTAL_STEPS))
    log_info "${PERCENT}% - $1"
}

# --- Verificação de Root ---
if [ "$EUID" -ne 0 ]; then
    log_error "Este script precisa ser executado como ROOT."
fi

# Memoriza somente serviços Azpro ativos para restaurá-los depois da troca dos binários.
for SERVICE_FILE in /etc/systemd/system/proxy-*.service; do
    [ -f "$SERVICE_FILE" ] || continue
    if grep -Eq '^[[:space:]]*ExecStart=.*/opt/mpro/(proxy|proxy-xhttp|proxy-integrated)([[:space:]]|$)' "$SERVICE_FILE"; then
        SERVICE_NAME=$(basename "$SERVICE_FILE")
        if systemctl is-active --quiet "$SERVICE_NAME" 2>/dev/null; then
            ACTIVE_AZPRO_SERVICES+=("$SERVICE_NAME")
        fi
    fi
done

clear
echo -e "${CYAN}${BOLD}    A Z P R O  V E R S I O N  F R E E${NC}"
echo -e "${BLUE}${BOLD}----------------------------------------${NC}"
log_info "Iniciando instalação do Azpro v3.1.0 (🇧🇷)..."

# --- Etapa 1 ---
show_progress "Verificando dependências..."
REQUIRED_PACKAGES=(curl build-essential git libssl-dev pkg-config openssl)
MISSING_PACKAGES=()
for PACKAGE in "${REQUIRED_PACKAGES[@]}"; do
    if [ "$(dpkg-query -W -f='${db:Status-Status}' "$PACKAGE" 2>/dev/null || true)" != "installed" ]; then
        MISSING_PACKAGES+=("$PACKAGE")
    fi
done
if ((${#MISSING_PACKAGES[@]} > 0)); then
    apt-get update -y > /dev/null 2>&1 || log_error "Falha ao atualizar a lista de pacotes."
    DEBIAN_FRONTEND=noninteractive apt-get install -y "${MISSING_PACKAGES[@]}" > /dev/null 2>&1 || log_error "Falha ao instalar dependências."
fi

# --- Etapa 2 ---
show_progress "Verificando Rust..."
if ! command -v cargo &> /dev/null; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y > /dev/null 2>&1
    source "$HOME/.cargo/env"
else
    source "$HOME/.cargo/env"
fi

# --- Etapa 3 ---
show_progress "Baixando código fonte..."
rm -rf "$BUILD_DIR"
git clone --depth 1 --branch "$REPO_BRANCH" "$REPO_URL" "$BUILD_DIR" > /dev/null 2>&1 || log_error "Falha ao clonar repositório."
cd "$BUILD_DIR" || log_error "Falha ao acessar diretório."

# --- Etapa 4 ---
show_progress "Compilando com cache e ${BUILD_JOBS} núcleos..."
mkdir -p "$CARGO_TARGET_DIR" || log_error "Falha ao criar cache de compilação."
CARGO_TARGET_DIR="$CARGO_TARGET_DIR" cargo build --release --bins --jobs "$BUILD_JOBS" > /tmp/mpro_build.log 2>&1
if [ $? -ne 0 ]; then
    cat /tmp/mpro_build.log
    log_error "Falha na compilação. Veja logs acima."
fi

# --- Etapa 5 ---
show_progress "Instalando binários..."
mkdir -p /opt/mpro

# Para somente os serviços Azpro que estavam ativos; o menu azpro continua
# executando durante a atualização.
SERVICES_STOPPED=1
for SERVICE in "${ACTIVE_AZPRO_SERVICES[@]}"; do
    systemctl stop "$SERVICE" || log_error "Não foi possível parar ${SERVICE}; binários não foram trocados."
done
systemctl stop proxy-websocket.service > /dev/null 2>&1 || true
systemctl disable proxy-websocket.service > /dev/null 2>&1 || true
pkill -f '^/opt/mpro/proxy-websocket([[:space:]]|$)' > /dev/null 2>&1 || true
rm -f /etc/systemd/system/proxy-websocket.service /opt/mpro/proxy-websocket
systemctl daemon-reload > /dev/null 2>&1 || true
pkill -f '^/opt/mpro/proxy([[:space:]]|$)' > /dev/null 2>&1 || true
pkill -f '^/opt/mpro/proxy-xhttp([[:space:]]|$)' > /dev/null 2>&1 || true
pkill -f '^/opt/mpro/proxy-integrated([[:space:]]|$)' > /dev/null 2>&1 || true
sleep 1

# Copiar com força (-f)
cp -f "$CARGO_TARGET_DIR/release/mpro" /opt/mpro/proxy || log_error "Falha ao copiar mpro. Verifique se o disco está cheio."
chmod +x /opt/mpro/proxy

if [ -f "$CARGO_TARGET_DIR/release/mpro-xhttp" ]; then
    cp -f "$CARGO_TARGET_DIR/release/mpro-xhttp" /opt/mpro/proxy-xhttp
    chmod +x /opt/mpro/proxy-xhttp
    ln -sf /opt/mpro/proxy-xhttp /usr/local/bin/mpro-xhttp
fi

if [ -f "$CARGO_TARGET_DIR/release/mpro-integrated" ]; then
    cp -f "$CARGO_TARGET_DIR/release/mpro-integrated" /opt/mpro/proxy-integrated
    chmod +x /opt/mpro/proxy-integrated
    ln -sf /opt/mpro/proxy-integrated /usr/local/bin/mpro-integrated
fi

# Retoma apenas serviços que estavam ativos antes da atualização.
for SERVICE in "${ACTIVE_AZPRO_SERVICES[@]}"; do
    systemctl start "$SERVICE" || log_error "Binários atualizados, mas falhou ao reiniciar ${SERVICE}. Verifique com: systemctl status ${SERVICE}"
done
SERVICES_STOPPED=0

# Menu
if [ -f "menu.sh" ]; then
    cp -f menu.sh /opt/mpro/menu
    chmod +x /opt/mpro/menu
    MENU_COMMAND="/usr/local/bin/${CMD_NAME}"
    if { [ -e "$MENU_COMMAND" ] || [ -L "$MENU_COMMAND" ]; } && \
        { [ ! -L "$MENU_COMMAND" ] || [ "$(readlink -- "$MENU_COMMAND")" != "/opt/mpro/menu" ]; }; then
        log_error "Já existe outro comando em ${MENU_COMMAND}; ele não foi sobrescrito."
    fi
    ln -sfn /opt/mpro/menu "$MENU_COMMAND"
    for LEGACY_COMMAND in /usr/local/bin/wzpro /usr/local/bin/wspro /usr/local/bin/mpro; do
        if [ -L "$LEGACY_COMMAND" ] && [ "$(readlink -- "$LEGACY_COMMAND")" = "/opt/mpro/menu" ]; then
            rm -f -- "$LEGACY_COMMAND"
        fi
    done
fi

# Certificados
if [ ! -f /opt/mpro/cert.pem ]; then
    openssl req -x509 -newkey rsa:2048 -keyout /opt/mpro/key.pem -out /opt/mpro/cert.pem -days 3650 -nodes -subj "/CN=Mpro" 2>/dev/null
fi

# --- Etapa 6 ---
show_progress "Limpando..."
rm -rf /tmp/Mpro_build

# --- Etapa 7 ---
log_success "Instalação concluída com sucesso!"
echo -e "Use o comando ${YELLOW}${CMD_NAME}${NC} para abrir o menu."
echo -e "--------------------------------------------------------------"
